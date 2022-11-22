use std::collections::HashMap;

use serde::{Serialize, Deserialize};
use serde_json::Value;

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Config {
    consumer_key: String,
    access_token: String,
    user_name: String,
    code: String,
    tags: String,
    extra_tags: String,
    ignore_common_tags: String
}

#[derive(Deserialize)]
struct Response {
    list: Option<Value>,
}

#[derive(Deserialize, PartialEq, Eq, Debug)]
struct List {
    resolved_url: Option<String>,
    given_url: Option<String>,
    tags: Option<Value>,
    resolved_title: Option<String>,
    given_title: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // get env var
    let env_var = std::env::var("PARSER_FOLDER_PATH").unwrap();
    // read and parser json file
    let json_file = std::fs::read_to_string("config.json").unwrap();
    // parse json file
    let config: Config = serde_json::from_str(&json_file).unwrap();
    // http request
    let client = reqwest::Client::new();

    let mut json = HashMap::new();
    json.insert("consumer_key", config.consumer_key);
    json.insert("access_token", config.access_token);
    json.insert("detailType", String::from("complete"));
    let res = client.post("https://getpocket.com/v3/get")
        .json(&json)
        .send()
        .await?;

    //get result data
    let data = res.text().await?;
    let response: Response = serde_json::from_str(&data).unwrap();
    //get list
    if let Some(list) = response.list {
            // hashmap
        let m: HashMap<String, List> = serde_json::from_value(list).unwrap();
        // iterate
        for (_, value) in m {
            let mut url = value.resolved_url;
            if None == url {
                url = value.given_url;
                if None == url {
                    url = None;
                }
            }
            let mut title = value.resolved_title;
            if None == title {
                title = value.given_title;
                if None == title {
                    title = None;
                }
            }
            let mut output = String::new();
            match title {
                Some(title) => {
                    match url {
                        Some(url) => {
                            output = "\n- ".to_string() + &title + "-" + "[" + &url + "](" + &url + ")" + " ;; ";
                        }
                        None => {
                            output = "\n- ".to_string() + &title + "-" + "[" + "xxx" + "](" + "xxx" + ")" + " ;; ";
                        }
                    }
                }
                None => {
                    match url {
                        Some(url) => {
                            output = "\n- ".to_string() + "Title" + "-" + "[" + &url + "](" + &url + ")" + " ;; ";
                        }
                        None => {
                            output = "\n- ".to_string() + "Title" + "-" + "[" + "xxx" + "](" + "xxx" + ")" + " ;; ";
                        }
                    }
                }
            }
            println!("{}", output); 
        }

    }

    Ok(())

}
