//! File operations and download utilities.

use std::path::Path;

use anyhow::{Result, anyhow};
use indicatif::MultiProgress;
use reqwest::Client;
use tokio::fs;
use tracing::{debug, info};

use crate::api::{
    rate_limiter::rate_limited_get,
    types::{EntryWithPath, HashObject},
};

/// Maximum number of retry attempts for downloads
const MAX_RETRIES: u32 = 3;

/// Downloads a file with retry logic and optional checksum verification.
///
/// # Arguments
///
/// * `raw_url` - The URL to download from
/// * `local_path` - Local path where the file should be saved
/// * `client` - HTTP client for making requests
/// * `checksum` - Optional hash for verification
/// * `m_pb` - Multi-progress bar for UI feedback
///
/// # Returns
///
/// Success if the file was downloaded and verified
///
/// # Errors
///
/// Returns an error if all retry attempts fail
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
            Ok(_) => return Ok(()),
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

/// Checks if the provider supports reliable checksums.
///
/// # Arguments
///
/// * `entry` - The file entry to check
///
/// # Returns
///
/// `true` if checksums are reliable for this provider
pub fn provider_checksum(entry: &EntryWithPath) -> bool {
    if entry.provider == "BaiduNetdisk" {
        return false;
    }
    true
}

/// Attempts to download a file once with checksum verification.
///
/// # Arguments
///
/// * `raw_url` - The URL to download from
/// * `local_path` - Local path where the file should be saved
/// * `client` - HTTP client for making requests
/// * `checksum` - Optional hash for verification
/// * `m_pb` - Multi-progress bar for UI feedback
///
/// # Returns
///
/// Success if the file was downloaded and verified
///
/// # Errors
///
/// Returns an error if the download or verification fails
async fn attempt_download_file(
    raw_url: &str,
    local_path: &Path,
    client: &Client,
    checksum: Option<HashObject>,
    m_pb: MultiProgress,
) -> Result<()> {
    debug!("Download to local file path: {}", local_path.display());

    if let Some(ref checksum_obj) = checksum {
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
    use tokio::io::AsyncWriteExt;
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
