pub mod audit;
pub mod catalog;
pub mod fingerprint;
pub mod pack;
pub mod repo;
pub mod signing;
pub mod trust;
pub mod validate;

use std::fmt::Write as _;

use sha2::{Digest, Sha256};

pub fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}
