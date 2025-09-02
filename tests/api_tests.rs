//! Tests for API functionality.

use alist_cli::api::types::{HashObject, is_metadata_file, is_streamable_file};

#[test]
fn test_hash_object_as_hash_str() {
    let sha1_hash = HashObject::Sha1 {
        sha1: "ABC123def".to_string(),
    };
    assert_eq!(sha1_hash.as_hash_str(), "abc123def");

    let md5_hash = HashObject::Md5 {
        md5: "DEF456ABC".to_string(),
    };
    assert_eq!(md5_hash.as_hash_str(), "def456abc");
}

#[test]
fn test_is_metadata_file() {
    assert!(is_metadata_file("nfo"));
    assert!(is_metadata_file("jpg"));
    assert!(is_metadata_file("png"));
    assert!(!is_metadata_file("mp4"));
    assert!(!is_metadata_file("unknown"));
}

#[test]
fn test_is_streamable_file() {
    assert!(is_streamable_file("mkv"));
    assert!(is_streamable_file("mp4"));
    assert!(is_streamable_file("avi"));
    assert!(!is_streamable_file("jpg"));
    assert!(!is_streamable_file("unknown"));
}

#[test]
fn test_file_extension_edge_cases() {
    // Test empty string
    assert!(!is_metadata_file(""));
    assert!(!is_streamable_file(""));

    // Test case sensitivity - should be case sensitive
    assert!(!is_metadata_file("JPG"));
    assert!(!is_streamable_file("MP4"));
}
