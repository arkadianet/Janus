use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum GgufValue {
    U8(u8),
    I8(i8),
    U16(u16),
    I16(i16),
    U32(u32),
    I32(i32),
    F32(f32),
    Bool(bool),
    String(String),
    U64(u64),
    I64(i64),
    F64(f64),
    Array(Vec<GgufValue>),
}

impl GgufValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            GgufValue::String(s) => Some(s.as_str()),
            _ => None,
        }
    }
    pub fn as_uint(&self) -> Option<u64> {
        match self {
            GgufValue::U8(v) => Some(*v as u64),
            GgufValue::U16(v) => Some(*v as u64),
            GgufValue::U32(v) => Some(*v as u64),
            GgufValue::U64(v) => Some(*v),
            GgufValue::I8(v) if *v >= 0 => Some(*v as u64),
            GgufValue::I16(v) if *v >= 0 => Some(*v as u64),
            GgufValue::I32(v) if *v >= 0 => Some(*v as u64),
            GgufValue::I64(v) if *v >= 0 => Some(*v as u64),
            GgufValue::Bool(b) => Some(*b as u64),
            _ => None,
        }
    }
    pub fn as_float(&self) -> Option<f64> {
        match self {
            GgufValue::F32(v) => Some(*v as f64),
            GgufValue::F64(v) => Some(*v),
            GgufValue::U8(v) => Some(*v as f64),
            GgufValue::U16(v) => Some(*v as f64),
            GgufValue::U32(v) => Some(*v as f64),
            GgufValue::U64(v) => Some(*v as f64),
            GgufValue::I8(v) => Some(*v as f64),
            GgufValue::I16(v) => Some(*v as f64),
            GgufValue::I32(v) => Some(*v as f64),
            GgufValue::I64(v) => Some(*v as f64),
            _ => None,
        }
    }
}

