pub mod config;
pub mod gguf;
pub mod onnx;
pub mod safetensors;

use crate::ev::{Field, Format, Kind, Level};
use std::io::{Read, Seek};
use std::path::Path;

pub struct Parsed {
    pub format: Format,
    pub general_name: Option<Field<String>>,
    pub basename: Option<Field<String>>,
    pub finetune: Option<Field<String>>,
    pub arch: Option<Field<String>>,
    pub params_total: Option<Field<f64>>,
    pub params_active: Option<Field<f64>>,
    pub context_len: Option<Field<i64>>,
    pub file_type: Option<Field<u32>>,
    pub quant_from_header: Option<Field<String>>,
    pub kind: Option<Field<Kind>>,
    pub parse_error: Option<String>,
}

fn no_facts(format: Format, err: Option<String>) -> Parsed {
    Parsed {
        format,
        general_name: None,
        basename: None,
        finetune: None,
        arch: None,
        params_total: None,
        params_active: None,
        context_len: None,
        file_type: None,
        quant_from_header: None,
        kind: None,
        parse_error: err,
    }
}

fn known<T>(value: T) -> Field<T> {
    Field { value, level: Level::Known }
}

pub fn parse_prefix(path: &Path, format: &Format, cap: usize) -> Parsed {
    let bytes = read_prefix(path, format, cap);
    match bytes {
        Some(b) => parse_bytes(&b, format),
        None => no_facts(format.clone(), Some("header_too_large".to_string())),
    }
}

fn read_prefix(path: &Path, format: &Format, cap: usize) -> Option<Vec<u8>> {
    let mut f = std::fs::File::open(path).ok()?;
    match format {
        Format::Gguf => {
            let size = f.metadata().ok()?.len().min(cap as u64) as usize;
            let mut buf = vec![0u8; size];
            f.read_exact(&mut buf).ok()?;
            Some(buf)
        }
        Format::Safetensors => {
            let mut head8 = [0u8; 8];
            f.read_exact(&mut head8).ok()?;
            let len = u64::from_le_bytes(head8) as usize;
            let limit = crate::detect::MAX_ST_HEADER_BYTES as usize;
            if len > limit {
                return None;
            }
            let need = 8usize.checked_add(len)?;
            let mut buf = vec![0u8; need];
            f.rewind().ok()?;
            f.read_exact(&mut buf).ok()?;
            Some(buf)
        }
        Format::Onnx | Format::Diffusers => {
            let mut buf = Vec::new();
            f.take(cap as u64).read_to_end(&mut buf).ok()?;
            Some(buf)
        }
        _ => {
            let mut buf = Vec::new();
            f.take(cap as u64).read_to_end(&mut buf).ok()?;
            Some(buf)
        }
    }
}

pub fn parse_bytes(bytes: &[u8], format: &Format) -> Parsed {
    match format {
        Format::Gguf => parse_gguf(bytes),
        Format::Safetensors => parse_safetensors(bytes),
        Format::Onnx => parse_onnx(bytes),
        Format::Diffusers => parse_diffusers(bytes),
        Format::Pytorch => no_facts(Format::Pytorch, Some("pickle_refused".to_string())),
        _ => Parsed {
            format: format.clone(),
            general_name: None,
            basename: None,
            finetune: None,
            arch: None,
            params_total: None,
            params_active: None,
            context_len: None,
            file_type: None,
            quant_from_header: None,
            kind: Some(Field { value: Kind::Unknown, level: Level::Detected }),
            parse_error: None,
        },
    }
}

fn parse_onnx(bytes: &[u8]) -> Parsed {
    match onnx::read(bytes) {
        Ok(info) => {
            let name = info.producer.clone();
            Parsed {
                format: Format::Onnx,
                general_name: name.as_ref().map(|s| known(s.clone())),
                basename: None,
                finetune: None,
                arch: None,
                params_total: None,
                params_active: None,
                context_len: None,
                file_type: None,
                quant_from_header: None,
                kind: Some(Field { value: Kind::Unknown, level: Level::Detected }),
                parse_error: None,
            }
        }
        Err(e) => no_facts(Format::Onnx, Some(e)),
    }
}

fn parse_diffusers(bytes: &[u8]) -> Parsed {
    let v: serde_json::Value = match serde_json::from_slice(bytes) {
        Ok(v) => v,
        Err(_) => return no_facts(Format::Diffusers, Some("diffusers: bad json".into())),
    };
    let class = v.get("_class_name").and_then(|x| x.as_str()).map(|s| s.to_string());
    Parsed {
        format: Format::Diffusers,
        general_name: class.map(known),
        basename: None,
        finetune: None,
        arch: None,
        params_total: None,
        params_active: None,
        context_len: None,
        file_type: None,
        quant_from_header: None,
        kind: Some(Field { value: Kind::Diffusion, level: Level::Detected }),
        parse_error: None,
    }
}

