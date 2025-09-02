//! High-level operations for file management and processing.

use std::{fmt::Write, path::PathBuf, sync::Arc};

use anyhow::{Result, anyhow};
use futures::stream::{self, StreamExt};
use indicatif::{MultiProgress, ProgressBar, ProgressState, ProgressStyle};
use reqwest::Client;
use tokio::fs;
use tracing::{debug, info, trace, warn};
use url::Url;

use super::{
    rate_limiter::rate_limited_request,
    types::{
        ApiData, ApiResponse, EntryWithPath, FileInfoRequest, is_metadata_file, is_streamable_file,
    },
};
use crate::{CONFIG, utils::file_ops::download_file_with_retries};

/// Gets the raw download URL for a given file entry.
///
/// # Arguments
///
/// * `client` - HTTP client for making requests
/// * `entry` - The file entry to get the URL for
///
/// # Returns
///
/// The raw download URL as a string
///
/// # Errors
///
/// Returns an error if the API request fails or returns invalid data
pub async fn get_raw_url(client: &Client, entry: &EntryWithPath) -> Result<String> {
    trace!("file: {:?}", entry);
    let payload = FileInfoRequest {
        path: entry.path_str.clone(),
        password: "".to_string(),
        page: 1,
        per_page: 0,
        refresh: false,
    };

    trace!("metadata current payload:{:?}", payload);
    let response = rate_limited_request(
        client,
        format!("{}/api/fs/get", CONFIG.server_address),
        payload,
    )
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

/// Copies metadata files (nfo, jpg, png, etc.) from the server to local
/// storage.
///
/// # Arguments
///
/// * `files_with_ext` - Slice of files with their extensions
/// * `output_path` - Local directory path where files should be saved
/// * `m_pb` - Multi-progress bar for UI feedback
/// * `client` - HTTP client for making requests
///
/// # Returns
///
/// Success if all metadata files were processed
///
/// # Errors
///
/// Individual file failures are logged but don't stop the overall operation
pub async fn copy_metadata(
    files_with_ext: &[(String, &EntryWithPath)],
    output_path: &str,
    m_pb: MultiProgress,
    client: Arc<Client>,
) -> Result<()> {
    info!("Start to copy metadata");

    let files_copy: Vec<&(String, &EntryWithPath)> = files_with_ext
        .iter()
        .filter(|(ext, _)| is_metadata_file(ext))
        .collect();

    let sty = ProgressStyle::with_template(
        "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} ({eta})",
    )
    .unwrap()
    .with_key("eta", |state: &ProgressState, w: &mut dyn Write| {
        write!(w, "{:.1}s", state.eta().as_secs_f64()).unwrap()
    })
    .progress_chars("#>-");

    let pb = m_pb.add(ProgressBar::new(files_copy.len() as u64));
    pb.set_style(sty.clone());

    // Create a stream of futures
    let tasks = stream::iter(files_copy.into_iter().map(|file| {
        // Clone necessary values for the async block
        let client = client.clone();
        let pb = pb.clone();
        let m_clone = m_pb.clone();
        let output_path = output_path.to_string();
        async move {
            // Construct the full local path
            let mut local_path = PathBuf::from(&output_path);
            let relative_p2 = file.1.path_str.trim_start_matches('/');
            local_path.push(relative_p2);

            // Obtain the raw URL asynchronously
            let raw_url = get_raw_url(&client, file.1).await?;
            // Attempt to download the file with retries
            if let Err(e) = download_file_with_retries(
                &raw_url,
                &local_path,
                &client,
                file.1.entry.hash_info.clone(),
                m_clone,
            )
            .await
            {
                warn!("Failed to download '{}': {}", raw_url, e);
            };

            pb.inc(1);
            Ok(())
        }
    }))
    .buffer_unordered(CONFIG.concurrent_limit);

    // Wait for all tasks to complete
    tasks
        .for_each(|res: Result<()>| async {
            if let Err(e) = res {
                // Optionally handle individual errors here
                warn!("Task failed with error: {}", e);
            }
        })
        .await;

    info!("Metadata files created");

    Ok(())
}

/// Creates .strm files for streamable media files.
///
/// .strm files contain URLs that media players can use to stream content
/// directly from the Alist server without downloading the entire file.
///
/// # Arguments
///
/// * `files_with_ext` - Slice of files with their extensions
/// * `output_path` - Local directory path where .strm files should be created
/// * `m_pb` - Multi-progress bar for UI feedback
/// * `client` - HTTP client for making requests
///
/// # Returns
///
/// Success if all .strm files were created
///
/// # Errors
///
/// Returns an error if file system operations fail
pub async fn create_strm_file(
    files_with_ext: &[(String, &EntryWithPath)],
    output_path: &str,
    m_pb: MultiProgress,
    client: Arc<Client>,
) -> Result<()> {
    let files_strm = files_with_ext
        .iter()
        .filter(|(ext, _)| is_streamable_file(ext));

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
            Ok::<(Url, PathBuf), anyhow::Error>((parsed_url, local_path))
        }
    }))
    .buffer_unordered(CONFIG.concurrent_limit);

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
