//! API client module for Alist server communication.
//!
//! This module provides the main interface for interacting with the Alist
//! server, including path structure retrieval, file operations, and metadata
//! handling.

pub mod client;
pub mod operations;
pub mod rate_limiter;
pub mod types;

pub use client::*;
pub use operations::*;
pub use types::*;
