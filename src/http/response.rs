use std::collections::{HashMap};
use chrono::Local;
// use std::collections::V;


const HTTP_VERSION: &str = "HTTP/1.1";
const HTTP_TERMINATOR: &str = "\r\n";

#[derive(Debug)]
pub struct HttpResponse {
    pub status: i32,
    pub headers: Option<HashMap<String, String>>,
    pub body_exist: bool,
    pub connection: String,
}

impl HttpResponse {
    pub fn new_empty() -> HttpResponse {
        HttpResponse {
            status: 200,
            headers: None,
            body_exist: false,
            connection: String::from("Close")
        }
    }

    pub fn new(status: i32, headers: Option<HashMap<String, String>>) -> HttpResponse {
        HttpResponse {
            status: status,
            headers: headers,
            body_exist: false,
            connection: String::from("Close")
        }
    }

    pub fn ok(headers: HashMap<String, String>, body_exist: bool) -> HttpResponse {
        HttpResponse {
            status: 200,
            headers: Some(headers),
            body_exist: body_exist,
            connection: String::from("Close")
        }
    }

    pub fn bad_request() -> HttpResponse {
        HttpResponse {
            status: 400,
            headers: None,
            body_exist: false,
            connection: String::from("Close")
        }
    }

    pub fn forbidden() -> HttpResponse {
        HttpResponse {
            status: 403,
            headers: None,
            body_exist: false,
            connection: String::from("Close")
        }
    }

   pub fn not_found() -> HttpResponse {
        HttpResponse{
            status: 404,
            headers: None,
            body_exist: false,
            connection: String::from("Close")
        }
    }

   pub fn not_allowed() -> HttpResponse {
        HttpResponse{
            status: 405,
            headers: None,
            body_exist: false,
            connection: String::from("Close")
        }
    }
    
    pub fn set_status(&mut self, status: i32) {
        self.status = status;
    }

    pub fn set_body_exist(&mut self, body_exist: bool) {
        self.body_exist = body_exist;
    }

    pub fn set_headers(&mut self, headers: HashMap<String, String>) {
        self.headers = Some(headers);
    }

    pub fn set_connection(&mut self, connection: String) {
        self.connection = connection;
    }

    pub fn to_vec_response(&mut self) -> Vec<u8> {
        let text = match self.status {
            200 => "Ok",
            400 => "Bad request",
            403 => "Forbidden",
            404 => "Not Found",
            405 => "Method not allowed",
            _ => "ERR"
        };

        let status_string = format!("{} {:?} {}{}", HTTP_VERSION, self.status, text, HTTP_TERMINATOR);
        
        let mut headers_string = format!("Server: Mavri \r\nDate: {}\r\nConnection: Closed\r\n", 
            Local::now().to_rfc2822()
        );

        let headers = self.headers.take();

        // Work with headers such as Content-Length
        match headers {
            Some(map) => {
                for (key, val) in map.iter() {
                    headers_string += key;
                    headers_string += ": ";
                    headers_string += val;
                    headers_string += HTTP_TERMINATOR;
                }
            },
            None => {}
        }
        
        // let body_exist = self.body_exist.clone();
        // let mut terminator = HTTP_TERMINATOR.to_owned();
        // if !body_exist {
            // headers_string += HTTP_TERMINATOR;
        // }

        let response_string = format!("{}{}{}", status_string, headers_string, HTTP_TERMINATOR);
                //terminator);
        let resp_byte = response_string.into_bytes().iter().map(|c|  *c).collect::<Vec<u8>>();
        // let mut resp_byte = Vec::new();
        // resp_byte.append(&mut response_string.into_bytes());
        return resp_byte;
    }

}