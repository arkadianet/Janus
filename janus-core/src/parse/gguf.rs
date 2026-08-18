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
        if self.p + n > self.b.len() {
            return Err("gguf: header truncated".to_string());
        }
        let s = &self.b[self.p..self.p + n];
        self.p += n;
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
        let len = self.u64()? as usize;
        let s = std::str::from_utf8(self.take(len)?).map_err(|_| "gguf: bad utf8".to_string())?;
        Ok(s.to_string())
    }
    fn value(&mut self, tag: u32) -> Result<GgufValue, String> {
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
                let elem = self.u32()?;
                let count = self.u64()? as usize;
                let mut out = Vec::with_capacity(count);
                for _ in 0..count {
                    out.push(self.value(elem)?);
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
    let _version = rd.u32()?;
    let tensor_count = rd.u64()?;
    let kv_count = rd.u64()?;
    let mut kv = HashMap::new();
    for _ in 0..kv_count {
        let key = rd.string()?;
        let tag = rd.u32()?;
        let value = rd.value(tag)?;
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
    kv.insert("__janus_params_total".to_string(), GgufValue::F64(params_total));
    Ok(kv)
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
        21 => Some("IQ3_XXS"),
        22 => Some("IQ1_S"),
        23 => Some("IQ4_NL"),
        24 => Some("IQ3_S"),
        25 => Some("IQ3_M"),
        26 => Some("IQ2_S"),
        27 => Some("IQ2_M"),
        28 => Some("IQ4_XS"),
        29 => Some("IQ1_M"),
        30 => Some("BF16"),
        _ => None,
    }
}