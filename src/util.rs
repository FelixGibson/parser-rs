
use std::env;
use std::fs::File;
use std::io::{prelude::*, BufReader, BufWriter};
use std::path::PathBuf;
use reqwest::{Client, Method, Url};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::process::Command;
use std::io::Error;
use regex::Regex;
use tempfile::NamedTempFile;

pub fn execute_command(highlights_string: &str, folder_path: &str) -> Result<String, Error> {
    let cmd = format!("grep --line-buffered --color=never -r \"\" * | fzf --filter=\"'{}\"", highlights_string);
    // print the command
    println!("{}", cmd);
    let output = Command::new("sh")
        .arg("-c")
        .arg(&cmd)
        .current_dir(folder_path) // Set the current directory to folder_path
        .output()?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(Error::new(std::io::ErrorKind::Other, "Error execute Command"))
    }
}

pub fn check_and_reset(folder_path: &str, url: &str, tags: &Vec<String>) -> Result<(), Error> {
    let search_result = execute_command(&("(".to_owned() + &url + ")"), &folder_path);
        match search_result {
            Ok(res) => {
                if !res.is_empty() {
                    // The URL was found in a file in the folder
                    let lines: Vec<&str> = res.split('\n').collect();
                    for line in lines {
                        let file_path_and_line_content: Vec<&str> = line.splitn(2, ':').collect();
                        if file_path_and_line_content.len() >= 2 {
                            let file_path = file_path_and_line_content[0];
                            let line_content = file_path_and_line_content[1];
                            // Open the file
                            let mut full_path = PathBuf::from(folder_path.clone());
                            full_path.push(file_path);

                            let file = File::open(&full_path).unwrap();
                            let reader = BufReader::new(file);
                            // Create a temporary file
                            let mut temp_file = NamedTempFile::new().unwrap();
                            {
                                let mut writer = BufWriter::new(&temp_file);
                                // Read the file line by line
                                let mut lines = reader.lines().peekable();
                                while let Some(line) = lines.next() {
                                    let line = line.unwrap();
                                    if line == line_content {
                                        // Modify the line
                                        let re = Regex::new(r"<!--SR:![^>]*-->").unwrap();
                                        let mut modified_line = re.replace(&line, "").to_string();
                                        // Append non-existent tags to the line
                                        for tag in tags {
                                            if !modified_line.contains(tag) {
                                                modified_line = format!("{} {}", modified_line, tag);
                                            }
                                        }
                                        let mut is_card = false;
                                        // check if the line is card
                                        if modified_line.contains(";;") {
                                            is_card = true;
                                        }
                                        // if the next line have "?"
                                        if let Some(Ok(next_line)) = lines.peek() {
                                            if next_line.contains("?") {
                                                is_card = true;
                                            }
                                        }
                                        if !is_card {
                                            modified_line = format!("{} ;; ", modified_line);
                                        }
                                
                                        writeln!(writer, "{}", modified_line);
                                    } else {
                                        // Write the original line
                                        writeln!(writer, "{}", line);
                                    }
                                }
                            }
                            // Replace the original file with the temporary file
                            temp_file.persist(full_path);
                        }
                    }
                } else {
                    // impossible
                }
            },
            Err(e) => {
                return Err(e);
            }
        }
    Ok(())
}

pub fn check(folder_path: &str, url: &str, tags: &Vec<String>) -> Result<(), Error> {
    let search_result = execute_command(&("(".to_owned() + &url + ")"), &folder_path);
        match search_result {
            Ok(res) => {
                
            },
            Err(e) => {
                return Err(e);
            }
        }
    Ok(())
}