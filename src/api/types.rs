//! Data types and structures for Alist API communication.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// File extensions that should be converted to .strm files
pub const FILE_STRM: [&str; 14] = [
    "mkv", "iso", "ts", "mp4", "avi", "rmvb", "wmv", "m2ts", "mpg", "flv", "rm", "mov", "wav",
    "mp3",
];

/// Metadata file extensions to copy alongside media files
const META_SUFF: [&str; 9] = [
    "nfo", "jpg", "png", "svg", "ass", "srt", "sup", "vtt", "txt",
];

/// Hash information for file verification
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum HashObject {
    Sha1 { sha1: String },
    Md5 { md5: String },
}

impl HashObject {
    /// Returns the inner hash string (either SHA1 or MD5) in lowercase.
    ///
    /// # Returns
    ///
    /// The hash string in lowercase format for consistent comparison.
    pub fn as_hash_str(&self) -> String {
        match self {
            HashObject::Sha1 { sha1 } => sha1.to_lowercase(),
            HashObject::Md5 { md5 } => md5.to_lowercase(),
        }
    }
}

/// Information about a single file or directory entry
#[derive(Serialize, Deserialize, Debug)]
pub struct FileInfo {
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
    pub modified: String,
    pub sign: String,
    pub thumb: String,
    #[serde(rename = "type")]
    pub file_type: u32,
    pub created: Option<String>,
    pub hashinfo: Option<String>,
    pub hash_info: Option<HashObject>,
    pub raw_url: String,
    pub readme: String,
    pub header: String,
    pub provider: String,
    pub related: Option<Value>,
}

/// Request payload for file information API calls
#[derive(Serialize, Deserialize, Debug)]
pub struct FileInfoRequest {
    pub path: String,
    pub password: String,
    pub page: u32,
    pub per_page: u32,
    pub refresh: bool,
}

/// Entry information used in directory listings
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EntryInfo {
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
    pub modified: String,
    pub sign: String,
    pub thumb: String,
    #[serde(rename = "type")]
    pub file_type: u32,
    pub created: Option<String>,
    pub hashinfo: Option<String>,
    pub hash_info: Option<HashObject>,
}

/// Information about a folder and its contents
#[derive(Serialize, Deserialize, Debug)]
pub struct FoldersInfo {
    pub content: Option<Vec<EntryInfo>>,
    pub total: u32,
    pub readme: String,
    pub write: bool,
    pub provider: String,
    pub header: String,
}

/// API response data that can be either file info or folder info
#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub enum ApiData {
    FileInfo(Box<FileInfo>),
    FoldersInfo(FoldersInfo),
}

/// Standard API response structure from Alist server
#[derive(Serialize, Deserialize, Debug)]
pub struct ApiResponse {
    pub code: u32,
    pub message: String,
    pub data: Option<ApiData>,
}

/// Entry combined with its full path information
#[derive(Debug, Clone)]
pub struct EntryWithPath {
    pub entry: EntryInfo,
    pub path_str: String,
    pub provider: String,
}

/// Checks if metadata should be copied based on file extension
///
/// # Arguments
///
/// * `extension` - The file extension to check
///
/// # Returns
///
/// `true` if the extension is in the metadata suffix list
pub fn is_metadata_file(extension: &str) -> bool {
    META_SUFF.contains(&extension)
}

/// Checks if a file should be converted to .strm format
///
/// # Arguments
///
/// * `extension` - The file extension to check
///
/// # Returns
///
/// `true` if the extension is in the STRM file list
pub fn is_streamable_file(extension: &str) -> bool {
    FILE_STRM.contains(&extension)
}
