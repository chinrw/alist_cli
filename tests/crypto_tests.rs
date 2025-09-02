//! Tests for cryptographic utilities.

use alist_cli::utils::crypto::_encrypt_md5;

#[test]
fn test_encrypt_md5() {
    let original_md5 = "d41d8cd98f00b204e9800998ecf8427e";
    let encrypted = _encrypt_md5(original_md5);

    // The encrypted string should have the same length as the original
    assert_eq!(encrypted.len(), original_md5.len());

    // The encrypted string should be different from the original
    assert_ne!(encrypted, original_md5);

    // Test that it consistently produces the same result
    let encrypted2 = _encrypt_md5(original_md5);
    assert_eq!(encrypted, encrypted2);
}

#[test]
#[should_panic(expected = "Character in rearranged MD5 string wasn't valid hex")]
fn test_encrypt_md5_invalid_hex() {
    let invalid_md5 = "x41d8cd98f00b204e9800998ecf8427e";
    _encrypt_md5(invalid_md5);
}

#[test]
#[should_panic]
fn test_encrypt_md5_wrong_length() {
    let short_md5 = "d41d8cd98f00b204";
    _encrypt_md5(short_md5);
}
