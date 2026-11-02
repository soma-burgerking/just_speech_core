//! Files the writer produced at each format and schema version, kept as
//! bytes: a reader change that cannot open one breaks files already shared.
//! Add a fixture when the writer's output changes; never regenerate one.

use std::fs::File;

use bundle::{sniff, BundleReader, SlideMapping, SNIFF_LEN};

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

    let mut deck = Vec::new();
    assert!(reader.copy_deck(&mut deck).unwrap());
    assert!(deck.starts_with(b"%PDF-1.4"));
    assert_eq!(deck.len(), 674);
}
