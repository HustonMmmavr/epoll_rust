extern crate nix;
extern crate chrono;
mod file_handler;
mod client_manager;

mod client;
mod http;
use nix::sys::epoll::*;
use nix::sys::socket::*;
use nix::sys::epoll::EpollCreateFlags;
// use nix::unistd::close;X?S
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
use client_manager::client_manager::ClientManager;

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

fn epoll_loop(server_sock: RawFd)-> nix::Result<()> {
    let path = "/home/mavr/httptest";
    // let mut manager = ClientManager::new("/home/mavr/httptest");
    let mut clients: HashMap<RawFd, HttpClient> = HashMap::new();
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
                 cur_event != cur_event & (EpollFlags::EPOLLIN|EpollFlags::EPOLLOUT) {
                    debug_print!("error big if {:?}", cur_event);
                    close(epoll_events[i].data() as i32);
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
                            println!("Error accept {:?}", err);
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
                    clients.insert(client_fd, HttpClient::new(client_fd, EpollFlags::EPOLLIN));
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

                    clients.remove(&cur_socket);
                    match shutdown(cur_socket as i32, Shutdown::Both) {
                        Ok(e) => {},
                        Err(err) => println!("Err shutdown {:?}", err)
                    }
                    close(cur_socket as i32)?;
                    closed += 1;
                    debug_print!("closed: {:?}", closed);
                }
            }
            continue;
        }
    }
}

fn main() {

    let server_sock = match socket(AddressFamily::Inet, SockType::Stream, SockFlag::SOCK_NONBLOCK, SockProtocol::Tcp) {
        Ok(sock) => sock,
        Err(err) => panic!("{:?}", err)
    };

    match setsockopt(server_sock, ReuseAddr, &true) {
        Ok(_) => {},
        Err(err) => panic!("Error set sock opt {:?}", err)
    }
    let addr = SockAddr::new_inet(InetAddr::new(IpAddr::new_v4(127, 0, 0, 1), 5467));
    
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

    for i in 0..3 {
        v.push(thread::spawn(move || {
                    epoll_loop(server_sock.clone());
                }
            )
        );
    }
    epoll_loop(server_sock.clone());    

    for th in v {
        th.join();
    }
}


    // let buffer = [0; BUFSIZE];

    // let mut clients: HashMap<RawFd, Client> = HashMap::new();//SockFlag::EPOLLET



    // let ac = vec![1, 2, 3];

    // let bc = ac.as_slice();
    // let cc = bc[1..].len();
    // println!("{:?}", cc);



// const BUFSIZE: usize =  16384;
// #[derive(Hash, Eq, PartialEq, Debug, Clone, Copy)]
// pub enum State {
//     Read,
//     Write,
// }

// #[derive(Clone, Debug)]
// pub struct Client {
//     pub readed: Vec<u8>,
//     pub state: State
// }

// impl Client {
//     fn new() -> Self {
//         Client {
//             readed: Vec::new(),
//             state: State::Read
//         }
//     }
// }

                    // println!("{:?}", clients.insert(client_fd, Client::new()));
                    // clients.insert(client_fd, Client::new());
                    // manager.add_client(client_fd, HttpClient::new(client_fd, EpollFlags::EPOLLIN));
                // let mut client = clients.get_mut(&cur_socket).unwrap();
                // let mut buf = [0;2048];
                // let mut b = false;
                // loop {
                //     let mut total_len = client.readed.len();
                //     let size = match read(cur_socket, &mut buf) {
                //         Ok(size) => {
                //             client.readed.extend_from_slice(&buf[..size]);
                //             if (size == 0) {
                //                 break;
                //             }
                //         },
                //         Err(Sys(err)) =>  {
                //             // print!("{:?}", e);
                //             if (err != Errno::EAGAIN)  {
                //                 print!("POPA1");
                //             }
                //             break;
                //         },
                //         Err(e) => {
                //             print!("POPA");
                //         }
                //     };
                // }

                // let req = std::str::from_utf8(&client.readed.as_slice()).unwrap().to_string();
                    
                // if !( req.find("\n\n").is_some() || req.find("\r\n\r\n").is_some() ){
                //     epoll_ctl(epfd, EpollOp::EpollCtlDel, cur_socket, &mut epoll_events[i]);
                //     close(cur_socket as i32)?;
                //     println!("clienrt {:?} request {}", cur_socket, req);
                //     continue;
                // }

                // let mut ev = EpollEvent::new(EpollFlags::EPOLLOUT, cur_socket as u64);
                // match epoll_ctl(epfd, EpollOp::EpollCtlMod, cur_socket, &mut ev) {
                //     Ok(e) => {},
                //     Err(err) => println!("Read ctl Err {:?}", err)
                // }

                // client.state = State::Write;
                // continue;


                    // println!("write");
                // let buf = "HTTP/1.1 200 Ok\nConnection: close\nContent-Type: text/plain\n\nha?\n\n";
                // let buf_len = buf.len();
                // let mut sended = 0;
                // let mut need_to_close = true;
                // // loop {
                //     match write(cur_socket, &buf.as_bytes()[sended..]) {
                //         Ok(len) => {
                //         //     if len == 0 {
                //         //         break;
                //         //     }
                //             sended += len;

                //                 need_to_close = true;

                //             if sended >=  buf_len {
                //                 need_to_close = true;
                //                 // break;
                //             }
                //         }, 
                //         Err(Sys(e)) => { 
                //             if (e != Errno::EAGAIN)  {
                //             print!("POPA");
                //             }
                //             // break;
                //         },
                //         Err(e) => {
                //             print!("{:?}", e);
                //         }
                //     }
                // }        