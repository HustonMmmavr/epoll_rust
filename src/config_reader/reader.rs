use std::fs::File;
use std::io::BufReader;
use std::io::BufRead;
use num_cpus;
use std::io::Error;
use std::io::ErrorKind;
const CONFIG_PATH: &str = "/etc/httpd.conf";

pub struct Config {
    path_to_static: String,
    thread_limit: usize,
    cpu_limit: usize,
    port: usize
}

impl Config {
    pub fn get_path_to_static(&self) -> String {
        return self.path_to_static.clone();
    }

    pub fn get_port(&self) -> usize {
        return self.port.clone();
    }

    pub fn get_cpu_count(&self) -> usize {
        return self.cpu_limit.clone();
    }
}

pub struct ConfigReader;

impl ConfigReader {
    pub fn default() -> Config {
        return Config {
            path_to_static: String::from("."),
            thread_limit: 50,
            cpu_limit: num_cpus::get(),
            port: 80
        }
    }

    pub fn read() -> Result<Config, Error> {
        let file = match File::open(CONFIG_PATH) {
            Ok(file) => file,
            Err(err) => return Err(err)
        };


        let reader = BufReader::new(&file);
        let mut config = ConfigReader::default();
        for line in reader.lines() {
            let line = line.unwrap();
            let parts = line.split(" ").collect::<Vec<&str>>();
            let key = parts[0];
            let val = parts[1].trim_right_matches("\n");

            match key {
                "listen" => config.port = val.parse::<usize>().unwrap(),
                "thread_limit" => config.thread_limit = val.parse::<usize>().unwrap(),
                "cpu_limit" => config.cpu_limit = val.parse::<usize>().unwrap(),
                "document_root" => config.path_to_static = String::from(val),
                _ => return Err(Error::from(ErrorKind::InvalidData))
            }
        }

        return Ok(config)
    }
}