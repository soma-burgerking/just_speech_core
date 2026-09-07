//! Speaker notes read from the deck's own XML, without rendering. Only the
//! zip-packaged formats store notes as text the deck itself carries; the binary
//! PPT keeps them in encoded text records and a PDF has none.

use std::collections::HashMap;
use std::io::{Read, Seek};

use anyhow::{anyhow, bail, Context, Result};
use roxmltree::{Document, Node};
use zip::result::ZipError;
use zip::ZipArchive;

use crate::{MAX_PRESENTATION_FILE_BYTES, ODF_DRAWING_NS, PRESENTATIONML_NS};

/// DrawingML, in the same two conformance classes as PresentationML.
const DRAWINGML_NS: [&str; 2] = [
    "http://schemas.openxmlformats.org/drawingml/2006/main",
    "http://purl.oclc.org/ooxml/drawingml/main",
];

/// The `r:id` attribute namespace binding a slide to its part.
const OFFICE_RELATIONSHIPS_NS: [&str; 2] = [
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
    "http://purl.oclc.org/ooxml/officeDocument/relationships",
];

/// `.rels` parts are OPC, not OOXML: one namespace across both classes.
const PACKAGE_RELATIONSHIPS_NS: [&str; 1] =
    ["http://schemas.openxmlformats.org/package/2006/relationships"];

const ODF_PRESENTATION_NS: [&str; 1] = ["urn:oasis:names:tc:opendocument:xmlns:presentation:1.0"];
const ODF_TEXT_NS: [&str; 1] = ["urn:oasis:names:tc:opendocument:xmlns:text:1.0"];

/// Notes per slide of a PPTX, in `sldIdLst` order (hidden slides included, as
/// the count and the render). A slide whose notes part is missing or holds no
/// text is `None`.
pub(crate) fn pptx<R: Read + Seek>(reader: R) -> Result<Vec<Option<String>>> {
    let mut package = Package::open(reader)?;
    let presentation = package.required("ppt/presentation.xml")?;
    let slide_ids = slide_relationship_ids(&presentation)?;
    if slide_ids.is_empty() {
        return Ok(Vec::new());
    }
    let rels = package.required("ppt/_rels/presentation.xml.rels")?;
    let slide_parts = relationships(&rels, "ppt", "/slide")?;

    let mut notes = Vec::with_capacity(slide_ids.len());
    for id in slide_ids {
        let part = id.as_deref().and_then(|id| slide_parts.get(id));
        notes.push(match part {
            Some(part) => slide_notes(&mut package, part)?,
            None => None,
        });
    }
    Ok(notes)
}

/// The `r:id` of each `sldId` in the main slide list, in order; `None` for an
/// entry without one (it has no part to follow).
fn slide_relationship_ids(xml: &str) -> Result<Vec<Option<String>>> {
    let doc = parse(xml, "ppt/presentation.xml")?;
    let root = doc.root_element();
    if !is(root, &PRESENTATIONML_NS, "presentation") {
        bail!("ppt/presentation.xml declares no PresentationML namespace");
    }
    let Some(list) = root
        .children()
        .find(|node| is(*node, &PRESENTATIONML_NS, "sldIdLst"))
    else {
        return Ok(Vec::new());
    };
    Ok(list
        .children()
        .filter(|node| is(*node, &PRESENTATIONML_NS, "sldId"))
        .map(|node| {
            OFFICE_RELATIONSHIPS_NS
                .iter()
                .find_map(|ns| node.attribute((*ns, "id")))
                .map(str::to_owned)
        })
        .collect())
}

fn slide_notes<R: Read + Seek>(package: &mut Package<R>, part: &str) -> Result<Option<String>> {
    let (dir, name) = part.rsplit_once('/').unwrap_or(("", part));
    let rels_part = format!("{dir}/_rels/{name}.rels");
    let Some(rels) = package.text(&rels_part)? else {
        return Ok(None);
    };
    let notes_parts = relationships(&rels, dir, "/notesSlide")?;
    let Some(notes_part) = notes_parts.values().next() else {
        return Ok(None);
    };
    let Some(xml) = package.text(notes_part)? else {
        return Ok(None);
    };
    notes_body_text(&xml, notes_part)
}

