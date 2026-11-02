//! Files the writer produced at each format and schema version, kept as
//! bytes: a reader change that cannot open one breaks files already shared.
//! Add a fixture when the writer's output changes; never regenerate one.

use std::fs::File;

use bundle::{sniff, BundleReader, SlideMapping, SNIFF_LEN};
use sha2::{Digest, Sha256};

const V1: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/v1.justspeech");

#[test]
fn v1_opens() {
    let bytes = std::fs::read(V1).unwrap();
    assert!(sniff(&bytes[..SNIFF_LEN]));

    let mut reader = BundleReader::open(File::open(V1).unwrap()).unwrap();
    let bundle = reader.bundle();
    assert_eq!(bundle.bundle_id, "0b5d6e1a-7c3f-4e2b-9a81-d4f0c2e6b7a9");
    assert_eq!(bundle.created_at, "2026-11-02T00:00:00Z");
    assert_eq!(bundle.title, "3분기 실적 발표");
    assert_eq!(
        bundle.script,
        "안녕하세요. 오늘은 3분기 실적을 말씀드리겠습니다.\n먼저 매출입니다. 매출은 전년 대비 12% 늘었습니다.\n마지막으로 다음 분기 계획입니다."
    );
    let m = |start, end, slide, title: Option<&str>, disabled| SlideMapping {
        start_token: start,
        end_token: end,
        slide,
        title: title.map(Into::into),
        disabled,
    };
    assert_eq!(
        bundle.mappings,
        Some(vec![
            m(0, 4, 1, None, false),
            m(5, 11, 2, Some("매출"), false),
            m(12, 15, 3, None, true)
        ])
    );
    assert!(bundle.skipped.is_empty());

    assert_eq!(bundle.slide_count, 3);
    let hashes = [
        "d7ddac033f95bab68e97a3f05b978275b4341afd3ec099a61ca6ff37f4d57b90",
        "2157fa0fccf0d9ee8ddcb49d42d136f5598cab2122885d1c38403817b8e52bb6",
        "cfde7b04be11702b3c04d4276cfc9f138adbe07408eb34a774c6963c35092657",
    ];
    for (i, expected) in hashes.iter().enumerate() {
        let mut slide = Vec::new();
        reader.copy_slide(i, &mut slide).unwrap();
        assert_eq!(hex(&Sha256::digest(&slide)), *expected, "slide {}", i + 1);
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
