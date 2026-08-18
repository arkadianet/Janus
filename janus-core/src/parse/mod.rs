pub mod gguf;
pub mod safetensors;

use crate::ev::{Field, Format, Kind, Level};
use std::io::Read;
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
            let need = 8 + len.min(cap);
            let mut buf = vec![0u8; need];
            let mut f = std::fs::File::open(path).ok()?;
            f.read_exact(&mut buf).ok()?;
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

fn parse_gguf(bytes: &[u8]) -> Parsed {
    match gguf::read(bytes) {
        Ok(kv) => {
            let txt = |v: &gguf::GgufValue| v.as_str().map(|s| s.to_string());
            let arch = kv.get("general.architecture").and_then(txt).map(|s| known(s));
            let params_total = kv.get("__janus_params_total").and_then(|v| v.as_float()).map(known);
            let params_active = kv.get("__janus_params_active").and_then(|v| v.as_float()).map(known);
            let context_len = kv.get("llama.context_length").and_then(|v| v.as_uint()).map(|u| known(u as i64));
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
            let arch = hdr
                .metadata
                .get("architectures")
                .and_then(|v| v.as_array())
                .and_then(|a| a.first())
                .and_then(|v| v.as_str())
                .map(|s| known(s.to_string()));
            let params_total = hdr.param_from_shapes.map(known);
            let kind = arch.as_ref().map(|_| Field { value: Kind::Llm, level: Level::Known });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gguf_roundtrip_from_scratch() {
        let mut b: Vec<u8> = Vec::new();
        b.extend_from_slice(b"GGUF");
        b.extend_from_slice(&1u32.to_le_bytes());
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
}