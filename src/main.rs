mod alist_api;

// use std::io::Write;

use anyhow::Result;
use clap::Parser;
use lazy_static::lazy_static;
use log::info;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// memcached server addr
    #[arg(short, long, default_value = "http://192.168.0.201:5244")]
    server_address: String,

    #[arg(short, long, required = true)]
    url_path: String,

    #[arg(short, long, required = true)]
    local_path: String,
}

lazy_static! {
    pub(crate) static ref ALIST_URL: String = Cli::parse().server_address;
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::new()
        .parse_filters(&std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()))
        .write_style(env_logger::WriteStyle::Always)
        // .format(|buf, record| {
        //     let timestamp = buf.timestamp();
        //     let info_style = buf.default_level_style(log::Level::Info);
        //
        //     writeln!(
        //         buf,
        //         "[{timestamp} {info_style}{:<5}{info_style:#} {}:{} {}] {}",
        //         record.level(),                       // Log level (e.g., DEBUG, INFO)
        //         record.file().unwrap_or("<unknown>"), // File name
        //         record.line().unwrap_or(0),           // Line number
        //         record.module_path().unwrap_or("<unknown>"), // Module path
        //         record.args()                         // The log message
        //     )
        // })
        .init();

    let args = Cli::parse();

    let res = alist_api::get_path_structure(args.url_path).await?;
    let files_with_ext = alist_api::get_file_ext(&res).await;

    info!("Start to copy metadata");
    alist_api::copy_metadata(&files_with_ext, &args.local_path).await?;
    alist_api::create_strm_file(&files_with_ext, &args.local_path).await?;
    Ok(())
}
