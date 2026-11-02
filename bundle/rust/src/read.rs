use std::collections::{HashMap, HashSet};
use std::fmt;
use std::io::{self, Read, Seek, SeekFrom, Write};

use serde::de::DeserializeOwned;
use serde_json::Value;
use sha2::{Digest, Sha256};
use zip::ZipArchive;

use crate::central_dir::declared_entries;
use crate::manifest::*;
use crate::{sniff, SNIFF_LEN};

/// The only two outcomes a user sees for a refused file.
#[derive(Debug)]
pub enum ReadError {
    /// Not a `.justspeech`, or broken: only another copy of the file helps.
    Damaged(String),
    /// Holds something only a newer app reads: updating helps.
    UpdateRequired(String),
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Damaged(why) => write!(f, "damaged bundle: {why}"),
            Self::UpdateRequired(why) => write!(f, "bundle needs a newer reader: {why}"),
        }
    }
}

impl std::error::Error for ReadError {}

#[derive(Debug)]
pub struct Bundle {
    pub bundle_id: String,
    pub created_at: String,
    pub generator: Option<String>,
    pub title: String,
    pub script: String,
    pub mappings: Option<Vec<SlideMapping>>,
    /// 0 when the bundle kept no deck.
    pub slide_count: usize,
    /// Ids of the parts left out: unknown, invalid, or depending on one that
    /// was. For logs; the rest of the bundle stands without them.
    pub skipped: Vec<String>,
}

pub struct BundleReader<R> {
    archive: ZipArchive<R>,
    bundle: Bundle,
    slides: Vec<String>,
}

/// Why a part was left out. A required part's reason becomes the error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Skip {
    Unknown,
    Invalid,
}

enum Content {
    Script(String),
    Mappings(Vec<SlideMapping>),
    /// Slide entry names in order; the bytes are copied out on demand.
    Deck(Vec<String>),
}

impl<R: Read + Seek> BundleReader<R> {
    /// Validates every part it keeps, each slide included, so a refused file
    /// is refused here and [Self::copy_slide] can only fail on I/O.
    pub fn open(reader: R) -> Result<Self, ReadError> {
        Self::open_with(reader, LIMITS)
    }

    pub(crate) fn open_with(mut reader: R, limits: Limits) -> Result<Self, ReadError> {
        let mut prefix = [0; SNIFF_LEN];
        reader
            .seek(SeekFrom::Start(0))
            .and_then(|_| reader.read_exact(&mut prefix))
            .map_err(|_| damaged("not a .justspeech"))?;
        if !sniff(&prefix) {
            return Err(damaged("not a .justspeech"));
        }
        if reader.seek(SeekFrom::End(0)).map_err(damaged)? > limits.file {
            return Err(damaged(format!("over {} bytes", limits.file)));
        }
        let declared = declared_entries(&mut reader).map_err(damaged)?;
        let mut archive = ZipArchive::new(reader).map_err(damaged)?;
        if declared != archive.len() as u64 {
            return Err(damaged("duplicate entry names"));
        }
        for i in 0..archive.len() {
            let name = archive.name_for_index(i).unwrap_or_default();
            if !safe_name(name) {
                return Err(damaged(format!("unsafe entry name {name:?}")));
            }
        }

        let manifest = read_manifest(&mut archive, limits)?;
        let mut skipped = HashMap::new();
        let mut contents = Vec::new();
        for part in &manifest.parts {
            match read_part(&mut archive, part, limits) {
                Ok(content) => contents.push((part.id.as_str(), content)),
                Err(skip) => {
                    skipped.insert(part.id.as_str(), skip);
                }
            }
        }
        skip_dependents(&manifest.parts, &mut skipped);

        for part in manifest.parts.iter().filter(|p| p.required) {
            match skipped.get(part.id.as_str()) {
                Some(Skip::Unknown) => return Err(update_required(format!("part {}", part.id))),
                Some(Skip::Invalid) => return Err(damaged(format!("part {}", part.id))),
                None => {}
            }
        }

        let mut script = None;
        let mut mappings = None;
        let mut slides = Vec::new();
        for (id, content) in contents {
            if skipped.contains_key(id) {
                continue;
            }
            match content {
                Content::Script(text) => script = Some(text),
                Content::Mappings(m) => mappings = Some(m),
                Content::Deck(paths) => slides = paths,
            }
        }
        // A writer may leave `required` off the script; nothing imports without one.
        let Some(script) = script else {
            let unknown = manifest
                .parts
                .iter()
                .any(|p| p.kind == SCRIPT && skipped.get(p.id.as_str()) == Some(&Skip::Unknown));
            return Err(if unknown {
                update_required("script")
            } else {
                damaged("no script")
            });
        };

        let mut skipped_ids: Vec<String> = skipped.keys().map(|id| id.to_string()).collect();
        skipped_ids.sort();
        let bundle = Bundle {
            bundle_id: manifest.bundle_id,
            created_at: manifest.created_at,
            generator: manifest.generator,
            title: manifest.title,
            script,
            mappings,
            slide_count: slides.len(),
            skipped: skipped_ids,
        };
        Ok(Self {
            archive,
            bundle,
            slides,
        })
    }

