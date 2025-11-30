use std::{path::PathBuf, sync::Arc};

use anyhow::Result;
use indicatif::MultiProgress;
use reqwest::Client;
use tokio::{sync::Semaphore, task::JoinSet};

use crate::{
    api::{get_path_structure, get_raw_url},
    get_config,
    utils::{download_file_with_retries, provider_checksum},
};

pub async fn download_folders(
    url_path: String,
    local_path: &str,
    m_pb: MultiProgress,
) -> Result<()> {
    let client = Arc::new(Client::builder().no_proxy().build()?);
    let res = get_path_structure(url_path, m_pb.clone(), Arc::clone(&client)).await?;
    let mut tasks = JoinSet::new();
    let semaphore = Arc::new(Semaphore::new(get_config().threads));

    for f in res {
        let client_cloned = Arc::clone(&client);
        let mut local_path_buf = PathBuf::from(local_path);
        let semaphore_cloned = Arc::clone(&semaphore);

        // Remove leading "/" from f.path_str
        let relative_p2 = f.path_str.trim_start_matches('/');
        local_path_buf.push(relative_p2);

        let m_clone = m_pb.clone();
        let file_path = f.path_str.clone();
        tasks.spawn(async move {
            // use semaphore to limit the concurrent downloader
            let _permit = semaphore_cloned.acquire().await?;
            let raw_url = get_raw_url(&client_cloned, &f).await?;
            let hash_info = if provider_checksum(&f) {
                f.entry.hash_info.clone()
            } else {
                None
            };

            download_file_with_retries(
                &raw_url,
                &local_path_buf,
                &client_cloned,
                hash_info,
                m_clone,
            )
            .await
            .map(|_| file_path)
        });
    }

    let mut failed_files = Vec::new();
    let mut succeeded = 0;
    let mut failed = 0;

    while let Some(result) = tasks.join_next().await {
        match result {
            Ok(Ok(file_path)) => {
                succeeded += 1;
                tracing::debug!("Successfully downloaded: {}", file_path);
            }
            Ok(Err(e)) => {
                failed += 1;
                let error_msg = format!("Download error: {}", e);
                tracing::error!("{}", error_msg);
                failed_files.push(error_msg);
            }
            Err(e) => {
                failed += 1;
                let error_msg = format!("Task join error: {}", e);
                tracing::error!("{}", error_msg);
                failed_files.push(error_msg);
            }
        }
    }

    // Report summary
    tracing::info!(
        "Download complete: {} succeeded, {} failed",
        succeeded,
        failed
    );

    if !failed_files.is_empty() {
        tracing::warn!("Failed downloads:");
        for error in &failed_files {
            tracing::warn!("  - {}", error);
        }
        return Err(anyhow::anyhow!(
            "Download completed with {} errors. See logs for details.",
            failed
        ));
    }

    Ok(())
}