/// Body placeholders only: the slide image, slide number, header, footer and
/// date placeholders are the page furniture around the notes.
fn notes_body_text(xml: &str, part: &str) -> Result<Option<String>> {
    let doc = parse(xml, part)?;
    let mut lines = Vec::new();
    for shape in doc
        .descendants()
        .filter(|node| is(*node, &PRESENTATIONML_NS, "sp"))
    {
        let is_body = shape.descendants().any(|node| {
            is(node, &PRESENTATIONML_NS, "ph") && node.attribute("type") == Some("body")
        });
        if !is_body {
            continue;
        }
        for body in shape
            .children()
            .filter(|node| is(*node, &PRESENTATIONML_NS, "txBody"))
        {
            for paragraph in body.children().filter(|node| is(*node, &DRAWINGML_NS, "p")) {
                lines.push(drawingml_paragraph(paragraph));
            }
        }
    }
    Ok(text_or_none(lines))
}

fn drawingml_paragraph(paragraph: Node) -> String {
    let mut text = String::new();
    for child in paragraph.children() {
        if is(child, &DRAWINGML_NS, "br") {
            text.push('\n');
        } else if is(child, &DRAWINGML_NS, "r") || is(child, &DRAWINGML_NS, "fld") {
            for t in child
                .children()
                .filter(|node| is(*node, &DRAWINGML_NS, "t"))
            {
                text.push_str(t.text().unwrap_or_default());
            }
        }
    }
    text
}

/// `Id → part name` for the relationships whose type ends in [type_suffix].
/// External targets are links, not parts.
fn relationships(xml: &str, base_dir: &str, type_suffix: &str) -> Result<HashMap<String, String>> {
    let doc = parse(xml, "relationships")?;
    let mut parts = HashMap::new();
    for rel in doc
        .root_element()
        .children()
        .filter(|node| is(*node, &PACKAGE_RELATIONSHIPS_NS, "Relationship"))
    {
        if rel.attribute("TargetMode") == Some("External") {
            continue;
        }
        let (Some(id), Some(kind), Some(target)) = (
            rel.attribute("Id"),
            rel.attribute("Type"),
            rel.attribute("Target"),
        ) else {
            continue;
        };
        if kind.ends_with(type_suffix) {
            parts.insert(id.to_owned(), resolve_part(base_dir, target));
        }
    }
    Ok(parts)
}

/// OPC targets are absolute from the package root or relative to the source
/// part's directory.
fn resolve_part(base_dir: &str, target: &str) -> String {
    let mut segments: Vec<&str> = if let Some(absolute) = target.strip_prefix('/') {
        absolute.split('/').collect()
    } else {
        base_dir.split('/').chain(target.split('/')).collect()
    };
    let mut resolved: Vec<&str> = Vec::with_capacity(segments.len());
    for segment in segments.drain(..) {
        match segment {
            "" | "." => {}
            ".." => {
                resolved.pop();
            }
            other => resolved.push(other),
        }
    }
    resolved.join("/")
}

/// Notes per page of an ODP, in `content.xml` order. A page whose notes frames
/// hold no text is `None`.
pub(crate) fn odp<R: Read + Seek>(reader: R) -> Result<Vec<Option<String>>> {
    let mut package = Package::open(reader)?;
    let content = package.required("content.xml")?;
    let doc = parse(&content, "content.xml")?;
    let root = doc.root_element();
    let declares_drawing = root
        .namespaces()
        .any(|ns| ODF_DRAWING_NS.contains(&ns.uri()));
    if !declares_drawing {
        bail!("content.xml declares no ODF drawing namespace");
    }
    Ok(doc
        .descendants()
        .filter(|node| is(*node, &ODF_DRAWING_NS, "page"))
        .map(page_notes)
        .collect())
}

