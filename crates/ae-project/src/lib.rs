//! Bounded, non-executing project ingestion. Schema 1 is an independently authored
//! interchange format. RIFX inventory is structural evidence only; no binary AEP
//! payload schema is decoded or claimed to be convertible.
mod model;
mod rifx;
mod validate;

pub use model::*;
pub use rifx::{ChunkInventory, RifxInventory, inspect_rifx};
use serde::{Deserialize, Serialize};
pub use validate::{CompositionClosure, ValidatedProject, parse_json, validate_project};

#[derive(Clone, Debug)]
pub struct Limits {
    pub max_file_bytes: usize,
    pub max_items: usize,
    pub max_layers: usize,
    pub max_properties: usize,
    pub max_links: usize,
    pub max_keys: usize,
    pub max_markers: usize,
    pub max_runs: usize,
    pub max_string_bytes: usize,
    pub max_total_string_bytes: usize,
    pub max_programs: usize,
    pub max_program_bytes: usize,
    pub max_total_program_bytes: usize,
    pub max_unsupported: usize,
    pub max_dimension: u32,
    pub max_abs_time_seconds: u32,
    pub max_dependency_depth: usize,
    pub max_chunks: usize,
    pub max_chunk_depth: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_file_bytes: 16 * 1024 * 1024,
            max_items: 4096,
            max_layers: 20_000,
            max_properties: 100_000,
            max_links: 100_000,
            max_keys: 250_000,
            max_markers: 100_000,
            max_runs: 100_000,
            max_string_bytes: 1024 * 1024,
            max_total_string_bytes: 8 * 1024 * 1024,
            max_programs: 4096,
            max_program_bytes: 16 * 1024,
            max_total_program_bytes: 1024 * 1024,
            max_unsupported: 10_000,
            max_dimension: 32768,
            max_abs_time_seconds: 31_536_000,
            max_dependency_depth: 64,
            max_chunks: 100_000,
            max_chunk_depth: 64,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticCode {
    InvalidJson,
    UnsupportedSchema,
    ResourceLimit,
    InvalidValue,
    DuplicateId,
    DanglingReference,
    CyclicReference,
    UnsupportedFeature,
    BinarySchemaUnverified,
    MalformedRifx,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    /// JSON-like model path, or byte offset for container errors.
    pub path: String,
    pub message: String,
}
impl Diagnostic {
    pub(crate) fn new(
        code: DiagnosticCode,
        path: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            path: path.into(),
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadError {
    pub diagnostics: Vec<Diagnostic>,
}
impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, diagnostic) in self.diagnostics.iter().enumerate() {
            if i != 0 {
                write!(f, "; ")?;
            }
            write!(f, "{}: {}", diagnostic.path, diagnostic.message)?;
        }
        Ok(())
    }
}
impl std::error::Error for ReadError {}
impl From<Diagnostic> for ReadError {
    fn from(value: Diagnostic) -> Self {
        Self {
            diagnostics: vec![value],
        }
    }
}
pub(crate) fn fail<T>(
    code: DiagnosticCode,
    path: &str,
    message: impl Into<String>,
) -> Result<T, ReadError> {
    Err(Diagnostic::new(code, path, message).into())
}
