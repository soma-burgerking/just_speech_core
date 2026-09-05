use lopdf::{dictionary, Object, ObjectId, Stream};

use super::*;

fn page(doc: &mut Document, parent: ObjectId) -> Object {
    doc.add_object(dictionary! { "Type" => "Page", "Parent" => parent })
        .into()
}

/// Root node with `leaf` direct pages plus one nested node of `nested` pages.
fn pdf(leaf: usize, nested: usize) -> Vec<u8> {
    let mut doc = Document::with_version("1.5");
    let root = doc.new_object_id();
    let inner = doc.new_object_id();
    let mut kids: Vec<Object> = (0..leaf).map(|_| page(&mut doc, root)).collect();
    let inner_kids: Vec<Object> = (0..nested).map(|_| page(&mut doc, inner)).collect();
    doc.objects.insert(
        inner,
        dictionary! {
            "Type" => "Pages", "Parent" => root,
            "Kids" => inner_kids, "Count" => nested as i64,
        }
        .into(),
    );
    kids.push(inner.into());
    doc.objects.insert(
        root,
        dictionary! {
            "Type" => "Pages", "Kids" => kids, "Count" => (leaf + nested) as i64,
        }
        .into(),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => root });
    doc.trailer.set("Root", catalog);
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

#[test]
fn counts_pages_across_the_tree() {
    assert_eq!(count_pages(pdf(2, 3).as_slice()).unwrap(), 5);
}

#[test]
fn empty_document_has_no_pages() {
    assert_eq!(count_pages(pdf(0, 0).as_slice()).unwrap(), 0);
}

#[test]
fn stream_bodies_are_dropped_without_losing_pages() {
    let mut doc = Document::with_version("1.5");
    let root = doc.new_object_id();
    let image = doc.add_object(Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Image" },
        vec![0xAB; 4 << 20],
    ));
    let contents = doc.add_object(Stream::new(dictionary! {}, vec![b' '; 1 << 20]));
    let kids: Vec<Object> = (0..3)
        .map(|_| {
            doc.add_object(dictionary! {
                "Type" => "Page", "Parent" => root, "Contents" => contents,
                "Resources" => dictionary! { "XObject" => dictionary! { "Im0" => image } },
            })
            .into()
        })
        .collect();
    doc.objects.insert(
        root,
        dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => 3 }.into(),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => root });
    doc.trailer.set("Root", catalog);
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();

    assert_eq!(count_pages(out.as_slice()).unwrap(), 3);
}

#[test]
fn garbage_is_an_error() {
    assert!(count_pages(&b"PK\x03\x04 not a pdf"[..]).is_err());
}
