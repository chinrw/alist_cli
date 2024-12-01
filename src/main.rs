mod alist_api;

use anyhow::Result;
use log::info;
use std::io::Write;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::new()
        .parse_filters(&std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()))
        .write_style(env_logger::WriteStyle::Always)
        .format(|buf, record| {
            let timestamp = buf.timestamp();
            let info_style = buf.default_level_style(log::Level::Info);

            writeln!(
                buf,
                "[{timestamp} {info_style}{:<5}{info_style:#} {}:{} {}] {}",
                record.level(),                       // Log level (e.g., DEBUG, INFO)
                record.file().unwrap_or("<unknown>"), // File name
                record.line().unwrap_or(0),           // Line number
                record.module_path().unwrap_or("<unknown>"), // Module path
                record.args()                         // The log message
            )
        })
        .default_format()
        .init();

    let res = alist_api::get_path_structure("/115/bk_plain/video/电影刮削中".to_string()).await?;

    let files_with_ext = alist_api::get_file_ext(&res).await;

    info!("Start to copy metadata");
    alist_api::copy_metadata(files_with_ext).await?;
    Ok(())
}
