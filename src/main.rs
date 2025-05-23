mod util;

use std::env;
use std::fs::File;
use std::io::{prelude::*, BufReader, BufWriter, ErrorKind};
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
use std::collections::HashSet;
use serde_json::from_reader;

#[derive(Debug, Serialize, Deserialize)]
struct Config {
    access_token: String,
    user_name: String,
    code: String,
}

#[derive(Deserialize)]
struct UrlEntry {
    name: String,
    url: String,
}

#[derive(Deserialize)]
struct UrlList {
    metadata: Metadata,
    list: Vec<ListItem>,
}

#[derive(Deserialize)]
struct Metadata {
    url_prefix: String,
}

#[derive(Deserialize)]
struct ListItem {
    name: String,
    url: String,
    extra_prefix: String,
    extra_suffix: String,
    tags: Vec<String>,  // 新增tags字段
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

struct UrlTransformation {
    prefix: String,
    replacements: Vec<String>,
}

impl UrlTransformation {
    fn new(prefix: &str, replacements: &[&str]) -> Self {
        Self {
            prefix: prefix.to_string(),
            replacements: replacements.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn apply(&self, url: &str, url_alternatives: &mut HashSet<String>) {
        if url.starts_with(&self.prefix) {
            for replacement in &self.replacements {
                let new_url = url.replace(&self.prefix, replacement);
                url_alternatives.insert(new_url);
            }
        }
    }
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

    let args: Vec<String> = env::args().collect();
    let mut pocket_list = PocketList { list: HashMap::new() };
    let mut is_data_input_from_pocket = true;
    if args.len() > 1 {
        is_data_input_from_pocket = false;
        let file_path = &args[1];
        let file = File::open(file_path)?;
        let reader = BufReader::new(file);
        let url_list: UrlList = from_reader(reader)?;

        for item in url_list.list {
            // 构建完整URL
            let url = format!(
                "{}{}",
                url_list.metadata.url_prefix,
                item.url
            );
            
            // 处理标签（仅使用原生tags字段）
            let mut tags: HashMap<String, Tag> = HashMap::new();
            for (i, tag_content) in item.tags.iter().enumerate() {
                let clean_tag = tag_content.trim();
                if !clean_tag.is_empty() {
                    // 自动添加标签格式
                    let formatted_tag = if clean_tag.starts_with("#[[") {
                        clean_tag.to_string()
                    } else {
                        format!("#[[{}]]", clean_tag)
                    };
                    
                    tags.insert(
                        i.to_string(),
                        Tag {
                            item_id: i.to_string(),
                            tag: formatted_tag,
                        },
                    );
                }
            }

            let pocket_item = PocketItem {
                given_url: url.clone(),
                resolved_url: Some(url),
                given_title: Some(item.name.clone()),
                resolved_title: None,
                tags: Some(tags),
            };
            pocket_list.list.insert(item.name.clone(), pocket_item);
        }
    } else {
        let url = Url::parse("https://getpocket.com/v3/get").unwrap();
        let request_json = json!({
            "consumer_key": consumer_key,
            "access_token": access_token,
            "detailType": "complete",
            "state": "unread"
        });
        let res: reqwest::Response = client
            .request(Method::POST, url)
            .json(&request_json)
            .send()
            .await?;
        
        pocket_list = {
            let json_data = res.json::<serde_json::Value>().await?;
            println!("{}", serde_json::to_string_pretty(&json_data).expect("Failed to print json_data"));
            if json_data["list"].is_array() && json_data["list"].as_array().unwrap().is_empty() {
                PocketList { list: HashMap::new() } // Empty hashmap when the list field does not contain data
            } else {
                serde_json::from_value(json_data)?
            }
        };
    }

    let mut output = String::new();
    if pocket_list.list.is_empty() {
        println!("Empty, nothing to parse");
        return Ok(());
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
            url = url.replace("twitter.com", "x.com");
            // appending /with_replies to Twitter URLs
            let twitter_pattern = regex::Regex::new(r"^(https://x\.com/[a-zA-Z0-9_]+/?$)").unwrap();
            if twitter_pattern.is_match(&url) {
                // to lower case first
                url = url.to_lowercase();
                url = if url.ends_with("/") { url + "with_replies" } else { url + "/with_replies" };
            }
        }

        if url.contains("m.youtube.com") {
            url = url.replace("m.youtube.com", "youtube.com");
        }

        let mut tags: Vec<String> = Vec::new();
        if let Some(item_tags) = item.tags {
            for tag in item_tags {
                let ignore_case_tag = tag.1.tag;
                tags.push(ignore_case_tag);
            }
        }
        if url.contains("youtube.com") || url.contains("bilibili.com") || url.contains("douyin.com") {
            tags.push("#[[vquest]]".to_string());
        }
        if tags.is_empty() {
            tags.push("#[[c]]".to_owned());
        }
        // Iterate and ensure each tag starts with '#'
        for tag in tags.iter_mut() {
            // Trim leading and trailing whitespace
            let trimmed_tag = tag.trim();
            
            if !trimmed_tag.starts_with('#') {
                *tag = format!("#[[{}]]", trimmed_tag);
            } else {
                *tag = trimmed_tag.to_string(); // Update the tag to the trimmed version if it already starts with '#'
            }
        }
        if url.contains("m.youtube.com") {
            url = url.replace("m.youtube.com", "youtube.com");
        }


        let mut title = item
            .resolved_title
            .unwrap_or_else(|| item.given_title.clone().unwrap_or_default());
        // replace all "#" in title
        if title.starts_with("http") || title.is_empty() {
            title = item.given_title.clone().unwrap_or_default();
        }
        title = title.replace("#", "");
        
        let mut res: Result<(), Error> = Err(Error::new(ErrorKind::Other, "Failed to execute command"));
        if true {
            let mut url_alternatives = HashSet::new();
            url_alternatives.insert(url.to_owned());

            // if url.starts_with("https://m.weibo.cn/") {
            //     let new_url1 = url.replace("https://m.weibo.cn/", "https://weibo.cn/");
            //     let new_url2 = url.replace("https://m.weibo.cn/", "https://weibo.com/");
            //     url_alternatives.insert(new_url1);
            //     url_alternatives.insert(new_url2);
            // } else if url.starts_with("https://weibo.cn/") {
            //     let new_url1 = url.replace("https://weibo.cn/", "https://m.weibo.cn/");
            //     let new_url2 = url.replace("https://weibo.cn/", "https://weibo.com/");
            //     url_alternatives.insert(new_url1);
            //     url_alternatives.insert(new_url2);
            // } else if url.starts_with("https://weibo.com/") {
            //     let new_url1 = url.replace("https://weibo.com/", "https://weibo.cn/");
            //     let new_url2 = url.replace("https://weibo.com/", "https://m.weibo.cn/");
            //     url_alternatives.insert(new_url1);
            //     url_alternatives.insert(new_url2);
            // }
            let url_transformations = vec![
                UrlTransformation::new("https://m.weibo.cn/", &["https://weibo.cn/", "https://weibo.com/"]),
                UrlTransformation::new("https://weibo.cn/", &["https://m.weibo.cn/", "https://weibo.com/"]),
                UrlTransformation::new("https://weibo.com/", &["https://weibo.cn/", "https://m.weibo.cn/"]),
                UrlTransformation::new("https://www.m.weibo.cn/", &["https://weibo.cn/", "https://weibo.com/", "https://m.weibo.cn/"]),
                UrlTransformation::new("https://www.weibo.cn/", &["https://m.weibo.cn/", "https://weibo.com/", "https://weibo.cn/"]),
                UrlTransformation::new("https://www.weibo.com/", &["https://weibo.cn/", "https://m.weibo.cn/", "https://weibo.com/"]),
            ];

            // Apply the transformation rules
            for transformation in &url_transformations {
                transformation.apply(&url, &mut url_alternatives);
            }


            for url in url_alternatives.clone().iter() {
                if !url.ends_with('/') {
                    let slash_url = url.to_owned() + "/";
                    url_alternatives.insert(slash_url);
                }
            }
            // reversion
            for url in url_alternatives.clone().iter() {
                if url.ends_with('/') {
                    let slash_url = url.trim_end_matches('/').to_owned();
                    url_alternatives.insert(slash_url);
                }
            }

            if is_data_input_from_pocket {
                for alternative_url in url_alternatives {
                    if let Ok(_) = util::check(&folder_path, &alternative_url, &tags) {
                        res = Ok(());
                        break;
                    }
                }
            } else {
                for alternative_url in url_alternatives {
                    if let Ok(_) = util::check_and_reset(&folder_path, &alternative_url, &tags) {
                        res = Ok(());
                        break;
                    }
                }
            }
        }
        
        if res.is_err() {
            let tags_string = tags.iter().map(|tag| format!("{}", tag)).collect::<Vec<String>>().join(" ");
            output += &format!(
                "\n- {}-[{}]({}) {} ;; ",
                title, site, url, tags_string
            );
        }

        let archive = PocketAction {
            action: "delete".to_owned(),
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
    

    if is_data_input_from_pocket {
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
    }

    Ok(())
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