//! The `.justspeech` reader over a file's bytes, for the web and the render
//! worker; build.sh installs it.

use std::io::Cursor;

use bundle::{BundleReader, ReadError};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct BundleFile {
    reader: BundleReader<Cursor<Vec<u8>>>,
}

/// The server's `SlideMapping`, as the bundle stores it.
#[wasm_bindgen(getter_with_clone)]
pub struct Mapping {
    #[wasm_bindgen(js_name = startToken)]
    pub start_token: u32,
    #[wasm_bindgen(js_name = endToken)]
    pub end_token: u32,
    pub slide: u32,
    pub title: Option<String>,
    pub disabled: bool,
}

#[wasm_bindgen]
impl BundleFile {
    /// Validates the whole file, every slide included. Throws an `Error` named
    /// `BundleDamaged` (only another copy of the file helps) or
    /// `BundleUpdateRequired` (a newer reader does).
    #[wasm_bindgen(constructor)]
    pub fn new(bytes: Vec<u8>) -> Result<BundleFile, JsValue> {
        let reader = BundleReader::open(Cursor::new(bytes)).map_err(|e| {
            let name = match e {
                ReadError::Damaged(_) => "BundleDamaged",
                ReadError::UpdateRequired(_) => "BundleUpdateRequired",
            };
            let error = js_sys::Error::new(&e.to_string());
            error.set_name(name);
            JsValue::from(error)
        })?;
        Ok(BundleFile { reader })
    }

    /// Empty for no title.
    #[wasm_bindgen(getter)]
    pub fn title(&self) -> String {
        self.reader.bundle().title.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn script(&self) -> String {
        self.reader.bundle().script.clone()
    }

    /// Empty when the file carries no mapping or one this build left out.
    #[wasm_bindgen(getter)]
    pub fn mappings(&self) -> Vec<Mapping> {
        self.reader
            .bundle()
            .mappings
            .iter()
            .flatten()
            .map(|m| Mapping {
                start_token: m.start_token,
                end_token: m.end_token,
                slide: m.slide,
                title: m.title.clone(),
                disabled: m.disabled,
            })
            .collect()
    }

    /// 0 when the file kept no deck.
    #[wasm_bindgen(getter, js_name = slideCount)]
    pub fn slide_count(&self) -> usize {
        self.reader.bundle().slide_count
    }

    /// Part ids left out, for logs.
    #[wasm_bindgen(getter)]
    pub fn skipped(&self) -> Vec<String> {
        self.reader.bundle().skipped.clone()
    }

    /// The PNG of slide `index`, 0-based. Its header was checked, its pixels
    /// were not: decode it as untrusted.
    pub fn slide(&mut self, index: usize) -> Result<Vec<u8>, JsError> {
        let mut png = Vec::new();
        self.reader
            .copy_slide(index, &mut png)
            .map_err(|e| JsError::new(&e.to_string()))?;
        Ok(png)
    }
}
