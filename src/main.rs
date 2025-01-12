mod alist_api;
mod download;

// use std::io::Write;

use anyhow::Result;
use clap::Parser;
use log::info;
use once_cell::sync::Lazy;
use url::Url;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
enum Commands {
    #[command(arg_required_else_help = true)]
    /// Create and refresh strm file and metadata for the Alist server
    AutoSym {
        /// alist server addr
        #[arg(short, long, default_value = "http://192.168.0.201:5244")]
        server_address: String,

        #[arg(short, long, required = true)]
        url_path: String,

        /// download path directory
        #[arg(short, long, required = true)]
        local_path: String,
    },
    Download {
        /// alist server addr
        #[arg(short, long, required = true)]
        url: String,

        /// download path directory
        #[arg(short, long, required = true)]
        local_path: String,
    },
}

static ALIST_URL: Lazy<String> = Lazy::new(|| {
    // This closure runs the first time SERVER_ADDRESS is accessed.
    let cli = Cli::parse();
    match cli.command {
        Commands::AutoSym { server_address, .. } => server_address,
        Commands::Download { url, .. } => Url::parse(&url)
            .expect("Wrong Server Url format")
            .host_str()
            .expect("Wrong Server Url format")
            .to_string(),
    }
});

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
    match args.command {
        Commands::AutoSym {
            server_address: _,
            url_path,
            local_path,
        } => {
            let res = alist_api::get_path_structure(url_path).await?;
            let files_with_ext = alist_api::get_file_ext(&res).await;

            info!("Start to copy metadata");
            alist_api::copy_metadata(&files_with_ext, &local_path).await?;
            alist_api::create_strm_file(&files_with_ext, &local_path).await?;
        }
        Commands::Download { url, local_path } => {
            let full_url = Url::parse(&url)?;

            download::test(full_url.path().to_string(), local_path).await?;
        }
    }

    Ok(())
}
