use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use xxhash_rust::xxh3;

pub fn ollama_named_sha256(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    let rest = name.strip_prefix("sha256-")?;
    if rest.len() == 64 && rest.bytes().all(|c| c.is_ascii_hexdigit()) {
        Some(rest.to_ascii_lowercase())
    } else {
        None
    }
}

pub const PARTIAL_BYTES: usize = 64 * 1024;

pub fn full_hash(path: &Path) -> std::io::Result<(String, String, u64, u64)> {
    let mut f = File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut sha = Sha256::new();
    let mut size = 0u64;
    // Heap: a 1 MiB stack array overflows the default Windows thread stack.
    let mut buf = vec![0u8; 1024 * 1024];
    let mut head = Vec::with_capacity(PARTIAL_BYTES);
    let mut tail = Vec::with_capacity(PARTIAL_BYTES * 2);
    let mut small = Vec::new();
    let mut keep_small = true;
    loop {
        let n = match f.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        size += n as u64;
        hasher.update(&buf[..n]);
        sha.update(&buf[..n]);
        if keep_small {
            small.extend_from_slice(&buf[..n]);
            if small.len() > PARTIAL_BYTES * 2 {
                keep_small = false;
                small.clear();
            }
        }
        if head.len() < PARTIAL_BYTES {
            let take = (PARTIAL_BYTES - head.len()).min(n);
            head.extend_from_slice(&buf[..take]);
        }
        tail.extend_from_slice(&buf[..n]);
        if tail.len() > PARTIAL_BYTES {
            tail.drain(0..tail.len() - PARTIAL_BYTES);
        }
    }
    let partial = if keep_small {
        xxh3::xxh3_64(&small)
    } else {
        let mut both = Vec::with_capacity(PARTIAL_BYTES * 2);
        both.extend_from_slice(&head);
        both.extend_from_slice(&tail);
        xxh3::xxh3_64(&both)
    };
    let b3 = hasher.finalize().to_hex().to_string();
    let s256 = hex::encode(sha.finalize());
    Ok((b3, s256, size, partial))
}

pub fn partial_hash(path: &Path) -> std::io::Result<(u64, u64)> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn ollama_blob_name_is_trusted_hex() {
        let p = Path::new("blobs/sha256-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        assert_eq!(
            ollama_named_sha256(p).as_deref(),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
        assert!(ollama_named_sha256(Path::new("model.gguf")).is_none());
        assert!(ollama_named_sha256(Path::new("sha256-short")).is_none());
    }
}
