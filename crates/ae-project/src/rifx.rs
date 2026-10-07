//! Structural big-endian RIFF-family inventory only. Leaf data is opaque.
//! Never infer project fields from names, printable strings or chunk fingerprints.
use crate::{Diagnostic, DiagnosticCode, Limits, ReadError, fail};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkInventory {
    pub id: [u8; 4],
    pub header_offset: usize,
    pub payload_offset: usize,
    pub payload_bytes: usize,
    pub padded_end: usize,
    pub depth: usize,
    /// Index of the parent inventory chunk; None identifies a root child.
    pub parent: Option<usize>,
    /// Present only for structural LIST/RIFX containers, never guessed leaf data.
    pub container_type: Option<[u8; 4]>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RifxInventory {
    pub form_type: [u8; 4],
    pub file_bytes: usize,
    pub chunks: Vec<ChunkInventory>,
    /// Always includes BinarySchemaUnverified, even for an empty container.
    pub diagnostics: Vec<Diagnostic>,
}
impl RifxInventory {
    /// Inventory does not contain a typed project and cannot authorize conversion.
    pub fn is_convertible(&self) -> bool {
        false
    }
}

pub fn inspect_rifx(bytes: &[u8], limits: &Limits) -> Result<RifxInventory, ReadError> {
    if bytes.len() > limits.max_file_bytes {
        return fail(
            DiagnosticCode::ResourceLimit,
            "byte[0]",
            "RIFX exceeds file byte limit",
        );
    }
    if bytes.len() < 12 || bytes.get(..4) != Some(b"RIFX") {
        return malformed(0, "Expected a complete big-endian RIFX header");
    }
    let size = be_u32(bytes, 4)? as usize;
    let end = 8usize
        .checked_add(size)
        .ok_or_else(|| error(4, "Root size overflow"))?;
    if size < 4 || end != bytes.len() {
        return malformed(
            4,
            "Root size must cover the complete file exactly, without trailing data",
        );
    }
    let form_type = fourcc(bytes, 8)?;
    let mut inventory = RifxInventory {
        form_type,
        file_bytes: bytes.len(),
        chunks: vec![],
        diagnostics: vec![Diagnostic::new(
            DiagnosticCode::BinarySchemaUnverified,
            "byte[0]",
            "RIFX structure is inventoried; AE binary field schemas are unverified. No composition, text, numeric property, expression, footage or usable project has been decoded.",
        )],
    };
    struct Frame {
        cursor: usize,
        end: usize,
        parent: Option<usize>,
        depth: usize,
    }
    let mut stack = vec![Frame {
        cursor: 12,
        end,
        parent: None,
        depth: 1,
    }];
    while let Some(frame) = stack.last_mut() {
        if frame.cursor == frame.end {
            stack.pop();
            continue;
        }
        if frame.depth > limits.max_chunk_depth {
            return fail(
                DiagnosticCode::ResourceLimit,
                &format!("byte[{}]", frame.cursor),
                "RIFX container depth exceeded",
            );
        }
        if inventory.chunks.len() >= limits.max_chunks {
            return fail(
                DiagnosticCode::ResourceLimit,
                &format!("byte[{}]", frame.cursor),
                "RIFX chunk count exceeded",
            );
        }
        let offset = frame.cursor;
        let payload_offset = offset
            .checked_add(8)
            .ok_or_else(|| error(offset, "Chunk header offset overflow"))?;
        if payload_offset > frame.end {
            return malformed(offset, "Truncated chunk header inside container");
        }
        let id = fourcc(bytes, offset)?;
        let payload_bytes = be_u32(bytes, offset + 4)? as usize;
        let payload_end = payload_offset
            .checked_add(payload_bytes)
            .ok_or_else(|| error(offset, "Chunk size overflow"))?;
        let padded_end = payload_end
            .checked_add(payload_bytes & 1)
            .ok_or_else(|| error(offset, "Chunk padding overflow"))?;
        if padded_end > frame.end {
            return malformed(
                offset,
                "Chunk payload or required alignment padding escapes its parent",
            );
        }
        // The IBM/Microsoft framing specification requires a zero pad byte.
        if payload_bytes & 1 != 0 && bytes[payload_end] != 0 {
            return malformed(payload_end, "Nonzero chunk alignment padding");
        }
        let container_type = if id == *b"LIST" || id == *b"RIFX" {
            if payload_bytes < 4 {
                return malformed(offset, "Container is missing its four-byte type");
            }
            Some(fourcc(bytes, payload_offset)?)
        } else if id == *b"RIFF" {
            return malformed(
                offset,
                "Mixed little-endian RIFF inside RIFX is not supported",
            );
        } else {
            None
        };
        let parent = frame.parent;
        let depth = frame.depth;
        frame.cursor = padded_end;
        let index = inventory.chunks.len();
        inventory.chunks.push(ChunkInventory {
            id,
            header_offset: offset,
            payload_offset,
            payload_bytes,
            padded_end,
            depth,
            parent,
            container_type,
        });
        if container_type.is_some() {
            // Check even empty containers against the depth budget.
            if depth > limits.max_chunk_depth {
                return fail(
                    DiagnosticCode::ResourceLimit,
                    &format!("byte[{offset}]"),
                    "RIFX container depth exceeded",
                );
            }
            stack.push(Frame {
                cursor: payload_offset + 4,
                end: payload_end,
                parent: Some(index),
                depth: depth + 1,
            });
        }
    }
    Ok(inventory)
}
fn fourcc(bytes: &[u8], offset: usize) -> Result<[u8; 4], ReadError> {
    let end = offset
        .checked_add(4)
        .ok_or_else(|| error(offset, "FOURCC offset overflow"))?;
    bytes
        .get(offset..end)
        .and_then(|value| value.try_into().ok())
        .ok_or_else(|| error(offset, "Truncated FOURCC"))
}
fn be_u32(bytes: &[u8], offset: usize) -> Result<u32, ReadError> {
    Ok(u32::from_be_bytes(fourcc(bytes, offset)?))
}
fn error(offset: usize, message: &str) -> ReadError {
    Diagnostic::new(
        DiagnosticCode::MalformedRifx,
        format!("byte[{offset}]"),
        message,
    )
    .into()
}
fn malformed<T>(offset: usize, message: &str) -> Result<T, ReadError> {
    Err(error(offset, message))
}