pub fn apply_config(parsed: &mut Parsed, cfg: &config::ConfigJson) {
    if parsed.basename.is_none() {
        if let Some(n) = &cfg.name {
            parsed.basename = Some(Field::inferred(n.clone()));
        }
    }
    if parsed.arch.is_none() {
        if let Some(a) = cfg.arch.as_ref().or(cfg.model_type.as_ref()) {
            parsed.arch = Some(Field::inferred(a.clone()));
        }
    }
}

fn parse_gguf(bytes: &[u8]) -> Parsed {
    match gguf::read(bytes) {
        Ok(kv) => {
            let txt = |v: &gguf::GgufValue| v.as_str().map(|s| s.to_string());
            let arch = kv.get("general.architecture").and_then(txt).map(|s| known(s));
            let params_total = kv
                .get("__janus_params_total")
                .and_then(|v| v.as_float())
                .filter(|n| n.is_finite() && *n > 0.0)
                .map(|n| known(if n >= 1_000_000.0 { n / 1e9 } else { n }));
            let params_active = kv
                .get("__janus_params_active")
                .and_then(|v| v.as_float())
                .filter(|n| n.is_finite())
                .map(known);
            let context_len = arch
                .as_ref()
                .and_then(|a| kv.get(&format!("{}.context_length", a.value)))
                .or_else(|| kv.get("llama.context_length"))
                .and_then(|v| v.as_uint())
                .map(|u| known(u as i64));
            let file_type = kv.get("general.file_type").and_then(|v| v.as_uint()).map(|u| known(u as u32));
            let quant_from_header =
                file_type.as_ref().and_then(|f| gguf::ftype_to_quant(f.value)).map(|s| known(s.to_string()));
            let kind = arch.as_ref().map(|a| {
                if a.value.contains("clip") || a.value.contains("vision") {
                    Field::known(Kind::Vision)
                } else {
                    Field::known(Kind::Llm)
                }
            });
            Parsed {
                format: Format::Gguf,
                general_name: kv.get("general.name").and_then(txt).map(|s| known(s)),
                basename: kv.get("general.basename").and_then(txt).map(|s| known(s)),
                finetune: kv.get("general.finetune").and_then(txt).map(|s| known(s)),
                arch,
                params_total,
                params_active,
                context_len,
                file_type,
                quant_from_header,
                kind,
                parse_error: None,
            }
        }
        Err(e) => no_facts(Format::Gguf, Some(format!("gguf:{e}"))),
    }
}

fn parse_safetensors(bytes: &[u8]) -> Parsed {
    match safetensors::read(bytes) {
        Ok(hdr) => {
            if hdr.metadata.is_empty() && hdr.param_from_shapes.is_none() {
                return no_facts(Format::Safetensors, None);
            }
            let arch_raw = hdr.metadata.get("architectures").and_then(|v| {
                if let Some(s) = v.as_str() {
                    Some(s.to_string())
                } else if let Some(a) = v.as_array() {
                    a.first().and_then(|x| x.as_str()).map(|s| s.to_string())
                } else {
                    None
                }
            });
            let arch = arch_raw.as_ref().map(|s| known(s.clone()));
            let params_total = hdr
                .param_from_shapes
                .filter(|n| n.is_finite() && *n > 0.0)
                .map(|n| known(if n >= 1_000_000.0 { n / 1e9 } else { n }));
            let kind = match arch_raw.as_deref() {
                Some(s) if s.to_ascii_lowercase().contains("clip") || s.to_ascii_lowercase().contains("vision") => {
                    Some(Field::inferred(Kind::Vision))
                }
                Some(s) if looks_like_llm_arch(s) => Some(Field::inferred(Kind::Llm)),
                _ => None,
            };
            Parsed {
                format: Format::Safetensors,
                general_name: None,
                basename: None,
                finetune: None,
                arch,
                params_total,
                params_active: None,
                context_len: None,
                file_type: None,
                quant_from_header: None,
                kind,
                parse_error: None,
            }
        }
        Err(e) => no_facts(Format::Safetensors, Some(format!("safetensors:{e}"))),
    }
}

