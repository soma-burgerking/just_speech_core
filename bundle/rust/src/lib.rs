//! The `.justspeech` presentation bundle: one presentation as one file. The
//! format is specified in bundle/JUSTSPEECH_FORMAT.md; readers skip what they do
//! not know, so new content ships as a new part type without breaking older
//! apps.

mod central_dir;
mod manifest;
mod read;
#[cfg(test)]
mod tests;
mod write;

pub use manifest::SlideMapping;
pub use read::{Bundle, BundleReader, ReadError};
pub use write::{write, NewBundle};

pub const MIME: &str = "application/vnd.justspeech.presentation+zip";

/// The first local file header (30 bytes), the `mimetype` name, the MIME.
pub const SNIFF_LEN: usize = 38 + MIME.len();

/// Whether a file's first [SNIFF_LEN] bytes start a `.justspeech`. Platforms
/// hand files over as `application/octet-stream` without an extension, so
/// this, not the name, decides.
pub fn sniff(prefix: &[u8]) -> bool {
    prefix.len() >= SNIFF_LEN
        && prefix[..4] == *b"PK\x03\x04"
        // Stored, name length 8, no extra field: the MIME then sits at 38.
        && prefix[8..10] == [0, 0]
        && prefix[26..30] == [8, 0, 0, 0]
        && prefix[30..38] == *b"mimetype"
        && prefix[38..SNIFF_LEN] == *MIME.as_bytes()
}
