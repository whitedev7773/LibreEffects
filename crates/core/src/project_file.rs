//! Libre Effects Project (`.lep`) container v1. Pure bytes; no filesystem or media I/O.
//!
//! CRC32 detects accidental corruption, not authenticity. Container versions and
//! the project model's schema version are independent. See `docs/lep-format-v1.md`.
use crate::{Project, document};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::Value;
use std::{borrow::Cow, collections::BTreeMap, fmt, sync::Arc};

pub const MAGIC: &[u8; 8] = b"\x89LEP\r\n\x1a\n";
pub const MAX_FILE_BYTES: usize = 256 * 1024 * 1024;
const HEADER_SIZE: usize = 32;
const CHUNK_HEADER_SIZE: usize = 20;
const MAX_JSON_BYTES: usize = 16 * 1024 * 1024;
const MAX_ENCODED_IMAGE_BYTES: usize = 12 * 1024 * 1024;
const MAX_RAW_IMAGE_BYTES: usize = 9 * 1024 * 1024;
const MAX_IMAGES: usize = 1000;
const MAX_CHUNKS: usize = MAX_IMAGES + 2;
const MAX_IMAGE_ID_BYTES: usize = 64;
const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

#[derive(Debug)]
pub struct DecodedProject<'a> {
    pub project: Project,
    /// Exact JSON bytes from the container; the caller owns view-state semantics.
    pub view: Option<&'a [u8]>,
}

/// Save deterministic compact metadata and stable-ID-ordered image chunks.
/// Canonical base64 PNGs store their original binary bytes, without pixel decoding.
/// Other core-compatible encoded strings are stored verbatim for lossless loading.
pub fn encode(project: &Project, view: Option<&[u8]>) -> Result<Vec<u8>, String> {
    project.validate()?;
    if let Some(view) = view {
        validate_native_json(view)?;
    }
    let prepared = document::prepare_native(project)?;
    // Reserve once using encoded sizes as an upper bound. A binary PNG never
    // exceeds its canonical base64 string, and fallback strings keep their size.
    let mut capacity = add_chunk_size(HEADER_SIZE, prepared.metadata.len())?;
    if let Some(view) = view {
        capacity = add_chunk_size(capacity, view.len())?;
    }
    for (id, image) in &prepared.images {
        let payload_len = 4usize
            .checked_add(id.len())
            .and_then(|len| len.checked_add(image.len()))
            .ok_or("Native image size overflow")?;
        capacity = add_chunk_size(capacity, payload_len)?;
    }
    let count = 1 + usize::from(view.is_some()) + prepared.images.len();
    let mut output = Vec::new();
    output
        .try_reserve_exact(capacity)
        .map_err(|_| "Cannot allocate native project output")?;
    output.extend_from_slice(MAGIC);
    output.extend_from_slice(&1u16.to_le_bytes());
    output.extend_from_slice(&(HEADER_SIZE as u16).to_le_bytes());
    output.extend_from_slice(&0u32.to_le_bytes());
    output.extend_from_slice(&0u64.to_le_bytes()); // Exact length is sealed below.
    output.extend_from_slice(
        &u32::try_from(count)
            .map_err(|_| "Native chunk count overflow")?
            .to_le_bytes(),
    );
    output.extend_from_slice(&0u32.to_le_bytes());
    append_chunk(&mut output, b"PROJ", &[&prepared.metadata])?;
    if let Some(view) = view {
        append_chunk(&mut output, b"VIEW", &[view])?;
    }
    // Decode and discard one temporary image at a time, never all images at once.
    for (id, image) in &prepared.images {
        let (kind, bytes) = match STANDARD.decode(image.as_bytes()) {
            Ok(raw)
                if raw.starts_with(PNG_SIGNATURE) && STANDARD.encode(&raw) == image.as_ref() =>
            {
                (0u8, Cow::Owned(raw))
            }
            _ => (1u8, Cow::Borrowed(image.as_bytes())),
        };
        let id_len = u16::try_from(id.len()).map_err(|_| "Invalid native image ID length")?;
        append_chunk(
            &mut output,
            b"IMAG",
            &[&id_len.to_le_bytes(), &[kind, 0], id.as_bytes(), &bytes],
        )?;
    }
    let total = u64::try_from(output.len()).map_err(|_| "Native project size overflow")?;
    output[16..24].copy_from_slice(&total.to_le_bytes());
    let crc = crc32fast::hash(&output[..28]);
    output[28..32].copy_from_slice(&crc.to_le_bytes());

    Ok(output)
}

