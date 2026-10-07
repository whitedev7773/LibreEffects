use super::*;
use crate::*;

const PNG: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a4WQAAAAASUVORK5CYII=";

fn add(editor: &mut Editor, content: Content, name: &str) {
    editor
        .execute(Command::AddContent {
            content,
            width: 16.0,
            height: 16.0,
            name: name.into(),
        })
        .unwrap();
}
fn image_project(encoded: &str) -> Project {
    let mut editor = Editor::default();
    add(
        &mut editor,
        Content::Image {
            png: encoded.into(),
        },
        "Image",
    );
    editor.project().clone()
}
fn chunks(bytes: &[u8]) -> Vec<([u8; 4], Vec<u8>)> {
    let mut cursor = HEADER_SIZE;
    let mut chunks = Vec::new();
    while cursor < bytes.len() {
        let len = read_u64(bytes, cursor + 8) as usize;
        chunks.push((
            bytes[cursor..cursor + 4].try_into().unwrap(),
            bytes[cursor + CHUNK_HEADER_SIZE..cursor + CHUNK_HEADER_SIZE + len].to_vec(),
        ));
        cursor += CHUNK_HEADER_SIZE + len;
    }
    chunks
}
fn seal_header(bytes: &mut [u8]) {
    let crc = crc32fast::hash(&bytes[..28]);
    bytes[28..32].copy_from_slice(&crc.to_le_bytes());
}
fn pack(chunks: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut bytes = vec![0; HEADER_SIZE];
    bytes[..8].copy_from_slice(MAGIC);
    bytes[8..10].copy_from_slice(&1u16.to_le_bytes());
    bytes[10..12].copy_from_slice(&(HEADER_SIZE as u16).to_le_bytes());
    for (tag, payload) in chunks {
        append_chunk(&mut bytes, tag, &[payload]).unwrap();
    }
    let length = bytes.len() as u64;
    bytes[16..24].copy_from_slice(&length.to_le_bytes());
    bytes[24..28].copy_from_slice(&(chunks.len() as u32).to_le_bytes());
    seal_header(&mut bytes);
    bytes
}
fn image_payload(id: &str, kind: u8, data: &[u8]) -> Vec<u8> {
    let mut payload = (id.len() as u16).to_le_bytes().to_vec();
    payload.extend_from_slice(&[kind, 0]);
    payload.extend_from_slice(id.as_bytes());
    payload.extend_from_slice(data);
    payload
}
fn metadata_with_refs(ids: &[String]) -> Vec<u8> {
    let mut project = image_project("YWJj");
    project.asset_library = Default::default();
    project.version = 7;
    let mut template = project.composition.layers[0].clone();
    template.asset = None;
    project.composition.layers = ids
        .iter()
        .enumerate()
        .map(|(n, _)| {
            let mut layer = template.clone();
            layer.id = n as u64 + 1;
            layer
        })
        .collect();
    project.next_layer_id = ids.len() as u64 + 1;
    let mut value = serde_json::to_value(project).unwrap();
    for (layer, id) in value["composition"]["layers"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .zip(ids)
    {
        layer["content"]["Image"] = serde_json::json!({"asset": id});
    }
    serde_json::to_vec(&value).unwrap()
}
fn error(bytes: &[u8], message: &str) {
    let actual = decode(bytes).unwrap_err();
    assert!(
        actual.contains(message),
        "expected {message:?}, got {actual:?}"
    );
}

#[test]
fn wire_header_crc_and_independent_schema_versions() {
    assert_eq!(crc32fast::hash(b"123456789"), 0xcbf43926);
    for schema in [1, PROJECT_VERSION] {
        let mut project = Project::default();
        project.version = schema;
        let bytes = encode(&project, None).unwrap();
        assert_eq!(&bytes[..8], MAGIC);
        assert_eq!(&bytes[8..16], &[1, 0, 32, 0, 0, 0, 0, 0]);
        assert_eq!(read_u64(&bytes, 16), bytes.len() as u64);
        assert_eq!(read_u32(&bytes, 24), 1);
        assert_eq!(crc32fast::hash(&bytes[..28]), read_u32(&bytes, 28));
        assert_eq!(&bytes[32..40], b"PROJ\x01\x00\x00\x00");
        let decoded = decode(&bytes).unwrap();
        assert_eq!(decoded.project, project);
        assert!(decoded.view.is_none());
        assert_eq!(encode(&decoded.project, None).unwrap(), bytes);
    }
}

#[test]
fn rich_multicomposition_roundtrip_preserves_assets_sharing_and_external_paths() {
    let mut editor = Editor::default();
    add(&mut editor, Content::Image { png: PNG.into() }, "PNG 雪");
    editor.execute(Command::DuplicateLayers(vec![1])).unwrap();
    add(
        &mut editor,
        Content::Image { png: "YWJj".into() },
        "Legacy encoded",
    );
    add(
        &mut editor,
        Content::ImageSequence {
            frames: Arc::new(vec![
                "/missing/shot 0001.png".into(),
                "C:\\media\\雪\\shot 0002.png".into(),
            ]),
            fps: 30.into(),
            missing: MissingFramePolicy::Hold,
            start_frame: 0,
            playback: Default::default(),
        },
        "Sequence",
    );
    add(
        &mut editor,
        Content::Video {
            path: "/unavailable/clip.mov".into(),
            audio: None,
            duration: 5.0,
            source_fps: 30.0,
            start_frame: 0,
            playback: Default::default(),
        },
        "Video",
    );
    add(
        &mut editor,
        Content::Audio {
            path: "relative/audio.wav".into(),
            audio: AudioMetadata {
                stream_index: 0,
                sample_rate: 48000,
                channels: 2,
                channel_layout: "stereo".into(),
                duration: 5.0,
                start_time: 0.0,
                file_offset: 0.0,
            },
            start_frame: 0,
            playback: Default::default(),
        },
        "Audio",
    );
    add(
        &mut editor,
        Content::Text {
            text: "A compact project\n雪".into(),
            font_size: 48.0,
        },
        "Text",
    );
    editor
        .execute(Command::ToggleKeyframe {
            id: 1,
            property: Property::Rotation,
            frame: 0,
        })
        .unwrap();
    editor
        .execute(Command::SetValue {
            id: 1,
            property: Property::Rotation,
            frame: 60,
            value: 123.45678912345678,
        })
        .unwrap();
    editor
        .execute(Command::SetBlendMode {
            id: 1,
            mode: BlendMode::Screen,
        })
        .unwrap();
    editor.execute(Command::DuplicateComposition).unwrap();
    let view = br#" { "playhead": 27, "selected": [1, 2], "zoom": 1.25 } "#;
    let bytes = encode(editor.project(), Some(view)).unwrap();
    let decoded = decode(&bytes).unwrap();
    assert_eq!(&decoded.project, editor.project());
    assert_eq!(decoded.view, Some(view.as_slice()));
    let borrowed = decoded.view.unwrap();
    assert!(bytes.as_ptr_range().contains(&borrowed.as_ptr()));
    assert_eq!(encode(&decoded.project, decoded.view).unwrap(), bytes);
    let chunks = chunks(&bytes);
    assert_eq!(
        chunks.iter().map(|(tag, _)| *tag).collect::<Vec<_>>(),
        [*b"PROJ", *b"VIEW", *b"IMAG", *b"IMAG"]
    );
    let metadata = std::str::from_utf8(&chunks[0].1).unwrap();
    assert!(!metadata.contains("image_assets"));
    assert!(!metadata.contains("\"png\""));
    assert!(metadata.contains("sequence_assets"));
    assert!(!metadata.contains(PNG));
    let mut image_arc = None;
    let mut sequence_arc = None;
    for content in decoded
        .project
        .asset_library
        .assets
        .values()
        .map(|a| &a.content)
        .chain(
            decoded
                .project
                .compositions()
                .iter()
                .flat_map(|(_, c)| c.layers.iter().map(|l| &l.content)),
        )
    {
        match content {
            Content::Image { png } if png.as_ref() == PNG => {
                if let Some(first) = image_arc {
                    assert!(Arc::ptr_eq(first, png));
                }
                image_arc = Some(png);
            }
            Content::ImageSequence { frames, .. } => {
                if let Some(first) = sequence_arc {
                    assert!(Arc::ptr_eq(first, frames));
                }
                sequence_arc = Some(frames);
            }
            _ => {}
        }
    }
    assert!(image_arc.is_some() && sequence_arc.is_some());
    assert_eq!(
        Project::from_json(&decoded.project.to_json().unwrap()).unwrap(),
        decoded.project
    );
}

#[test]
fn exact_png_bytes_and_legacy_encoded_strings_roundtrip() {
    let original = STANDARD.decode(PNG).unwrap();
    assert!(original.starts_with(PNG_SIGNATURE));
    for encoded in [PNG, "YWJj", "A", "YQ===", "iVBORw0KGgo="] {
        let project = image_project(encoded);
        let bytes = encode(&project, None).unwrap();
        let entries = chunks(&bytes);
        let (_, image, _) = parse_image(&entries[1].1).unwrap();
        if encoded == PNG || encoded == "iVBORw0KGgo=" {
            assert_eq!(image.kind, 0);
            assert_eq!(image.bytes, STANDARD.decode(encoded).unwrap());
        } else {
            assert_eq!(image.kind, 1);
            assert_eq!(image.bytes, encoded.as_bytes());
        }
        assert_eq!(decode(&bytes).unwrap().project, project);
    }
    // Pixel decoding belongs to the desktop gate; a signature-only raw payload
    // remains lossless and structurally valid in the core codec.
    assert_eq!(
        original,
        parse_image(&chunks(&encode(&image_project(PNG), None).unwrap())[1].1)
            .unwrap()
            .1
            .bytes
    );
}

#[test]
fn reader_accepts_any_chunk_order_and_borrows_even_empty_object_view() {
    let project = image_project(PNG);
    let bytes = encode(&project, Some(b"{}")).unwrap();
    let mut entries = chunks(&bytes);
    entries.reverse();
    let reversed = pack(&entries);
    let decoded = decode(&reversed).unwrap();
    assert_eq!(decoded.project, project);
    assert_eq!(decoded.view, Some(b"{}".as_slice()));
}

#[test]
fn all_truncation_prefixes_and_each_single_byte_corruption_are_rejected() {
    let bytes = encode(&image_project("YWJj"), Some(br#"{"zoom":1}"#)).unwrap();
    for end in 0..bytes.len() {
        assert!(decode(&bytes[..end]).is_err(), "accepted prefix {end}");
    }
    let mut corrupt = bytes.clone();
    for offset in 0..bytes.len() {
        corrupt[offset] ^= 1;
        assert!(decode(&corrupt).is_err(), "accepted changed byte {offset}");
        corrupt[offset] ^= 1;
    }
}

#[test]
fn checksummed_invalid_file_headers_and_extreme_lengths_are_rejected() {
    let bytes = encode(&Project::default(), None).unwrap();
    for (offset, patch) in [
        (8, 0u16.to_le_bytes().to_vec()),
        (8, 2u16.to_le_bytes().to_vec()),
        (10, 31u16.to_le_bytes().to_vec()),
        (12, 1u32.to_le_bytes().to_vec()),
        (16, u64::MAX.to_le_bytes().to_vec()),
        (16, 32u64.to_le_bytes().to_vec()),
        (24, 0u32.to_le_bytes().to_vec()),
        (24, 1003u32.to_le_bytes().to_vec()),
        (24, u32::MAX.to_le_bytes().to_vec()),
        (24, 2u32.to_le_bytes().to_vec()),
    ] {
        let mut bad = bytes.clone();
        bad[offset..offset + patch.len()].copy_from_slice(&patch);
        seal_header(&mut bad);
        assert!(
            decode(&bad).is_err(),
            "accepted header offset {offset} {patch:?}"
        );
    }
    for (offset, patch) in [
        (32, b"JUNK".to_vec()),
        (36, 2u16.to_le_bytes().to_vec()),
        (38, 1u16.to_le_bytes().to_vec()),
        (40, u64::MAX.to_le_bytes().to_vec()),
        (40, (MAX_JSON_BYTES as u64 + 1).to_le_bytes().to_vec()),
    ] {
        let mut bad = bytes.clone();
        bad[offset..offset + patch.len()].copy_from_slice(&patch);
        let mut crc = crc32fast::Hasher::new();
        crc.update(&bad[32..48]);
        crc.update(&bad[52..]);
        bad[48..52].copy_from_slice(&crc.finalize().to_le_bytes());
        assert!(
            decode(&bad).is_err(),
            "accepted chunk offset {offset} {patch:?}"
        );
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    error(&trailing, "length mismatch");
    let length = trailing.len() as u64;
    trailing[16..24].copy_from_slice(&length.to_le_bytes());
    seal_header(&mut trailing);
    error(&trailing, "Trailing");
    let mut no_payload = bytes[..52].to_vec();
    no_payload[16..24].copy_from_slice(&52u64.to_le_bytes());
    seal_header(&mut no_payload);
    error(&no_payload, "Truncated native chunk payload");
}

#[test]
fn duplicate_missing_and_unused_chunks_or_references_are_rejected() {
    let entries = chunks(&encode(&image_project(PNG), Some(b"{}")).unwrap());
    for index in 0..3 {
        let mut duplicate = entries.clone();
        duplicate.push(entries[index].clone());
        error(&pack(&duplicate), "Duplicate");
    }
    error(&pack(&entries[1..]), "Missing native PROJ");
    error(&pack(&entries[..2]), "Missing or ambiguous image");
    let mut unreferenced = entries.clone();
    unreferenced.push((*b"IMAG", image_payload("unused", 1, b"YWJj")));
    error(&pack(&unreferenced), "Unreferenced");
    let mut missing = entries.clone();
    let mut value: Value = serde_json::from_slice(&missing[0].1).unwrap();
    value["composition"]["layers"][0]["content"]["Image"]["asset"] = "absent".into();
    missing[0].1 = serde_json::to_vec(&value).unwrap();
    error(&pack(&missing), "Missing or ambiguous image");
}

#[test]
fn image_identifiers_storage_kinds_reserved_bytes_and_empty_data_are_strict() {
    let entries = chunks(&encode(&image_project("YWJj"), None).unwrap());
    for payload in [
        vec![],
        vec![1, 0, 1],
        image_payload("", 1, b"YWJj"),
        image_payload(&"a".repeat(65), 1, b"YWJj"),
        image_payload("bad id", 1, b"YWJj"),
        image_payload("雪", 1, b"YWJj"),
        image_payload("id", 2, b"YWJj"),
        image_payload("id", 1, b""),
        image_payload("id", 0, b"notPNG"),
        image_payload("id", 1, &[0xff]),
        image_payload("id", 1, b"has space"),
        image_payload("id", 1, b"A?"),
    ] {
        assert!(decode(&pack(&[entries[0].clone(), (*b"IMAG", payload)])).is_err());
    }
    let mut reserved = entries.clone();
    reserved[1].1[3] = 1;
    error(&pack(&reserved), "reserved");
    let mut huge_id = entries.clone();
    huge_id[1].1[..2].copy_from_slice(&u16::MAX.to_le_bytes());
    error(&pack(&huge_id), "ID length");
    for id in ["a".to_owned(), "AZaz09_-".to_owned(), "x".repeat(64)] {
        let metadata = metadata_with_refs(std::slice::from_ref(&id));
        decode(&pack(&[
            (*b"PROJ", metadata),
            (*b"IMAG", image_payload(&id, 1, b"YWJj")),
        ]))
        .unwrap();
    }
}

#[test]
fn native_metadata_forbids_inline_images_and_ambiguous_references() {
    let entries = chunks(&encode(&image_project("YWJj"), None).unwrap());
    for target in ["table", "layer", "asset"] {
        let mut bad = entries.clone();
        let mut value: Value = serde_json::from_slice(&bad[0].1).unwrap();
        match target {
            "table" => value["image_assets"] = serde_json::json!({}),
            "layer" => value["composition"]["layers"][0]["content"]["Image"]["png"] = "YWJj".into(),
            _ => value["asset_library"]["assets"]["1"]["content"]["Image"]["png"] = "YWJj".into(),
        }
        bad[0].1 = serde_json::to_vec(&value).unwrap();
        error(&pack(&bad), "inline");
    }
    let raw = serde_json::to_vec(&image_project("YWJj")).unwrap();
    error(&pack(&[(*b"PROJ", raw)]), "inline");
}

#[test]
fn native_json_rejects_duplicate_keys_at_every_depth_and_keeps_legacy_behavior() {
    let project = Project::default();
    let json = serde_json::to_string(&project).unwrap();
    let duplicate = format!("{{\"version\":1,{}", &json[1..]);
    assert_eq!(Project::from_json(&duplicate).unwrap(), project);
    error(&pack(&[(*b"PROJ", duplicate.into_bytes())]), "Duplicate");
    for json in [
        br#"{"nested":{"x":1,"x":2}}"#.as_slice(),
        br#"{"array":[{"x":1,"\u0078":2}]}"#.as_slice(),
        br#"{"x":1,"x":1}"#.as_slice(),
    ] {
        assert!(native_json(json).unwrap_err().contains("Duplicate"));
        assert!(
            encode(&project, Some(json))
                .unwrap_err()
                .contains("Duplicate")
        );
        error(
            &pack(&[
                (*b"PROJ", serde_json::to_vec(&project).unwrap()),
                (*b"VIEW", json.to_vec()),
            ]),
            "Duplicate",
        );
    }
    for json in [b"".as_slice(), b"{}{}", b"{", &[0xff], b"NaN"] {
        assert!(native_json(json).is_err());
        assert!(encode(&project, Some(json)).is_err());
    }
    let deep = format!("{}0{}", "[".repeat(128), "]".repeat(128));
    assert!(
        native_json(deep.as_bytes())
            .unwrap_err()
            .contains("recursion limit")
    );
    assert_eq!(
        native_json(br#" [null,true,false,-2,18446744073709551615,1.2345678901234567,"text"] "#)
            .unwrap(),
        serde_json::from_slice::<Value>(
            br#"[null,true,false,-2,18446744073709551615,1.2345678901234567,"text"]"#
        )
        .unwrap()
    );
}

#[test]
fn future_project_versions_report_bounds_before_assets_or_model_variants() {
    let mut editor = Editor::default();
    editor.execute(Command::AddRectangle).unwrap();
    for project in [editor.project().clone(), image_project(PNG)] {
        let json: Value = serde_json::from_str(&project.to_json().unwrap()).unwrap();
        let entries = chunks(&encode(&project, Some(b"{\"opaque\":true}")).unwrap());
        for version in [u64::from(PROJECT_VERSION) + 1, u64::MAX] {
            let expected = format!(
                "Unsupported project version {version}; this build supports up to version {PROJECT_VERSION}"
            );
            for change in ["version", "unknown_fields", "unknown_kind", "future_assets"] {
                let change_schema = |value: &mut Value| {
                    value["version"] = version.into();
                    match change {
                        "unknown_fields" => {
                            value["future_settings"] = serde_json::json!({"enabled": true});
                            value["composition"]["future_setting"] = true.into();
                        }
                        "unknown_kind" => {
                            value["composition"]["layers"][0]["content"] =
                                serde_json::json!({"FutureContent": {"mode": "NewMode"}});
                        }
                        "future_assets" => {
                            value["sequence_assets"] = serde_json::json!(["new layout"]);
                        }
                        _ => {}
                    }
                };
                let mut future_json = json.clone();
                change_schema(&mut future_json);
                let future_json = future_json.to_string();
                assert_eq!(Project::from_json(&future_json).unwrap_err(), expected);

                let mut future_entries = entries.clone();
                let mut metadata: Value = serde_json::from_slice(&future_entries[0].1).unwrap();
                change_schema(&mut metadata);
                future_entries[0].1 = serde_json::to_vec(&metadata).unwrap();
                let bytes = pack(&future_entries);
                assert_eq!(decode(&bytes).unwrap_err(), expected);
                // A failed read leaves the original portable bytes available.
                assert_eq!(bytes, pack(&future_entries));
            }
        }
        assert_eq!(Project::from_json(&json.to_string()).unwrap(), project);
        assert_eq!(decode(&pack(&entries)).unwrap().project, project);
    }
}

#[test]
fn future_version_preflight_keeps_supported_json_and_native_roundtrips_exact() {
    for mut project in [Project::default(), image_project(PNG)] {
        for version in [project.version, PROJECT_VERSION] {
            project.version = version;
            let json = project.to_json().unwrap();
            let loaded = Project::from_json(&json).unwrap();
            assert_eq!(loaded, project);
            assert_eq!(loaded.to_json().unwrap(), json);
            let view = br#" { "version":2, "opaque":{"selection":[1],"future_view_field":true} } "#;
            let bytes = encode(&project, Some(view)).unwrap();
            let loaded = decode(&bytes).unwrap();
            assert_eq!(loaded.project, project);
            assert_eq!(loaded.view, Some(view.as_slice()));
            assert_eq!(encode(&loaded.project, loaded.view).unwrap(), bytes);
        }
    }
}

#[test]
fn future_version_preflight_preserves_missing_invalid_and_lower_image_versions() {
    let invalid_versions = [
        None,
        Some(Value::Null),
        Some(serde_json::json!(-1)),
        Some(serde_json::json!(1.5)),
        Some(serde_json::json!(PROJECT_VERSION.to_string())),
        Some(Value::Bool(true)),
        Some(serde_json::json!([])),
        Some(serde_json::json!({})),
        Some(serde_json::json!(0)),
    ];
    for version in &invalid_versions {
        let mut value = serde_json::to_value(Project::default()).unwrap();
        if let Some(version) = version {
            value["version"] = version.clone();
        } else {
            value.as_object_mut().unwrap().remove("version");
        }
        // Preserve the original typed-model and validation diagnostics.
        let expected = match serde_json::from_value::<Project>(value.clone()) {
            Ok(project) => project.validate().unwrap_err(),
            Err(error) => error.to_string(),
        };
        assert_eq!(
            Project::from_json(&value.to_string()).unwrap_err(),
            expected
        );
        assert_eq!(
            decode(&pack(&[(*b"PROJ", serde_json::to_vec(&value).unwrap())])).unwrap_err(),
            expected
        );
    }
    let metadata = metadata_with_refs(&["old-image".into()]);
    for version in invalid_versions.into_iter().chain([Some(6.into())]) {
        let mut value: Value = serde_json::from_slice(&metadata).unwrap();
        if let Some(version) = version {
            value["version"] = version;
        } else {
            value.as_object_mut().unwrap().remove("version");
        }
        let bytes = pack(&[
            (*b"PROJ", serde_json::to_vec(&value).unwrap()),
            (*b"IMAG", image_payload("old-image", 1, b"YWJj")),
        ]);
        assert_eq!(
            decode(&bytes).unwrap_err(),
            "Image assets require project version 7"
        );
        value["image_assets"] = serde_json::json!({"old-image":"YWJj"});
        assert_eq!(
            Project::from_json(&value.to_string()).unwrap_err(),
            "Image assets require project version 7"
        );
    }
}

#[test]
fn future_version_preflight_does_not_bypass_native_container_checks() {
    let mut entries = chunks(&encode(&image_project(PNG), None).unwrap());
    let mut metadata: Value = serde_json::from_slice(&entries[0].1).unwrap();
    metadata["version"] = (PROJECT_VERSION + 1).into();
    entries[0].1 = serde_json::to_vec(&metadata).unwrap();
    let bytes = pack(&entries);

    let mut bad = bytes.clone();
    bad[28] ^= 1;
    error(&bad, "Native header checksum mismatch");
    let mut bad = bytes.clone();
    *bad.last_mut().unwrap() ^= 1;
    error(&bad, "Native chunk checksum mismatch");
    let mut bad = bytes.clone();
    bad[40..48].copy_from_slice(&(MAX_JSON_BYTES as u64 + 1).to_le_bytes());
    error(&bad, "Native JSON exceeds 16 MiB");
    let mut bad = entries.clone();
    bad.push(entries[0].clone());
    error(&pack(&bad), "Duplicate native PROJ chunk");
    let mut bad = entries.clone();
    bad[1].1 = image_payload("image-1", 2, b"YWJj");
    error(&pack(&bad), "Unsupported native image storage kind");
    let mut bad = entries.clone();
    bad.push((*b"VIEW", br#"{"key":1,"key":2}"#.to_vec()));
    error(&pack(&bad), "Duplicate native JSON object key");
    let mut bad = entries.clone();
    bad[0].1 = format!(
        "{{\"version\":{},\"version\":{}}}",
        PROJECT_VERSION + 1,
        PROJECT_VERSION + 1
    )
    .into_bytes();
    error(&pack(&bad), "Duplicate native JSON object key");
    metadata["image_assets"] = serde_json::json!({});
    entries[0].1 = serde_json::to_vec(&metadata).unwrap();
    error(
        &pack(&entries),
        "Native metadata must not contain inline image_assets",
    );
}

#[test]
fn native_rejects_unsupported_project_schema_and_invalid_sequence_references() {
    for version in [0, PROJECT_VERSION + 1] {
        let mut value = serde_json::to_value(Project::default()).unwrap();
        value["version"] = version.into();
        error(
            &pack(&[(*b"PROJ", serde_json::to_vec(&value).unwrap())]),
            "Unsupported project version",
        );
    }
    let mut editor = Editor::default();
    add(
        &mut editor,
        Content::ImageSequence {
            frames: Arc::new(vec!["one.png".into()]),
            fps: 30.into(),
            missing: Default::default(),
            start_frame: 0,
            playback: Default::default(),
        },
        "Sequence",
    );
    let value: Value =
        serde_json::from_slice(&chunks(&encode(editor.project(), None).unwrap())[0].1).unwrap();
    let mut unused = value.clone();
    unused["sequence_assets"]["unused"] = serde_json::json!(["unused.png"]);
    error(
        &pack(&[(*b"PROJ", serde_json::to_vec(&unused).unwrap())]),
        "Unreferenced native sequence",
    );
    for field in ["frames", "manifest"] {
        let mut invalid = value.clone();
        let sequence = &mut invalid["composition"]["layers"][0]["content"]["ImageSequence"];
        sequence[field] = if field == "frames" {
            serde_json::json!(["inline.png"])
        } else {
            "missing".into()
        };
        error(
            &pack(&[(*b"PROJ", serde_json::to_vec(&invalid).unwrap())]),
            "Missing or ambiguous sequence",
        );
    }
}

#[test]
fn exact_json_boundaries_and_checked_size_helpers() {
    let mut view = vec![b' '; MAX_JSON_BYTES];
    view[..2].copy_from_slice(b"{}");
    let bytes = encode(&Project::default(), Some(&view)).unwrap();
    assert_eq!(decode(&bytes).unwrap().view.unwrap(), view);
    view.push(b' ');
    assert!(
        encode(&Project::default(), Some(&view))
            .unwrap_err()
            .contains("16 MiB")
    );
    let metadata = serde_json::to_vec(&Project::default()).unwrap();
    let mut padded = metadata;
    padded.resize(MAX_JSON_BYTES, b' ');
    decode(&pack(&[(*b"PROJ", padded.clone())])).unwrap();
    padded.push(b' ');
    error(&pack(&[(*b"PROJ", padded)]), "16 MiB");
    assert_eq!(
        add_chunk_size(
            HEADER_SIZE,
            MAX_FILE_BYTES - HEADER_SIZE - CHUNK_HEADER_SIZE
        )
        .unwrap(),
        MAX_FILE_BYTES
    );
    assert!(
        add_chunk_size(
            HEADER_SIZE,
            MAX_FILE_BYTES - HEADER_SIZE - CHUNK_HEADER_SIZE + 1
        )
        .is_err()
    );
    assert!(add_chunk_size(usize::MAX, 0).is_err());
    assert!(add_chunk_size(0, usize::MAX).is_err());
    assert_eq!(
        encoded_image_len(9 * 1024 * 1024).unwrap(),
        MAX_ENCODED_IMAGE_BYTES
    );
    assert_eq!(encoded_image_len(1).unwrap(), 4);
    assert_eq!(encoded_image_len(3).unwrap(), 4);
    assert_eq!(encoded_image_len(4).unwrap(), 8);
    assert!(encoded_image_len(usize::MAX).is_err());
    assert_eq!(
        charge_image(0, document::MAX_IMAGE_BYTES).unwrap(),
        document::MAX_IMAGE_BYTES
    );
    assert!(charge_image(document::MAX_IMAGE_BYTES, 1).is_err());
    assert!(charge_image(usize::MAX, 1).is_err());
}

#[test]
fn exact_per_image_encoded_and_raw_boundaries_are_enforced() {
    let text = "A".repeat(MAX_ENCODED_IMAGE_BYTES);
    let project = image_project(&text);
    let bytes = encode(&project, None).unwrap();
    assert_eq!(decode(&bytes).unwrap().project, project);
    let mut payload = image_payload("image-1", 1, text.as_bytes());
    assert_eq!(parse_image(&payload).unwrap().2, MAX_ENCODED_IMAGE_BYTES);
    payload.push(b'A');
    assert!(parse_image(&payload).unwrap_err().contains("12 MiB"));
    let mut raw = vec![0; MAX_RAW_IMAGE_BYTES];
    raw[..8].copy_from_slice(PNG_SIGNATURE);
    let mut payload = image_payload("image-1", 0, &raw);
    assert_eq!(parse_image(&payload).unwrap().2, MAX_ENCODED_IMAGE_BYTES);
    payload.push(0);
    assert!(parse_image(&payload).unwrap_err().contains("bounded PNG"));
}

#[test]
fn maximum_image_and_chunk_counts_allow_shared_content_but_not_extra_images() {
    let ids: Vec<_> = (0..MAX_IMAGES).map(|i| format!("image-{i}")).collect();
    let mut entries = vec![
        (*b"PROJ", metadata_with_refs(&ids)),
        (*b"VIEW", b"{}".to_vec()),
    ];
    entries.extend(ids.iter().map(|id| (*b"IMAG", image_payload(id, 1, b"A"))));
    let bytes = pack(&entries);
    assert_eq!(read_u32(&bytes, 24), 1002);
    let decoded = decode(&bytes).unwrap();
    assert_eq!(decoded.project.composition.layers.len(), 1000);
    // Interning is allowed after every declared image was charged.
    assert_eq!(decoded.project.asset_library.assets.len(), 1);
    entries.remove(1);
    entries.push((*b"IMAG", image_payload("extra", 1, b"A")));
    error(&pack(&entries), "image count");
    entries.push((*b"VIEW", b"{}".to_vec()));
    error(&pack(&entries), "chunk count");
}

#[test]
fn all_declared_images_are_budgeted_before_interning_or_unused_asset_checks() {
    let ids: Vec<_> = (0..11).map(|i| format!("image-{i}")).collect();
    let metadata = metadata_with_refs(&ids);
    let repeated = vec![b'A'; MAX_ENCODED_IMAGE_BYTES];
    let remainder = document::MAX_IMAGE_BYTES - 10 * repeated.len();
    let mut bytes = vec![0; HEADER_SIZE];
    bytes[..8].copy_from_slice(MAGIC);
    bytes[8..10].copy_from_slice(&1u16.to_le_bytes());
    bytes[10..12].copy_from_slice(&(HEADER_SIZE as u16).to_le_bytes());
    append_chunk(&mut bytes, b"PROJ", &[&metadata]).unwrap();
    let mut final_start = 0;
    for (index, id) in ids.iter().enumerate() {
        let image = if index == 10 {
            &repeated[..remainder]
        } else {
            &repeated
        };
        final_start = bytes.len();
        append_chunk(
            &mut bytes,
            b"IMAG",
            &[
                &(id.len() as u16).to_le_bytes(),
                &[1, 0],
                id.as_bytes(),
                image,
            ],
        )
        .unwrap();
    }
    let length = bytes.len() as u64;
    bytes[16..24].copy_from_slice(&length.to_le_bytes());
    bytes[24..28].copy_from_slice(&12u32.to_le_bytes());
    seal_header(&mut bytes);
    let decoded = decode(&bytes).unwrap();
    assert_eq!(decoded.project.asset_library.assets.len(), 2);
    drop(decoded);
    // Same content under distinct IDs must still exceed the declared-byte cap.
    bytes.push(b'A');
    let payload_len = read_u64(&bytes, final_start + 8) + 1;
    bytes[final_start + 8..final_start + 16].copy_from_slice(&payload_len.to_le_bytes());
    let mut crc = crc32fast::Hasher::new();
    crc.update(&bytes[final_start..final_start + 16]);
    crc.update(&bytes[final_start + 20..]);
    bytes[final_start + 16..final_start + 20].copy_from_slice(&crc.finalize().to_le_bytes());
    let length = bytes.len() as u64;
    bytes[16..24].copy_from_slice(&length.to_le_bytes());
    seal_header(&mut bytes);
    error(&bytes, "128 MiB");
    // Remove the references while preserving total metadata length. The cap is
    // still checked before an unused-image rejection or model reconstruction.
    let metadata_len = read_u64(&bytes, 40) as usize;
    let minimal = serde_json::to_vec(&Project::default()).unwrap();
    bytes[52..52 + metadata_len].fill(b' ');
    bytes[52..52 + minimal.len()].copy_from_slice(&minimal);
    let mut crc = crc32fast::Hasher::new();
    crc.update(&bytes[32..48]);
    crc.update(&bytes[52..52 + metadata_len]);
    bytes[48..52].copy_from_slice(&crc.finalize().to_le_bytes());
    error(&bytes, "128 MiB");
}

#[test]
fn native_and_legacy_share_mask_shape_and_asset_migrations() {
    let mut editor = Editor::default();
    editor.execute(Command::AddRectangle).unwrap();
    editor
        .execute(Command::SetPathMasks {
            id: 1,
            masks: vec![PathMask {
                path: VectorPath {
                    closed: true,
                    vertices: [[0.0, 0.0], [80.0, 0.0], [40.0, 80.0]]
                        .map(PathVertex::corner)
                        .to_vec(),
                },
                ..Default::default()
            }],
        })
        .unwrap();
    let mut old = serde_json::to_value(editor.project()).unwrap();
    old["version"] = 29.into();
    let layer = &mut old["composition"]["layers"][0];
    layer.as_object_mut().unwrap().remove("next_mask_id");
    for mask in layer["path_masks"].as_array_mut().unwrap() {
        mask.as_object_mut().unwrap().remove("id");
        mask.as_object_mut().unwrap().remove("parameters");
    }
    let legacy = Project::from_json(&old.to_string()).unwrap();
    let native = decode(&pack(&[(*b"PROJ", serde_json::to_vec(&old).unwrap())]))
        .unwrap()
        .project;
    assert_eq!(native, legacy);
    assert_eq!(native.version, 30);

    let mut editor = Editor::default();
    add(
        &mut editor,
        Content::Shape(Shape {
            kind: ShapeKind::Star,
            ..Default::default()
        }),
        "Star",
    );
    editor
        .execute(Command::Contents {
            id: 1,
            edit: ContentsEdit::Promote,
        })
        .unwrap();
    let mut old = editor.project().clone();
    old.version = 43;
    let Content::ShapeContents(contents) = &mut old.composition.layers[0].content else {
        panic!()
    };
    let group = contents.node_mut(1).unwrap();
    group.parameters.remove(&ContentsParam::Skew);
    group.parameters.remove(&ContentsParam::SkewAxis);
    let raw = serde_json::to_vec(&old).unwrap();
    let legacy = Project::from_json(std::str::from_utf8(&raw).unwrap()).unwrap();
    let native = decode(&pack(&[(*b"PROJ", raw)])).unwrap().project;
    assert_eq!(native, legacy);
    assert_eq!(native.version, 44);

    // A referenced-image schema-7 file enters the same asset synchronization.
    let metadata = metadata_with_refs(&["old-image".into()]);
    let native = decode(&pack(&[
        (*b"PROJ", metadata.clone()),
        (*b"IMAG", image_payload("old-image", 1, b"YWJj")),
    ]))
    .unwrap()
    .project;
    let mut legacy: Value = serde_json::from_slice(&metadata).unwrap();
    legacy["image_assets"] = serde_json::json!({"old-image":"YWJj"});
    assert_eq!(native, Project::from_json(&legacy.to_string()).unwrap());
    assert_eq!(native.version, 22);
}

#[test]
fn trim_paths_schema_50_roundtrips_native_without_container_or_view_version_changes() {
    let mut editor = Editor::default();
    add(&mut editor, Content::Shape(Shape::default()), "Trim native");
    editor
        .execute(Command::Contents {
            id: 1,
            edit: ContentsEdit::Promote,
        })
        .unwrap();
    editor
        .execute(Command::Contents {
            id: 1,
            edit: ContentsEdit::Add {
                parent: 1,
                kind: ContentsKind::TrimPaths,
            },
        })
        .unwrap();
    editor
        .execute(Command::Contents {
            id: 1,
            edit: ContentsEdit::Enabled {
                item: 5,
                enabled: false,
            },
        })
        .unwrap();
    editor.execute(Command::NewComposition).unwrap();
    assert_eq!(editor.project().version, 50);
    assert!(editor.project().composition.layers.is_empty());
    for view in [b"{\"version\":1}".as_slice(), b"{\"version\":2}".as_slice()] {
        let bytes = encode(editor.project(), Some(view)).unwrap();
        assert_eq!(&bytes[..8], MAGIC);
        assert_eq!(u16::from_le_bytes(bytes[8..10].try_into().unwrap()), 1);
        let decoded = decode(&bytes).unwrap();
        assert_eq!(&decoded.project, editor.project());
        assert_eq!(decoded.view, Some(view));
        assert_eq!(encode(&decoded.project, decoded.view).unwrap(), bytes);
        assert_eq!(decoded.project.version, 50);
    }
    let mut invalid = editor.project().clone();
    invalid.version = 49;
    assert!(encode(&invalid, None).is_err());
}

#[test]
fn trim_same_displayed_value_keeps_native_project_and_view_bytes_exact() {
    let mut editor = Editor::default();
    add(
        &mut editor,
        Content::ShapeContents(ShapeContents::default()),
        "Trim native no-op",
    );
    editor
        .execute(Command::Contents {
            id: 1,
            edit: ContentsEdit::Add {
                parent: 0,
                kind: ContentsKind::TrimPaths,
            },
        })
        .unwrap();
    let parameter = ContentsParam::Trim(TrimParam::End);
    editor
        .execute(Command::Contents {
            id: 1,
            edit: ContentsEdit::Track {
                item: 1,
                parameter,
                edit: TrackEdit::ToggleAnimation { frame: 0 },
            },
        })
        .unwrap();
    editor
        .execute(Command::Contents {
            id: 1,
            edit: ContentsEdit::Track {
                item: 1,
                parameter,
                edit: TrackEdit::Value {
                    frame: 30,
                    value: 0.,
                },
            },
        })
        .unwrap();
    let view = b"{\"version\":2,\"fixture\":\"unchanged opaque view\"}";
    let before = encode(editor.project(), Some(view)).unwrap();
    editor
        .execute(Command::Batch(vec![Command::EditTrack {
            id: 1,
            property: PropertyPath::Contents { item: 1, parameter },
            edit: TrackEdit::Value {
                frame: 15,
                value: 50.,
            },
        }]))
        .unwrap();
    assert_eq!(encode(editor.project(), Some(view)).unwrap(), before);
}

#[test]
fn luma_schema51_native_roundtrip_keeps_container_view_addresses_and_shared_images() {
    let mut editor = Editor::default();
    add(
        &mut editor,
        Content::Image { png: PNG.into() },
        "Luma image",
    );
    editor
        .execute(Command::Effect {
            id: 1,
            edit: EffectEdit::Add(EffectKind::LumaKey),
        })
        .unwrap();
    editor
        .execute(Command::Effect {
            id: 1,
            edit: EffectEdit::SetLumaKeyMode {
                effect: 1,
                mode: LumaKeyMode::KeepDarker,
            },
        })
        .unwrap();
    for edit in [
        EffectEdit::ToggleAnimation {
            effect: 1,
            parameter: EffectParam::LumaThreshold,
            frame: 0,
        },
        EffectEdit::SetValue {
            effect: 1,
            parameter: EffectParam::LumaThreshold,
            frame: 30,
            value: 64.0,
        },
        EffectEdit::Bypass {
            effect: 1,
            bypassed: true,
        },
    ] {
        editor.execute(Command::Effect { id: 1, edit }).unwrap();
    }
    editor.execute(Command::DuplicateLayer(1)).unwrap();
    editor.execute(Command::NewComposition).unwrap();
    assert_eq!(editor.project().version, 51);
    assert!(editor.project().composition().layers().is_empty());
    for view in [
        br#" {"version":1,"effect_address":{"layer":1,"effect":1,"parameter":"LumaThreshold"}} "#
            .as_slice(),
        br#" {"version":2,"effect_address":{"layer":1,"effect":1,"parameter":"LumaSoftness"}} "#
            .as_slice(),
    ] {
        let bytes = encode(editor.project(), Some(view)).unwrap();
        assert_eq!(&bytes[..8], MAGIC);
        assert_eq!(u16::from_le_bytes(bytes[8..10].try_into().unwrap()), 1);
        let sections = chunks(&bytes);
        assert_eq!(
            sections.iter().map(|(tag, _)| *tag).collect::<Vec<_>>(),
            [*b"PROJ", *b"VIEW", *b"IMAG"]
        );
        let metadata: Value = serde_json::from_slice(&sections[0].1).unwrap();
        assert_eq!(metadata["version"], 51);
        assert_eq!(sections[1].1.as_slice(), view);
        assert!(!String::from_utf8_lossy(&sections[0].1).contains(PNG));
        let decoded = decode(&bytes).unwrap();
        assert_eq!(&decoded.project, editor.project());
        assert_eq!(decoded.view, Some(view));
        assert_eq!(encode(&decoded.project, decoded.view).unwrap(), bytes);
        let inactive = decoded.project.composition_by_id(1).unwrap();
        let Content::Image { png: a } = inactive.layer(1).unwrap().content() else {
            panic!("image");
        };
        let Content::Image { png: b } = inactive.layer(2).unwrap().content() else {
            panic!("image");
        };
        assert!(Arc::ptr_eq(a, b));
        assert_eq!(a.as_ref(), PNG);
        for layer in inactive.layers() {
            let effect = &layer.effect_stack()[0];
            assert!(effect.bypassed());
            assert_eq!(effect.luma_key_mode(), Some(LumaKeyMode::KeepDarker));
            assert_eq!(effect.value_at(EffectParam::LumaThreshold, 15), 96.0);
        }
        for change in ["old_schema", "missing_mode", "linear_rgb"] {
            let mut sections = sections.clone();
            let mut metadata: Value = serde_json::from_slice(&sections[0].1).unwrap();
            match change {
                "old_schema" => metadata["version"] = 50.into(),
                "missing_mode" => {
                    metadata["other_compositions"]["1"]["layers"][0]["effect_stack"][0]
                        .as_object_mut()
                        .unwrap()
                        .remove("luma_key_mode");
                }
                _ => {
                    metadata["other_compositions"]["1"]["layers"][0]["effect_stack"][0]["color_space"] =
                        "LinearRgb".into()
                }
            }
            sections[0].1 = serde_json::to_vec(&metadata).unwrap();
            assert!(decode(&pack(&sections)).is_err(), "accepted {change}");
        }
    }
}

#[test]
fn luma_nested_scalar_and_mode_noops_keep_native_and_opaque_view_bytes_exact() {
    let mut editor = Editor::default();
    editor.execute(Command::AddRectangle).unwrap();
    for edit in [
        EffectEdit::Add(EffectKind::LumaKey),
        EffectEdit::ToggleAnimation {
            effect: 1,
            parameter: EffectParam::LumaThreshold,
            frame: 0,
        },
        EffectEdit::SetValue {
            effect: 1,
            parameter: EffectParam::LumaThreshold,
            frame: 30,
            value: 64.0,
        },
    ] {
        editor.execute(Command::Effect { id: 1, edit }).unwrap();
    }
    let view = br#" { "version":2, "pin":{"effect":1,"parameter":"LumaThreshold"} } "#;
    let before = encode(editor.project(), Some(view)).unwrap();
    editor
        .execute(Command::Batch(vec![Command::Batch(vec![
            Command::EditTrack {
                id: 1,
                property: PropertyPath::Effect {
                    effect: 1,
                    parameter: EffectParam::LumaThreshold,
                },
                edit: TrackEdit::Value {
                    frame: 15,
                    value: 96.0,
                },
            },
            Command::Effect {
                id: 1,
                edit: EffectEdit::SetLumaKeyMode {
                    effect: 1,
                    mode: LumaKeyMode::KeepBrighter,
                },
            },
        ])]))
        .unwrap();
    assert_eq!(encode(editor.project(), Some(view)).unwrap(), before);
    assert_eq!(decode(&before).unwrap().view, Some(view.as_slice()));
}
