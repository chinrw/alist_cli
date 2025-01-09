use std::{
    collections::{HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Ok, Result, anyhow};
use log::{debug, info, trace};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha1::{Digest, Sha1};
use tokio::{fs, io::AsyncWriteExt, sync::Mutex};
use url::Url;

use crate::ALIST_URL; // Use the async-aware Mutex from tokio

const META_SUFF: [&str; 9] = [
    "nfo", "jpg", "png", "svg", "ass", "srt", "sup", "vtt", "txt",
];

const FILE_STRM: [&str; 14] = [
    "mkv", "iso", "ts", "mp4", "avi", "rmvb", "wmv", "m2ts", "mpg", "flv", "rm", "mov", "wav",
    "mp3",
];

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
enum HashObject {
    Sha1 { sha1: String },
    Md5 { md5: String },
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
    // #[serde(rename = "hash_info")]
    hash_info: Option<HashObject>,
    raw_url: String,
    readme: String,
    header: String,
    provider: String,
    related: Option<Value>,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct FileInfoRequest {
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
    file_type: u32,
    created: Option<String>,
    hashinfo: Option<String>,
    // #[serde(rename = "hash_info")]
    hash_info: Option<HashObject>,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct FoldersInfo {
    content: Option<Vec<EntryInfo>>,
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
    data: Option<ApiData>,
}

#[derive(Debug)]
pub(crate) struct EntryWithPath {
    pub(crate) entry: EntryInfo,
    pub(crate) path_str: String,
}

pub(crate) async fn get_path_structure(path: String) -> Result<Vec<EntryWithPath>> {
    let visited_paths = Arc::new(Mutex::new(HashSet::new()));
    {
        let mut visited_paths_lock = visited_paths.lock().await;
        visited_paths_lock.insert(path.clone());
    }

    // Fetch the folder contents iteratively and get all entries with paths
    let entries_with_paths = fetch_folder_contents(path, visited_paths.clone()).await?;

    // Return the collected entries along with their paths
    Ok(entries_with_paths)
}

async fn fetch_folder_contents(
    path: String,
    visited_paths: Arc<Mutex<HashSet<String>>>,
) -> Result<Vec<EntryWithPath>> {
    let client = Client::new();
    let mut entries_with_paths = Vec::new();
    let mut directories_to_process = VecDeque::new();
    directories_to_process.push_back(path.clone());

    // Process the directories iteratively using a queue
    while let Some(current_path) = directories_to_process.pop_front() {
        // Prepare the JSON payload
        let payload = FileInfoRequest {
            path: current_path.clone(),
            password: "".to_string(),
            page: 1,
            per_page: 0,
            refresh: false,
        };

        trace!("Payload: {:?}", payload);
        let response = client
            .post(format!("{}/api/fs/list", *ALIST_URL))
            .json(&payload)
            .header("Content-Type", "application/json")
            .send()
            .await?;
        if response.status().is_success() {
            let api_response: ApiResponse = response.json().await?;
            trace!("list api_response: {:?}", api_response);
            if let Some(ApiData::FoldersInfo(folders_info)) = api_response.data {
                // Add the current path to the list of entries
                if folders_info.content.is_none() {
                    continue;
                }
                for file in &folders_info.content.unwrap() {
                    let full_path = format!("{}/{}", current_path, file.name);
                    debug!("entry path: {}", full_path);

                    // Add this entry and its full path to the list
                    entries_with_paths.push(EntryWithPath {
                        entry: file.clone(),
                        path_str: full_path.clone(),
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
                return Err(anyhow!("fetch_folder_contents Invalid data"));
            }
        } else {
            return Err(anyhow!(
                "fetch_folder_contents Request failed {:?}",
                response
            ));
        }
    }

    Ok(entries_with_paths)
}

pub(crate) async fn get_file_ext(
    entries_with_paths: &[EntryWithPath],
) -> Vec<(String, &EntryWithPath)> {
    entries_with_paths
        .iter()
        .filter(|x| !x.entry.is_dir)
        .filter_map(|x| {
            Path::new(&x.path_str)
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext_str| (ext_str.to_owned(), x))
        })
        .collect()
}

pub(crate) async fn get_raw_url(
    client: &Client,
    file: &(String, &EntryWithPath),
) -> Result<String> {
    trace!("file: {:?}", file);
    let payload = FileInfoRequest {
        path: file.1.path_str.clone(),
        password: "".to_string(),
        page: 1,
        per_page: 0,
        refresh: false,
    };

    trace!("metadata current payload:{:?}", payload);
    let response = client
        .post(format!("{}/api/fs/get", *ALIST_URL))
        .json(&payload)
        .header("Content-Type", "application/json")
        .send()
        .await?;

    if response.status().is_success() {
        let api_response: ApiResponse = response.json().await?;
        trace!("metadata api_response: {:?}", api_response);

        if let Some(ApiData::FileInfo(file_info)) = api_response.data {
            let raw_url = file_info.raw_url;
            debug!("raw_url: {}", raw_url);
            Ok(raw_url)
        } else {
            Err(anyhow!("Invalid data"))
        }
    } else {
        Err(anyhow!("Request failed"))
    }
}

pub(crate) async fn copy_metadata(
    files_with_ext: &Vec<(String, &EntryWithPath)>,
    output_path: &str,
) -> Result<()> {
    let files_copy = files_with_ext
        .iter()
        .filter(|(ext, _)| META_SUFF.contains(&ext.as_str()));

    let client = Client::new();
    for file in files_copy {
        let mut local_path = PathBuf::from(output_path);
        // remove the leading "/"
        let relative_p2 = file.1.path_str.trim_start_matches('/');
        local_path.push(relative_p2);

        // Check if the local file exist
        if Path::new(&local_path).exists() {
            // Check if the local file has the same sha1 with the remote one
            match &file.1.entry.hash_info {
                Some(HashObject::Sha1 { sha1 }) => {
                    let data = fs::read(&local_path).await?;
                    let mut hasher = Sha1::new();
                    hasher.update(&data);
                    let local_sha1 = format!("{:x}", hasher.finalize()).to_uppercase();
                    if sha1 == &local_sha1 {
                        info!("File exist on local path {}", local_path.display());
                        continue;
                    }
                    debug!("diff local sha1 {} remote sha1 {}", local_sha1, sha1);
                }
                Some(HashObject::Md5 { md5 }) => {
                    debug!("MD5 not impl yet");
                }
                _ => {}
            }
        }

        let raw_url = get_raw_url(&client, file).await?;
        download_file(&raw_url, local_path, &client).await?;
    }
    Ok(())
}

async fn download_file(raw_url: &str, local_path: PathBuf, client: &Client) -> Result<()> {
    debug!("local file path: {}", local_path.display());

    // Send the GET request
    let mut response = client
        .get(raw_url)
        .send()
        .await
        .map_err(|e| anyhow!("Request failed for '{}': {}", raw_url, e))?;

    // Check status code
    if !response.status().is_success() {
        return Err(anyhow!(
            "Server returned error status {} for '{}'",
            response.status(),
            raw_url
        ));
    }

    // Ensure the parent directory exists
    if let Some(parent_dir) = local_path.parent() {
        if !parent_dir.exists() {
            fs::create_dir_all(parent_dir).await?;
        }
    }

    // Create or truncate the file
    let mut file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&local_path)
        .await
        .map_err(|e| anyhow!("Failed to open file '{:?}': {}", local_path, e))?;

    // Stream the file contents
    while let Some(chunk) = response.chunk().await? {
        file.write_all(&chunk).await?
    }

    info!("File downloaded successfully to {}", local_path.display());
    Ok(())
}

pub(crate) async fn create_strm_file(
    files_with_ext: &Vec<(String, &EntryWithPath)>,
    output_path: &str,
) -> Result<()> {
    let client = Client::new();
    let files_strm = files_with_ext
        .iter()
        .filter(|(ext, _)| FILE_STRM.contains(&ext.as_str()))
        .map(|f| {
            let client_ref = &client;
            async move {
                let raw_url = get_raw_url(client_ref, f).await?;
                let mut local_path = PathBuf::from(output_path);
                let relative_p2 = f.1.path_str.trim_start_matches('/');
                local_path.push(relative_p2);
                local_path.set_extension("strm");

                let parsed_url = Url::parse(&raw_url)
                    .map_err(|e| anyhow!("Failed to parse URL '{}': {}", raw_url, e))?;
                Ok((parsed_url, local_path))
            }
        });

    for file in files_strm {
        let (raw_url, local_path) = file.await?;

        // Ensure the parent directory exists
        if let Some(parent_dir) = local_path.parent() {
            if !parent_dir.exists() {
                fs::create_dir_all(parent_dir).await?;
            }
        }

        // Create or truncate the file
        fs::write(&local_path, raw_url.as_str()).await?;
    }
    Ok(())
}