fn add_chunk_size(total: usize, payload: usize) -> Result<usize, String> {
    total
        .checked_add(CHUNK_HEADER_SIZE)
        .and_then(|n| n.checked_add(payload))
        .filter(|n| *n <= MAX_FILE_BYTES)
        .ok_or_else(|| "Native project exceeds 256 MiB".into())
}

fn append_chunk(output: &mut Vec<u8>, tag: &[u8; 4], parts: &[&[u8]]) -> Result<(), String> {
    let payload_len = parts
        .iter()
        .try_fold(0usize, |sum, part| sum.checked_add(part.len()))
        .ok_or("Native chunk size overflow")?;
    add_chunk_size(output.len(), payload_len)?;
    let start = output.len();
    output.extend_from_slice(tag);
    output.extend_from_slice(&1u16.to_le_bytes());
    output.extend_from_slice(&0u16.to_le_bytes());
    output.extend_from_slice(
        &u64::try_from(payload_len)
            .map_err(|_| "Native chunk size overflow")?
            .to_le_bytes(),
    );
    let mut crc = crc32fast::Hasher::new();
    crc.update(&output[start..]);
    for part in parts {
        crc.update(part);
    }
    output.extend_from_slice(&crc.finalize().to_le_bytes());
    for part in parts {
        output.extend_from_slice(part);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug)]
struct Image<'a> {
    kind: u8,
    bytes: &'a [u8],
}

/// Validate every declared range, checksum and image budget before reconstructing
/// images. All declared image chunks are charged, before content interning.
pub fn decode(input: &[u8]) -> Result<DecodedProject<'_>, String> {
    if input.len() > MAX_FILE_BYTES {
        return Err("Native project exceeds 256 MiB".into());
    }
    let header = input
        .get(..HEADER_SIZE)
        .ok_or("Truncated native project header")?;
    if &header[..8] != MAGIC {
        return Err("Not a Libre Effects Project file".into());
    }
    if read_u16(header, 8) != 1 || usize::from(read_u16(header, 10)) != HEADER_SIZE {
        return Err("Unsupported native container version or header size".into());
    }
    if read_u32(header, 12) != 0 {
        return Err("Unsupported native container flags".into());
    }
    let declared_len =
        usize::try_from(read_u64(header, 16)).map_err(|_| "Native file length overflow")?;
    if declared_len != input.len() {
        return Err("Native file length mismatch".into());
    }
    let count = usize::try_from(read_u32(header, 24)).map_err(|_| "Native chunk count overflow")?;
    if !(1..=MAX_CHUNKS).contains(&count)
        || count
            .checked_mul(CHUNK_HEADER_SIZE)
            .and_then(|n| n.checked_add(HEADER_SIZE))
            .is_none_or(|n| n > input.len())
    {
        return Err("Invalid native chunk count".into());
    }
    if crc32fast::hash(&header[..28]) != read_u32(header, 28) {
        return Err("Native header checksum mismatch".into());
    }
    let mut metadata = None;
    let mut view = None;
    let mut images = BTreeMap::new();
    let mut encoded_bytes = 0usize;
    let mut cursor = HEADER_SIZE;
    for _ in 0..count {
        let payload_start = cursor
            .checked_add(CHUNK_HEADER_SIZE)
            .ok_or("Native chunk range overflow")?;
        let header = input
            .get(cursor..payload_start)
            .ok_or("Truncated native chunk header")?;
        if read_u16(header, 4) != 1 || read_u16(header, 6) != 0 {
            return Err("Unsupported native chunk version or flags".into());
        }
        let payload_len =
            usize::try_from(read_u64(header, 8)).map_err(|_| "Native chunk length overflow")?;
        match &header[..4] {
            b"PROJ" | b"VIEW" if payload_len <= MAX_JSON_BYTES => {}
            b"IMAG" if payload_len <= MAX_ENCODED_IMAGE_BYTES + MAX_IMAGE_ID_BYTES + 4 => {}
            b"PROJ" | b"VIEW" => return Err("Native JSON exceeds 16 MiB".into()),
            b"IMAG" => return Err("Native image exceeds 12 MiB encoded".into()),
            _ => return Err("Unsupported native chunk tag".into()),
        }
        let end = payload_start
            .checked_add(payload_len)
            .ok_or("Native chunk range overflow")?;
        let payload = input
            .get(payload_start..end)
            .ok_or("Truncated native chunk payload")?;
        let mut crc = crc32fast::Hasher::new();
        crc.update(&header[..16]);
        crc.update(payload);
        if crc.finalize() != read_u32(header, 16) {
            return Err("Native chunk checksum mismatch".into());
        }
        match &header[..4] {
            b"PROJ" => {
                if metadata.replace(payload).is_some() {
                    return Err("Duplicate native PROJ chunk".into());
                }
            }
            b"VIEW" => {
                if view.replace(payload).is_some() {
                    return Err("Duplicate native VIEW chunk".into());
                }
            }
            b"IMAG" => {
                if images.len() >= MAX_IMAGES {
                    return Err("Native image count exceeds 1000".into());
                }
                let (id, image, charge) = parse_image(payload)?;
                encoded_bytes = charge_image(encoded_bytes, charge)?;
                if images.insert(id, image).is_some() {
                    return Err("Duplicate native image ID".into());
                }
            }
            _ => unreachable!("tag checked before slicing"),
        }
        cursor = end;
    }
    if cursor != input.len() {
        return Err("Trailing native data or chunk count mismatch".into());
    }
    let metadata = native_json(metadata.ok_or("Missing native PROJ chunk")?)?;
    if let Some(view) = view {
        validate_native_json(view)?;
    }
    let images = images
        .into_iter()
        .map(|(id, image)| {
            let encoded: Arc<str> = if image.kind == 0 {
                Arc::from(STANDARD.encode(image.bytes))
            } else {
                // parse_image checked ASCII and the core's encoded-image alphabet.
                Arc::from(std::str::from_utf8(image.bytes).expect("validated ASCII image"))
            };
            (id.to_owned(), encoded)
        })
        .collect();
    Ok(DecodedProject {
        project: document::decode_native(metadata, images)?,
        view,
    })
}

