use std::io::{Cursor, Write};

use super::*;

fn record(instance: u16, rec_type: u16, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + body.len());
    out.extend_from_slice(&(instance << 4).to_le_bytes());
    out.extend_from_slice(&rec_type.to_le_bytes());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(body);
    out
}

fn slide_list(instance: u16, slides: u32) -> Vec<u8> {
    let mut body = Vec::new();
    for slide in 0..slides {
        let mut atom = Vec::new();
        atom.extend_from_slice(&(slide + 2).to_le_bytes()); // persistIdRef
        atom.extend_from_slice(&0u32.to_le_bytes()); // flags
        atom.extend_from_slice(&1u32.to_le_bytes()); // cTexts
        atom.extend_from_slice(&(256 + slide).to_le_bytes()); // slideId
        atom.extend_from_slice(&0u32.to_le_bytes()); // unused
        body.extend(record(0, RT_SLIDE_PERSIST_ATOM, &atom));
        // Text records that follow a SlidePersistAtom inside the list.
        body.extend(record(0, 0x0F9F, &[0, 0, 0, 0]));
    }
    record(instance, RT_SLIDE_LIST_WITH_TEXT, &body)
}

fn document_container(slides: u32) -> Vec<u8> {
    let mut body = record(0, 0x03E9, &[0u8; 40]); // documentAtom
    body.extend(slide_list(1, 1)); // masters
    body.extend(slide_list(SLIDE_LIST_INSTANCE, slides));
    body.extend(slide_list(2, slides)); // notes
    body.extend(record(0, 0x03EA, &[])); // endDocumentAtom
    record(0, RT_DOCUMENT, &body)
}

/// One `(persistId, offset)` per entry, as PowerPoint writes runs of one.
fn persist_directory_atom(entries: &[(u32, u32)]) -> Vec<u8> {
    let mut body = Vec::new();
    for (id, offset) in entries {
        body.extend_from_slice(&(id | (1 << 20)).to_le_bytes());
        body.extend_from_slice(&offset.to_le_bytes());
    }
    record(0, RT_PERSIST_DIRECTORY_ATOM, &body)
}

fn user_edit_atom(last_edit: u32, directory: u32, doc_persist_id: u32) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&256u32.to_le_bytes()); // lastSlideIdRef
    body.extend_from_slice(&0u16.to_le_bytes()); // version
    body.push(0); // minorVersion
    body.push(3); // majorVersion
    body.extend_from_slice(&last_edit.to_le_bytes());
    body.extend_from_slice(&directory.to_le_bytes());
    body.extend_from_slice(&doc_persist_id.to_le_bytes());
    body.extend_from_slice(&100u32.to_le_bytes()); // persistIdSeed
    body.extend_from_slice(&1u16.to_le_bytes()); // lastView
    body.extend_from_slice(&0u16.to_le_bytes()); // unused
    record(0, RT_USER_EDIT_ATOM, &body)
}

fn current_user_atom(current_edit: u32, encrypted: bool) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&0x14u32.to_le_bytes()); // size
    let token = if encrypted {
        ENCRYPTED_HEADER_TOKEN
    } else {
        0xE391_C05F
    };
    body.extend_from_slice(&token.to_le_bytes());
    body.extend_from_slice(&current_edit.to_le_bytes());
    body.extend_from_slice(&0u16.to_le_bytes()); // lenUserName
    body.extend_from_slice(&0x03F4u16.to_le_bytes()); // docFileVersion
    body.push(3); // majorVersion
    body.push(0); // minorVersion
    body.extend_from_slice(&0u16.to_le_bytes()); // unused
    body.extend_from_slice(&0u32.to_le_bytes()); // relVersion
    record(0, RT_CURRENT_USER_ATOM, &body)
}

/// Document stream holding one save generation per entry of `slides`,
/// each chained to the one before, plus the matching Current User stream.
fn streams(generations: &[u32], encrypted: bool) -> (Vec<u8>, Vec<u8>) {
    let mut document = Vec::new();
    let mut last_edit = 0u32;
    for &slides in generations {
        let doc_offset = document.len() as u32;
        document.extend(document_container(slides));
        let dir_offset = document.len() as u32;
        document.extend(persist_directory_atom(&[(1, doc_offset)]));
        let edit_offset = document.len() as u32;
        document.extend(user_edit_atom(last_edit, dir_offset, 1));
        last_edit = edit_offset;
    }
    (current_user_atom(last_edit, encrypted), document)
}

fn compound_file(current_user: &[u8], document: &[u8]) -> Cursor<Vec<u8>> {
    let mut cfb = cfb::CompoundFile::create(Cursor::new(Vec::new())).unwrap();
    cfb.create_stream("/Current User")
        .unwrap()
        .write_all(current_user)
        .unwrap();
    cfb.create_stream("/PowerPoint Document")
        .unwrap()
        .write_all(document)
        .unwrap();
    let mut cursor = cfb.into_inner();
    cursor.set_position(0);
    cursor
}

#[test]
fn counts_slides_of_a_single_save() {
    let (current_user, document) = streams(&[4], false);
    assert_eq!(
        count_slides(compound_file(&current_user, &document)).unwrap(),
        4
    );
}

#[test]
fn newest_generation_wins_after_incremental_saves() {
    let (current_user, document) = streams(&[2, 5, 3], false);
    assert_eq!(count_in_streams(&current_user, &document).unwrap(), 3);
}

#[test]
fn deck_without_slides_counts_zero() {
    let (current_user, document) = streams(&[0], false);
    assert_eq!(count_in_streams(&current_user, &document).unwrap(), 0);
}

#[test]
fn encrypted_document_is_rejected() {
    let (current_user, document) = streams(&[1], true);
    let error = count_in_streams(&current_user, &document).unwrap_err();
    assert!(error.to_string().contains("encrypted"));
}

#[test]
fn truncated_stream_is_an_error_not_a_panic() {
    let (current_user, document) = streams(&[3], false);
    let truncated = &document[..document.len() / 2];
    assert!(count_in_streams(&current_user, truncated).is_err());
}

#[test]
fn non_ole_bytes_are_an_error() {
    assert!(count_slides(Cursor::new(b"PK\x03\x04".to_vec())).is_err());
}
