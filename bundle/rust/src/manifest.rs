use serde::{Deserialize, Deserializer, Serialize};

pub(crate) const FORMAT_VERSION: u64 = 1;

pub(crate) const MIMETYPE_PATH: &str = "mimetype";
pub(crate) const MANIFEST_PATH: &str = "manifest.json";
pub(crate) const SCRIPT_PATH: &str = "parts/script.json";
pub(crate) const DECK_PATH: &str = "parts/deck.pdf";
pub(crate) const MAPPING_PATH: &str = "parts/mapping.json";

pub(crate) const SCRIPT: &str = "script";
pub(crate) const DECK: &str = "deck";
pub(crate) const SLIDE_MAPPING: &str = "slide-mapping";
pub(crate) const V1_TYPES: [&str; 3] = [SCRIPT, DECK, SLIDE_MAPPING];

pub(crate) const SCRIPT_SCHEMA: u32 = 1;
pub(crate) const MAPPING_SCHEMA: u32 = 1;
pub(crate) const PDF: &str = "application/pdf";
/// Tokens are `script.text` split on ECMAScript `\s+`, empty pieces dropped:
/// how the server and the app's `countScriptTokens` count.
pub(crate) const WHITESPACE: &str = "whitespace";

/// The server's caps on `title` and `scriptSlideMappings` (REQUEST-400); the
/// 200s are the render pipeline's slide cap.
const MAX_TITLE: usize = 255;
const MAX_MAPPINGS: usize = 200;
const MAX_SLIDE: u32 = 200;

/// Bytes read before a part counts as invalid. The deck's is the render
/// pipeline's MAX_FILE_MB, so every bundled deck is one the server can render.
#[derive(Clone, Copy)]
pub(crate) struct Limits {
    pub manifest: u64,
    pub script: u64,
    pub mapping: u64,
    pub deck: u64,
}

pub(crate) const LIMITS: Limits = Limits {
    manifest: 1 << 20,
    script: 2 << 20,
    mapping: 1 << 20,
    deck: 100 << 20,
};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Manifest {
    pub format_version: u64,
    pub bundle_id: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generator: Option<String>,
    pub title: String,
    pub parts: Vec<PartDecl>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PartDecl {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refs: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct ScriptPart {
    pub text: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MappingPart {
    pub token_unit: String,
    pub mappings: Vec<SlideMapping>,
}

/// The server's `SlideMapping`, field for field: whitespace-token range
/// (0-based, inclusive) and the 1-based slide it shows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlideMapping {
    pub start_token: u32,
    pub end_token: u32,
    pub slide: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(
        default,
        deserialize_with = "null_as_false",
        skip_serializing_if = "is_false"
    )]
    pub disabled: bool,
}

/// UTF-16 units, as the server's Kotlin `String.length` counts.
pub(crate) fn title_fits(title: &str) -> bool {
    title.encode_utf16().count() <= MAX_TITLE
}

pub(crate) fn script_valid(text: &str) -> bool {
    !text.trim().is_empty()
}

pub(crate) fn mappings_valid(mappings: &[SlideMapping]) -> bool {
    mappings.len() <= MAX_MAPPINGS
        && mappings.iter().all(|m| {
            (1..=MAX_SLIDE).contains(&m.slide) && m.title.as_deref().is_none_or(title_fits)
        })
}

pub(crate) fn part_id_valid(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn is_false(value: &bool) -> bool {
    !value
}

fn null_as_false<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    Ok(Option::<bool>::deserialize(d)?.unwrap_or(false))
}
