use std::io::{Cursor, Read, Write};

use serde_json::{json, Value};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::manifest::{Limits, LIMITS};
use crate::write::write_with;
use crate::{sniff, write, BundleReader, NewBundle, ReadError, SlideMapping, MIME, SNIFF_LEN};

const SCRIPT_TEXT: &str = "안녕하세요. 오늘은 3분기 실적을 말씀드리겠습니다. 먼저 매출입니다.";
const SCRIPT_JSON: &[u8] = r#"{"text":"안녕하세요. 발표를 시작하겠습니다."}"#.as_bytes();
const DECK: &[u8] = b"%PDF-1.4\nnot rendered here\n%%EOF\n";

fn mapping(start: u32, end: u32, slide: u32) -> SlideMapping {
    SlideMapping {
        start_token: start,
        end_token: end,
        slide,
        title: None,
        disabled: false,
    }
}

fn exported(
    title: &str,
    mappings: &[SlideMapping],
    deck: Option<&[u8]>,
) -> anyhow::Result<Vec<u8>> {
    let bundle = NewBundle {
        bundle_id: "6f1c2a9e-0b7d-4f3a-9c55-2e8d1b4a7f10",
        created_at: "2026-11-02T10:00:00Z",
        generator: Some("test"),
        title,
        script: SCRIPT_TEXT,
        mappings,
    };
    let mut deck = deck.map(Cursor::new);
    let out = write(
        Cursor::new(Vec::new()),
        &bundle,
        deck.as_mut().map(|d| d as &mut dyn Read),
    )?;
    Ok(out.into_inner())
}