    pub fn bundle(&self) -> &Bundle {
        &self.bundle
    }

    /// Copies the PNG of slide `index` (0-based, below
    /// [Bundle::slide_count]) into `out`. On error `out` holds a partial copy
    /// for the caller to discard.
    pub fn copy_slide<W: Write>(&mut self, index: usize, out: &mut W) -> io::Result<()> {
        let Some(path) = self.slides.get(index) else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("no slide {index}"),
            ));
        };
        let mut entry = self.archive.by_name(path)?;
        io::copy(&mut entry, out)?;
        Ok(())
    }
}

/// Entries are never extracted by name, but a name another tool would put
/// outside its folder marks a crafted file.
fn safe_name(name: &str) -> bool {
    let body = name.strip_suffix('/').unwrap_or(name);
    !body.is_empty()
        && !body.contains(['\\', ':', '\0'])
        && body
            .split('/')
            .all(|s| !s.is_empty() && s != "." && s != "..")
}

fn read_manifest<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    limits: Limits,
) -> Result<Manifest, ReadError> {
    let mut raw = Vec::new();
    stream_entry(archive, MANIFEST_PATH, limits.manifest, None, &mut raw)
        .map_err(|_| damaged("unreadable manifest"))?;
    let value: Value = serde_json::from_slice(&raw).map_err(damaged)?;
    // Before the typed parse: a newer format may have changed any other field.
    match value.get("formatVersion").and_then(Value::as_u64) {
        Some(v) if v > FORMAT_VERSION => return Err(update_required(format!("formatVersion {v}"))),
        Some(v) if v >= 1 => {}
        _ => return Err(damaged("formatVersion")),
    }
    let manifest: Manifest = serde_json::from_value(value).map_err(damaged)?;

    if manifest.bundle_id.is_empty() {
        return Err(damaged("empty bundleId"));
    }
    if !title_fits(&manifest.title) {
        return Err(damaged("title too long"));
    }
    let mut ids = HashSet::new();
    for part in &manifest.parts {
        if !part_id_valid(&part.id) || !ids.insert(part.id.as_str()) {
            return Err(damaged(format!("part id {:?}", part.id)));
        }
    }
    for kind in V1_TYPES {
        if manifest.parts.iter().filter(|p| p.kind == kind).count() > 1 {
            return Err(damaged(format!("more than one {kind}")));
        }
    }
    Ok(manifest)
}

