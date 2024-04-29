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


#[derive(Debug, Serialize, Deserialize)]
struct Config {
    access_token: String,
    user_name: String,
    code: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct PocketItem {
    given_url: String,
    resolved_url: Option<String>,
    given_title: Option<String>,
    resolved_title: Option<String>,
    tags: Option<HashMap<String, Tag>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Tag {
    item_id: String,
    tag: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct PocketList {
    list: HashMap<String, PocketItem>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PocketAction {
    action: String,
    item_id: String,
    time: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();
    let consumer_key = env::var("consumerKey")?;
    let folder_path = env::var("folderPath")?;

    let mut code = String::new();
    let mut access_token = String::new();
    let mut user_name = String::new();
    let mut action: Vec<PocketAction> = Vec::new();

    let config_str = std::fs::read_to_string("config.json");
    let config = match config_str {
        Ok(str) => {
            let mut value: serde_json::Value = serde_json::from_str(&str)?;
            let access_token = value["accessToken"].take().as_str().unwrap_or_default().to_owned();
            let user_name = value["userName"].take().as_str().unwrap_or_default().to_owned();
            let code = value["code"].take().as_str().unwrap_or_default().to_owned();
    
            Config {
                access_token,
                user_name,
                code,
            }
        }
        Err(_) => {
            let (c, a, u) = get_code(&consumer_key).await?;
            code = c;
            access_token = a;
            user_name = u;
            Config {
                access_token: access_token.clone(),
                user_name: user_name.clone(),
                code: code.clone(),
            }
        }
    };

    access_token = config.access_token.clone();
    user_name = config.user_name.clone();
    code = config.code.clone();

    let client = Client::new();

    let url = Url::parse("https://getpocket.com/v3/get").unwrap();
    let request_json = json!({
        "consumer_key": consumer_key,
        "access_token": access_token,
        "detailType": "complete"
    });
    let res: reqwest::Response = client
        .request(Method::POST, url)
        .json(&request_json)
        .send()
        .await?;
    
    let pocket_list: PocketList = {
        let json_data = res.json::<serde_json::Value>().await?;
        if json_data["list"].is_array() && json_data["list"].as_array().unwrap().is_empty() {
            PocketList { list: HashMap::new() } // Empty hashmap when the list field does not contain data
        } else {
            serde_json::from_value(json_data)?
        }
    };

    let mut output = String::new();
    if pocket_list.list.is_empty() {
        println!("Empty, nothing to parse");
        return  Ok(());
    }
    for (key, item) in pocket_list.list {
        let mut url = item.given_url;
        if url.is_empty() {
            url = item.resolved_url.unwrap_or_default();
        }
        let site = url
            .replace("https://", "")
            .replace("http://", "")
            .replace(|c: char| c == '/' || c == ':', "")
            .trim_start_matches("www.")
            .trim_end_matches(".com")
            // only keep the first part of the domain
            .split('.')
            .next()
            .unwrap_or_default()
            .to_owned();
        let mut reg = regex::Regex::new(r"https://www.zhihu.com/question/\d+/answer/").unwrap();
        if reg.is_match(&url) {
            let replace = regex::Regex::new(r"\?\S+$").unwrap();
            url = replace.replace_all(&url, "").to_owned().to_string();
        }
        reg = regex::Regex::new(r"https://www.zhihu.com/people/").unwrap();
        if reg.is_match(&url) {
            let replace = regex::Regex::new(r"\?\S+$").unwrap();
            url = replace.replace_all(&url, "").to_owned().to_string();
        }
        let pattern = regex::Regex::new(r"\?.*").unwrap();
        if url.starts_with("https://twitter.com/") || url.starts_with("https://x.com") {
            url = pattern.replace(&url, "").to_owned().to_string();
            // Replace x.com with twitter.com
            url = url.replace("x.com", "twitter.com");
            // appending /with_replies to Twitter URLs
            let twitter_pattern = regex::Regex::new(r"^(https://twitter\.com/[a-zA-Z0-9_]+/?$)").unwrap();
            if twitter_pattern.is_match(&url) {
                url = if url.ends_with("/") { url + "with_replies" } else { url + "/with_replies" };
            }
        }

        let mut tags: Vec<String> = Vec::new();
        if let Some(item_tags) = item.tags {
            for tag in item_tags {
                let ignore_case_tag = tag.1.tag;
                // tags += &ignore_case_tag;
                // tags += " ";
                tags.push(ignore_case_tag);
            }
        }
        if tags.is_empty() {
            tags.push("#c".to_owned());
        }
        let title = item
            .resolved_title
            .unwrap_or_else(|| item.given_title.unwrap_or_default());
        let search_result = execute_command(&("(".to_owned() + &url + ")"), &folder_path, &tags);
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

                            let file = File::open(&full_path)?;
                            let reader = BufReader::new(file);
                            // Create a temporary file
                            let mut temp_file = NamedTempFile::new()?;
                            {
                                let mut writer = BufWriter::new(&temp_file);
                                // Read the file line by line
                                for line in reader.lines() {
                                    let line = line?;
                                    if line == line_content {
                                        // Modify the line
                                        let re = Regex::new(r"<!--SR:![^>]*-->").unwrap();
                                        let modified_line = re.replace(&line, "").to_string();
                                        writeln!(writer, "{}", modified_line)?;
                                    } else {
                                        // Write the original line
                                        writeln!(writer, "{}", line)?;
                                    }
                                }
                            }
                            // Replace the original file with the temporary file
                            temp_file.persist(full_path)?;
                        }
                    }
                } else {
                    // impossible
                }
            },
            Err(e) => {
                let tags_string = tags.iter().map(|tag| format!("{}", tag)).collect::<Vec<String>>().join(" ");
                output += &format!(
                    "\n- {}-[{}]({}) {} ;; ",
                    title, site, url, tags_string
                );
            }
        }


        
        let archive = PocketAction {
            action: "archive".to_owned(),
            item_id: key.to_owned(),
            time: (chrono::Utc::now().timestamp() as u64).to_string(),
        };
        action.push(archive);
    }

    if !output.is_empty() {
        let date = chrono::Utc::now().format("%Y_%m_%d").to_string();
        let file_path = format!("{}{}.md", folder_path + "/journals/", date);
    
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(file_path)?;
        println!("{:?}", &output);
        file.write_all(output.as_bytes())?;
    }
    

    let url = Url::parse_with_params(
        "https://getpocket.com/v3/send",
        &[
            ("consumer_key", consumer_key.as_str()),
            ("access_token", access_token.as_str()),
            ("actions", serde_json::to_string(&action)?.as_str()),
        ],
    )
    .unwrap();
    let res = client.request(Method::GET, url).send().await?;
    println!("{:?}", res.text().await?);

    Ok(())
}

