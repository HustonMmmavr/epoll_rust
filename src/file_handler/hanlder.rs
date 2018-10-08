use std::fs;
use std::path::Path;
use std::fs::File;
use std::io::prelude::*;

pub struct FileHandler;

impl FileHandler {

    pub fn get_file(path: &str, path_to_file: &mut String, need_to_read: bool) -> Result<(u64, String, String), bool> {

        // err privilegies access
        if path_to_file.contains("/..") {
            return Err(true);
        }


        let mut is_dir = true;
        let mut new_path_to_file = format!("{}{}", path, path_to_file);
        match path_to_file.pop() {
            Some(ch) => {
                let d = ch;        
                match ch { 
                    '/' => new_path_to_file += "index.html", 
                    _ => is_dir = false
                }
            },
            None => {
                print!("Error");
            } 
        }

        // todo check path for root
        let mut path_to_file_ref: &str = &new_path_to_file;
        let path = Path::new(path_to_file_ref);

        match path.exists() {
            true => {
                return Ok(
                        (
                            fs::metadata(path_to_file_ref).unwrap().len(),
                            String::from(path_to_file_ref),
                            String::from(path.extension().unwrap().to_str().unwrap())
                        )
                    )
            }
            // if dir -> forbidden
            false => return Err(is_dir)
        }
    }
}