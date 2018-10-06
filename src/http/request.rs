
// pub enum 

#[derive(Debug)]
pub struct HttpRequest {
    pub uri: String,
    pub method: String,
}


impl HttpRequest {
    fn new() -> HttpRequest {
        HttpRequest {
            uri: String::new(),
            method: String::new()
        }
    }

    fn decode_url(encoded_url: &str) -> String {
        let mut idx = 0;
        let size = encoded_url.len();
        let mut decoded_url = String::new();

        //todo check for valid
        while idx < size {
            let ch = encoded_url.get(idx..idx+1).unwrap();
            if ch == "%" {
                let ch = u8::from_str_radix(encoded_url.get(idx+1..idx+3).unwrap(), 16).unwrap();
                decoded_url.push(ch as char);
                idx += 3;
            } else {
                decoded_url += ch;
                idx += 1;
            }
        }
        return decoded_url;
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
        if (request_info.len() < 3) {
            return Err(());
        }
        for ch in request_info[1].chars() {
            stopPath += 1;
            if ch == ' ' || ch == '?' {
                stopPath -= 1;
                break;
            }
        }

        let uri = &request_info[1][0..stopPath];
        let uri = HttpRequest::decode_url(uri);

        // its crutch for maybe future development
        Ok(HttpRequest {
            method: request_info[0].to_string(),
            uri: uri.to_string(),
        })
    }
}