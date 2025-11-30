use alist_cli::*;

use anyhow::Result;
use clap::Parser;
use indicatif::MultiProgress;
use tokio::fs;
use tracing::{info, trace};
use tracing_bridge::MakeSuspendingWriter;
use tracing_subscriber::{EnvFilter, Registry, fmt, layer::SubscriberExt};
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

    /// alist token
    #[arg(short = 't', long, global = true, default_value = "")]
    token: String,

    /// Limit HTTP transactions per second to this
    #[arg(
        long,
        global = true,
        default_value_t = u32::MAX,
        allow_negative_numbers(false)
    )]
    tpslimit: u32,

    /// Request timeout in seconds
    #[arg(long, global = true, default_value_t = 10)]
    timeout: u64,

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

async fn remove_noexist_files(
    local_path: String,
    url_path: String,
    existing_files: &HashSet<String>,
    delete: bool,
) -> Result<()> {
    // The realpath on the filesystem
    info!("Start to remove non-existent files");
    let folder_path = std::path::Path::new(&local_path).join(url_path.trim_start_matches('/'));

    trace!("folder_path {}", folder_path.display());
    let iter = WalkDir::new(&folder_path)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file()) // Only keep files
        .filter(|entry| {
            // Keep only items whose file name is NOT in `existing_files`
            // (i.e., we want to remove them because they're "non-existent" remotely)
            let file_path = entry.path();
            let remote_path = match file_path.strip_prefix(&local_path) {
                Ok(rel_path) => format!("/{}", rel_path.to_string_lossy()),
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
    // Parse CLI arguments and initialize global CONFIG
    let args = Cli::parse();
    CONFIG
        .set(Config {
            server_address: args.server_address.clone(),
            threads: args.threads,
            token: args.token.clone(),
            tpslimit: args.tpslimit,
            concurrent_limit: std::cmp::max(args.threads, 10), // Min 10 for buffer_unordered operations
            timeout: args.timeout,
        })
        .expect("CONFIG already initialized");

    let m_pb = MultiProgress::new();
    // let wrapper = tracing_bridge::TracingWrapper::new(m_pb.clone());

    let make_writer = MakeSuspendingWriter::new(std::io::stdout, m_pb.clone());
    let fmt_layer = fmt::layer()
        .with_writer(make_writer)
        .with_ansi(true)
        .with_file(true)
        .with_line_number(true);

    // Set up the tracing subscriber
    let subscriber = Registry::default()
        .with(fmt_layer)
        .with(EnvFilter::from_default_env());

    tracing::subscriber::set_global_default(subscriber)?;

    match args.command {
        Commands::AutoSym { local_path, delete } => {
            let client = std::sync::Arc::new(reqwest::Client::builder().no_proxy().build()?);
            let res =
                api::get_path_structure(args.url_path.clone(), m_pb.clone(), Arc::clone(&client))
                    .await?;

            // Single pass: collect files with extensions AND build the final files_set
            let mut files_with_ext: Vec<(String, &api::EntryWithPath)> = Vec::new();
            let mut files_set = HashSet::with_capacity(res.len());

            for entry in &res {
                if entry.entry.is_dir {
                    continue;
                }

                let path = Path::new(&entry.path_str);

                // Extract extension and add to files_with_ext if present
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    files_with_ext.push((ext.to_owned(), entry));

                    // Build files_set: replace extension with "strm" if streamable, otherwise keep
                    // original
                    let final_path = if api::FILE_STRM.contains(&ext) {
                        path.with_extension("strm").to_string_lossy().into_owned()
                    } else {
                        entry.path_str.clone()
                    };
                    files_set.insert(final_path);
                } else {
                    // No extension, just add the path as-is
                    files_set.insert(entry.path_str.clone());
                }
            }

            api::copy_metadata(
                &files_with_ext,
                &local_path,
                m_pb.clone(),
                Arc::clone(&client),
            )
            .await?;
            api::create_strm_file(&files_with_ext, &local_path, m_pb, client).await?;

            remove_noexist_files(local_path, args.url_path, &files_set, delete).await?;
        }
        Commands::Download { local_path } => {
            download::download_folders(args.url_path, &local_path, m_pb).await?;
        }
    }

    Ok(())
}