fn looks_like_llm_arch(s: &str) -> bool {
    let l = s.to_ascii_lowercase();
    l.contains("causallm")
        || l.contains("llama")
        || l.contains("qwen")
        || l.contains("mistral")
        || l.contains("gemma")
        || l.contains("phi")
        || l.contains("gpt")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gguf_roundtrip_from_scratch() {
        let mut b: Vec<u8> = Vec::new();
        b.extend_from_slice(b"GGUF");
        b.extend_from_slice(&3u32.to_le_bytes());
        b.extend_from_slice(&0u64.to_le_bytes());
        b.extend_from_slice(&2u64.to_le_bytes());
        b.extend_from_slice(&20u64.to_le_bytes());
        b.extend_from_slice(b"general.architecture");
        b.extend_from_slice(&8u32.to_le_bytes());
        b.extend_from_slice(&5u64.to_le_bytes());
        b.extend_from_slice(b"qwen3");
        b.extend_from_slice(&17u64.to_le_bytes());
        b.extend_from_slice(b"general.file_type");
        b.extend_from_slice(&4u32.to_le_bytes());
        b.extend_from_slice(&15u32.to_le_bytes());
        let p = parse_bytes(&b, &Format::Gguf);
        assert_eq!(p.parse_error, None);
        assert_eq!(p.arch.as_ref().map(|f| f.value.as_str()), Some("qwen3"));
        assert_eq!(p.quant_from_header.as_ref().map(|f| f.value.as_str()), Some("Q4_K_M"));
        assert_eq!(p.kind.as_ref().map(|k| k.value), Some(Kind::Llm));
    }

    #[test]
    fn pytorch_never_unpickles() {
        let p = parse_bytes(b"\x80\x02some-pickle", &Format::Pytorch);
        assert_eq!(p.parse_error.as_deref(), Some("pickle_refused"));
    }

    fn put_str(b: &mut Vec<u8>, s: &str) {
        b.extend_from_slice(&(s.len() as u64).to_le_bytes());
        b.extend_from_slice(s.as_bytes());
    }

    fn put_kv_str(b: &mut Vec<u8>, key: &str, val: &str) {
        put_str(b, key);
        b.extend_from_slice(&8u32.to_le_bytes());
        put_str(b, val);
    }

    fn put_kv_u32(b: &mut Vec<u8>, key: &str, val: u32) {
        put_str(b, key);
        b.extend_from_slice(&4u32.to_le_bytes());
        b.extend_from_slice(&val.to_le_bytes());
    }

    fn put_kv_u8_array(b: &mut Vec<u8>, key: &str, n: u64) {
        put_str(b, key);
        b.extend_from_slice(&9u32.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&n.to_le_bytes());
        b.resize(b.len() + n as usize, 0);
    }

    #[test]
    fn gguf_huge_tokenizer_array_still_returns_basename_arch() {
        let mut b: Vec<u8> = Vec::new();
        b.extend_from_slice(b"GGUF");
        b.extend_from_slice(&3u32.to_le_bytes());
        b.extend_from_slice(&0u64.to_le_bytes());
        b.extend_from_slice(&4u64.to_le_bytes());
        put_kv_u8_array(&mut b, "tokenizer.ggml.tokens", 8192);
        put_kv_str(&mut b, "general.architecture", "llama");
        put_kv_str(&mut b, "general.basename", "kimi-k2");
        put_kv_u32(&mut b, "general.file_type", 15);
        let p = parse_bytes(&b, &Format::Gguf);
        assert_eq!(p.parse_error, None, "skipable tokenizer array must not abort the header");
        assert_eq!(p.arch.as_ref().map(|f| f.value.as_str()), Some("llama"));
        assert_eq!(p.basename.as_ref().map(|f| f.value.as_str()), Some("kimi-k2"));
        assert_eq!(p.file_type.as_ref().map(|f| f.value), Some(15));
    }

    #[test]
    fn gguf_truncated_tokenizer_keeps_earlier_arch() {
        let mut b: Vec<u8> = Vec::new();
        b.extend_from_slice(b"GGUF");
        b.extend_from_slice(&3u32.to_le_bytes());
        b.extend_from_slice(&0u64.to_le_bytes());
        b.extend_from_slice(&3u64.to_le_bytes());
        put_kv_str(&mut b, "general.architecture", "qwen3");
        put_kv_str(&mut b, "general.basename", "qwen3-8b");
        put_kv_u8_array(&mut b, "tokenizer.ggml.tokens", 200_000);
        b.truncate(256);
        let p = parse_bytes(&b, &Format::Gguf);
        assert_eq!(p.parse_error, None, "truncated skipable array must keep prior keys");
        assert_eq!(p.arch.as_ref().map(|f| f.value.as_str()), Some("qwen3"));
        assert_eq!(p.basename.as_ref().map(|f| f.value.as_str()), Some("qwen3-8b"));
    }
}