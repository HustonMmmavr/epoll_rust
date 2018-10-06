use std::collections::HashMap;
use client::client::HttpClient;
use std::os::unix::io::RawFd;

pub struct ClientManager {//<'a> {
    clients: HashMap<RawFd, HttpClient>,
    file_path: String//&'a str
}

impl ClientManager {//<'a> ClientManager<'a> {
    pub fn new(file_path: &str) -> Self {
        return ClientManager {
            clients: HashMap::new(),
            file_path: String::from(file_path)//file_path
        }
    }

    pub fn add_client(&mut self, sock: RawFd, client: HttpClient) {
        self.clients.insert(sock, client).unwrap();
    }

    pub fn remove_client(&mut self, sock: RawFd) {
        self.clients.remove(&sock).unwrap();
    }

    pub fn get_client(&mut self, sock: RawFd) ->  &mut HttpClient {
        return self.clients.get_mut(&sock).unwrap();
    }

    pub fn get_path(&self) -> &str {
        return &self.file_path;
    }
}

