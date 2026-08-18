use serde_json::Value;

pub struct StHeader {
    pub metadata: serde_json::Map<String, Value>,
    pub param_from_shapes: Option<f64>,
}

pub fn read(bytes: &[u8]) -> Result<StHeader, String> {
    if bytes.len() < 8 {
        return Err("safetensors: truncated".to_string());
    }
    let len = u64::from_le_bytes(bytes[..8].try_into().unwrap()) as usize;
    if len < 2 || len > 8 * 1024 * 1024 {
        return Err("safetensors: bad header length".to_string());
    }
    let end = 8usize.saturating_add(len);
    if end > bytes.len() {
        return Err("safetensors: header truncated".to_string());
    }
    let json: Value = serde_json::from_slice(&bytes[8..end])
        .map_err(|_| "safetensors: bad header json".to_string())?;
    let obj = json.as_object().ok_or("safetensors: header not an object")?;
    let metadata = match obj.get("__metadata__") {
        Some(Value::Object(m)) => m.clone(),
        _ => serde_json::Map::new(),
    };
    let mut total = 0.0f64;
    for (k, v) in obj {
        if k == "__metadata__" {
            continue;
        }
        if let Some(shapes) = v.as_object().and_then(|o| o.get("shape")).and_then(|s| s.as_array()) {
            let mut elems = 1.0f64;
            for d in shapes {
                if let Some(d) = d.as_u64() {
                    elems *= d as f64;
                }
            }
            total += elems;
        }
    }
    let param_from_shapes = if total > 0.0 { Some(total) } else { None };
    Ok(StHeader { metadata, param_from_shapes })
}