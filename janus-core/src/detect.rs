use crate::ev::Format;
use std::fs::File;
use std::io::Read;
use std::path::Path;

pub const MAX_HEAD_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_ST_HEADER_BYTES: u64 = 8 * 1024 * 1024;

pub fn detect(path: &Path) -> std::io::Result<Format> {
    let mut f = File::open(path)?;
    let mut head = Vec::with_capacity(64);
    head.resize(64, 0u8);
    let n = read_up_to(&mut f, &mut head, 64)?;
    let head = &head[..n];

    if head.len() >= 4 && &head[..4] == b"GGUF" {
        return Ok(Format::Gguf);
    }
    if head.len() >= 9 {
        let len = u64::from_le_bytes(head[..8].try_into().unwrap());
        if len >= 2 && len <= MAX_ST_HEADER_BYTES && head[8] == b'{' {
            return Ok(Format::Safetensors);
        }
    }
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if name.eq_ignore_ascii_case("model_index.json") {
        return Ok(Format::Diffusers);
    }
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    match ext.as_str() {
        "onnx" => return Ok(Format::Onnx),
        "pt" | "pth" | "bin" => return Ok(Format::Pytorch),
        _ => {}
    }
    Ok(Format::Unknown)
}

pub fn read_head(path: &Path, cap: usize) -> std::io::Result<Vec<u8>> {
    let mut f = File::open(path)?;
    let mut buf = Vec::with_capacity(cap);
    buf.resize(cap, 0u8);
    let n = read_up_to(&mut f, &mut buf, cap)?;
    buf.truncate(n);
    Ok(buf)
}

fn read_up_to(r: &mut impl Read, buf: &mut [u8], cap: usize) -> std::io::Result<usize> {
    let want = cap.min(buf.len());
    let mut total = 0usize;
    while total < want {
        let n = r.read(&mut buf[total..want])?;
        if n == 0 {
            break;
        }
        total += n;
    }
    Ok(total)
}