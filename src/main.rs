extern crate nix;
extern crate chrono;
extern crate num_cpus;
mod file_handler;
mod client_manager;
mod config_reader;
mod client;
mod http;
use nix::sys::epoll::*;
use config_reader::reader::ConfigReader;
use nix::sys::socket::*;
use nix::sys::epoll::EpollCreateFlags;
use nix::sys::socket::sockopt::ReuseAddr;
use std::collections::HashMap;
use std::os::unix::io::RawFd;
use nix::errno::Errno;
use nix::Error;
use nix::unistd::{read, write, close};
use nix::fcntl::*;
use std::thread;
use nix::Error::Sys;
use client::client::{ClientState, HttpClient};

#[cfg(feature = "my_debug")]
macro_rules! debug_print {
    ($( $args:expr ),*) => { println!( $( $args ),* ); }
}

// Non-debug version
#[cfg(not(feature = "my_debug"))]
macro_rules! debug_print {
    ($( $args:expr ),*) => {}
}

fn socket_to_nonblock(sock: RawFd) -> Result<(), nix::Error> {
    let flags  = match fcntl(sock, F_GETFL) {
        Ok(fl) => fl,
        Err(err) => return Err(err)
    };
    let new_flags = flags | OFlag::O_NONBLOCK.bits();
    let new_flags = OFlag::from_bits(new_flags).unwrap();

    match fcntl(sock, F_SETFL(new_flags)) {
        Ok(_) => return Ok(()),
        Err(err) => return Err(err)
    }
}

