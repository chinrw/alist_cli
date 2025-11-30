//! Alist CLI library
//!
//! This library provides functionality for interacting with Alist servers,
//! including file downloads, directory structure traversal, and metadata
//! management.

pub mod api;
pub mod download;
pub mod tracing_bridge;
pub mod utils;

pub use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

#[derive(Debug)]
pub struct Config {
    pub server_address: String,
    pub threads: usize,
    pub token: String,
    pub tpslimit: u32,
    pub concurrent_limit: usize,
    pub timeout: u64,
}

impl Config {
    /// Returns default config for testing/library usage
    pub fn default_test_config() -> Self {
        Self {
            server_address: "http://localhost:5244".to_string(),
            threads: 4,
            token: String::new(),
            tpslimit: u32::MAX,
            concurrent_limit: 4,
            timeout: 10,
        }
    }
}

pub static CONFIG: OnceLock<Config> = OnceLock::new();

/// Helper to get CONFIG reference, initializing with test defaults if not
/// already set
pub fn get_config() -> &'static Config {
    CONFIG.get_or_init(Config::default_test_config)
}
