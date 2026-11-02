//! Browser entry for the slide reader, over the bytes of a picked File; build.sh
//! installs it into just_speech_web.

use std::io::Cursor;

use slides::SlideFormat;
use wasm_bindgen::prelude::*;

/// `extension` is the file's (`pptx | ppt | odp | pdf`).
fn format(extension: &str) -> Result<SlideFormat, JsError> {
    SlideFormat::from_extension(extension)
        .ok_or_else(|| JsError::new(&format!("unsupported format: {extension}")))
}

/// Hidden slides are included, as in the server's render. Throws when the
/// bytes can't be read as `format`.
#[wasm_bindgen]
pub fn count_slides(bytes: &[u8], format: &str) -> Result<u32, JsError> {
    let format = self::format(format)?;
    slides::count_slides_in(Cursor::new(bytes), format)
        .map_err(|error| JsError::new(&format!("{error:#}")))
}

/// The text a deck stores for one slide: the title placeholder and the
/// speaker notes, paragraphs joined by `\n`.
#[wasm_bindgen(getter_with_clone)]
pub struct SlideText {
    /// 1-based, numbered as the render numbers slides (hidden ones included).
    pub slide: u32,
    pub title: Option<String>,
    pub notes: Option<String>,
}

/// Every slide in order. Throws when `format` stores no text (PPT, PDF) or the
/// bytes can't be read as `format`.
#[wasm_bindgen]
pub fn slide_texts(bytes: &[u8], format: &str) -> Result<Vec<SlideText>, JsError> {
    let format = self::format(format)?;
    let texts = slides::slide_texts_in(Cursor::new(bytes), format)
        .map_err(|error| JsError::new(&format!("{error:#}")))?;
    Ok(texts
        .into_iter()
        .enumerate()
        .map(|(i, text)| SlideText {
            slide: i as u32 + 1,
            title: text.title,
            notes: text.notes,
        })
        .collect())
}
