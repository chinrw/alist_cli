use std::{path::PathBuf, sync::Arc};

use anyhow::{Ok, Result};
use indicatif::MultiProgress;
use reqwest::Client;
use tokio::{sync::Semaphore, task::JoinSet};

use crate::{
    THREADS_NUM,
    alist_api::{download_file_with_retries, get_path_structure, get_raw_url, provider_checksum},
};

pub(super) async fn download_folders(
    url_path: String,
    local_path: &str,
    m_pb: MultiProgress,
) -> Result<()> {
    let res = get_path_structure(url_path, m_pb.clone()).await?;
    let mut tasks = JoinSet::new();
    let client = Arc::new(Client::new());
    let semaphore = Arc::new(Semaphore::new(*THREADS_NUM)); // Limit concurrency to 4

    for f in res {
        let client_cloned = Arc::clone(&client);
        let mut local_path_buf = PathBuf::from(local_path);
        let semaphore_cloned = Arc::clone(&semaphore);

        // Remove leading "/" from f.path_str
        let relative_p2 = f.path_str.trim_start_matches('/');
        local_path_buf.push(relative_p2);

        let m_clone = m_pb.clone();
        tasks.spawn(async move {
            // use semaphore to limit the concurrent downloader
            let _permit = semaphore_cloned.acquire().await.unwrap();
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
        });
    }

    let _ = tasks.join_all().await;

    Ok(())
}
