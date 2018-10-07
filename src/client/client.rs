use std::collections::HashMap;
use std::io::prelude::*;
use std::cell::Cell;
use http::{request::HttpRequest, response::HttpResponse};
use std::str;
use file_handler::hanlder::FileHandler;
use std::io;
use nix::sys::epoll::*;
use nix::sys::socket::*;
use nix::Error::{Sys};
use nix::errno::Errno;
use nix::Error;
use std::os::unix::io::RawFd;
use std::path::Path;
use nix::unistd::{read, write, close};
use nix::fcntl::{open, OFlag};
use nix::sys::stat::Mode;
use nix::sys::sendfile::{sendfile};
use std::fs;

const FILE_BUF: usize =  524288; //16384;

#[derive(PartialEq, Clone, Debug)]
pub enum ClientState {
    START,
    READING,
    REQUEST_READED,
    WRITING,
    RESPONSE_WRITED,
    OPENING_FILE,
    FILE_WRITING,
    FILE_WRITED,
    ERROR
}

const READ_LEN: usize = 8192;

#[derive(Debug)]
pub struct HttpClient {
    pub socket: RawFd,
    pub state: ClientState,
    interest: EpollFlags,
    req: Option<HttpRequest>,
    buffer_read: Vec<u8>,
    buffer_write: Vec<u8>,
    readed: usize,
    writed: usize,
    file_sended: usize,
    len_write: usize,
    path: String,
    file_fd: RawFd,
    file_len: usize,
    need_send_file: bool,
    // path1: &'a str
}

impl HttpClient {
    pub fn new(client: RawFd, interest: EpollFlags) -> HttpClient {
        HttpClient {
            socket: client,
            state: ClientState::READING,
            interest: interest,
            req: None,
            buffer_read: Vec::new(), 
            buffer_write: Vec::new(),
            readed: 0,
            writed: 0,
            file_sended: 0,
            len_write: 0,
            path: String::new(),
            file_fd: 0,
            need_send_file: false,
            file_len: 0
        }
    }

    fn process_request(&self) -> HttpResponse {
        let mut http_response = match self.req {
            Some(ref http_request) => return HttpClient::create_response(http_request).0,
            None => return HttpResponse::bad_request()
        };
    }

    pub fn read(&mut self) -> Result<ClientState, Error> {
        // if self.state == ClientState::READING 
        let mut buf: [u8; READ_LEN] = [0; READ_LEN];
        loop {
            let mut total_len = self.readed;
            match read(self.socket, &mut buf) {
                Ok(size) => {
                    self.buffer_read.extend_from_slice(&buf[..size]);

                    // we found EOF and nothing in buffer
                    if size == 0 && self.buffer_read.len() == 0 {
                        return Ok(ClientState::ERROR);
                        // break;
                    }

                    if size == 0 {
                        break;
                    }
                },
                Err(Sys(err)) => {
                    if err == Errno::EAGAIN {
                        break;
                    }
                    // return Err(Sys(err));
                },
                Err(err) => return Err(err)
            }
        }
        
        let req = str::from_utf8(self.buffer_read.as_slice()).unwrap().to_string();
            
        if !(req.find("\n\n").is_some() || req.find("\r\n\r\n").is_some() ){
            return Ok(ClientState::READING);
        }

        match HttpRequest::parse(&req) {
            Ok(request) => self.req = Some(request),
            Err(_) => return Ok(ClientState::ERROR)
        }
        self.state = ClientState::REQUEST_READED;
        Ok(ClientState::REQUEST_READED)
    }

        fn get_http_ext(file_ext: &str) -> String {
        match file_ext {
            "png" => return String::from("image/png"),
            "swf" => return String::from("application/x-shockwave-flash"),
            "gif" => return String::from("image/gif"),
            "css" => return String::from("text/css"),
            "js" => return String::from("text/javascript"),
            "jpg" => return String::from("image/jpeg"),
            "txt" =>  return String::from("text/plain"),
            "jpeg" => return String::from("image/jpeg"),
            "html" => return String::from("text/html"),
            _ => return String::from("")
        }
    } 

    fn create_response_part(req: &HttpRequest, is_get: bool) -> (HttpResponse, Option<String>) {
        let mut path_to_file = req.uri.clone();
        match FileHandler::get_file(&mut path_to_file, is_get) {
            Ok(value) => {
                let (size, path, ext) = value;
                let mut headers: HashMap<String, String> = HashMap::new();
                headers.insert(String::from("Content-type"), HttpClient::get_http_ext(&ext));
                headers.insert(String::from("Content-Length"), size.to_string());
                let mut path = Some(path);
                if !is_get  {
                    path = None;
                }
                return (HttpResponse::ok(headers, is_get), path);
            },
            Err(is_forbidden) => {
                let status = match is_forbidden {
                    true => return (HttpResponse::forbidden(), None),
                    false => return (HttpResponse::not_found(), None)
                };
            }
        }
    }