fn parse_image(payload: &[u8]) -> Result<(&str, Image<'_>, usize), String> {
    let header = payload.get(..4).ok_or("Truncated native image header")?;
    let id_len = usize::from(read_u16(header, 0));
    if !(1..=MAX_IMAGE_ID_BYTES).contains(&id_len) || header[3] != 0 {
        return Err("Invalid native image ID length or reserved byte".into());
    }
    let data_start = 4usize
        .checked_add(id_len)
        .ok_or("Native image range overflow")?;
    let id = payload
        .get(4..data_start)
        .ok_or("Truncated native image ID")?;
    if !id
        .iter()
        .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(b))
    {
        return Err("Invalid native image ID".into());
    }
    let id = std::str::from_utf8(id).map_err(|_| "Invalid native image ID")?;
    let bytes = payload
        .get(data_start..)
        .ok_or("Truncated native image data")?;
    if bytes.is_empty() {
        return Err("Empty native image".into());
    }
    let kind = header[2];
    let charge = match kind {
        0 => {
            if bytes.len() > MAX_RAW_IMAGE_BYTES || !bytes.starts_with(PNG_SIGNATURE) {
                return Err("Native raw image must be a bounded PNG".into());
            }
            encoded_image_len(bytes.len())?
        }
        1 => {
            if !bytes
                .iter()
                .all(|b| b.is_ascii_alphanumeric() || b"+/=".contains(b))
            {
                return Err("Invalid native encoded image".into());
            }
            bytes.len()
        }
        _ => return Err("Unsupported native image storage kind".into()),
    };
    if charge > MAX_ENCODED_IMAGE_BYTES {
        return Err("Native image exceeds 12 MiB encoded".into());
    }
    Ok((id, Image { kind, bytes }, charge))
}