fn execute_command(highlights_string: &str, folder_path: &str, tags: &Vec<String>) -> Result<String, Error> {
    let tags_string = tags.iter().map(|tag| format!("'{}", tag)).collect::<Vec<String>>().join(" ");
    let cmd = format!("grep --line-buffered --color=never -r \"\" * | fzf --filter=\"{} {}\"", highlights_string, tags_string);
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

async fn get_code(
    consumer_key: &str,
) -> Result<(String, String, String), Box<dyn std::error::Error>> {
    let client = Client::new();
    let url = Url::parse("https://getpocket.com/v3/oauth/request").unwrap();
    let request_json = json!({
        "consumer_key": consumer_key,
        "redirect_uri": "http://localhost:3000/callback",
    });
    let res = client
        .request(Method::POST, url)
        .json(&request_json)
        .send()
        .await?;
    let code = res.text().await?.split('=').nth(1).unwrap().to_owned();
    let authorize_url = format!("https://getpocket.com/auth/authorize?request_token={}&redirect_uri=", code);
    println!("{}", authorize_url);
    let url = Url::parse("https://getpocket.com/v3/oauth/authorize").unwrap();
    let request_json = json!({
        "consumer_key": consumer_key,
        "code": code,
    });
    let res = client
        .request(Method::POST, url)
        .json(&request_json)
        .send()
        .await?;
    let data: Vec<String> = res
        .text()
        .await?
        .split('&')
        .map(|x| x.split('=').nth(1).unwrap().to_owned())
        .collect::<Vec<_>>();


    let access_token = data[0].to_owned();
    let user_name = data[1].to_owned();
    let user_name_tmp = user_name.clone();
    let config = Config {
        access_token: access_token.clone(),
        user_name: user_name_tmp.clone(),
        code: code.clone(),
    };
    let config_str = serde_json::to_string(&config)?;
    std::fs::write("config.json", config_str)?;
    Ok((code, access_token.to_string(), user_name_tmp.clone()))
}