/// Header, footer, date-time and page-number frames sit on the notes page
/// too, each declaring its class; an unclassed frame is from a writer that
/// classes nothing.
fn page_notes(page: Node) -> Option<String> {
    let notes = page
        .children()
        .find(|node| is(*node, &ODF_PRESENTATION_NS, "notes"))?;
    let mut lines = Vec::new();
    for frame in notes
        .descendants()
        .filter(|node| is(*node, &ODF_DRAWING_NS, "frame"))
    {
        let class = ODF_PRESENTATION_NS
            .iter()
            .find_map(|ns| frame.attribute((*ns, "class")));
        if !matches!(class, None | Some("notes")) {
            continue;
        }
        for paragraph in frame
            .descendants()
            .filter(|node| is(*node, &ODF_TEXT_NS, "p"))
        {
            lines.push(odf_paragraph(paragraph));
        }
    }
    text_or_none(lines)
}

/// ODF spells whitespace as elements: `s` (`c` spaces), `tab`, `line-break`.
fn odf_paragraph(paragraph: Node) -> String {
    let mut text = String::new();
    for node in paragraph.descendants().skip(1) {
        if node.is_text() {
            text.push_str(node.text().unwrap_or_default());
        } else if is(node, &ODF_TEXT_NS, "s") {
            let count = ODF_TEXT_NS
                .iter()
                .find_map(|ns| node.attribute((*ns, "c")))
                .and_then(|c| c.parse::<usize>().ok())
                .unwrap_or(1);
            text.push_str(&" ".repeat(count));
        } else if is(node, &ODF_TEXT_NS, "tab") {
            text.push('\t');
        } else if is(node, &ODF_TEXT_NS, "line-break") {
            text.push('\n');
        }
    }
    text
}

/// An empty placeholder is "no notes", not a note that is blank.
fn text_or_none(lines: Vec<String>) -> Option<String> {
    let text = lines.join("\n");
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

fn is(node: Node, namespaces: &[&str], name: &str) -> bool {
    node.is_element()
        && node.tag_name().name() == name
        && node
            .tag_name()
            .namespace()
            .is_some_and(|ns| namespaces.contains(&ns))
}

/// A leading BOM is legal in a part and not XML; the parser sees only the XML.
fn parse<'a>(xml: &'a str, part: &str) -> Result<Document<'a>> {
    Document::parse(xml.trim_start_matches('\u{feff}'))
        .with_context(|| format!("{part} is not well-formed XML"))
}

/// One open zip, read part by part. A part over the upload cap is refused
/// before it is inflated, as in the counter.
struct Package<R: Read + Seek> {
    archive: ZipArchive<R>,
}

impl<R: Read + Seek> Package<R> {
    fn open(reader: R) -> Result<Self> {
        Ok(Self {
            archive: ZipArchive::new(reader).context("not a zip package")?,
        })
    }

    fn required(&mut self, name: &str) -> Result<String> {
        self.text(name)?
            .ok_or_else(|| anyhow!("{name} missing from package"))
    }

    /// `None` when the package has no such part.
    fn text(&mut self, name: &str) -> Result<Option<String>> {
        let entry = match self.archive.by_name(name) {
            Ok(entry) => entry,
            Err(ZipError::FileNotFound) => return Ok(None),
            Err(error) => return Err(error).with_context(|| format!("{name} unreadable")),
        };
        let mut xml = String::new();
        entry
            .take(MAX_PRESENTATION_FILE_BYTES + 1)
            .read_to_string(&mut xml)
            .with_context(|| format!("{name} is not UTF-8 text"))?;
        if xml.len() as u64 > MAX_PRESENTATION_FILE_BYTES {
            bail!("{name} exceeds {MAX_PRESENTATION_FILE_BYTES} bytes");
        }
        Ok(Some(xml))
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    use super::*;
    use crate::{count_odp, count_pptx};

    fn package(entries: &[(&str, &str)]) -> Cursor<Vec<u8>> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, xml) in entries {
            writer
                .start_file(*name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(xml.as_bytes()).unwrap();
        }
        let mut cursor = writer.finish().unwrap();
        cursor.set_position(0);
        cursor
    }

