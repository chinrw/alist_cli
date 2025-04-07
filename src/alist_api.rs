use std::{
    collections::{HashSet, VecDeque},
    fmt::Write,
    num::NonZeroU32,
    ops::Add,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::{Ok, Result, anyhow};
use digest::{Digest, OutputSizeUser, generic_array::ArrayLength};
use futures::stream::{self, StreamExt};
use governor::{
    Quota, RateLimiter,
    clock::DefaultClock,
    state::{InMemoryState, NotKeyed},
};
use indicatif::{MultiProgress, ProgressBar, ProgressState, ProgressStyle};
use md5::Md5;
use once_cell::sync::Lazy;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha1::Sha1;
use tokio::{
    fs::{self, File},
    io::{AsyncReadExt, AsyncWriteExt, BufReader},
    sync::Mutex,
};
use tracing::{Level, debug, enabled, error, info, trace, warn};
use url::Url;

use crate::{ALIST_URL, TOKEN}; // Use the async-aware Mutex from tokio

const META_SUFF: [&str; 9] = [
    "nfo", "jpg", "png", "svg", "ass", "srt", "sup", "vtt", "txt",
];

pub const FILE_STRM: [&str; 14] = [
    "mkv", "iso", "ts", "mp4", "avi", "rmvb", "wmv", "m2ts", "mpg", "flv", "rm", "mov", "wav",
    "mp3",
];

const ALIST_CONCURRENT_LIMIT: usize = 10;
const MAX_RETRIES: u32 = 3;
const RETRY_DELAY_MS: u64 = 1000;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub(crate) enum HashObject {
    Sha1 { sha1: String },
    Md5 { md5: String },
}

impl HashObject {
    /// Returns the inner hash string (either the Sha1 or Md5).
    fn as_hash_str(&self) -> String {
        match self {
            HashObject::Sha1 { sha1 } => sha1.to_lowercase(),
            HashObject::Md5 { md5 } => md5.to_lowercase(),
        }
    }

    async fn hash_process_bar<D: Digest + Default>(
        mut reader: BufReader<File>,
        file_size: u64,
        local_path: &Path,
        m_pb: MultiProgress,
    ) -> Result<String>
    where
        <D as OutputSizeUser>::OutputSize: Add,
        <<D as OutputSizeUser>::OutputSize as Add>::Output: ArrayLength<u8>,
    {
        // Check if we're in verbose log mode if true print hash progress bar
        let is_verbose_logging = enabled!(Level::DEBUG);

        let pb = if is_verbose_logging {
            let pb = m_pb.insert_from_back(1, ProgressBar::new(file_size));
            pb.set_style(
                ProgressStyle::with_template(
                    "{spinner:.green} {msg} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({eta})",
                )
                .unwrap()
                .with_key("eta", |state: &ProgressState, w: &mut dyn Write| {
                    write!(w, "{:.1}s", state.eta().as_secs_f64()).unwrap()
                })
                .progress_chars("#>-"),
            );
            pb
        } else {
            // Create a hidden/dummy progress bar when in debug mode
            let pb = m_pb.insert_from_back(1, ProgressBar::new(file_size));
            pb.set_style(ProgressStyle::default_bar());
            pb.set_draw_target(indicatif::ProgressDrawTarget::hidden());
            pb
        };

        let mut total_read = 0;

        let mut buffer = [0u8; 8192];
        let mut hasher = D::new();
        // Read the file in chunks
        loop {
            let n = reader.read(&mut buffer).await?;
            if n == 0 {
                break; // EOF reached
            }
            hasher.update(&buffer[..n]);
            total_read += n as u64;
            pb.set_position(total_read);
        }
        pb.finish_with_message(format!("Done hashing {}", local_path.display()));
        Ok(format!("{:x}", hasher.finalize()))
    }

    pub(crate) async fn compute_file_checksum(
        &self,
        local_path: &Path,
        m_pb: MultiProgress,
    ) -> Result<String> {
        let file = File::open(local_path).await?;
        let file_size = file.metadata().await?.len();
        let reader = BufReader::new(file);

        match self {
            HashObject::Sha1 { .. } => {
                Self::hash_process_bar::<Sha1>(reader, file_size, local_path, m_pb).await
            }
            HashObject::Md5 { .. } => {
                Self::hash_process_bar::<Md5>(reader, file_size, local_path, m_pb).await
            }
        }
    }

    /// Computes a fresh checksum from the file and compares it to
    /// the stored hash. Returns `true` if they match, `false` otherwise.
    pub async fn verify_file_checksum(
        &self,
        local_path: &Path,
        m_pb: MultiProgress,
    ) -> Result<bool> {
        let mut res = false;
        // Check if the local file exist
        if Path::new(&local_path).exists() {
            let computed = self.compute_file_checksum(local_path, m_pb).await?;
            debug!(
                "local checksum: {} remote file checksum: {}",
                computed,
                self.as_hash_str()
            );
            res = computed == self.as_hash_str();
        }
        Ok(res)
    }
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
    pub(crate) is_dir: bool,
    modified: String,
    sign: String,
    thumb: String,
    #[serde(rename = "type")]
    file_type: u32,
    created: Option<String>,
    pub(crate) hashinfo: Option<String>,
    // #[serde(rename = "hash_info")]
    pub(crate) hash_info: Option<HashObject>,
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
    FileInfo(Box<FileInfo>),
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
    pub(crate) provider: String,
}

static TPS_RATE_LIMITER: Lazy<RateLimiter<NotKeyed, InMemoryState, DefaultClock>> =
    Lazy::new(|| {
        let quota = Quota::per_second(NonZeroU32::new(*crate::TPSLIMIT).unwrap());
        RateLimiter::direct(quota)
    });

async fn rate_limited_request<T>(
    client: &Client,
    url: String,
    payload: T,
) -> Result<reqwest::Response>
where
    T: serde::Serialize,
{
    // Wait until we're allowed to make a request
    TPS_RATE_LIMITER.until_ready().await;

    // Now make the request
    let response = client
        .post(url)
        .json(&payload)
        .header("Authorization", TOKEN.clone())
        .header("Content-Type", "application/json")
        .send()
        .await?;

    Ok(response)
}

// Rate-limited GET request
async fn rate_limited_get(client: &Client, url: &str) -> Result<reqwest::Response> {
    // Wait until we're allowed to make a request
    TPS_RATE_LIMITER.until_ready().await;

    // Now make the request
    let response = client.get(url).send().await?;

    Ok(response)
}

pub(crate) async fn get_path_structure(
    path: String,
    m_pb: MultiProgress,
) -> Result<Vec<EntryWithPath>> {
    let visited_paths = Arc::new(Mutex::new(HashSet::new()));
    {
        let mut visited_paths_lock = visited_paths.lock().await;
        visited_paths_lock.insert(path.clone());
    }

    // Fetch the folder contents iteratively and get all entries with paths
    let entries_with_paths = fetch_folder_contents(path, visited_paths.clone(), m_pb).await?;

    // Return the collected entries along with their paths
    Ok(entries_with_paths)
}

async fn get_api_response(client: &Client, payload: &FileInfoRequest) -> Result<ApiResponse> {
    let response =
        rate_limited_request(client, format!("{}/api/fs/list", *ALIST_URL), payload).await?;

    if !response.status().is_success() {
        return Err(anyhow!("HTTP error: {}", response.status()));
    }

    let api_response: ApiResponse = response
        .json()
        .await
        .map_err(|e| anyhow!("Failed to parse API response: {}", e))?;

    trace!("list api_response: {:?}", api_response);
    Ok(api_response)
}

async fn process_folder_contents(
    client: &Client,
    current_path: &str,
    payload: &FileInfoRequest,
    entries_with_paths: &mut Vec<EntryWithPath>,
    directories_to_process: &mut VecDeque<String>,
    visited_paths: &Arc<Mutex<HashSet<String>>>,
    pb: &ProgressBar,
) -> Result<()> {
    let mut retry_count = 0;

    while retry_count <= MAX_RETRIES {
        // Break early if this is a retry attempt
        if retry_count > 0 {
            info!(
                "Retrying request for path {} ({}/{})",
                current_path, retry_count, MAX_RETRIES
            );
            tokio::time::sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
        }

        // Attempt to get API response
        let api_response = match get_api_response(client, payload).await {
            std::result::Result::Ok(response) => response,
            Err(err) => {
                warn!("Request failed: {}", err);
                retry_count += 1;
                if retry_count > MAX_RETRIES {
                    error!("Failed after {} retries: {}", MAX_RETRIES, current_path);
                    return Err(err);
                }
                continue;
            }
        };

        // Check for error codes in API response
        if api_response.code != 200 {
            warn!(
                "API returned error code {}: {}",
                api_response.code, api_response.message
            );
            retry_count += 1;
            if retry_count > MAX_RETRIES {
                error!("Failed after {} retries: {}", MAX_RETRIES, current_path);
                return Err(anyhow!(
                    "API error code {}: {}",
                    api_response.code,
                    api_response.message
                ));
            }
            continue;
        }

        // Process the response data
        match api_response.data {
            Some(ApiData::FoldersInfo(folders_info)) => {
                // Skip if no content
                if folders_info.content.is_none() {
                    return Ok(());
                }

                for file in &folders_info.content.unwrap() {
                    let full_path = format!("{}/{}", current_path, file.name);
                    debug!("entry path: {}", full_path);
                    pb.set_message(format!("Scanning: {}", full_path));

                    // Add this entry and its full path to the list
                    entries_with_paths.push(EntryWithPath {
                        entry: file.clone(),
                        path_str: full_path.clone(),
                        provider: folders_info.provider.clone(),
                    });

                    // If the item is a directory and hasn't been visited, add it to the queue
                    if file.is_dir {
                        let mut visited = visited_paths.lock().await;
                        if visited.insert(full_path.clone()) {
                            directories_to_process.push_back(full_path);
                        }
                    }
                    pb.inc(1);
                }

                return Ok(());
            }
            _ => {
                retry_count += 1;
                if retry_count > MAX_RETRIES {
                    error!("Failed after {} retries: {}", MAX_RETRIES, current_path);
                    return Err(anyhow!("Invalid data format in API response"));
                }
                continue;
            }
        }
    }

    Err(anyhow!("Failed to process directory after maximum retries"))
}

async fn fetch_folder_contents(
    path: String,
    visited_paths: Arc<Mutex<HashSet<String>>>,
    m_pb: MultiProgress,
) -> Result<Vec<EntryWithPath>> {
    let client = Client::builder().no_proxy().build()?;
    let mut entries_with_paths = Vec::new();
    let mut directories_to_process = VecDeque::new();
    directories_to_process.push_back(path.clone());

    let spinner_style =
        ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{len}] {wide_msg}")
            .unwrap();
    let pb = m_pb.add(ProgressBar::new_spinner());
    pb.set_style(spinner_style.clone());

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

        if let Err(err) = process_folder_contents(
            &client,
            &current_path,
            &payload,
            &mut entries_with_paths,
            &mut directories_to_process,
            &visited_paths,
            &pb,
        )
        .await
        {
            warn!(
                "Failed to process path after {} retries: {}",
                MAX_RETRIES, current_path
            );
            debug!("Error details: {:?}", err);
            // Continue with next directory instead of returning error
        }
    }
    info!("Processed {} files", pb.position());
    Ok(entries_with_paths)
}

