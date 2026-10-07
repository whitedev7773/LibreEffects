use libre_effects_ae_project::*;
const BINARY: &[u8] = include_bytes!("fixtures/synthetic-inventory.rifx");

fn chunk(id: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut bytes = id.to_vec();
    bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    bytes.extend_from_slice(payload);
    if payload.len() & 1 != 0 {
        bytes.push(0);
    }
    bytes
}
fn root(children: &[u8]) -> Vec<u8> {
    let mut payload = b"TEST".to_vec();
    payload.extend_from_slice(children);
    chunk(b"RIFX", &payload)
}
fn malformed(bytes: &[u8]) {
    assert_eq!(
        inspect_rifx(bytes, &Limits::default())
            .unwrap_err()
            .diagnostics[0]
            .code,
        DiagnosticCode::MalformedRifx
    );
}

#[test]
fn synthetic_inventory_has_exact_offsets_and_no_conversion_claim() {
    let inventory = inspect_rifx(BINARY, &Limits::default()).unwrap();
    assert_eq!(inventory.form_type, *b"TEST");
    assert_eq!(inventory.file_bytes, 46);
    assert_eq!(
        inventory.chunks,
        vec![
            ChunkInventory {
                id: *b"raw1",
                header_offset: 12,
                payload_offset: 20,
                payload_bytes: 3,
                padded_end: 24,
                depth: 1,
                parent: None,
                container_type: None
            },
            ChunkInventory {
                id: *b"LIST",
                header_offset: 24,
                payload_offset: 32,
                payload_bytes: 14,
                padded_end: 46,
                depth: 1,
                parent: None,
                container_type: Some(*b"demo")
            },
            ChunkInventory {
                id: *b"data",
                header_offset: 36,
                payload_offset: 44,
                payload_bytes: 2,
                padded_end: 46,
                depth: 2,
                parent: Some(1),
                container_type: None
            },
        ]
    );
    assert!(!inventory.is_convertible());
    assert_eq!(
        inventory.diagnostics[0].code,
        DiagnosticCode::BinarySchemaUnverified
    );
}

#[test]
fn every_truncated_fixture_prefix_is_rejected() {
    for end in 0..BINARY.len() {
        malformed(&BINARY[..end]);
    }
}

#[test]
fn trailing_bytes_wrong_endian_and_small_root_reject() {
    let mut bytes = BINARY.to_vec();
    bytes.push(0);
    malformed(&bytes);
    let mut bytes = BINARY.to_vec();
    bytes[..4].copy_from_slice(b"RIFF");
    malformed(&bytes);
    let mut bytes = BINARY.to_vec();
    bytes[4..8].copy_from_slice(&38u32.to_le_bytes());
    malformed(&bytes);
    let mut bytes = b"RIFX\0\0\0\x03abc".to_vec();
    malformed(&bytes);
    bytes[4..8].copy_from_slice(&u32::MAX.to_be_bytes());
    malformed(&bytes);
}

#[test]
fn chunk_overflow_missing_padding_and_nonzero_padding_reject() {
    let mut bytes = BINARY.to_vec();
    bytes[16..20].copy_from_slice(&u32::MAX.to_be_bytes());
    malformed(&bytes);
    let mut raw = chunk(b"data", b"x");
    raw.pop();
    malformed(&root(&raw));
    let mut bytes = BINARY.to_vec();
    bytes[23] = 17;
    malformed(&bytes);
}

#[test]
fn container_escape_and_missing_type_reject() {
    malformed(&root(&chunk(b"LIST", b"xy")));
    let mut child = b"TEST".to_vec();
    child.extend_from_slice(b"data\0\0\0\x04ab");
    let mut children = chunk(b"LIST", &child);
    children.extend_from_slice(&chunk(b"more", b"12"));
    malformed(&root(&children));
    malformed(&root(b"abc"));
}

#[test]
fn chunk_count_depth_and_file_budgets_reject() {
    for limits in [
        Limits {
            max_file_bytes: 45,
            ..Limits::default()
        },
        Limits {
            max_chunks: 2,
            ..Limits::default()
        },
        Limits {
            max_chunk_depth: 1,
            ..Limits::default()
        },
    ] {
        assert_eq!(
            inspect_rifx(BINARY, &limits).unwrap_err().diagnostics[0].code,
            DiagnosticCode::ResourceLimit
        );
    }
    let mut nested = chunk(b"data", b"x");
    for _ in 0..100 {
        let mut payload = b"nest".to_vec();
        payload.extend(nested);
        nested = chunk(b"LIST", &payload);
    }
    assert_eq!(
        inspect_rifx(&root(&nested), &Limits::default())
            .unwrap_err()
            .diagnostics[0]
            .code,
        DiagnosticCode::ResourceLimit
    );
}

#[test]
fn empty_container_is_still_not_a_project() {
    let inventory = inspect_rifx(&root(&[]), &Limits::default()).unwrap();
    assert!(inventory.chunks.is_empty());
    assert!(!inventory.is_convertible());
    assert!(!inventory.diagnostics.is_empty());
}

#[test]
fn opaque_payloads_are_not_scanned_as_containers_or_script() {
    let payload = b"LIST\0\0\0\x04fake value = app.project.item(1); Synthetic timing board";
    let inventory = inspect_rifx(&root(&chunk(b"data", payload)), &Limits::default()).unwrap();
    assert_eq!(inventory.chunks.len(), 1);
    assert_eq!(inventory.chunks[0].container_type, None);
    assert!(!inventory.is_convertible());
}

#[test]
fn deterministic_random_inputs_and_header_mutations_do_not_panic() {
    let mut state = 0x9134_7711u32;
    for size in 0..512 {
        let bytes: Vec<_> = (0..size)
            .map(|_| {
                state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                (state >> 24) as u8
            })
            .collect();
        let _ = inspect_rifx(&bytes, &Limits::default());
    }
    for position in 0..BINARY.len() {
        for byte in [0, 1, 127, 255] {
            let mut changed = BINARY.to_vec();
            changed[position] = byte;
            if let Ok(inventory) = inspect_rifx(&changed, &Limits::default()) {
                assert!(!inventory.is_convertible());
                for chunk in inventory.chunks {
                    assert!(chunk.padded_end <= changed.len());
                    assert!(chunk.payload_offset <= chunk.padded_end);
                }
            }
        }
    }
}
