mod alist_api;
mod download;

pub use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use anyhow::{Ok, Result};
use clap::Parser;
use log::{info, trace};
use once_cell::sync::Lazy;
use tokio::fs;
use walkdir::WalkDir;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// alist server addr
    #[arg(
        short,
        long,
        global = true,
        default_value = "http://192.168.0.201:5244"
    )]
    server_address: String,

    #[arg(short, long, global = true, default_value = "/")]
    url_path: String,

    #[arg(short = 'j', long, global = true, default_value_t = 4)]
    threads: usize,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
enum Commands {
    #[command(arg_required_else_help = true)]
    /// Create and refresh strm file and metadata for the Alist server
    AutoSym {
        /// download path directory
        #[arg(short, long, required = true)]
        local_path: String,

        /// Do the actual remove the non-existent file
        #[arg(short, long, default_value_t = false)]
        delete: bool,
    },
    Download {
        /// download path directory
        #[arg(short, long, required = true)]
        local_path: String,
    },
}

static ALIST_URL: Lazy<String> = Lazy::new(|| {
    // This closure runs the first time SERVER_ADDRESS is accessed.
    Cli::parse().server_address
});

static THREADS_NUM: Lazy<usize> = Lazy::new(|| Cli::parse().threads);

async fn remove_noexist_files(
    local_path: String,
    url_path: String,
    existing_files: &HashSet<String>,
    delete: bool,
) -> Result<()> {
    // The realpath on the filesystem
    info!("Start to remove non-existent files");
    let folder_path = local_path.clone() + &url_path;

    trace!("folder_path {}", folder_path);
    let iter = WalkDir::new(&folder_path)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file()) // Only keep files
        .filter(|entry| {
            // Keep only items whose file name is NOT in `existing_files`
            // (i.e., we want to remove them because they're "non-existent" remotely)
            let file_path = entry.path();
            let remote_path = match file_path.strip_prefix(local_path.clone()) {
                Result::Ok(rel_path) => format!("/{}", rel_path.to_string_lossy()),
                Err(_) => return true, // if strip_prefix fails, keep the file
            };
            !existing_files.contains(&remote_path)
        });

    for entry in iter {
        info!(
            "Found non-existent Entry {}",
            entry.path().to_string_lossy()
        );
        if delete {
            fs::remove_file(entry.path()).await?;
        }
    }

    for entry in WalkDir::new(&folder_path)
        .contents_first(true)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_dir())
    {
        let file_path = entry.path();
        if tokio::fs::remove_dir(file_path).await.is_ok() {
            info!("Removed empty directory: {}", file_path.display());
        }
    }

    Ok(())
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

    // Set up a new multi-progress bar.
    let _multibar = std::sync::Arc::new(indicatif::MultiProgress::new());

    match args.command {
        Commands::AutoSym { local_path, delete } => {
            let res = alist_api::get_path_structure(args.url_path.clone()).await?;
            let files_with_ext = alist_api::get_file_ext(&res).await;

            info!("Start to copy metadata");
            alist_api::copy_metadata(&files_with_ext, &local_path).await?;
            alist_api::create_strm_file(&files_with_ext, &local_path).await?;
            let files_set: HashSet<String> = res
                .into_iter()
                .map(|s| s.path_str)
                .filter_map(|file| {
                    let path = Path::new(&file);
                    // Check if the file extension is valid
                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        if alist_api::FILE_STRM.contains(&ext) {
                            // Replace the file's extension with "strm"
                            return path.with_extension("strm").to_str().map(String::from);
                        }
                    }
                    Some(file)
                })
                .collect();

            remove_noexist_files(local_path, args.url_path, &files_set, delete).await?;
        }
        Commands::Download { local_path } => {
            download::download_folders(args.url_path, &local_path).await?;
        }
    }

    Ok(())
}