pub(crate) async fn get_raw_url(client: &Client, entry: &EntryWithPath) -> Result<String> {
    trace!("file: {:?}", entry);
    let payload = FileInfoRequest {
        path: entry.path_str.clone(),
        password: "".to_string(),
        page: 1,
        per_page: 0,
        refresh: false,
    };

    trace!("metadata current payload:{:?}", payload);
    let response =
        rate_limited_request(client, format!("{}/api/fs/get", *ALIST_URL), payload).await?;

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
    m_pb: MultiProgress,
) -> Result<()> {
    info!("Start to copy metadata");

    let files_copy: Vec<&(String, &EntryWithPath)> = files_with_ext
        .iter()
        .filter(|(ext, _)| META_SUFF.contains(&ext.as_str()))
        .collect();

    let sty = ProgressStyle::with_template(
        "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} ({eta})",
    )
    .unwrap()
    .with_key("eta", |state: &ProgressState, w: &mut dyn Write| {
        write!(w, "{:.1}s", state.eta().as_secs_f64()).unwrap()
    })
    .progress_chars("#>-");

    let pb = m_pb
        .add(ProgressBar::new(files_copy.len() as u64))
        .with_style(sty.clone());

    let client = Arc::new(Client::builder().no_proxy().build()?);
    // Create a stream of futures.
    let tasks = stream::iter(files_copy.into_iter().map(|file| {
        // Clone necessary values for the async block.
        let client = client.clone();
        let pb = pb.clone();
        let m_clone = m_pb.clone();
        let output_path = output_path.to_string();
        async move {
            // Construct the full local path.
            let mut local_path = PathBuf::from(&output_path);
            let relative_p2 = file.1.path_str.trim_start_matches('/');
            local_path.push(relative_p2);

            // Obtain the raw URL asynchronously.
            let raw_url = get_raw_url(&client, file.1).await?;
            // Attempt to download the file with retries.
            if let Err(e) = download_file_with_retries(
                &raw_url,
                &local_path,
                &client,
                file.1.entry.hash_info.clone(),
                m_clone.clone(),
            )
            .await
            {
                warn!("Failed to download '{}': {}", raw_url, e);
            };

            pb.inc(1);
            Ok(())
        }
    }))
    .buffer_unordered(ALIST_CONCURRENT_LIMIT);

    // Wait for all tasks to complete.
    tasks
        .for_each(|res| async {
            if let Err(e) = res {
                // Optionally handle individual errors here.
                warn!("Task failed with error: {}", e);
            }
        })
        .await;

    info!("Metadata files created");

    Ok(())
}