/// Entry by entry, stored, `mimetype` first and `manifest.json` last: files
/// this crate's writer would never produce.
fn crafted(manifest: &Value, entries: &[(&str, &[u8])]) -> Vec<u8> {
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file("mimetype", stored).unwrap();
    zip.write_all(MIME.as_bytes()).unwrap();
    for (name, bytes) in entries {
        zip.start_file(*name, stored).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.start_file("manifest.json", stored).unwrap();
    zip.write_all(manifest.to_string().as_bytes()).unwrap();
    zip.finish().unwrap().into_inner()
}

fn manifest(parts: Value) -> Value {
    json!({
        "formatVersion": 1,
        "bundleId": "b1",
        "createdAt": "2026-11-02T10:00:00Z",
        "title": "제목",
        "parts": parts,
    })
}

fn script_decl() -> Value {
    json!({ "id": "script", "type": "script", "schema": 1, "path": "parts/script.json", "required": true })
}

fn mapping_decl() -> Value {
    json!({ "id": "mapping", "type": "slide-mapping", "schema": 1, "path": "parts/mapping.json", "refs": ["script"] })
}

fn deck_decl() -> Value {
    json!({ "id": "deck", "type": "deck", "mediaType": "application/pdf", "path": "parts/deck.pdf" })
}

const MAPPING_JSON: &[u8] =
    br#"{"tokenUnit":"whitespace","mappings":[{"startToken":0,"endToken":1,"slide":1}]}"#;

fn open(bytes: Vec<u8>) -> BundleReader<Cursor<Vec<u8>>> {
    match BundleReader::open(Cursor::new(bytes)) {
        Ok(reader) => reader,
        Err(e) => panic!("refused: {e}"),
    }
}

fn refused(bytes: Vec<u8>) -> ReadError {
    refused_with(bytes, LIMITS)
}

fn refused_with(bytes: Vec<u8>, limits: Limits) -> ReadError {
    match BundleReader::open_with(Cursor::new(bytes), limits) {
        Ok(_) => panic!("opened"),
        Err(e) => e,
    }
}

fn is_damaged(e: ReadError) -> bool {
    matches!(e, ReadError::Damaged(_))
}

fn damaged_by(e: ReadError, check: &str) -> bool {
    matches!(e, ReadError::Damaged(why) if why.contains(check))
}

fn needs_update(e: ReadError) -> bool {
    matches!(e, ReadError::UpdateRequired(_))
}

#[test]
fn export_reads_back_whole() {
    let mut titled = mapping(9, 12, 2);
    titled.title = Some("매출".into());
    titled.disabled = true;
    let mappings = vec![mapping(0, 8, 1), titled];
    let mut reader = open(exported("3분기 실적 발표", &mappings, Some(DECK)).unwrap());

    let bundle = reader.bundle();
    assert_eq!(bundle.bundle_id, "6f1c2a9e-0b7d-4f3a-9c55-2e8d1b4a7f10");
    assert_eq!(bundle.created_at, "2026-11-02T10:00:00Z");
    assert_eq!(bundle.generator.as_deref(), Some("test"));
    assert_eq!(bundle.title, "3분기 실적 발표");
    assert_eq!(bundle.script, SCRIPT_TEXT);
    assert_eq!(bundle.mappings.as_deref(), Some(&mappings[..]));
    assert!(bundle.has_deck);
    assert!(bundle.skipped.is_empty());

    let mut deck = Vec::new();
    assert!(reader.copy_deck(&mut deck).unwrap());
    assert_eq!(deck, DECK);
}

#[test]
fn script_alone_is_a_bundle() {
    let mut reader = open(exported("", &[], None).unwrap());
    assert_eq!(reader.bundle().title, "");
    assert_eq!(reader.bundle().mappings, None);
    assert!(!reader.bundle().has_deck);
    assert!(!reader.copy_deck(&mut Vec::new()).unwrap());
}

#[test]
fn sniff_sees_the_mimetype_entry_only() {
    let bytes = exported("t", &[], Some(DECK)).unwrap();
    assert!(sniff(&bytes[..SNIFF_LEN]));
    assert!(!sniff(&bytes[..SNIFF_LEN - 1]));

    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file("a.txt", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(MIME.as_bytes()).unwrap();
    let other = zip.finish().unwrap().into_inner();
    assert!(!sniff(&other));
}

#[test]
fn not_a_bundle_is_damaged() {
    assert!(is_damaged(refused(Vec::new())));
    assert!(is_damaged(refused(
        b"%PDF-1.4 definitely not a zip".to_vec()
    )));

    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file("manifest.json", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(manifest(json!([script_decl()])).to_string().as_bytes())
        .unwrap();
    assert!(is_damaged(refused(zip.finish().unwrap().into_inner())));
}

#[test]
fn missing_or_malformed_manifest_is_damaged() {
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file("mimetype", stored).unwrap();
    zip.write_all(MIME.as_bytes()).unwrap();
    assert!(is_damaged(refused(zip.finish().unwrap().into_inner())));

    let mut no_title = manifest(json!([script_decl()]));
    no_title.as_object_mut().unwrap().remove("title");
    assert!(is_damaged(refused(crafted(
        &no_title,
        &[("parts/script.json", SCRIPT_JSON)]
    ))));

    let zero =
        json!({ "formatVersion": 0, "bundleId": "b1", "createdAt": "", "title": "", "parts": [] });
    assert!(is_damaged(refused(crafted(&zero, &[]))));
}

#[test]
fn newer_format_version_needs_update_whatever_else_changed() {
    let future = json!({ "formatVersion": 2, "parts": "no longer a list" });
    assert!(needs_update(refused(crafted(&future, &[]))));
}

#[test]
fn unknown_fields_and_optional_parts_are_ignored() {
    let mut m = manifest(json!([
        script_decl(),
        { "id": "notes", "type": "speaker-notes", "schema": 1, "path": "parts/notes.json", "encoding": "x" },
    ]));
    m["theme"] = json!({ "accent": "blue" });
    // The unknown part's entry is absent: skipped parts are never opened.
    let reader = open(crafted(&m, &[("parts/script.json", SCRIPT_JSON)]));
    assert_eq!(reader.bundle().skipped, ["notes"]);
    assert_eq!(reader.bundle().script, "안녕하세요. 발표를 시작하겠습니다.");
}

#[test]
fn unknown_required_part_needs_update() {
    let m = manifest(json!([
        script_decl(),
        { "id": "cues", "type": "cue-track", "schema": 1, "path": "parts/cues.json", "required": true },
    ]));
    assert!(needs_update(refused(crafted(
        &m,
        &[("parts/script.json", SCRIPT_JSON)]
    ))));
}

#[test]
fn newer_schema_is_unknown() {
    let mut newer_mapping = mapping_decl();
    newer_mapping["schema"] = json!(2);
    let m = manifest(json!([script_decl(), newer_mapping]));
    let entries: &[(&str, &[u8])] = &[
        ("parts/script.json", SCRIPT_JSON),
        ("parts/mapping.json", MAPPING_JSON),
    ];
    let reader = open(crafted(&m, entries));
    assert_eq!(reader.bundle().skipped, ["mapping"]);
    assert_eq!(reader.bundle().mappings, None);

    let mut newer_script = script_decl();
    newer_script["schema"] = json!(2);
    assert!(needs_update(refused(crafted(
        &manifest(json!([newer_script])),
        entries
    ))));

    // A writer that left `required` off still cannot hand over a script this reads.
    newer_script["required"] = json!(false);
    assert!(needs_update(refused(crafted(
        &manifest(json!([newer_script])),
        entries
    ))));
}

#[test]
fn unsupported_deck_media_type_is_skipped() {
    let mut pptx = deck_decl();
    pptx["mediaType"] =
        json!("application/vnd.openxmlformats-officedocument.presentationml.presentation");
    let m = manifest(json!([script_decl(), pptx]));
    let reader = open(crafted(
        &m,
        &[("parts/script.json", SCRIPT_JSON), ("parts/deck.pdf", DECK)],
    ));
    assert_eq!(reader.bundle().skipped, ["deck"]);
    assert!(!reader.bundle().has_deck);
}

#[test]
fn dependents_of_a_skipped_part_are_skipped() {
    let notes =
        json!({ "id": "notes", "type": "speaker-notes", "schema": 1, "path": "parts/notes.json" });
    let entries: &[(&str, &[u8])] = &[
        ("parts/script.json", SCRIPT_JSON),
        ("parts/mapping.json", MAPPING_JSON),
    ];

    let mut on_notes = mapping_decl();
    on_notes["refs"] = json!(["script", "notes"]);
    let reader = open(crafted(
        &manifest(json!([script_decl(), on_notes, notes.clone()])),
        entries,
    ));
    assert_eq!(reader.bundle().skipped, ["mapping", "notes"]);

    let mut on_ghost = mapping_decl();
    on_ghost["refs"] = json!(["ghost"]);
    let reader = open(crafted(
        &manifest(json!([script_decl(), on_ghost])),
        entries,
    ));
    assert_eq!(reader.bundle().skipped, ["mapping"]);

    let mut script_on_notes = script_decl();
    script_on_notes["refs"] = json!(["notes"]);
    assert!(needs_update(refused(crafted(
        &manifest(json!([script_on_notes, notes])),
        entries
    ))));
}

#[test]
fn unsafe_entry_names_are_damaged() {
    let m = manifest(json!([script_decl()]));
    for name in [
        "../evil",
        "/abs",
        "parts\\x",
        "C:x",
        "parts//x",
        "./x",
        "a/../../x",
    ] {
        let bytes = crafted(&m, &[("parts/script.json", SCRIPT_JSON), (name, b"x")]);
        assert!(damaged_by(refused(bytes), "unsafe entry name"), "{name}");
    }
    // Directory entries, as archivers add them, are fine.
    open(crafted(
        &m,
        &[("parts/", b""), ("parts/script.json", SCRIPT_JSON)],
    ));
}

#[test]
fn duplicate_entry_names_are_damaged() {
    let m = manifest(json!([script_decl()]));
    let mut bytes = crafted(
        &m,
        &[
            ("parts/script.json", SCRIPT_JSON),
            ("parts/scrip2.json", SCRIPT_JSON),
        ],
    );
    // Rename the second entry in its local header and the central directory.
    let (from, to) = (b"parts/scrip2.json", b"parts/script.json");
    let mut renamed = 0;
    let mut i = 0;
    while let Some(at) = bytes[i..].windows(from.len()).position(|w| w == from) {
        bytes[i + at..i + at + to.len()].copy_from_slice(to);
        i += at + to.len();
        renamed += 1;
    }
    assert_eq!(renamed, 2);
    assert!(damaged_by(refused(bytes), "duplicate entry names"));
}

#[test]
fn repeated_part_ids_or_v1_types_are_damaged() {
    let entries: &[(&str, &[u8])] = &[("parts/script.json", SCRIPT_JSON), ("parts/deck.pdf", DECK)];
    let mut second_deck = deck_decl();
    second_deck["id"] = json!("deck-2");
    assert!(is_damaged(refused(crafted(
        &manifest(json!([script_decl(), deck_decl(), second_deck])),
        entries
    ))));

    let mut deck_as_script = deck_decl();
    deck_as_script["id"] = json!("script");
    assert!(is_damaged(refused(crafted(
        &manifest(json!([script_decl(), deck_as_script])),
        entries
    ))));

    let mut bad_id = deck_decl();
    bad_id["id"] = json!("Deck");
    assert!(is_damaged(refused(crafted(
        &manifest(json!([script_decl(), bad_id])),
        entries
    ))));
}

#[test]
fn bad_script_is_damaged() {
    let m = manifest(json!([script_decl()]));
    let blank = r#"{"text":" \n\t "}"#.as_bytes();
    assert!(is_damaged(refused(crafted(
        &m,
        &[("parts/script.json", blank)]
    ))));
    assert!(is_damaged(refused(crafted(
        &m,
        &[("parts/script.json", b"not json")]
    ))));
    assert!(is_damaged(refused(crafted(&m, &[]))));

    let mut no_schema = script_decl();
    no_schema.as_object_mut().unwrap().remove("schema");
    assert!(is_damaged(refused(crafted(
        &manifest(json!([no_schema])),
        &[("parts/script.json", SCRIPT_JSON)]
    ))));

    assert!(is_damaged(refused(crafted(&manifest(json!([])), &[]))));
}

#[test]
fn bad_mapping_is_skipped() {
    let m = manifest(json!([script_decl(), mapping_decl()]));
    let cases: [&[u8]; 5] = [
        br#"{"tokenUnit":"whitespace","mappings":[{"startToken":0,"endToken":1,"slide":0}]}"#,
        br#"{"tokenUnit":"whitespace","mappings":[{"startToken":-1,"endToken":1,"slide":1}]}"#,
        br#"{"tokenUnit":"grapheme","mappings":[{"startToken":0,"endToken":1,"slide":1}]}"#,
        br#"{"mappings":[]}"#,
        br#"[]"#,
    ];
    for json in cases {
        let reader = open(crafted(
            &m,
            &[
                ("parts/script.json", SCRIPT_JSON),
                ("parts/mapping.json", json),
            ],
        ));
        assert_eq!(
            reader.bundle().skipped,
            ["mapping"],
            "{}",
            String::from_utf8_lossy(json)
        );
    }

    let many: Vec<Value> = (0..201)
        .map(|i| json!({ "startToken": i, "endToken": i, "slide": 1 }))
        .collect();
    let json = json!({ "tokenUnit": "whitespace", "mappings": many }).to_string();
    let reader = open(crafted(
        &m,
        &[
            ("parts/script.json", SCRIPT_JSON),
            ("parts/mapping.json", json.as_bytes()),
        ],
    ));
    assert_eq!(reader.bundle().skipped, ["mapping"]);
}

#[test]
fn null_disabled_is_false() {
    let m = manifest(json!([script_decl(), mapping_decl()]));
    let json = br#"{"tokenUnit":"whitespace","mappings":[{"startToken":0,"endToken":1,"slide":1,"title":null,"disabled":null}]}"#;
    let reader = open(crafted(
        &m,
        &[
            ("parts/script.json", SCRIPT_JSON),
            ("parts/mapping.json", json),
        ],
    ));
    assert_eq!(reader.bundle().mappings, Some(vec![mapping(0, 1, 1)]));
}

#[test]
fn parts_over_their_cap_are_invalid() {
    let small = Limits {
        deck: DECK.len() as u64 - 1,
        ..LIMITS
    };
    let m = manifest(json!([script_decl(), deck_decl()]));
    let bytes = crafted(
        &m,
        &[("parts/script.json", SCRIPT_JSON), ("parts/deck.pdf", DECK)],
    );
    let reader = BundleReader::open_with(Cursor::new(bytes.clone()), small).unwrap();
    assert_eq!(reader.bundle().skipped, ["deck"]);
    BundleReader::open_with(
        Cursor::new(bytes.clone()),
        Limits {
            deck: DECK.len() as u64,
            ..LIMITS
        },
    )
    .unwrap();

    let tiny_script = Limits {
        script: 4,
        ..LIMITS
    };
    assert!(is_damaged(refused_with(bytes.clone(), tiny_script)));
    let tiny_manifest = Limits {
        manifest: 4,
        ..LIMITS
    };
    assert!(is_damaged(refused_with(bytes, tiny_manifest)));
}

#[test]
fn hash_mismatch_is_invalid() {
    let mut wrong = deck_decl();
    wrong["sha256"] = json!("00".repeat(32));
    let m = manifest(json!([script_decl(), wrong]));
    let reader = open(crafted(
        &m,
        &[("parts/script.json", SCRIPT_JSON), ("parts/deck.pdf", DECK)],
    ));
    assert_eq!(reader.bundle().skipped, ["deck"]);

    let mut wrong_script = script_decl();
    wrong_script["sha256"] = json!("00".repeat(32));
    let m = manifest(json!([wrong_script]));
    assert!(is_damaged(refused(crafted(
        &m,
        &[("parts/script.json", SCRIPT_JSON)]
    ))));
}

#[test]
fn writer_refuses_what_a_reader_would() {
    let title_255 = "😀".repeat(127) + "a";
    exported(&title_255, &[], None).unwrap();
    let title_256 = "😀".repeat(128);
    assert!(exported(&title_256, &[], None).is_err());
    assert!(exported("t", &[mapping(0, 1, 201)], None).is_err());

    let bundle = NewBundle {
        bundle_id: "b1",
        created_at: "2026-11-02T10:00:00Z",
        generator: None,
        title: "t",
        script: "  \n",
        mappings: &[],
    };
    assert!(write(Cursor::new(Vec::new()), &bundle, None).is_err());
}

#[test]
fn writer_refuses_json_past_the_readers_caps() {
    fn written(bundle: &NewBundle, limits: Limits) -> anyhow::Result<Vec<u8>> {
        Ok(write_with(Cursor::new(Vec::new()), bundle, None, limits)?.into_inner())
    }
    let base = NewBundle {
        bundle_id: "b1",
        created_at: "2026-11-02T10:00:00Z",
        generator: None,
        title: "t",
        script: "hi",
        mappings: &[],
    };
    // {"text":"hi"} is 13 bytes: at the cap it is written and read back.
    let at_cap = Limits {
        script: 13,
        ..LIMITS
    };
    let bytes = written(&base, at_cap).unwrap();
    BundleReader::open_with(Cursor::new(bytes), at_cap).unwrap();

    // 15 quotes escape to 30 bytes: under the cap as text, over it as JSON.
    let quotes = "\"".repeat(15);
    let escaped = NewBundle {
        script: &quotes,
        ..base
    };
    let cap = Limits {
        script: 40,
        ..LIMITS
    };
    assert!(quotes.len() < 40);
    assert!(written(&escaped, cap).is_err());

    let long_generator = "g".repeat(400);
    let named = NewBundle {
        generator: Some(&long_generator),
        ..base
    };
    let small_manifest = Limits {
        manifest: 300,
        ..LIMITS
    };
    assert!(written(&named, small_manifest).is_err());

    let mappings = [mapping(0, 0, 1), mapping(1, 1, 2)];
    let mapped = NewBundle {
        mappings: &mappings,
        ..base
    };
    let small_mapping = Limits {
        mapping: 30,
        ..LIMITS
    };
    assert!(written(&mapped, small_mapping).is_err());
}
