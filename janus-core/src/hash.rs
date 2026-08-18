use blake3::Hasher;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use xxhash_rust::xxh3;

pub const PARTIAL_BYTES: usize = 64 * 1024;

pub fn full_hash(path: &Path) -> std::io::Result<(String, String, u64)> {
    let mut f = File::open(path)?;
    let size = f.metadata()?.len();
    let mut hasher = blake3::Hasher::new();
    let mut sha = Sha256::new();
    let mut buf = [0u8; 1024 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        sha.update(&buf[..n]);
    }
    let b3 = hasher.finalize().to_hex().to_string();
    let s256 = hex::encode(sha.finalize());
    Ok((b3, s256, size))
}

pub fn partial_hash(path: &Path) -> std::io::Result<(u64, u64)> {
    let _ = Hasher::new();
    let f = File::open(path)?;
    let size = f.metadata()?.len();
    if size <= (PARTIAL_BYTES * 2) as u64 {
        let mut buf = Vec::with_capacity(size as usize);
        let mut f = File::open(path)?;
        f.read_to_end(&mut buf)?;
        return Ok((xxh3::xxh3_64(&buf), 0));
    }
    let mut head = vec![0u8; PARTIAL_BYTES];
    let mut tail = vec![0u8; PARTIAL_BYTES];
    let mut f = File::open(path)?;
    f.read_exact(&mut head)?;
    f.seek(SeekFrom::End(-(PARTIAL_BYTES as i64)))?;
    f.read_exact(&mut tail)?;
    let mut both = Vec::with_capacity(PARTIAL_BYTES * 2);
    both.extend(head);
    both.extend(tail);
    Ok((xxh3::xxh3_64(&both), size))
}