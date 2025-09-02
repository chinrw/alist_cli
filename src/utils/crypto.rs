//! Cryptographic utilities for file verification and hashing.

use std::{fmt::Write, path::Path};

use anyhow::Result;
use digest::{Digest, OutputSizeUser, generic_array::ArrayLength};
use indicatif::{MultiProgress, ProgressBar, ProgressState, ProgressStyle};
use md5::Md5;
use sha1::Sha1;
use tokio::{
    fs::File,
    io::{AsyncReadExt, BufReader},
};
use tracing::{Level, enabled};

use crate::api::types::HashObject;

impl HashObject {
    /// Computes a hash progress bar for the given file.
    ///
    /// # Arguments
    ///
    /// * `reader` - Buffered file reader
    /// * `file_size` - Total size of the file for progress tracking
    /// * `local_path` - Path to the file being hashed
    /// * `m_pb` - Multi-progress bar for UI feedback
    ///
    /// # Returns
    ///
    /// The computed hash as a hexadecimal string
    ///
    /// # Errors
    ///
    /// Returns an error if file reading fails
    async fn hash_process_bar<D: Digest + Default>(
        mut reader: BufReader<File>,
        file_size: u64,
        local_path: &Path,
        m_pb: MultiProgress,
    ) -> Result<String>
    where
        <D as OutputSizeUser>::OutputSize: Add,
        <<D as OutputSizeUser>::OutputSize as Add>::Output: ArrayLength<u8>,
    {
        // Check if we're in verbose log mode if true print hash progress bar
        let is_verbose_logging = enabled!(Level::DEBUG);

        let pb = if is_verbose_logging {
            let pb = m_pb.insert_from_back(1, ProgressBar::new(file_size));
            pb.set_style(
                ProgressStyle::with_template(
                    "{spinner:.green} {msg} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({eta})",
                )
                .unwrap()
                .with_key("eta", |state: &ProgressState, w: &mut dyn Write| {
                    write!(w, "{:.1}s", state.eta().as_secs_f64()).unwrap()
                })
                .progress_chars("#>-"),
            );
            pb
        } else {
            // Create a hidden/dummy progress bar when in debug mode
            let pb = m_pb.insert_from_back(1, ProgressBar::new(file_size));
            pb.set_style(ProgressStyle::default_bar());
            pb.set_draw_target(indicatif::ProgressDrawTarget::hidden());
            pb
        };

        let mut total_read = 0;

        let mut buffer = [0u8; 8192];
        let mut hasher = D::new();
        // Read the file in chunks
        loop {
            let n = reader.read(&mut buffer).await?;
            if n == 0 {
                break; // EOF reached
            }
            hasher.update(&buffer[..n]);
            total_read += n as u64;
            pb.set_position(total_read);
        }
        pb.finish_with_message(format!("Done hashing {}", local_path.display()));
        Ok(format!("{:x}", hasher.finalize()))
    }

    /// Computes the file checksum for this hash object type.
    ///
    /// # Arguments
    ///
    /// * `local_path` - Path to the file to hash
    /// * `m_pb` - Multi-progress bar for UI feedback
    ///
    /// # Returns
    ///
    /// The computed hash as a string
    ///
    /// # Errors
    ///
    /// Returns an error if file operations fail
    pub async fn compute_file_checksum(
        &self,
        local_path: &Path,
        m_pb: MultiProgress,
    ) -> Result<String> {
        let file = File::open(local_path).await?;
        let file_size = file.metadata().await?.len();
        let reader = BufReader::new(file);

        match self {
            HashObject::Sha1 { .. } => {
                Self::hash_process_bar::<Sha1>(reader, file_size, local_path, m_pb).await
            }
            HashObject::Md5 { .. } => {
                Self::hash_process_bar::<Md5>(reader, file_size, local_path, m_pb).await
            }
        }
    }

    /// Verifies that a local file matches the expected checksum.
    ///
    /// # Arguments
    ///
    /// * `local_path` - Path to the file to verify
    /// * `m_pb` - Multi-progress bar for UI feedback
    ///
    /// # Returns
    ///
    /// `true` if the file exists and matches the expected hash, `false`
    /// otherwise
    ///
    /// # Errors
    ///
    /// Returns an error if file operations fail
    pub async fn verify_file_checksum(
        &self,
        local_path: &Path,
        m_pb: MultiProgress,
    ) -> Result<bool> {
        let mut res = false;
        // Check if the local file exists
        if Path::new(&local_path).exists() {
            let computed = self.compute_file_checksum(local_path, m_pb).await?;
            tracing::debug!(
                "local checksum: {} remote file checksum: {}",
                computed,
                self.as_hash_str()
            );
            res = computed == self.as_hash_str();
        }
        Ok(res)
    }
}

/// Encrypts MD5 sum using a proprietary algorithm.
///
/// This function is used by BAIDU_NETDISK to encrypt MD5 sum results to match
/// the MD5 from provider. However, this may not provide the correct MD5.
///
/// # Arguments
///
/// * `md5str` - The original MD5 string to encrypt
///
/// # Returns
///
/// The encrypted MD5 string
///
/// # Panics
///
/// Panics if the MD5 string is malformed or contains invalid hex characters
pub fn _encrypt_md5(md5str: &str) -> String {
    // 1) Rearrange the string: [8..16] + [0..8] + [24..32] + [16..24]
    let rearranged = format!(
        "{}{}{}{}",
        &md5str[8..16],
        &md5str[0..8],
        &md5str[24..32],
        &md5str[16..24],
    );

    // 2) Build `encryptstr`: for each char, parse as hex digit, XOR with (15 &
    //    index), format as hex
    let mut encryptstr: String =
        rearranged
            .chars()
            .enumerate()
            .fold(String::new(), |mut output, (i, ch)| {
                let val = ch
                    .to_digit(16)
                    .expect("Character in rearranged MD5 string wasn't valid hex.");
                let _ = write!(output, "{:x}", val ^ (15 & i as u32));
                output
            });

    // 3) Modify the 10th character (index 9): encryptstr[9] => 'g' + hexDigit
    let val_9 = encryptstr
        .chars()
        .nth(9)
        .expect("encryptstr shorter than expected")
        .to_digit(16)
        .expect("Character at index 9 wasn't valid hex.");
    let new_char = std::char::from_u32(('g' as u32) + val_9)
        .expect("Adding offset to 'g' went out of valid Unicode range.");
    encryptstr.replace_range(9..10, &new_char.to_string());

    encryptstr
}

// Fix the missing import
use std::ops::Add;
