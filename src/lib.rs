//! Slide counts and speaker notes read from each format's own structure,
//! without rendering.

mod notes;
mod pdf;
mod ppt;

use std::borrow::Cow;
use std::fs::File;
use std::io::{Read, Seek};
use std::path::Path;

use anyhow::{bail, Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlideFormat {
    Pptx,
    Ppt,
    Odp,
    Pdf,
}

/// OOXML ships two conformance classes and PowerPoint offers both in its save
/// dialog; only the namespace tells them apart. Everything the count reads —
/// `sldIdLst`, `sldId` — is spelled the same under either.
pub(crate) const PRESENTATIONML_NS: [&str; 2] = [
    "http://schemas.openxmlformats.org/presentationml/2006/main",
    "http://purl.oclc.org/ooxml/presentationml/main",
];

/// Fixed across ODF versions — the `1.0` names the namespace, not the format.
pub(crate) const ODF_DRAWING_NS: [&str; 1] = ["urn:oasis:names:tc:opendocument:xmlns:drawing:1.0"];

impl SlideFormat {
    /// The format a file extension names, case-insensitively; the extensions
    /// are the ones the server's presign accepts.
    pub fn from_extension(extension: &str) -> Option<Self> {
        match extension.to_ascii_lowercase().as_str() {
            "pptx" => Some(Self::Pptx),
            "ppt" => Some(Self::Ppt),
            "odp" => Some(Self::Odp),
            "pdf" => Some(Self::Pdf),
            _ => None,
        }
    }

    /// PPT keeps notes in encoded text records and a PDF has none.
    pub fn supports_notes(self) -> bool {
        matches!(self, Self::Pptx | Self::Odp)
    }
}

/// Hidden slides are counted, as the server render does.
pub fn count_slides(path: &Path, format: SlideFormat) -> Result<u32> {
    let file = File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    count_slides_in(file, format)
}

/// [count_slides] over an open source; the browser build has no filesystem.
pub fn count_slides_in<R: Read + Seek>(reader: R, format: SlideFormat) -> Result<u32> {
    match format {
        SlideFormat::Pptx => count_pptx(reader),
        SlideFormat::Odp => count_odp(reader),
        SlideFormat::Pdf => pdf::count_pages(reader),
        SlideFormat::Ppt => ppt::count_slides(reader),
    }
}

/// Speaker notes per slide: index `i` is slide `i + 1` as [count_slides]
/// numbers them (hidden slides included), `None` where a slide has none.
/// `Err` for a format without stored notes ([SlideFormat::supports_notes]) or
/// a file that can't be read as `format`.
pub fn speaker_notes(path: &Path, format: SlideFormat) -> Result<Vec<Option<String>>> {
    let file = File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    speaker_notes_in(file, format)
}

/// [speaker_notes] over an open source; the browser build has no filesystem.
pub fn speaker_notes_in<R: Read + Seek>(
    reader: R,
    format: SlideFormat,
) -> Result<Vec<Option<String>>> {
    match format {
        SlideFormat::Pptx => notes::pptx(reader),
        SlideFormat::Odp => notes::odp(reader),
        SlideFormat::Ppt | SlideFormat::Pdf => {
            bail!("{format:?} carries no readable speaker notes")
        }
    }
}

/// The server's upload cap (just_speech_api MAX_UPLOAD_BYTES; the app's
/// kMaxPresentationFileBytes and the web's MAX_PRESENTATION_FILE_BYTES): a
/// file past it is refused before upload, so a part larger than the largest
/// accepted file is a zip bomb, not a deck, and is never inflated into memory.
pub const MAX_PRESENTATION_FILE_BYTES: u64 = 100 << 20;

/// Only the `sldIdLst` in the main namespace: section lists (`p14`) repeat
/// every slide id. The URI can be bound both as the default namespace and under
/// a prefix, and only the element itself says which binding it uses.
fn count_pptx<R: Read + Seek>(reader: R) -> Result<u32> {
    let raw = read_zip_entry(reader, "ppt/presentation.xml")?;
    let xml = without_unparsed_text(&raw);
    let ns = namespace_declarations(&PRESENTATIONML_NS);
    let mut declared = false;
    for capture in ns.captures_iter(&xml) {
        declared = true;
        let prefix = capture
            .get(1)
            .map(|m| format!("{}:", m.as_str()))
            .unwrap_or_default();
        let p = regex::escape(&prefix);
        let list = Regex::new(&format!(r"(?s)<{p}sldIdLst\b[^>]*>(.*?)</{p}sldIdLst>")).unwrap();
        let Some(body) = list.captures(&xml).and_then(|c| c.get(1)) else {
            continue;
        };
        let id = Regex::new(&format!(r"<{p}sldId[\s/>]")).unwrap();
        return Ok(id.find_iter(body.as_str()).count() as u32);
    }
    if !declared {
        bail!("presentation.xml declares no PresentationML namespace");
    }
    Ok(0)
}

/// `<draw:page>` in `content.xml`, under whatever prefix binds the ODF drawing
/// namespace — a vendor extension whose local name is also `page` is not one.
/// The trailing class keeps `draw:page-thumbnail` (notes) from matching.
fn count_odp<R: Read + Seek>(reader: R) -> Result<u32> {
    let raw = read_zip_entry(reader, "content.xml")?;
    let xml = without_unparsed_text(&raw);
    let ns = namespace_declarations(&ODF_DRAWING_NS);
    let mut declared = false;
    for capture in ns.captures_iter(&xml) {
        declared = true;
        let prefix = capture
            .get(1)
            .map(|m| format!("{}:", m.as_str()))
            .unwrap_or_default();
        let page = Regex::new(&format!(r"<{}page[\s/>]", regex::escape(&prefix))).unwrap();
        let count = page.find_iter(&xml).count() as u32;
        if count > 0 {
            return Ok(count);
        }
    }
    if !declared {
        bail!("content.xml declares no ODF drawing namespace");
    }
    Ok(0)
}

fn read_zip_entry<R: Read + Seek>(reader: R, entry: &str) -> Result<String> {
    read_zip_entry_within(reader, entry, MAX_PRESENTATION_FILE_BYTES)
}

/// Declarations binding any of [uris], capturing the prefix each binds it to
/// (none for the default namespace). Both quote characters and whitespace
/// around the `=` are legal XML, and missing one reads as a deck with no slide
/// list.
fn namespace_declarations(uris: &[&str]) -> Regex {
    let any = uris
        .iter()
        .map(|uri| regex::escape(uri))
        .collect::<Vec<_>>()
        .join("|");
    Regex::new(&format!(
        r#"xmlns(?::([\w.-]+))?\s*=\s*(?:"(?:{any})"|'(?:{any})')"#
    ))
    .unwrap()
}

/// Comments and CDATA blanked — the only text the element regexes below can
/// mistake for markup, since an attribute value cannot hold a bare `<`. With
/// them gone every `<prefix:name` left is a real element. Borrows when there is
/// nothing to strip: the entry may be 100MB.
fn without_unparsed_text(xml: &str) -> Cow<'_, str> {
    static UNPARSED: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?s)<!--.*?-->|<!\[CDATA\[.*?\]\]>").unwrap());
    UNPARSED.replace_all(xml, "")
}

