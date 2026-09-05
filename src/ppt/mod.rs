//! Slide count for the legacy binary PowerPoint format (MS-PPT). Incremental
//! saves append to the document stream, so it holds every generation of the
//! slide list; the live one is reached the way PowerPoint opens the file:
//! `Current User` → newest `UserEditAtom` → persist directory → `DocumentContainer`.

use std::collections::{HashMap, HashSet};

use super::MAX_PRESENTATION_FILE_BYTES;
use std::io::{Read, Seek};

use anyhow::{anyhow, bail, ensure, Context, Result};

const RT_DOCUMENT: u16 = 0x03E8;
const RT_SLIDE_PERSIST_ATOM: u16 = 0x03F3;
const RT_SLIDE_LIST_WITH_TEXT: u16 = 0x0FF0;
const RT_USER_EDIT_ATOM: u16 = 0x0FF5;
const RT_CURRENT_USER_ATOM: u16 = 0x0FF6;
const RT_PERSIST_DIRECTORY_ATOM: u16 = 0x1772;

/// `SlideListWithTextContainer.rh.recInstance` of the presentation slides
/// (1 = masters, 2 = notes).
const SLIDE_LIST_INSTANCE: u16 = 0;

/// `CurrentUserAtom.headerToken` of an encrypted document.
const ENCRYPTED_HEADER_TOKEN: u32 = 0xF3D1_C4DF;

const HEADER_LEN: usize = 8;

#[derive(Clone, Copy)]
struct RecordHeader {
    instance: u16,
    rec_type: u16,
    len: usize,
}

pub fn count_slides<F: Read + Seek>(file: F) -> Result<u32> {
    let mut cfb = cfb::CompoundFile::open(file).context("not an OLE compound file")?;
    let current_user = read_stream(&mut cfb, "/Current User")?;
    let document = read_stream(&mut cfb, "/PowerPoint Document")?;
    count_in_streams(&current_user, &document)
}

fn read_stream<F: Read + Seek>(cfb: &mut cfb::CompoundFile<F>, name: &str) -> Result<Vec<u8>> {
    let mut stream = cfb
        .open_stream(name)
        .with_context(|| format!("{name} stream missing"))?;
    // Pictures live in their own stream, so a deck's document stream is a
    // fraction of the file; one past the largest accepted file is not a deck.
    if stream.len() > MAX_PRESENTATION_FILE_BYTES {
        bail!("{name} stream exceeds {MAX_PRESENTATION_FILE_BYTES} bytes");
    }
    let mut bytes = Vec::with_capacity(stream.len() as usize);
    stream.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn count_in_streams(current_user: &[u8], document: &[u8]) -> Result<u32> {
    let current_edit = current_edit_offset(current_user)?;
    let persist = persist_directory(document, current_edit)?;
    let doc_persist_id = u32_at(document, current_edit + 24)?;
    let doc_offset = *persist.get(&doc_persist_id).ok_or_else(|| {
        anyhow!("document container (persist id {doc_persist_id}) missing from persist directory")
    })?;
    count_in_document(document, doc_offset as usize)
}

/// Offset of the newest `UserEditAtom` in the document stream.
fn current_edit_offset(current_user: &[u8]) -> Result<usize> {
    let rh = header(current_user, 0)?;
    ensure!(
        rh.rec_type == RT_CURRENT_USER_ATOM,
        "Current User stream doesn't start with a CurrentUserAtom"
    );
    if u32_at(current_user, 12)? == ENCRYPTED_HEADER_TOKEN {
        bail!("encrypted .ppt files are not supported");
    }
    Ok(u32_at(current_user, 16)? as usize)
}

/// Persist id → stream offset. Walks the edit chain from the newest edit
/// backwards; an older edit's directory only fills ids the newer ones don't
/// have, so superseded objects never win.
fn persist_directory(document: &[u8], current_edit: usize) -> Result<HashMap<u32, u32>> {
    let mut map = HashMap::new();
    let mut visited = HashSet::new();
    let mut edit = current_edit;
    loop {
        ensure!(
            visited.insert(edit),
            "user edit chain loops at offset {edit}"
        );
        let rh = header(document, edit)?;
        ensure!(
            rh.rec_type == RT_USER_EDIT_ATOM,
            "expected a UserEditAtom at offset {edit}"
        );
        let directory = u32_at(document, edit + 20)? as usize;
        let dir = header(document, directory)?;
        ensure!(
            dir.rec_type == RT_PERSIST_DIRECTORY_ATOM,
            "expected a PersistDirectoryAtom at offset {directory}"
        );
        let mut cursor = directory + HEADER_LEN;
        let end = cursor + dir.len;
        while cursor < end {
            let entry = u32_at(document, cursor)?;
            cursor += 4;
            let first_id = entry & 0x000F_FFFF;
            for i in 0..entry >> 20 {
                map.entry(first_id + i).or_insert(u32_at(document, cursor)?);
                cursor += 4;
            }
        }
        match u32_at(document, edit + 16)? {
            0 => return Ok(map),
            previous => edit = previous as usize,
        }
    }
}

fn count_in_document(document: &[u8], at: usize) -> Result<u32> {
    let rh = header(document, at)?;
    ensure!(
        rh.rec_type == RT_DOCUMENT,
        "expected a DocumentContainer at offset {at}"
    );
    let slide_list = children(document, at + HEADER_LEN, rh.len)?
        .into_iter()
        .find(|(_, c)| c.rec_type == RT_SLIDE_LIST_WITH_TEXT && c.instance == SLIDE_LIST_INSTANCE);
    // The slide list is optional: a deck can be saved with no slides at all.
    let Some((start, list)) = slide_list else {
        return Ok(0);
    };
    let count = children(document, start + HEADER_LEN, list.len)?
        .iter()
        .filter(|(_, c)| c.rec_type == RT_SLIDE_PERSIST_ATOM)
        .count();
    Ok(count as u32)
}

/// Records laid out back to back in `document[start..start + len]`, with the
/// offset each one starts at.
fn children(document: &[u8], start: usize, len: usize) -> Result<Vec<(usize, RecordHeader)>> {
    let end = start
        .checked_add(len)
        .filter(|end| *end <= document.len())
        .ok_or_else(|| anyhow!("container at offset {start} runs past the stream"))?;
    let mut out = Vec::new();
    let mut cursor = start;
    while cursor < end {
        let rh = header(document, cursor)?;
        out.push((cursor, rh));
        cursor += HEADER_LEN + rh.len;
    }
    Ok(out)
}

fn header(buf: &[u8], at: usize) -> Result<RecordHeader> {
    let b = buf
        .get(at..at + HEADER_LEN)
        .ok_or_else(|| anyhow!("record header at offset {at} runs past the stream"))?;
    Ok(RecordHeader {
        instance: u16::from_le_bytes([b[0], b[1]]) >> 4,
        rec_type: u16::from_le_bytes([b[2], b[3]]),
        len: u32::from_le_bytes([b[4], b[5], b[6], b[7]]) as usize,
    })
}

fn u32_at(buf: &[u8], at: usize) -> Result<u32> {
    let b = buf
        .get(at..at + 4)
        .ok_or_else(|| anyhow!("offset {at} runs past the stream"))?;
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

#[cfg(test)]
mod tests;
