use hex::encode;
use sha1::{Digest, Sha1};
use std::path::Path;

pub fn compute_file_hash<P: AsRef<Path>>(path: P) -> String {
    let file_buf = std::fs::read(path).unwrap();
    let mut hasher = Sha1::new();
    hasher.update(file_buf);
    let result = hasher.finalize();
    let hex_string = encode(result);
    hex_string
}

pub fn compute_slice_hash(slice: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(slice);
    let result = hasher.finalize();
    let hex_string = encode(result);
    hex_string
}
