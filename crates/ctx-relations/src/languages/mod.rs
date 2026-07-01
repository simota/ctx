// crates/ctx-relations/src/languages/mod.rs
//
// Per-language extractor modules. Each file mirrors the corresponding
// Go file under internal/relations/.

pub mod common;
pub mod go;
pub mod jsts;
pub mod jvm;
pub mod php;
pub mod py;
pub mod swift;

pub use common::{file_set, supported_ext, FileEntry};