    fn create_response(req: &HttpRequest) -> (HttpResponse, Option<String>) {
        match req.method.as_ref() {
            "GET" => return HttpClient::create_response_part(req, true),
            "HEAD" => {
                println!("HEAD");
                return HttpClient::create_response_part(req, false);
            } 
            _ => return (HttpResponse::not_allowed(), None)
        }
    }

    pub fn write(&mut self, path: &str) -> Result<ClientState, Error> {
        if self.state == ClientState::REQUEST_READED {
            self.state = ClientState::WRITING;
            let (mut resp, path) = HttpClient::create_response(&self.req.take().unwrap());
            match path {
                Some(path) => {
                    let mode = Mode::S_IRUSR | Mode::S_IRGRP | Mode::S_IROTH;
                    self.file_fd = match open(Path::new(&path), OFlag::O_RDONLY, mode) {
                        Ok(fd) => fd, 
                        Err(err) => {

                            println!("{:?}", err);
                            return Err(err);
                            // panic!();
                        }
                    }; 
                    self.file_len = fs::metadata(&path).unwrap().len() as usize;
                    self.buffer_write = resp.to_vec_response();
                    self.need_send_file = true;

                }, 
                None => self.buffer_write = resp.to_vec_response()
            }
        }

        // or open file every time when we cant open file

        if self.state == ClientState::WRITING {
            match write(self.socket, &self.buffer_write.as_slice()[self.writed..]) {
                Ok(size) => {
                    self.writed += size;
                    if self.writed >= self.buffer_write.len() {
                        if self.need_send_file {
                            self.state = ClientState::FILE_WRITING; //
                        } else {
                            self.state = ClientState::RESPONSE_WRITED;
                            return Ok(self.state.clone());
                        }
                    }
                },
                Err(err) => {
                    if self.need_send_file {
                        close(self.file_fd);
                    }
                    // close(self.file_fd);
                    print!("Write {:?}", err);
                }
            }
        }

        if self.state == ClientState::FILE_WRITING {
            let mut offt = self.file_sended as i64; 
            let need_to_send = self.file_len - self.file_sended;

            let to_send = match FILE_BUF < need_to_send {
                true => FILE_BUF,
                false => need_to_send
            };

            // whats wrong
            match sendfile(self.socket, self.file_fd, Some(&mut offt), to_send) {
                Ok(sended) => {
                    self.file_sended += sended;
                    if self.file_sended >= self.file_len {
                        self.state = ClientState::RESPONSE_WRITED;
                        close(self.file_fd);
                        return Ok(self.state.clone());
                    }
                }, 
                Err(Sys(err)) => {
                    print!("{:?}", err);
                    if err == Errno::EAGAIN {
                        return Ok(self.state.clone());
                    }
                        return Ok(self.state.clone());

                    // return Err(Sys(err));
                },
                Err(err) => {
                        return Ok(self.state.clone());

                    // return Err(err)
                }
            }
        }
        
        Ok(self.state.clone())
    }

}

// 26602944989
// 26718100000
                    // println!("{:?}", str::from_utf8(self.buffer_write.as_slice()));


    // fn handle(&mut self, flg: EpollFlags) {
    //     if self.state == ClientState::READING {
    //         if self.interest == self.interest & EpollFlags::EPOLLIN {
    //             self.read();
    //         }
    //     }
    // } 


        // fn create_response_part(req: &HttpRequest, is_get: bool) -> HttpResponse {
    //     let mut path_to_file = req.uri.clone();
    //     match FileHandler::get_file(&mut path_to_file, is_get) {
    //         Ok(value) => {
    //             let (body, size, ext) = value;
    //             let mut headers: HashMap<String, String> = HashMap::new();
    //             headers.insert(String::from("Content-type"), ext);
    //             headers.insert(String::from("Content-Length"), size.to_string());
    //             return HttpResponse::ok(headers, None);
    //         },
    //         Err(is_forbidden) => {
    //             let status = match is_forbidden {
    //                 true => return HttpResponse::forbidden(),
    //                 false => return HttpResponse::not_found()
    //             };
    //         }
    //     }
    // }

    // fn create_response(req: &HttpRequest) -> HttpResponse {
    //     match req.method.as_ref() {
    //         "GET" => return HttpClient::create_response_part(req, true),
    //         "HEAD" => return HttpClient::create_response_part(req, false), 
    //         _ => return HttpResponse::not_allowed()
    //     }
    // }