/// This function is used by BAIDU_NETDISK to encryptstr md5sum res to match the
/// md5 from provider However, this may not provide the correct md5
pub fn _encrypt_md5(md5str: &str) -> String {
    // 1) Rearrange the string: [8..16] + [0..8] + [24..32] + [16..24]
    let rearranged = format!(
        "{}{}{}{}",
        &md5str[8..16],
        &md5str[0..8],
        &md5str[24..32],
        &md5str[16..24],
    );

    // 2) Build `encryptstr`: for each char, parse as hex digit, XOR with (15 &
    //    index), format as hex
    let mut encryptstr: String =
        rearranged
            .chars()
            .enumerate()
            .fold(String::new(), |mut output, (i, ch)| {
                let val = ch
                    .to_digit(16)
                    .expect("Character in rearranged MD5 string wasn't valid hex.");
                let _ = write!(output, "{:x}", val ^ (15 & i as u32));
                output
            });

    // 3) Modify the 10th character (index 9): encryptstr[9] => 'g' + hexDigit
    let val_9 = encryptstr
        .chars()
        .nth(9)
        .expect("encryptstr shorter than expected")
        .to_digit(16)
        .expect("Character at index 9 wasn't valid hex.");
    let new_char = std::char::from_u32(('g' as u32) + val_9)
        .expect("Adding offset to 'g' went out of valid Unicode range.");
    encryptstr.replace_range(9..10, &new_char.to_string());

    encryptstr
}