fn read_part<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    part: &PartDecl,
    limits: Limits,
) -> Result<Content, Skip> {
    match part.kind.as_str() {
        SCRIPT => {
            check_schema(part, SCRIPT_SCHEMA)?;
            let script: ScriptPart = read_json(archive, part, limits.script)?;
            if !script_valid(&script.text) {
                return Err(Skip::Invalid);
            }
            Ok(Content::Script(script.text))
        }
        SLIDE_MAPPING => {
            check_schema(part, MAPPING_SCHEMA)?;
            let mapping: MappingPart = read_json(archive, part, limits.mapping)?;
            if mapping.token_unit != WHITESPACE {
                return Err(Skip::Unknown);
            }
            if !mappings_valid(&mapping.mappings) {
                return Err(Skip::Invalid);
            }
            Ok(Content::Mappings(mapping.mappings))
        }
        DECK => {
            check_schema(part, DECK_SCHEMA)?;
            let deck: DeckPart = read_json(archive, part, limits.deck_index)?;
            if deck.slides.iter().any(|s| s.media_type != PNG) {
                return Err(Skip::Unknown);
            }
            // Partly read, the deck would move every later slide's mapping.
            if deck.slides.is_empty() || deck.slides.len() > MAX_SLIDE as usize {
                return Err(Skip::Invalid);
            }
            let mut left = limits.deck;
            for slide in &deck.slides {
                let mut head = Head::default();
                left -= stream_entry(
                    archive,
                    &slide.path,
                    left,
                    slide.sha256.as_deref(),
                    &mut head,
                )?;
                if !slide_head_valid(&head.0) {
                    return Err(Skip::Invalid);
                }
            }
            Ok(Content::Deck(
                deck.slides.into_iter().map(|s| s.path).collect(),
            ))
        }
        _ => Err(Skip::Unknown),
    }
}

/// A schema newer than this build reads is unknown; a JSON part without one is
/// invalid.
fn check_schema(part: &PartDecl, known: u32) -> Result<(), Skip> {
    match part.schema {
        Some(schema) if schema > known => Err(Skip::Unknown),
        Some(schema) if schema >= 1 => Ok(()),
        _ => Err(Skip::Invalid),
    }
}

fn read_json<R: Read + Seek, T: DeserializeOwned>(
    archive: &mut ZipArchive<R>,
    part: &PartDecl,
    cap: u64,
) -> Result<T, Skip> {
    let mut raw = Vec::new();
    stream_entry(archive, &part.path, cap, part.sha256.as_deref(), &mut raw)?;
    serde_json::from_slice(&raw).map_err(|_| Skip::Invalid)
}

/// Stops at `cap` bytes actually read: the sizes an entry header declares are
/// the writer's word, not a bound. Returns the bytes read.
fn stream_entry<R: Read + Seek, W: Write>(
    archive: &mut ZipArchive<R>,
    path: &str,
    cap: u64,
    sha256: Option<&str>,
    out: &mut W,
) -> Result<u64, Skip> {
    let mut entry = archive.by_name(path).map_err(|_| Skip::Invalid)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0; 64 * 1024];
    let mut total = 0u64;
    loop {
        let n = entry.read(&mut buf).map_err(|_| Skip::Invalid)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > cap {
            return Err(Skip::Invalid);
        }
        hasher.update(&buf[..n]);
        out.write_all(&buf[..n]).map_err(|_| Skip::Invalid)?;
    }
    if let Some(expected) = sha256 {
        if !hex(&hasher.finalize()).eq_ignore_ascii_case(expected) {
            return Err(Skip::Invalid);
        }
    }
    Ok(total)
}

/// Keeps the first [PNG_HEAD_LEN] bytes written to it.
#[derive(Default)]
pub(crate) struct Head(pub Vec<u8>);

impl Write for Head {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let take = PNG_HEAD_LEN.saturating_sub(self.0.len()).min(buf.len());
        self.0.extend_from_slice(&buf[..take]);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Leaves out every part that refers to a skipped or absent one, transitively.
/// It inherits the reason; an absent target makes the manifest itself wrong.
fn skip_dependents<'a>(parts: &'a [PartDecl], skipped: &mut HashMap<&'a str, Skip>) {
    let ids: HashSet<&str> = parts.iter().map(|p| p.id.as_str()).collect();
    loop {
        let mut changed = false;
        for part in parts {
            if skipped.contains_key(part.id.as_str()) {
                continue;
            }
            let reason = part
                .refs
                .iter()
                .find_map(|r| match skipped.get(r.as_str()) {
                    Some(&skip) => Some(skip),
                    None if !ids.contains(r.as_str()) => Some(Skip::Invalid),
                    None => None,
                });
            if let Some(reason) = reason {
                skipped.insert(part.id.as_str(), reason);
                changed = true;
            }
        }
        if !changed {
            return;
        }
    }
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn damaged(why: impl fmt::Display) -> ReadError {
    ReadError::Damaged(why.to_string())
}

fn update_required(why: impl fmt::Display) -> ReadError {
    ReadError::UpdateRequired(why.to_string())
}
