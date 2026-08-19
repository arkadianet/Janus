/// Shallow ONNX protobuf walk: producer, ir, opset, graph I/O names.
/// Not a full ModelProto decoder.

#[derive(Debug, Clone, Default)]
pub struct OnnxInfo {
    pub producer: Option<String>,
    pub ir_version: Option<i64>,
    pub opset: Option<i64>,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}

pub fn read(bytes: &[u8]) -> Result<OnnxInfo, String> {
    if bytes.is_empty() {
        return Err("onnx: empty".into());
    }
    let mut info = OnnxInfo::default();
    walk_message(bytes, 0, bytes.len(), 0, &mut info)?;
    Ok(info)
}

fn walk_message(buf: &[u8], mut i: usize, end: usize, depth: u32, info: &mut OnnxInfo) -> Result<(), String> {
    if depth > 6 {
        return Ok(());
    }
    while i < end {
        let (tag, n) = read_varint(buf, i)?;
        i = n;
        let field = (tag >> 3) as u32;
        let wire = (tag & 7) as u32;
        match wire {
            0 => {
                let (v, n) = read_varint(buf, i)?;
                i = n;
                if depth == 0 && field == 1 {
                    info.ir_version = Some(v as i64);
                }
                if depth == 1 && field == 3 {
                    info.opset = Some(v as i64);
                }
            }
            1 => {
                i = i.checked_add(8).ok_or("onnx: truncated")?;
                if i > end {
                    return Err("onnx: truncated".into());
                }
            }
            2 => {
                let (len, n) = read_varint(buf, i)?;
                i = n;
                let len = len as usize;
                let next = i.checked_add(len).ok_or("onnx: truncated")?;
                if next > end {
                    return Err("onnx: truncated".into());
                }
                let inner = &buf[i..next];
                if depth == 0 && field == 2 {
                    info.producer = as_utf8(inner);
                }
                if depth == 0 && field == 7 {
                    walk_graph(inner, info)?;
                }
                if depth == 0 && field == 8 {
                    walk_message(inner, 0, inner.len(), depth + 1, info)?;
                }
                i = next;
            }
            5 => {
                i = i.checked_add(4).ok_or("onnx: truncated")?;
                if i > end {
                    return Err("onnx: truncated".into());
                }
            }
            _ => return Err("onnx: bad wire type".into()),
        }
    }
    Ok(())
}

fn walk_graph(buf: &[u8], info: &mut OnnxInfo) -> Result<(), String> {
    let mut i = 0usize;
    let end = buf.len();
    while i < end {
        let (tag, n) = read_varint(buf, i)?;
        i = n;
        let field = (tag >> 3) as u32;
        let wire = (tag & 7) as u32;
        match wire {
            0 => {
                let (_, n) = read_varint(buf, i)?;
                i = n;
            }
            1 => i = i.saturating_add(8),
            2 => {
                let (len, n) = read_varint(buf, i)?;
                i = n;
                let len = len as usize;
                let next = i.saturating_add(len);
                if next > end {
                    return Err("onnx: graph truncated".into());
                }
                let inner = &buf[i..next];
                if field == 11 {
                    if let Some(name) = value_info_name(inner) {
                        info.inputs.push(name);
                    }
                } else if field == 12 {
                    if let Some(name) = value_info_name(inner) {
                        info.outputs.push(name);
                    }
                }
                i = next;
            }
            5 => i = i.saturating_add(4),
            _ => return Err("onnx: bad graph wire".into()),
        }
    }
    Ok(())
}

fn value_info_name(buf: &[u8]) -> Option<String> {
    let mut i = 0usize;
    while i < buf.len() {
        let (tag, n) = read_varint(buf, i).ok()?;
        i = n;
        let field = (tag >> 3) as u32;
        let wire = (tag & 7) as u32;
        if wire == 2 {
            let (len, n) = read_varint(buf, i).ok()?;
            i = n;
            let len = len as usize;
            let next = i.saturating_add(len);
            if next > buf.len() {
                return None;
            }
            if field == 1 {
                return as_utf8(&buf[i..next]);
            }
            i = next;
        } else if wire == 0 {
            let (_, n) = read_varint(buf, i).ok()?;
            i = n;
        } else if wire == 1 {
            i = i.saturating_add(8);
        } else if wire == 5 {
            i = i.saturating_add(4);
        } else {
            return None;
        }
    }
    None
}

fn as_utf8(b: &[u8]) -> Option<String> {
    let s = std::str::from_utf8(b).ok()?.trim();
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

fn read_varint(buf: &[u8], mut i: usize) -> Result<(u64, usize), String> {
    let mut x = 0u64;
    let mut shift = 0;
    loop {
        if i >= buf.len() {
            return Err("onnx: truncated varint".into());
        }
        let b = buf[i];
        i += 1;
        x |= ((b & 0x7f) as u64) << shift;
        if b & 0x80 == 0 {
            return Ok((x, i));
        }
        shift += 7;
        if shift > 63 {
            return Err("onnx: varint overflow".into());
        }
    }
}

pub fn encode_varint(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let mut b = (v & 0x7f) as u8;
        v >>= 7;
        if v != 0 {
            b |= 0x80;
        }
        out.push(b);
        if v == 0 {
            break;
        }
    }
}

pub fn encode_tag(out: &mut Vec<u8>, field: u32, wire: u32) {
    encode_varint(out, ((field as u64) << 3) | wire as u64);
}

pub fn encode_string(out: &mut Vec<u8>, field: u32, s: &str) {
    encode_tag(out, field, 2);
    encode_varint(out, s.len() as u64);
    out.extend_from_slice(s.as_bytes());
}

pub fn encode_bytes(out: &mut Vec<u8>, field: u32, data: &[u8]) {
    encode_tag(out, field, 2);
    encode_varint(out, data.len() as u64);
    out.extend_from_slice(data);
}

pub fn encode_varint_field(out: &mut Vec<u8>, field: u32, v: u64) {
    encode_tag(out, field, 0);
    encode_varint(out, v);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_producer_and_io() {
        let mut graph = Vec::new();
        encode_string(&mut graph, 2, "main");
        let mut input = Vec::new();
        encode_string(&mut input, 1, "tokens");
        encode_bytes(&mut graph, 11, &input);
        let mut output = Vec::new();
        encode_string(&mut output, 1, "logits");
        encode_bytes(&mut graph, 12, &output);

        let mut model = Vec::new();
        encode_varint_field(&mut model, 1, 8);
        encode_string(&mut model, 2, "pytorch");
        encode_bytes(&mut model, 7, &graph);
        let mut opset = Vec::new();
        encode_varint_field(&mut opset, 3, 17);
        encode_bytes(&mut model, 8, &opset);

        let info = read(&model).unwrap();
        assert_eq!(info.producer.as_deref(), Some("pytorch"));
        assert_eq!(info.ir_version, Some(8));
        assert_eq!(info.opset, Some(17));
        assert_eq!(info.inputs, vec!["tokens"]);
        assert_eq!(info.outputs, vec!["logits"]);
    }
}
