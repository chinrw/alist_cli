use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashSet, VecDeque};
use std::sync::Arc; // Add Arc and Mutex
use tokio::sync::Mutex; // Use the async-aware Mutex from tokio

use anyhow::{anyhow, Context, Result};

static ALIST_URL: &str = "http://192.168.0.201:5244";

#[derive(Serialize, Deserialize)]
pub(crate) struct FileInfoRequest {
    path: String,
    password: String,
    page: u32,
    per_page: u32,
    refresh: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct FileInfo {
    name: String,
    size: u64,
    is_dir: bool,
    modified: String,
    sign: String,
    thumb: String,
    #[serde(rename = "type")]
    file_type: u32, // Renamed `type` field to avoid Rust keyword conflict
    created: Option<String>,
    hashinfo: Option<String>,
    #[serde(rename = "hash_info")]
    hash_info: Option<Value>,
    raw_url: String,
    readme: String,
    header: String,
    provider: String,
    related: Option<Value>,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct ListFolder {
    path: String,
    password: String,
    page: u32,
    per_page: u32,
    refresh: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) struct EntryInfo {
    name: String,
    size: u64,
    is_dir: bool,
    modified: String,
    sign: String,
    thumb: String,
    #[serde(rename = "type")]
    file_type: u32, // We need to rename `type` since it's a reserved keyword in Rust
    created: Option<String>,
    hashinfo: Option<String>,
    #[serde(rename = "hash_info")]
    hash_info: Option<Value>,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct FoldersInfo {
    content: Vec<EntryInfo>,
    total: u32,
    readme: String,
    write: bool,
    provider: String,
    header: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)] // Tells serde to figure out the correct struct based on the content of the response
pub(crate) enum ApiData {
    FileInfo(FileInfo),
    FoldersInfo(FoldersInfo),
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct ApiResponse {
    code: u32,
    message: String,
    data: ApiData,
}

#[derive(Debug)]
pub(crate) struct EntryWithPath {
    pub(crate) entry: EntryInfo,
    pub(crate) path: String,
}

pub(crate) async fn get_path_structure(path: String) -> Result<Vec<EntryWithPath>> {
    let client = Client::new();
    let visited_paths = Arc::new(Mutex::new(HashSet::new()));
    {
        let mut visited_paths_lock = visited_paths.lock().await;
        visited_paths_lock.insert(path.clone());
    }

    // Fetch the folder contents iteratively and get all entries with paths
    let entries_with_paths = fetch_folder_contents(path, &client, visited_paths.clone()).await?;

    // Return the collected entries along with their paths
    Ok(entries_with_paths)
}

async fn fetch_folder_contents(
    path: String,
    client: &Client,
    visited_paths: Arc<Mutex<HashSet<String>>>,
) -> Result<Vec<EntryWithPath>> {
    let mut entries_with_paths = Vec::new();
    let mut directories_to_process = VecDeque::new();
    directories_to_process.push_back(path.clone());

    // Process the directories iteratively using a queue
    while let Some(current_path) = directories_to_process.pop_front() {
        // Prepare the JSON payload
        let payload = ListFolder {
            path: current_path.clone(),
            password: "".to_string(),
            page: 1,
            per_page: 0,
            refresh: false,
        };

        println!("current payload:{:?}", payload);
        let response = client
            .post(format!("{}/api/fs/list", ALIST_URL))
            .json(&payload)
            .header("Content-Type", "application/json")
            .send()
            .await
            .context("Request failed")?;

        if response.status().is_success() {
            let api_response: ApiResponse =
                response.json().await.context("Failed to parse response")?;

            if let ApiData::FoldersInfo(folders_info) = api_response.data {
                // Add the current path to the list of entries
                for file in &folders_info.content {
                    let full_path = format!("{}/{}", current_path, file.name);
                    println!("{}", full_path);

                    // Add this entry and its full path to the list
                    entries_with_paths.push(EntryWithPath {
                        entry: file.clone(),
                        path: full_path.clone(),
                    });

                    // If the item is a directory and hasn't been visited, add it to the queue
                    if file.is_dir {
                        let mut visited = visited_paths.lock().await;
                        if visited.insert(full_path.clone()) {
                            directories_to_process.push_back(full_path);
                        }
                    }
                }
            } else {
                return Err(anyhow!("Invalid data"));
            }
        } else {
            return Err(anyhow!("Request failed"));
        }
    }

    Ok(entries_with_paths)
}
