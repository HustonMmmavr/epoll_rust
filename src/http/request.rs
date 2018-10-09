use std::str;
#[derive(Debug)]
pub struct HttpRequest {
    pub uri: String,
    pub method: String,
}

const SPACE: u8 = 32;
const QUESTION: u8 = 63;
const PERCENT: u8 = 37;


impl HttpRequest {

    
    fn new() -> HttpRequest {
        HttpRequest {
            uri: String::new(),
            method: String::new()
        }
    }

    fn decode_url(encoded_url: &[u8]) -> Result<String, ()> {
        let mut idx = 0;
        let size = encoded_url.len();
        let mut decoded_vec = Vec::<u8>::new();

        while idx < size {
            let b = match encoded_url.get(idx..idx+1) {
                Some(b) => b,
                None => return Err(())
            };

            if b[0] == PERCENT {
                let bytes = match encoded_url.get(idx+1..idx+3) {
                    Some(bytes) => bytes,
                    None => return Err(())
                };
                let mut hex_arr: [u8; 2] = [0, 2];
                let mut i = 0;
                for byte in bytes {
                    hex_arr[i] = match *byte as char {
                        'a' => 10,
                        'A' => 10,
                        'b' => 11,
                        'B' => 11,
                        'c' => 12,
                        'C' => 12,
                        'd' => 13,
                        'D' => 13,
                        'e' => 14,
                        'E' => 14,
                        'd' => 15,
                        'D' => 15,
                        _ => byte - 0x30
                    };
                    i += 1;
                }
                let byte = (hex_arr[1]) | (hex_arr[0]) << 4;
                decoded_vec.push(byte);
                idx += 3;
            } else {
                decoded_vec.push(b[0]);
                idx += 1;
            }
        }

        let decoded_url = match str::from_utf8(decoded_vec.as_slice()) {
            Ok(s) => s,
            Err(e) => return Err(())
        };

        return Ok(decoded_url.to_string())
    }

    // todo check for valid (Get, mehod)
    pub fn parse(raw_request: &str) -> Result<HttpRequest, ()> {
        let http_data_split = raw_request.split("\r\n");
        let http_data_vec = http_data_split.collect::<Vec<&str>>();
        
        // if empty request
        if http_data_vec.len() == 0 {
            return Err(());
        }
        
        let request_line = http_data_vec[0];
        let request_info = request_line.split(" ").collect::<Vec<&str>>();
        
        // 0 - method 1 - url 2 - httpver
        
        let mut stopPath: usize = 0;
        if request_info.len() < 3 {
            return Err(());
        }

        let path_bytes = request_info[1].clone();
        let path_bytes = path_bytes.as_bytes();
        let mut stopPath = 0;
        for b in path_bytes {
            stopPath += 1;
            if *b == SPACE || *b == QUESTION {
                stopPath -= 1;
                break;
            }
        }

        let uri = &path_bytes[0..stopPath];
        let uri = match HttpRequest::decode_url(uri) {
            Ok (uri) => uri,
            Err(e) => return Err(())
        };

        // its crutch for maybe future development
        Ok(HttpRequest {
            method: request_info[0].to_string(),
            uri: uri.to_string(),
        }) 
    }
}