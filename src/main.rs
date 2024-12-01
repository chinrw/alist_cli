mod alist_api;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let res = alist_api::get_path_structure("/115/bk_plain/video/AV".to_string()).await?;

    alist_api::get_file_ext(&res).await;
    Ok(())
}