struct Rd<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> Rd<'a> {
    fn new(b: &'a [u8]) -> Self {
        Rd { b, p: 4 }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.p.checked_add(n).ok_or_else(|| "gguf: header truncated".to_string())?;
        if end > self.b.len() {
            return Err("gguf: header truncated".to_string());
        }
        let s = &self.b[self.p..end];
        self.p = end;
        Ok(s)
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn f32(&mut self) -> Result<f32, String> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn f64(&mut self) -> Result<f64, String> {
        Ok(f64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn string(&mut self) -> Result<String, String> {
        const MAX_STR: u64 = 1 << 20;
        let n = self.u64()?;
        if n > MAX_STR {
            return Err("gguf: string too long".to_string());
        }
        let len = usize::try_from(n).map_err(|_| "gguf: string too long".to_string())?;
        let s = std::str::from_utf8(self.take(len)?).map_err(|_| "gguf: bad utf8".to_string())?;
        Ok(s.to_string())
    }
    fn value(&mut self, tag: u32, depth: u32) -> Result<GgufValue, String> {
        if depth > 4 {
            return Err("gguf: array nesting too deep".to_string());
        }
        match tag {
            0 => Ok(GgufValue::U8(self.take(1)?[0])),
            1 => Ok(GgufValue::I8(self.take(1)?[0] as i8)),
            2 => Ok(GgufValue::U16(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))),
            3 => Ok(GgufValue::I16(i16::from_le_bytes(self.take(2)?.try_into().unwrap()))),
            4 => Ok(GgufValue::U32(self.u32()?)),
            5 => Ok(GgufValue::I32(self.u32()? as i32)),
            6 => Ok(GgufValue::F32(self.f32()?)),
           7 => Ok(GgufValue::Bool(self.take(1)?[0] != 0)),
            8 => Ok(GgufValue::String(self.string()?)),
            9 => {
                const MAX_ARR: u64 = 4096;
                let elem = self.u32()?;
                let count = self.u64()?;
                if count > MAX_ARR {
                    return Err("gguf: array too large".to_string());
                }
                let n = usize::try_from(count).map_err(|_| "gguf: array too large".to_string())?;
                let remaining = self.b.len().saturating_sub(self.p);
                if n > remaining {
                    return Err("gguf: array too large".to_string());
                }
                let mut out = Vec::with_capacity(n);
                for _ in 0..n {
                    out.push(self.value(elem, depth + 1)?);
                }
                Ok(GgufValue::Array(out))
            }
            10 => Ok(GgufValue::U64(self.u64()?)),
            11 => Ok(GgufValue::I64(self.u64()? as i64)),
            12 => Ok(GgufValue::F64(self.f64()?)),
            _ => Err("gguf: bad value tag".to_string()),
        }
    }
}

pub fn read(bytes: &[u8]) -> Result<HashMap<String, GgufValue>, String> {
    if bytes.len() < 4 || &bytes[..4] != b"GGUF" {
        return Err("gguf: magic".to_string());
    }
    let mut rd = Rd::new(bytes);
    let version = rd.u32()?;
    if !(2..=3).contains(&version) {
        return Err(format!("gguf: unsupported version {version}"));
    }
    let tensor_count = rd.u64()?;
    let kv_count = rd.u64()?;
    let mut kv = HashMap::new();
    for _ in 0..kv_count {
        let key = rd.string()?;
        let tag = rd.u32()?;
        let value = rd.value(tag, 0)?;
        kv.insert(key, value);
    }
    let mut params_total = 0.0;
    for _ in 0..tensor_count {
        let _name = rd.string()?;
        let dims = rd.u32()? as usize;
        let mut elems = 1.0f64;
        for _ in 0..dims {
            let d = rd.u64()?;
            elems *= d as f64;
        }
        let _typ = rd.u32()?;
        let _off = rd.u64()?;
        params_total += elems;
    }
    if !params_total.is_finite() {
        return Err("gguf: non-finite params".to_string());
    }
    kv.insert("__janus_params_total".to_string(), GgufValue::F64(params_total));
    Ok(kv)
}

pub fn quant_to_ftype(quant: &str) -> Option<u32> {
    match quant {
        "F32" => Some(0),
        "F16" => Some(1),
        "Q4_0" => Some(2),
        "Q4_1" => Some(3),
        "Q4_1_SOME_F16" => Some(4),
        "Q8_0" => Some(7),
        "Q5_0" => Some(8),
        "Q5_1" => Some(9),
        "Q2_K" => Some(10),
        "Q3_K_S" => Some(11),
        "Q3_K_M" => Some(12),
        "Q3_K_L" => Some(13),
        "Q4_K_S" => Some(14),
        "Q4_K_M" => Some(15),
        "Q5_K_S" => Some(16),
        "Q5_K_M" => Some(17),
        "Q6_K" => Some(18),
        "IQ2_XXS" => Some(19),
        "IQ2_XS" => Some(20),
        "Q2_K_S" => Some(21),
        "IQ3_XS" => Some(22),
        "IQ3_XXS" => Some(23),
        "IQ1_S" => Some(24),
        "IQ4_NL" => Some(25),
        "IQ3_S" => Some(26),
        "IQ3_M" => Some(27),
        "IQ2_S" => Some(28),
        "IQ2_M" => Some(29),
        "IQ4_XS" => Some(30),
        "IQ1_M" => Some(31),
        "BF16" => Some(32),
        _ => None,
    }
}

pub fn ftype_to_quant(ftype: u32) -> Option<&'static str> {
    match ftype {
        0 => Some("F32"),
        1 => Some("F16"),
        2 => Some("Q4_0"),
        3 => Some("Q4_1"),
        4 => Some("Q4_1_SOME_F16"),
        7 => Some("Q8_0"),
        8 => Some("Q5_0"),
        9 => Some("Q5_1"),
        10 => Some("Q2_K"),
        11 => Some("Q3_K_S"),
        12 => Some("Q3_K_M"),
        13 => Some("Q3_K_L"),
        14 => Some("Q4_K_S"),
        15 => Some("Q4_K_M"),
        16 => Some("Q5_K_S"),
        17 => Some("Q5_K_M"),
        18 => Some("Q6_K"),
        19 => Some("IQ2_XXS"),
        20 => Some("IQ2_XS"),
        21 => Some("Q2_K_S"),
        22 => Some("IQ3_XS"),
        23 => Some("IQ3_XXS"),
        24 => Some("IQ1_S"),
        25 => Some("IQ4_NL"),
        26 => Some("IQ3_S"),
        27 => Some("IQ3_M"),
        28 => Some("IQ2_S"),
        29 => Some("IQ2_M"),
        30 => Some("IQ4_XS"),
        31 => Some("IQ1_M"),
        32 => Some("BF16"),
        _ => None,
    }
}