fn epoll_loop<'a>(server_sock: RawFd, path: &'a str)-> nix::Result<()> {
    // let path = "/home/mavr/http-test";
    // let mut manager = ClientManager::new("/home/mavr/httptest");
    let mut clients: HashMap<RawFd, HttpClient<'a>> = HashMap::new();
    let epfd = epoll_create1(EpollCreateFlags::from_bits(0).unwrap())?;

    let mut ev = EpollEvent::new(EpollFlags::EPOLLIN, server_sock as u64);
    epoll_ctl(epfd, EpollOp::EpollCtlAdd, server_sock, &mut ev)?;

    let mut epoll_events = vec![EpollEvent::empty(); 1024];
    let critical_error = false;
    let mut accepted = 0;
    let mut closed = 0;

    let mut refused = 0;
    loop {
        // println!("wait");
        let nfds = match epoll_wait(epfd, &mut epoll_events, -1) {
            Ok(n) => n,
            Err(e) => {
                println!("Err wait {:?}", e);
                panic!();
            }
        };

        for i in 0..nfds {
            let cur_socket = epoll_events[i].data() as i32;
            let cur_event = epoll_events[i].events();

            if cur_event == cur_event & EpollFlags::EPOLLERR || cur_event == cur_event & EpollFlags::EPOLLHUP ||
                 cur_event != cur_event & (EpollFlags::EPOLLIN|EpollFlags::EPOLLOUT) || cur_event == cur_event & EpollFlags::EPOLLRDHUP {
                    debug_print!("error big if {:?}", cur_event);
                    println!("hi");
                    close(epoll_events[i].data() as i32);
                    epoll_ctl(epfd, EpollOp::EpollCtlDel, cur_socket, &mut epoll_events[i]);
                    let client = clients.remove(&cur_socket).unwrap();
                    client.clear();
                    continue;
            }                


            if server_sock == cur_socket {
                debug_print!("accept");
                loop {
                    let client_fd = match accept(server_sock) {
                        Ok(client) => {
                            accepted += 1;
                            debug_print!("Accepted {:?} Closed {:?} Dif: {:?} Refused {:?} Events len: {:?}", accepted, closed, accepted - closed, refused, nfds);
                            client
                        }
                        Err(err) => {
                            refused += 1;
                            debug_print!("Error accept {:?}", err);
                            break;
                        }
                    };

                    match socket_to_nonblock(client_fd) {
                        Ok(_) => {},
                        Err(err) => println!("Non block err {:?}", err)
                    }

                    let mut ev = EpollEvent::new(EpollFlags::EPOLLIN | EpollFlags::EPOLLET, client_fd as u64);
                    match epoll_ctl(epfd, EpollOp::EpollCtlAdd, client_fd, &mut ev) {
                        Ok(e) => {},
                        Err(err) => println!("Server accept ctl {:?}", err)
                    }
                    clients.insert(client_fd, HttpClient::new(client_fd, EpollFlags::EPOLLIN, path));
                    // clients.insert(client_fd. Client::new());
                    break;
                }
                debug_print!("loop breaked");
                continue;
            }
                
            if cur_event == cur_event & EpollFlags::EPOLLIN {
                // println!("read");
                let mut is_broken = false;
                
                {
                    let client = clients.get_mut(&cur_socket).unwrap();
                    // let client = manager.get_client(cur_socket);
                    match client.read() {
                        Ok(cli_state) => {
                            match cli_state {
                                ClientState::READING => {},
                                ClientState::REQUEST_READED => {
                                    let mut ev = EpollEvent::new(EpollFlags::EPOLLOUT, cur_socket as u64);
                                    match epoll_ctl(epfd, EpollOp::EpollCtlMod, cur_socket, &mut ev) {
                                        Ok(e) => {},
                                        Err(err) => println!("Read ctl Err {:?}", err)
                                    }
                                    continue;
                                },
                                ClientState::ERROR => is_broken = true,
                                _ => is_broken = true
                            }
                        }, 
                        Err(err) => {
                            is_broken = true;
                            println!("{:?}", err);
                        }
                    }
                }

                if is_broken {
                    close(cur_socket);
                    epoll_ctl(epfd, EpollOp::EpollCtlDel, cur_socket, &mut epoll_events[i]);
                    let cl = clients.remove(&cur_socket);
                    // manager.remove_client(cur_socket);
                    // println!("{:?}", cl);                    
                }
                continue;
            }

            if cur_event == cur_event & EpollFlags::EPOLLOUT {

                let mut need_to_close = false;
                {
                    // let client = manager.get_client(cur_socket);
                    let client = clients.get_mut(&cur_socket).unwrap();
                    match client.write(path) {
                        Ok(state) => {
                            match state {
                                ClientState::RESPONSE_WRITED => need_to_close = true,
                                ClientState::FILE_WRITING => {},
                                ClientState::WRITING => {},
                                _ => need_to_close = true
                            }
                        },
                        Err(err) => {
                            need_to_close = true;
                        }
                    }
                }

                if (need_to_close) {
                    match epoll_ctl(epfd, EpollOp::EpollCtlDel, cur_socket as i32, &mut epoll_events[i]) {
                        Ok(e) => {},
                        Err(err) => println!("Err epollctl write {:?}", err)
                    }
                    // manager.remove_client(cur_socket);

                    let cl = clients.remove(&cur_socket).unwrap();
                    match shutdown(cur_socket as i32, Shutdown::Both) {
                        Ok(e) => {},
                        Err(err) => println!("Err shutdown {:?}", err)
                    }
                    close(cur_socket as i32)?;
                    closed += 1;
                    cl.clear();
                    debug_print!("closed: {:?}", closed);
                }
            }
            continue;
        }
    }
}

fn main() {
    let config = match ConfigReader::read() {
        Ok(conf) => conf, 
        Err(err) => {
            println!("Error parse config {:?}", err);
            panic!("");
        }
    };

    let server_sock = match socket(AddressFamily::Inet, SockType::Stream, SockFlag::SOCK_NONBLOCK, SockProtocol::Tcp) {
        Ok(sock) => sock,
        Err(err) => panic!("{:?}", err)
    };

    match setsockopt(server_sock, ReuseAddr, &true) {
        Ok(_) => {},
        Err(err) => panic!("Error set sock opt {:?}", err)
    }
    let addr = SockAddr::new_inet(InetAddr::new(IpAddr::new_v4(0, 0, 0, 0), config.get_port() as u16));
    
    match bind(server_sock, &addr) {
        Ok(_) => {},
        Err(err) => panic!("Error bind: {:?}", err)
    }   

    socket_to_nonblock(server_sock);
    match listen(server_sock, 1024) {
        Ok(_) => {},
        Err(err) => panic!("Error listen: {:?}", err)
    }
    
    let mut v = vec![];


    let cpu_limit = config.get_cpu_count();
    println!("cpu limit {:?}", cpu_limit);
    for i in 0..cpu_limit {
        let path = config.get_path_to_static();
        v.push(thread::spawn(move || {
                    epoll_loop(server_sock.clone(), &path);
                }
            )
        );
    }

    for th in v {
        th.join();
    }
}