fn encoded_image_len(raw_len: usize) -> Result<usize, String> {
    raw_len
        .checked_add(2)
        .map(|n| n / 3)
        .and_then(|n| n.checked_mul(4))
        .ok_or_else(|| "Native encoded image size overflow".into())
}

fn charge_image(total: usize, encoded_len: usize) -> Result<usize, String> {
    total
        .checked_add(encoded_len)
        .filter(|n| *n <= document::MAX_IMAGE_BYTES)
        .ok_or_else(|| "Native embedded images exceed 128 MiB".into())
}

// These helpers receive headers only after their exact minimum range was checked.
fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
}
fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}
fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

/// Native-only strict JSON: reject duplicate object keys at every depth. Input and
/// node/key counts are bounded; never trust collection size hints. serde_json's
/// default recursion limit stays enabled. Legacy JSON keeps its existing parser.
fn native_json(bytes: &[u8]) -> Result<Value, String> {
    parse_native_json(bytes, true)
}

fn validate_native_json(bytes: &[u8]) -> Result<(), String> {
    parse_native_json(bytes, false).map(|_| ())
}

fn parse_native_json(bytes: &[u8], build: bool) -> Result<Value, String> {
    if bytes.len() > MAX_JSON_BYTES {
        return Err("Native JSON exceeds 16 MiB".into());
    }
    // Every JSON value and key requires at least one input byte. This bound
    // permits all valid documents inside the byte cap without an arbitrary
    // smaller model limit, and also bounds visitor allocations/iterations.
    let mut remaining = bytes.len();
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let value = UniqueValue {
        remaining: &mut remaining,
        build,
    }
    .deserialize(&mut deserializer)
    .map_err(|e| format!("Invalid native JSON: {e}"))?;
    deserializer
        .end()
        .map_err(|e| format!("Invalid native JSON: {e}"))?;
    Ok(value)
}

struct UniqueValue<'a> {
    remaining: &'a mut usize,
    build: bool,
}
impl UniqueValue<'_> {
    fn charge<E: de::Error>(&mut self) -> Result<(), E> {
        *self.remaining = self
            .remaining
            .checked_sub(1)
            .ok_or_else(|| E::custom("Native JSON node limit"))?;
        Ok(())
    }
}
impl<'de> DeserializeSeed<'de> for UniqueValue<'_> {
    type Value = Value;
    fn deserialize<D: de::Deserializer<'de>>(mut self, deserializer: D) -> Result<Value, D::Error> {
        self.charge()?;
        deserializer.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for UniqueValue<'_> {
    type Value = Value;
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("JSON without duplicate object keys")
    }
    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Value, E> {
        Ok(Value::Bool(value))
    }
    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Value, E> {
        Ok(Value::Number(value.into()))
    }
    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Value, E> {
        Ok(Value::Number(value.into()))
    }
    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Value, E> {
        serde_json::Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| E::custom("Nonfinite JSON number"))
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<Value, E> {
        Ok(if self.build {
            Value::String(value.to_owned())
        } else {
            Value::Null
        })
    }
    fn visit_string<E: de::Error>(self, value: String) -> Result<Value, E> {
        Ok(if self.build {
            Value::String(value)
        } else {
            Value::Null
        })
    }
    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_none<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Value, A::Error> {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element_seed(UniqueValue {
            remaining: self.remaining,
            build: self.build,
        })? {
            if self.build {
                values.push(value);
            }
        }
        Ok(if self.build {
            Value::Array(values)
        } else {
            Value::Null
        })
    }
    fn visit_map<A: MapAccess<'de>>(mut self, mut object: A) -> Result<Value, A::Error> {
        let mut values = serde_json::Map::new();
        while let Some(key) = object.next_key::<String>()? {
            self.charge()?;
            if values.contains_key(&key) {
                return Err(de::Error::custom("Duplicate native JSON object key"));
            }
            let value = object.next_value_seed(UniqueValue {
                remaining: self.remaining,
                build: self.build,
            })?;
            values.insert(key, value);
        }
        Ok(if self.build {
            Value::Object(values)
        } else {
            Value::Null
        })
    }
}

#[cfg(test)]
#[path = "project_file_tests.rs"]
mod tests;
