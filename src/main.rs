extern crate nix;
extern crate chrono;
extern crate num_cpus;
// extern crate hwloc;
extern crate libc;
mod file_handler;
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
use std::time::{SystemTime, UNIX_EPOCH};
use std::ops::Sub;
use std::sync::{Arc, Mutex};
// use hwloc::{Topology, ObjectType, CPUBIND_THREAD, CpuSet};
use nix::libc::rlimit64;
use nix::libc::setrlimit64;
use nix::unistd::Pid;
use std::process;


#[cfg(feature = "my_debug")]
macro_rules! debug_print {
    ($( $args:expr ),*) => { println!( $( $args ),* ); }
}

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

fn epoll_loop<'a>(server_sock: RawFd, worker_count: usize, path: &'a str) -> nix::Result<()> {
    let mut clients: HashMap<RawFd, HttpClient<'a>> = HashMap::new();
    let epfd = epoll_create1(EpollCreateFlags::from_bits(0).unwrap())?;

    let mut ev = EpollEvent::new(EpollFlags::EPOLLIN | EpollFlags::EPOLLET, server_sock as u64);
    epoll_ctl(epfd, EpollOp::EpollCtlAdd, server_sock, &mut ev)?;

    let mut epoll_events = vec![EpollEvent::empty(); 1024];
    let critical_error = false;
    let mut accepted = 0;
    let mut closed = 0;
    let mut refused = 0;
    let mut counter_wait = 0;
    loop {
        let nfds = match epoll_wait(epfd, &mut epoll_events, -1) {
            Ok(n) => n,
            Err(e) => {
                println!("Err wait {:?}", e);
                panic!("vuh");
            }
        };

        for i in 0..nfds {
            let cur_socket = epoll_events[i].data() as i32;
            let cur_event = epoll_events[i].events();
            let mut cli_size = 0;

            if cur_event == cur_event & EpollFlags::EPOLLERR || cur_event == cur_event & EpollFlags::EPOLLHUP ||
                 cur_event != cur_event & (EpollFlags::EPOLLIN|EpollFlags::EPOLLOUT) || cur_event == cur_event & EpollFlags::EPOLLRDHUP {
                    debug_print!("error big if {:?}", cur_event);
                    debug_print!("hi");
                    close(epoll_events[i].data() as i32);
                    epoll_ctl(epfd, EpollOp::EpollCtlDel, cur_socket, &mut epoll_events[i]);
                    let client = clients.remove(&cur_socket).unwrap();
                    client.clear();
                    continue;
            } else {             
                let mut count_acc = 0;  
                if server_sock == cur_socket {
                    debug_print!("accept");
                    loop {
                        
                        let client_fd = match accept(server_sock) {
                            Ok(client) => {
                                debug_print!("Accepted {:?} Closed {:?} Dif: {:?} Refused {:?} Events len: {:?}", accepted, closed, accepted - closed, refused, nfds);
                                // println!("Wait № {:?} Thread {:?} acccepted", counter_wait, thread::current().id());
                                client
                            }
                            Err(err) => {
                                // println!("Wait № {:?} Thread {:?} {:?}", counter_wait, thread::current().id(), err );
                                debug_print!("Error accept {:?}", err);
                                break;
                            }
                        };

                        match socket_to_nonblock(client_fd) {
                            Ok(_) => {},
                            Err(err) => {
                                println!("Non block err {:?}", err);
                                close(client_fd);
                                break;
                            } 
                        }

                        let mut ev = EpollEvent::new(EpollFlags::EPOLLIN | EpollFlags::EPOLLET, client_fd as u64);
                        match epoll_ctl(epfd, EpollOp::EpollCtlAdd, client_fd, &mut ev) {
                            Ok(e) => {},
                            Err(err) => {
                                println!("Server accept ctl {:?}", err);
                                close(client_fd);
                                break;
                            }
                        }
                        accepted += 1;

                        clients.insert(client_fd, HttpClient::new(client_fd, EpollFlags::EPOLLIN, path));
                    }
                    // println!("Thread {:?} accepted {:?}", thread::current().id(), accepted);
                    debug_print!("loop breaked");
                    continue;
                }
                    
                if cur_event == cur_event & EpollFlags::EPOLLIN {
                    let mut is_broken = false;
                    let mut readed = false;
                    {
                        let client = clients.get_mut(&cur_socket).unwrap();
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
                                        readed = true;
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

                    if is_broken && !readed {
                        close(cur_socket);
                        epoll_ctl(epfd, EpollOp::EpollCtlDel, cur_socket, &mut epoll_events[i]);
                        let cl = clients.remove(&cur_socket);
                    }
                    continue;
                }

                if cur_event == cur_event & EpollFlags::EPOLLOUT {
                    let mut need_to_close = false;
                    {
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

                        let cl = clients.remove(&cur_socket).unwrap();
                        close(cur_socket as i32)?;
                        cl.clear();
                        debug_print!("closed: {:?}", closed);
                    }
                }
                continue;
            }
        }
    }
}

fn main() {
    let lim = rlimit64 {
        rlim_cur: 200000,
        rlim_max: 200000
    };

    // let d = process::id();

    // println!("{:?}", process::id());

    let d = unsafe {setrlimit64(libc::RLIMIT_NOFILE, &lim) };

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
    match listen(server_sock, 256) {
        Ok(_) => {},
        Err(err) => panic!("Error listen: {:?}", err)
    }
    
    // let topo = Arc::new(Mutex::new(Topology::new()));
    let cpu_limit = config.get_cpu_count();
    let accpet_locker = Arc::new(Mutex::new(0));
    println!("{:?}", cpu_limit);
    let handles: Vec<_> = (0..cpu_limit)
        .map(|i| {
            let path = config.get_path_to_static();
            let worker_count = config.get_cpu_count();
            let acc_locker = accpet_locker.clone(); 
            thread::spawn(move || {
                epoll_loop(server_sock.clone(), worker_count, &path);//, acc_locker);
            })
        }).collect();
    
    // performance worse
    // let handles: Vec<_> = (0..cpu_limit)
    //     .map(|i| {
    //         let child_topo = topo.clone();
    //         let path = config.get_path_to_static();
    //         let worker_count = config.get_cpu_count();
    //         thread::spawn(move || {
    //             {
    //                 let tid = unsafe {libc::pthread_self()};
    //                 let mut locked_topo = child_topo.lock().unwrap();
    //                 let before = locked_topo.get_cpubind_for_thread(tid, CPUBIND_THREAD);
    //                 let mut bind_to = cpuset_for_core(&*locked_topo, i);
    //                 bind_to.singlify();
    //                 locked_topo
    //                     .set_cpubind_for_thread(tid, bind_to, CPUBIND_THREAD)
    //                     .unwrap();
    //                 let after = locked_topo.get_cpubind_for_thread(tid, CPUBIND_THREAD);
    //                 println!("Thread {}: Before {:?}, After {:?}", i, before, after);
    //             }
    //             epoll_loop(server_sock.clone(), worker_count, &path);
    //         })
    //     }).collect();

    for h in handles {
        h.join();
    }
}


// fn cpuset_for_core(topology: &Topology, idx: usize) -> CpuSet {
//     let cores = (*topology).objects_with_type(&ObjectType::Core).unwrap();
//     match cores.get(idx) {
//         Some(val) => val.cpuset().unwrap(),
//         None => panic!("No Core found with id {}", idx)
//     }
// }