    const P_NS: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
    const A_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
    const R_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
    const REL_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";

    fn presentation(p_ns: &str, r_ns: &str, ids: &[&str]) -> String {
        let list = ids
            .iter()
            .enumerate()
            .map(|(i, id)| format!(r#"<p:sldId id="{}" r:id="{id}"/>"#, 256 + i))
            .collect::<String>();
        format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentation xmlns:p="{p_ns}" xmlns:r="{r_ns}">
  <p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst>
  <p:sldIdLst>{list}</p:sldIdLst>
</p:presentation>"#
        )
    }

    fn rels(r_ns: &str, entries: &[(&str, &str, &str)]) -> String {
        let body = entries
            .iter()
            .map(|(id, kind, target)| {
                format!(r#"<Relationship Id="{id}" Type="{r_ns}/{kind}" Target="{target}"/>"#)
            })
            .collect::<String>();
        format!(r#"<Relationships xmlns="{REL_NS}">{body}</Relationships>"#)
    }

    /// A notes part as PowerPoint writes it: slide image, body, slide number.
    fn notes_part(p_ns: &str, a_ns: &str, body: &str) -> String {
        format!(
            r#"<p:notes xmlns:a="{a_ns}" xmlns:p="{p_ns}"><p:cSld><p:spTree>
<p:sp><p:nvSpPr><p:cNvPr id="2" name="Slide Image Placeholder 1"/><p:nvPr><p:ph type="sldImg"/></p:nvPr></p:nvSpPr></p:sp>
<p:sp><p:nvSpPr><p:cNvPr id="3" name="Notes Placeholder 2"/><p:nvPr><p:ph type="body" idx="1"/></p:nvPr></p:nvSpPr>
<p:txBody><a:bodyPr/><a:lstStyle/>{body}</p:txBody></p:sp>
<p:sp><p:nvSpPr><p:cNvPr id="4" name="Slide Number Placeholder 3"/><p:nvPr><p:ph type="sldNum" sz="quarter" idx="5"/></p:nvPr></p:nvSpPr>
<p:txBody><a:p><a:fld id="{{X}}" type="slidenum"><a:t>1</a:t></a:fld></a:p></p:txBody></p:sp>
</p:spTree></p:cSld></p:notes>"#
        )
    }

    #[test]
    fn pptx_reads_body_text_per_slide_in_slide_list_order() {
        // The list names slide2 first; slide1's notes carry two runs, an
        // entity, and a paragraph with a break; slide2's placeholder is empty.
        let deck = package(&[
            (
                "ppt/presentation.xml",
                &presentation(P_NS, R_NS, &["rId3", "rId2"]),
            ),
            (
                "ppt/_rels/presentation.xml.rels",
                &rels(
                    R_NS,
                    &[
                        ("rId1", "slideMaster", "slideMasters/slideMaster1.xml"),
                        ("rId2", "slide", "slides/slide1.xml"),
                        ("rId3", "slide", "/ppt/slides/slide2.xml"),
                    ],
                ),
            ),
            (
                "ppt/slides/_rels/slide1.xml.rels",
                &rels(
                    R_NS,
                    &[
                        ("rId1", "slideLayout", "../slideLayouts/slideLayout1.xml"),
                        ("rId2", "notesSlide", "../notesSlides/notesSlide1.xml"),
                    ],
                ),
            ),
            (
                "ppt/slides/_rels/slide2.xml.rels",
                &rels(
                    R_NS,
                    &[("rId2", "notesSlide", "../notesSlides/notesSlide2.xml")],
                ),
            ),
            (
                "ppt/notesSlides/notesSlide1.xml",
                &notes_part(
                    P_NS,
                    A_NS,
                    r#"<a:p><a:r><a:rPr lang="ko-KR"/><a:t>안녕하세요, </a:t></a:r><a:r><a:t>시작합니다 &amp; 환영합니다.</a:t></a:r></a:p>
<a:p><a:r><a:t>첫 줄</a:t></a:r><a:br/><a:r><a:t>둘째 줄</a:t></a:r></a:p>"#,
                ),
            ),
            (
                "ppt/notesSlides/notesSlide2.xml",
                &notes_part(P_NS, A_NS, r#"<a:p><a:endParaRPr lang="ko-KR"/></a:p>"#),
            ),
        ]);
        assert_eq!(
            pptx(deck.clone()).unwrap(),
            vec![
                None,
                Some("안녕하세요, 시작합니다 & 환영합니다.\n첫 줄\n둘째 줄".to_owned()),
            ]
        );
        assert_eq!(count_pptx(deck).unwrap() as usize, 2);
    }

    #[test]
    fn pptx_reads_a_strict_conformance_deck() {
        let p_ns = "http://purl.oclc.org/ooxml/presentationml/main";
        let a_ns = "http://purl.oclc.org/ooxml/drawingml/main";
        let r_ns = "http://purl.oclc.org/ooxml/officeDocument/relationships";
        let deck = package(&[
            ("ppt/presentation.xml", &presentation(p_ns, r_ns, &["rId2"])),
            (
                "ppt/_rels/presentation.xml.rels",
                &rels(r_ns, &[("rId2", "slide", "slides/slide1.xml")]),
            ),
            (
                "ppt/slides/_rels/slide1.xml.rels",
                &rels(
                    r_ns,
                    &[("rId2", "notesSlide", "../notesSlides/notesSlide1.xml")],
                ),
            ),
            (
                "ppt/notesSlides/notesSlide1.xml",
                &notes_part(p_ns, a_ns, r#"<a:p><a:r><a:t>strict</a:t></a:r></a:p>"#),
            ),
        ]);
        assert_eq!(pptx(deck).unwrap(), vec![Some("strict".to_owned())]);
    }

    #[test]
    fn pptx_slides_without_a_notes_part_are_none() {
        // slide1 has no .rels at all; slide2's .rels names no notes part;
        // slide3's list entry has no r:id.
        let xml = format!(
            r#"<p:presentation xmlns:p="{P_NS}" xmlns:r="{R_NS}"><p:sldIdLst>
<p:sldId id="256" r:id="rId2"/><p:sldId id="257" r:id="rId3"/><p:sldId id="258"/>
</p:sldIdLst></p:presentation>"#
        );
        let deck = package(&[
            ("ppt/presentation.xml", &xml),
            (
                "ppt/_rels/presentation.xml.rels",
                &rels(
                    R_NS,
                    &[
                        ("rId2", "slide", "slides/slide1.xml"),
                        ("rId3", "slide", "slides/slide2.xml"),
                    ],
                ),
            ),
            (
                "ppt/slides/_rels/slide2.xml.rels",
                &rels(
                    R_NS,
                    &[("rId1", "slideLayout", "../slideLayouts/slideLayout1.xml")],
                ),
            ),
        ]);
        assert_eq!(pptx(deck).unwrap(), vec![None, None, None]);
    }

    #[test]
    fn pptx_with_an_empty_slide_list_has_no_notes() {
        let xml = format!(r#"<p:presentation xmlns:p="{P_NS}"><p:sldIdLst/></p:presentation>"#);
        let deck = package(&[("ppt/presentation.xml", &xml)]);
        assert_eq!(pptx(deck).unwrap(), Vec::<Option<String>>::new());
    }

    #[test]
    fn pptx_without_the_main_namespace_is_an_error() {
        let deck = package(&[(
            "ppt/presentation.xml",
            r#"<presentation xmlns="urn:something-else"><sldIdLst/></presentation>"#,
        )]);
        assert!(pptx(deck).is_err());
        assert!(pptx(package(&[("other.xml", "<a/>")])).is_err());
    }

    #[test]
    fn pptx_skips_a_bom_and_malformed_xml_is_an_error() {
        let with_bom =
            format!("\u{feff}<p:presentation xmlns:p=\"{P_NS}\"><p:sldIdLst/></p:presentation>");
        assert!(pptx(package(&[("ppt/presentation.xml", &with_bom)]))
            .unwrap()
            .is_empty());
        let broken = format!(r#"<p:presentation xmlns:p="{P_NS}"><p:sldIdLst>"#);
        assert!(pptx(package(&[("ppt/presentation.xml", &broken)])).is_err());
    }

    #[test]
    fn relationship_targets_resolve_against_the_source_part() {
        assert_eq!(
            resolve_part("ppt", "slides/slide1.xml"),
            "ppt/slides/slide1.xml"
        );
        assert_eq!(
            resolve_part("ppt/slides", "../notesSlides/notesSlide1.xml"),
            "ppt/notesSlides/notesSlide1.xml"
        );
        assert_eq!(
            resolve_part("ppt/slides", "/ppt/notesSlides/n.xml"),
            "ppt/notesSlides/n.xml"
        );
        assert_eq!(resolve_part("ppt/slides", "./x.xml"), "ppt/slides/x.xml");
    }

    const ODP_HEAD: &str = r#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
  xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0"
  xmlns:presentation="urn:oasis:names:tc:opendocument:xmlns:presentation:1.0"
  xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0">
  <office:body><office:presentation>"#;
    const ODP_TAIL: &str = "</office:presentation></office:body></office:document-content>";

    #[test]
    fn odp_reads_notes_frames_per_page() {
        // page1: a list and a paragraph with spelled-out whitespace, plus a
        // header frame that is furniture; page2: an empty notes frame; page3:
        // no notes element at all.
        let xml = format!(
            r#"{ODP_HEAD}
<draw:page draw:name="page1"><draw:frame/>
  <presentation:notes><draw:page-thumbnail/>
    <draw:frame presentation:class="header"><draw:text-box><text:p>Header</text:p></draw:text-box></draw:frame>
    <draw:frame presentation:class="notes"><draw:text-box>
      <text:list><text:list-item><text:p>첫<text:s text:c="2"/>항목</text:p></text:list-item></text:list>
      <text:p><text:span>둘째</text:span><text:line-break/>줄<text:tab/>끝</text:p>
    </draw:text-box></draw:frame>
  </presentation:notes></draw:page>
<draw:page draw:name="page2"><presentation:notes>
  <draw:frame presentation:class="notes"><draw:text-box><text:p/></draw:text-box></draw:frame>
</presentation:notes></draw:page>
<draw:page draw:name="page3"/>
{ODP_TAIL}"#
        );
        let deck = package(&[("content.xml", &xml)]);
        assert_eq!(
            odp(deck.clone()).unwrap(),
            vec![Some("첫  항목\n둘째\n줄\t끝".to_owned()), None, None]
        );
        assert_eq!(count_odp(deck).unwrap() as usize, 3);
    }

    #[test]
    fn odp_takes_an_unclassed_frame_as_notes() {
        let xml = format!(
            r#"{ODP_HEAD}<draw:page draw:name="page1"><presentation:notes>
<draw:frame><draw:text-box><text:p>plain</text:p></draw:text-box></draw:frame>
</presentation:notes></draw:page>{ODP_TAIL}"#
        );
        assert_eq!(
            odp(package(&[("content.xml", &xml)])).unwrap(),
            vec![Some("plain".to_owned())]
        );
    }

    #[test]
    fn odp_without_the_drawing_namespace_is_an_error() {
        let xml = r#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"/>"#;
        assert!(odp(package(&[("content.xml", xml)])).is_err());
        assert!(odp(package(&[("other.xml", "<a/>")])).is_err());
    }

    #[test]
    fn an_oversized_part_is_refused_and_a_missing_one_is_none() {
        let mut package = Package::open(package(&[("a.xml", "<a/>")])).unwrap();
        assert_eq!(package.text("a.xml").unwrap().as_deref(), Some("<a/>"));
        assert!(package.text("b.xml").unwrap().is_none());
        assert!(package
            .required("b.xml")
            .unwrap_err()
            .to_string()
            .contains("b.xml"));
    }
}
