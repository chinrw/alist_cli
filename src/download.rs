use std::path::PathBuf;

use anyhow::{Ok, Result};
use reqwest::Client;
use std::sync::Arc;
use tokio::task::JoinSet;

use crate::alist_api::{
    download_file_with_retries, get_path_structure, get_raw_url, provider_checksum,
};

pub(super) async fn download_folders(url_path: String, local_path: &str) -> Result<()> {
    let res = get_path_structure(url_path).await?;
    let mut tasks = JoinSet::new();
    let client = Arc::new(Client::new());

    for f in res {
        let client_cloned = Arc::clone(&client);
        let mut local_path_buf = PathBuf::from(local_path);

        // Remove leading "/" from f.path_str
        let relative_p2 = f.path_str.trim_start_matches('/');
        local_path_buf.push(relative_p2);

        tasks.spawn(async move {
            let raw_url = get_raw_url(&client_cloned, &f).await?;
            let hash_info = if provider_checksum(&f) {
                f.entry.hash_info.clone()
            } else {
                None
            };

            download_file_with_retries(raw_url, local_path_buf, &client_cloned, hash_info).await
        });
    }

    let _ = tasks.join_all().await;

    Ok(())
}