pub async fn download_file_with_retries(
    raw_url: &str,
    local_path: &Path,
    client: &Client,
    checksum: Option<HashObject>,
    m_pb: MultiProgress,
) -> Result<()> {
    for attempt in 1..=MAX_RETRIES {
        match attempt_download_file(raw_url, local_path, client, checksum.clone(), m_pb.clone())
            .await
        {
            std::result::Result::Ok(_) => return Ok(()),
            Err(e) if attempt < MAX_RETRIES => info!(
                "Download attempt #{} for '{}' failed: {}. Retrying...",
                attempt, raw_url, e
            ),

            Err(e) => {
                return Err(anyhow!(
                    "Failed after {} attempts for '{}': {}",
                    attempt,
                    raw_url,
                    e
                ));
            }
        }
    }
    // Should never reach here unless the loop is changed.
    unreachable!("All retry attempts have returned by this point.");
}

pub(crate) fn provider_checksum(entry: &EntryWithPath) -> bool {
    if entry.provider == "BaiduNetdisk" {
        return false;
    }
    true
}

async fn attempt_download_file(
    raw_url: &str,
    local_path: &Path,
    client: &Client,
    checksum: Option<HashObject>,
    m_pb: MultiProgress,
) -> Result<()> {
    debug!("Download to local file path: {}", local_path.display());

    if let Some(checksum_obj) = checksum.clone() {
        if checksum_obj
            .verify_file_checksum(local_path, m_pb.clone())
            .await?
        {
            return Ok(());
        }
    }

    // Send GET Request
    let mut response = rate_limited_get(client, raw_url)
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

    // Verify the file checksum (if provided)
    if let Some(checksum_obj) = checksum.clone() {
        let verified = checksum_obj
            .verify_file_checksum(local_path, m_pb.clone())
            .await?;
        if !verified {
            return Err(anyhow!(
                "Checksum mismatch. Downloaded file does not match the expected hash."
            ));
        }
        debug!("Downloaded file verified successfully against the provided hash.");
    }

    Ok(())
}

