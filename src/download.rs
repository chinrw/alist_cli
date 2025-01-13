use std::path::PathBuf;

use anyhow::{Ok, Result};
use reqwest::Client;

use crate::alist_api::{download_file_with_retries, get_path_structure, get_raw_url};

pub(super) async fn download_folders(url_path: String, local_path: &str) -> Result<()> {
    let res = get_path_structure(url_path).await?;
    for f in res {
        let client = Client::new();
        let mut local_path_buf = PathBuf::from(local_path);

        // Remove leading "/" from f.path_str
        let relative_p2 = f.path_str.trim_start_matches('/');
        local_path_buf.push(relative_p2);

        let raw_url = get_raw_url(&client, &f).await?;

        // Download and retry on failure
        download_file_with_retries(&raw_url, local_path_buf, &client, &f.entry.hash_info).await?;
    }

    Ok(())
}
