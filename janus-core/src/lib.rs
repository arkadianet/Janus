pub mod cases;
pub mod db;
pub mod dedup;
pub mod detect;
pub mod doctor;
pub mod ev;
pub mod export;
pub mod filename;
pub mod hash;
pub mod identity;
pub mod parse;
pub mod scan;
pub mod store;

pub const SCHEMA_VERSION: &str = "1";
pub const FAMILY_KEY_ALGO: &str = "1";
pub const REPO_ROOT: &str = env!("CARGO_MANIFEST_DIR");