//! Browser entry for the slide counter, over the bytes of a picked File; build.sh
//! installs it into just_speech_web.

use std::io::Cursor;

use slides::SlideFormat;
use wasm_bindgen::prelude::*;

/// `format` is the file extension (`pptx | ppt | odp | pdf`). Hidden slides are
/// included, as in the server's render. Throws when the bytes can't be read as
/// `format`.
#[wasm_bindgen]
pub fn count_slides(bytes: &[u8], format: &str) -> Result<u32, JsError> {
    let format = SlideFormat::from_extension(format)
        .ok_or_else(|| JsError::new(&format!("unsupported format: {format}")))?;
    slides::count_slides_in(Cursor::new(bytes), format)
        .map_err(|error| JsError::new(&format!("{error:#}")))
}