fn read_zip_entry_within<R: Read + Seek>(reader: R, entry: &str, max_bytes: u64) -> Result<String> {
    let mut archive = zip::ZipArchive::new(reader).context("not a zip package")?;
    let mut xml = String::new();
    archive
        .by_name(entry)
        .with_context(|| format!("{entry} missing from package"))?
        .take(max_bytes + 1)
        .read_to_string(&mut xml)
        .with_context(|| format!("{entry} is not UTF-8 text"))?;
    if xml.len() as u64 > max_bytes {
        bail!("{entry} exceeds {max_bytes} bytes");
    }
    Ok(xml)
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    use super::*;

    fn package(entry: &str, xml: &str) -> Cursor<Vec<u8>> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file(entry, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(xml.as_bytes()).unwrap();
        let mut cursor = writer.finish().unwrap();
        cursor.set_position(0);
        cursor
    }

    #[test]
    fn pptx_counts_the_main_slide_list_only() {
        // Sections repeat every slide id under p14; masters and sizes never match.
        let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
  xmlns:p14="http://schemas.microsoft.com/office/powerpoint/2010/main">
  <p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst>
  <p:sldIdLst>
    <p:sldId id="256" r:id="rId2"/>
    <p:sldId id="257" r:id="rId3"/>
    <p:sldId id="258" r:id="rId4"></p:sldId>
  </p:sldIdLst>
  <p:sldSz cx="12192000" cy="6858000"/>
  <p:extLst><p:ext uri="{521415D9-36F7-43E2-AB2F-B90AF26B5E84}">
    <p14:sectionLst><p14:section name="A"><p14:sldIdLst>
      <p14:sldId id="256"/><p14:sldId id="257"/><p14:sldId id="258"/>
    </p14:sldIdLst></p14:section></p14:sectionLst>
  </p:ext></p:extLst>
</p:presentation>"#;
        assert_eq!(count_pptx(package("ppt/presentation.xml", xml)).unwrap(), 3);
    }

    /// A Strict deck, as PowerPoint's "Strict Open XML Presentation" writes it:
    /// same elements, a different namespace for the whole conformance class.
    #[test]
    fn pptx_counts_a_strict_conformance_deck() {
        let slides = (256..268)
            .map(|id| format!(r#"<p:sldId id="{id}" r:id="rId{id}"/>"#))
            .collect::<String>();
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentation xmlns:p="http://purl.oclc.org/ooxml/presentationml/main"
  xmlns:r="http://purl.oclc.org/ooxml/officeDocument/relationships">
  <p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst>
  <p:sldIdLst>{slides}</p:sldIdLst>
</p:presentation>"#
        );
        assert_eq!(
            count_pptx(package("ppt/presentation.xml", &xml)).unwrap(),
            12
        );
    }

    /// A deck at the pipeline's own ceiling still counts exactly, so the
    /// preflight decides on 200 rather than on a parse that gave up.
    #[test]
    fn pptx_counts_a_deck_at_the_page_cap() {
        let slides = (0..200)
            .map(|i| format!(r#"<p:sldId id="{}"/>"#, 256 + i))
            .collect::<String>();
        let xml = format!(
            r#"<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
<p:sldIdLst>{slides}</p:sldIdLst></p:presentation>"#
        );
        assert_eq!(
            count_pptx(package("ppt/presentation.xml", &xml)).unwrap(),
            200
        );
    }

    /// A one-slide deck and an empty list are the two ends the preflight acts
    /// on: it takes the first and refuses the second.
    #[test]
    fn pptx_counts_the_smallest_decks() {
        let one = r#"<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
<p:sldIdLst><p:sldId id="256"/></p:sldIdLst></p:presentation>"#;
        assert_eq!(count_pptx(package("ppt/presentation.xml", one)).unwrap(), 1);
        let none = r#"<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
<p:sldIdLst/></p:presentation>"#;
        assert_eq!(
            count_pptx(package("ppt/presentation.xml", none)).unwrap(),
            0
        );
    }

    #[test]
    fn odp_counts_every_page_of_a_long_deck() {
        let pages = (1..=64)
            .map(|n| format!(r#"<draw:page draw:name="page{n}"><draw:frame/></draw:page>"#))
            .collect::<String>();
        let xml = format!(
            r#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
  xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0">
  <office:body><office:presentation>{pages}</office:presentation></office:body>
</office:document-content>"#
        );
        assert_eq!(count_odp(package("content.xml", &xml)).unwrap(), 64);
    }

    #[test]
    fn pptx_follows_whatever_prefix_binds_the_main_namespace() {
        let xml = r#"<x:presentation xmlns:x="http://schemas.openxmlformats.org/presentationml/2006/main">
<x:sldIdLst><x:sldId id="256"/><x:sldId id="257"/></x:sldIdLst></x:presentation>"#;
        assert_eq!(count_pptx(package("ppt/presentation.xml", xml)).unwrap(), 2);
        let no_list = r#"<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"/>"#;
        assert_eq!(
            count_pptx(package("ppt/presentation.xml", no_list)).unwrap(),
            0
        );
    }

    #[test]
    fn pptx_reads_the_binding_the_slide_list_actually_uses() {
        // The URI is bound twice — default first, prefix second — and the list
        // uses the prefix. Taking the first declaration would find no list.
        let xml = r#"<p:presentation xmlns="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
<p:sldIdLst><p:sldId id="256"/><p:sldId id="257"/><p:sldId id="258"/></p:sldIdLst></p:presentation>"#;
        assert_eq!(count_pptx(package("ppt/presentation.xml", xml)).unwrap(), 3);
    }

    #[test]
    fn pptx_without_the_main_namespace_is_an_error() {
        let xml = r#"<presentation xmlns="urn:something-else"><sldIdLst/></presentation>"#;
        assert!(count_pptx(package("ppt/presentation.xml", xml)).is_err());
    }

    #[test]
    fn pptx_accepts_single_quoted_and_spaced_namespace_declarations() {
        let xml = r#"<p:presentation xmlns:p = 'http://schemas.openxmlformats.org/presentationml/2006/main'>
<p:sldIdLst><p:sldId id="256"/><p:sldId id="257"/></p:sldIdLst></p:presentation>"#;
        assert_eq!(count_pptx(package("ppt/presentation.xml", xml)).unwrap(), 2);
    }

    #[test]
    fn pptx_ignores_a_commented_out_slide_list() {
        let xml = r#"<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
<!-- <p:sldIdLst><p:sldId id="1"/><p:sldId id="2"/><p:sldId id="3"/></p:sldIdLst> -->
<p:sldIdLst><p:sldId id="256"/><!-- <p:sldId id="257"/> --></p:sldIdLst></p:presentation>"#;
        assert_eq!(count_pptx(package("ppt/presentation.xml", xml)).unwrap(), 1);
    }

    #[test]
    fn odp_ignores_a_commented_out_page() {
        let xml = r#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
  xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0">
  <office:body><office:presentation>
    <draw:page draw:name="page1"/>
    <!-- <draw:page draw:name="page2"/> -->
  </office:presentation></office:body>
</office:document-content>"#;
        assert_eq!(count_odp(package("content.xml", xml)).unwrap(), 1);
    }

    #[test]
    fn odp_counts_draw_pages_but_not_thumbnails() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
  xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0"
  xmlns:presentation="urn:oasis:names:tc:opendocument:xmlns:presentation:1.0">
  <office:automatic-styles><style:page-layout style:name="PM0"/></office:automatic-styles>
  <office:body><office:presentation>
    <draw:page draw:name="page1" draw:style-name="dp1">
      <draw:frame/>
      <presentation:notes><draw:page-thumbnail draw:layer="layout"/></presentation:notes>
    </draw:page>
    <draw:page draw:name="page2"/>
  </office:presentation></office:body>
</office:document-content>"#;
        assert_eq!(count_odp(package("content.xml", xml)).unwrap(), 2);
    }

    #[test]
    fn odp_ignores_a_page_element_from_another_namespace() {
        let xml = r#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
  xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0"
  xmlns:v="urn:vendor:extension">
  <v:page/>
  <office:body><office:presentation>
    <draw:page draw:name="page1"/>
  </office:presentation></office:body>
</office:document-content>"#;
        assert_eq!(count_odp(package("content.xml", xml)).unwrap(), 1);
    }

    #[test]
    fn odp_without_the_drawing_namespace_is_an_error() {
        let xml = r#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"/>"#;
        assert!(count_odp(package("content.xml", xml)).is_err());
    }

    #[test]
    fn missing_entry_is_an_error() {
        let result = count_odp(package("other.xml", "<a/>"));
        assert!(result.unwrap_err().to_string().contains("content.xml"));
    }

    #[test]
    fn an_oversized_entry_is_refused_without_inflating_it() {
        let xml = "<draw:page/>".repeat(100);
        let result = read_zip_entry_within(package("content.xml", &xml), "content.xml", 64);
        assert!(result.unwrap_err().to_string().contains("exceeds"));
        let ok = read_zip_entry_within(package("content.xml", &xml), "content.xml", 4096).unwrap();
        assert_eq!(ok.matches("<draw:page/>").count(), 100);
    }

    #[test]
    fn non_zip_bytes_are_an_error() {
        let result = count_odp(Cursor::new(b"%PDF-1.4".to_vec()));
        assert!(result.is_err());
    }
}
