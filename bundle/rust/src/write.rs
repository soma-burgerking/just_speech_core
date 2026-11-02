use std::io::{Read, Seek, Write};

use anyhow::{bail, ensure, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::manifest::*;
use crate::read::hex;
use crate::MIME;

pub struct NewBundle<'a> {
    /// A fresh UUID per export: it is how a reader tells a file it took before.
    pub bundle_id: &'a str,
    /// RFC 3339, UTC.
    pub created_at: &'a str,
    pub generator: Option<&'a str>,
    pub title: &'a str,
    pub script: &'a str,
    /// Empty writes no mapping part.
    pub mappings: &'a [SlideMapping],
}

/// Refuses what a reader would refuse or leave out, so every archive it
/// finishes opens whole. On error `out` holds a partial archive to discard.
pub fn write<W: Write + Seek>(
    out: W,
    bundle: &NewBundle,
    deck: Option<&mut dyn Read>,
) -> Result<W> {
    write_with(out, bundle, deck, LIMITS)
}

/// [write] against the caps a reader opening with `limits` applies.
pub(crate) fn write_with<W: Write + Seek>(
    out: W,
    bundle: &NewBundle,
    deck: Option<&mut dyn Read>,
    limits: Limits,
) -> Result<W> {
    ensure!(!bundle.bundle_id.is_empty(), "empty bundle id");
    ensure!(title_fits(bundle.title), "title too long");
    ensure!(script_valid(bundle.script), "blank script");
    ensure!(
        mappings_valid(bundle.mappings),
        "mappings past the server's caps"
    );

    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut zip = ZipWriter::new(out);
    zip.start_file(MIMETYPE_PATH, stored)?;
    zip.write_all(MIME.as_bytes())?;

    zip.start_file(SCRIPT_PATH, deflated)?;
    let script = ScriptPart {
        text: bundle.script.to_owned(),
    };
    zip.write_all(&json_within(&script, limits.script, "script")?)?;
    let mut parts = vec![PartDecl {
        id: SCRIPT.into(),
        kind: SCRIPT.into(),
        path: SCRIPT_PATH.into(),
        schema: Some(SCRIPT_SCHEMA),
        media_type: None,
        required: true,
        refs: vec![],
        sha256: None,
    }];

    if let Some(deck) = deck {
        // PDF streams are compressed already; deflate would only cost time.
        zip.start_file(DECK_PATH, stored)?;
        let sha256 = copy_hashed(deck, &mut zip, limits.deck)?;
        parts.push(PartDecl {
            id: DECK.into(),
            kind: DECK.into(),
            path: DECK_PATH.into(),
            schema: None,
            media_type: Some(PDF.into()),
            required: false,
            refs: vec![],
            sha256: Some(sha256),
        });
    }

    if !bundle.mappings.is_empty() {
        zip.start_file(MAPPING_PATH, deflated)?;
        let mapping = MappingPart {
            token_unit: WHITESPACE.into(),
            mappings: bundle.mappings.to_vec(),
        };
        zip.write_all(&json_within(&mapping, limits.mapping, "mapping")?)?;
        parts.push(PartDecl {
            id: SLIDE_MAPPING.into(),
            kind: SLIDE_MAPPING.into(),
            path: MAPPING_PATH.into(),
            schema: Some(MAPPING_SCHEMA),
            media_type: None,
            required: false,
            refs: vec![SCRIPT.into()],
            sha256: None,
        });
    }

    // Last, because it carries the deck's hash.
    zip.start_file(MANIFEST_PATH, deflated)?;
    let manifest = Manifest {
        format_version: FORMAT_VERSION,
        bundle_id: bundle.bundle_id.into(),
        created_at: bundle.created_at.into(),
        generator: bundle.generator.map(Into::into),
        title: bundle.title.into(),
        parts,
    };
    zip.write_all(&json_within(&manifest, limits.manifest, "manifest")?)?;
    Ok(zip.finish()?)
}

/// The reader caps a part by its uncompressed bytes, which JSON escaping can
/// make several times the text it carries.
fn json_within(value: &impl Serialize, cap: u64, part: &str) -> Result<Vec<u8>> {
    let json = serde_json::to_vec(value)?;
    ensure!(json.len() as u64 <= cap, "{part} over {cap} bytes");
    Ok(json)
}

fn copy_hashed(from: &mut dyn Read, to: &mut impl Write, cap: u64) -> Result<String> {
    let mut hasher = Sha256::new();
    let mut buf = vec![0; 64 * 1024];
    let mut total = 0u64;
    loop {
        let n = from.read(&mut buf)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > cap {
            bail!("deck over {cap} bytes");
        }
        hasher.update(&buf[..n]);
        to.write_all(&buf[..n])?;
    }
    Ok(hex(&hasher.finalize()))
}