pub(crate) async fn create_strm_file(
    files_with_ext: &Vec<(String, &EntryWithPath)>,
    output_path: &str,
    m_pb: MultiProgress,
) -> Result<()> {
    let client = Client::builder().no_proxy().build()?;
    let files_strm = files_with_ext
        .iter()
        .filter(|(ext, _)| FILE_STRM.contains(&ext.as_str()));

    let pb = m_pb.add(ProgressBar::new(files_strm.clone().count() as u64));
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} ({eta})",
        )
        .unwrap()
        .with_key("eta", |state: &ProgressState, w: &mut dyn Write| {
            write!(w, "{:.1}s", state.eta().as_secs_f64()).unwrap()
        })
        .progress_chars("#>-"),
    );

    info!("Start to create strm files");
    let mut results = stream::iter(files_strm.map(|f| {
        let client_ref = &client;
        async move {
            let raw_url = get_raw_url(client_ref, f.1).await?;
            let mut local_path = PathBuf::from(output_path);
            let relative_p2 = f.1.path_str.trim_start_matches('/');
            local_path.push(relative_p2);
            local_path.set_extension("strm");

            let parsed_url = Url::parse(&raw_url)
                .map_err(|e| anyhow!("Failed to parse URL '{}': {}", raw_url, e))?;
            Ok((parsed_url, local_path))
        }
    }))
    .buffer_unordered(ALIST_CONCURRENT_LIMIT);

    while let Some(result) = results.next().await {
        let (raw_url, local_path) = result?;

        if let Some(parent_dir) = local_path.parent() {
            if !parent_dir.exists() {
                fs::create_dir_all(parent_dir).await?;
            }
        }
        fs::write(&local_path, raw_url.as_str()).await?;
        pb.inc(1);
    }

    info!("strm file created");

    Ok(())
}
