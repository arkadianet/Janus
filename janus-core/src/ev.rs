#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Known,
    Detected,
    Inferred,
    External,
    Manual,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Field<T> {
    pub value: T,
    pub level: Level,
}

impl<T> Field<T> {
    pub fn known(value: T) -> Self {
        Field { value, level: Level::Known }
    }
    pub fn inferred(value: T) -> Self {
        Field { value, level: Level::Inferred }
    }
    pub fn detected(value: T) -> Self {
        Field { value, level: Level::Detected }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Llm,
    Vision,
    Audio,
    Embeddings,
    Rerank,
    Adapter,
    Diffusion,
    Unknown,
}

impl Kind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Kind::Llm => "llm",
            Kind::Vision => "vision",
            Kind::Audio => "audio",
            Kind::Embeddings => "embeddings",
            Kind::Rerank => "rerank",
            Kind::Adapter => "adapter",
            Kind::Diffusion => "diffusion",
            Kind::Unknown => "unknown",
        }
    }
    pub fn from_str(s: &str) -> Kind {
        match s {
            "llm" => Kind::Llm,
            "vision" => Kind::Vision,
            "audio" => Kind::Audio,
            "embeddings" => Kind::Embeddings,
            "rerank" => Kind::Rerank,
            "adapter" => Kind::Adapter,
            "diffusion" => Kind::Diffusion,
            _ => Kind::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Weights,
    Shard,
    Tokenizer,
    Config,
    Mmproj,
    Lora,
    Sidecar,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Weights => "weights",
            Role::Shard => "shard",
            Role::Tokenizer => "tokenizer",
            Role::Config => "config",
            Role::Mmproj => "mmproj",
            Role::Lora => "lora",
            Role::Sidecar => "sidecar",
        }
    }
    pub fn from_str(s: &str) -> Option<Role> {
        match s {
            "weights" => Some(Role::Weights),
            "shard" => Some(Role::Shard),
            "tokenizer" => Some(Role::Tokenizer),
            "config" => Some(Role::Config),
            "mmproj" => Some(Role::Mmproj),
            "lora" => Some(Role::Lora),
            "sidecar" => Some(Role::Sidecar),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Gguf,
    Safetensors,
    Onnx,
    Pytorch,
    Mlx,
    Diffusers,
    Unknown,
}

impl Format {
    pub fn as_str(&self) -> &'static str {
        match self {
            Format::Gguf => "gguf",
            Format::Safetensors => "safetensors",
            Format::Onnx => "onnx",
            Format::Pytorch => "pytorch",
            Format::Mlx => "mlx",
            Format::Diffusers => "diffusers",
            Format::Unknown => "unknown",
        }
    }
}