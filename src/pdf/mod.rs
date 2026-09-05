use std::io::Read;

use anyhow::{Context, Result};
use lopdf::{Document, LoadOptions, Object, ObjectId};

/// Walks the page tree rather than trusting the root `/Count`, which
/// incremental updates leave stale.
pub fn count_pages<R: Read>(reader: R) -> Result<u32> {
    let document =
        Document::load_from_with_options(reader, LoadOptions::with_filter(without_stream_bodies))
            .context("not a readable PDF")?;
    Ok(document.get_pages().len() as u32)
}

/// Drops every ordinary stream as it is parsed: images and fonts are most of a
/// deck's bytes and the page tree never points into them, so the document
/// costs its dictionaries rather than a second copy of the file. Object
/// streams stay — the pages they compress are parsed out of them after this
/// runs — and a stream must be dropped whole, not emptied: lopdf re-reads an
/// empty stream's bytes from the file.
fn without_stream_bodies(id: ObjectId, object: &mut Object) -> Option<(ObjectId, Object)> {
    if let Ok(stream) = object.as_stream_mut() {
        if !stream.dict.has_type(b"ObjStm") {
            *object = Object::Null;
        }
    }
    Some((id, object.clone()))
}

#[cfg(test)]
mod tests;
