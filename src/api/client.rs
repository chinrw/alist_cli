//! Core API client functionality for communicating with Alist server.

use std::{
    collections::{HashSet, VecDeque},
    sync::Arc,
    time::Duration,
};

use anyhow::{Result, anyhow};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use reqwest::Client;
use tokio::sync::Mutex;
use tracing::{debug, error, info, trace, warn};

use super::{
    rate_limiter::rate_limited_request,
    types::{ApiData, ApiResponse, EntryWithPath, FileInfoRequest},
};
use crate::CONFIG;

/// Maximum number of retry attempts for failed requests
const MAX_RETRIES: u32 = 3;

/// Delay between retry attempts in milliseconds
const RETRY_DELAY_MS: u64 = 1000;

/// Retrieves the complete directory structure from the Alist server.
///
/// This function recursively traverses the directory structure starting from
/// the given path and returns all files and directories found.
///
/// # Arguments
///
/// * `path` - The starting path to scan
/// * `m_pb` - Multi-progress bar for UI feedback
/// * `client` - HTTP client for making requests
///
/// # Returns
///
/// A vector of all entries found with their full paths
///
/// # Errors
///
/// Returns an error if the API requests fail or if there are network issues
pub async fn get_path_structure(
    path: String,
    m_pb: MultiProgress,
    client: Arc<Client>,
) -> Result<Vec<EntryWithPath>> {
    let visited_paths = Arc::new(Mutex::new(HashSet::new()));
    {
        let mut visited_paths_lock = visited_paths.lock().await;
        visited_paths_lock.insert(path.clone());
    }

    // Fetch the folder contents iteratively and get all entries with paths
    let entries_with_paths =
        fetch_folder_contents(path, visited_paths.clone(), m_pb, client).await?;

    // Return the collected entries along with their paths
    Ok(entries_with_paths)
}

/// Makes an API request to get directory contents.
///
/// # Arguments
///
/// * `client` - HTTP client for making requests
/// * `payload` - Request payload with path and pagination info
///
/// # Returns
///
/// The parsed API response
///
/// # Errors
///
/// Returns an error if the request fails or response parsing fails
async fn get_api_response(client: &Client, payload: &FileInfoRequest) -> Result<ApiResponse> {
    let response = rate_limited_request(
        client,
        format!("{}/api/fs/list", CONFIG.server_address),
        payload,
    )
    .await?;

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

/// Processes the contents of a single folder with retry logic.
///
/// # Arguments
///
/// * `client` - HTTP client for making requests
/// * `current_path` - Path of the folder to process
/// * `payload` - Request payload for the API call
/// * `entries_with_paths` - Vector to collect found entries
/// * `directories_to_process` - Queue of directories still to process
/// * `visited_paths` - Set of already visited paths to avoid cycles
/// * `pb` - Progress bar for UI feedback
///
/// # Returns
///
/// Success if the folder was processed successfully
///
/// # Errors
///
/// Returns an error if all retry attempts fail
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
            Ok(response) => response,
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
                let Some(content) = &folders_info.content else {
                    return Ok(());
                };

                for file in content {
                    let full_path = format!("{}/{}", current_path, file.name);
                    debug!("entry path: {}", full_path);
                    pb.set_message(format!("Scanning: {full_path}"));

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

/// Fetches folder contents recursively using breadth-first traversal.
///
/// # Arguments
///
/// * `path` - Starting path to fetch from
/// * `visited_paths` - Set of already visited paths
/// * `m_pb` - Multi-progress bar for UI feedback
/// * `client` - HTTP client for requests
///
/// # Returns
///
/// Vector of all found entries with their paths
///
/// # Errors
///
/// Returns an error if critical API calls fail
async fn fetch_folder_contents(
    path: String,
    visited_paths: Arc<Mutex<HashSet<String>>>,
    m_pb: MultiProgress,
    client: Arc<Client>,
) -> Result<Vec<EntryWithPath>> {
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

    pb.finish_with_message(format!("Processed {} files", pb.position()));
    Ok(entries_with_paths)
}
