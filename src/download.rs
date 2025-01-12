use alist_api::get_path_structure;
use anyhow::{Ok, Result};
use reqwest::Client;

use crate::alist_api::{self, download_file_with_retries};
pub(super) async fn test(url_path: String, local_path: String) -> Result<()> {
    let res = alist_api::get_path_structure(url_path).await?;
    // res.iter().map(|f| async {
    //
    //     let client = Client::new();
    //     let raw_url = f.entry.
    // });

    Ok(())
}
