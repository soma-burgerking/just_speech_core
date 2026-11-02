use std::io::{self, Read, Seek, SeekFrom};

const EOCD_SIG: [u8; 4] = *b"PK\x05\x06";
const EOCD_LEN: u64 = 22;
const ZIP64_LOCATOR_SIG: [u8; 4] = *b"PK\x06\x07";
const ZIP64_LOCATOR_LEN: u64 = 20;
const ZIP64_EOCD_SIG: [u8; 4] = *b"PK\x06\x06";

/// The entry count the end-of-central-directory record declares. zip keys its
/// archive by name and keeps one of the entries sharing a name, so comparing
/// this with the archive's length is the only way to see a duplicate.
pub(crate) fn declared_entries<R: Read + Seek>(r: &mut R) -> io::Result<u64> {
    let len = r.seek(SeekFrom::End(0))?;
    // The record ends the file, followed only by a comment of at most 64KiB.
    let tail_len = len.min(EOCD_LEN + u16::MAX as u64);
    if tail_len < EOCD_LEN {
        return Err(invalid("too short for a zip"));
    }
    let tail_start = len - tail_len;
    r.seek(SeekFrom::Start(tail_start))?;
    let mut tail = vec![0; tail_len as usize];
    r.read_exact(&mut tail)?;
    let at = (0..=tail.len() - EOCD_LEN as usize)
        .rev()
        .find(|&i| tail[i..i + 4] == EOCD_SIG)
        .ok_or_else(|| invalid("no end of central directory"))?;
    let entries = u16::from_le_bytes([tail[at + 10], tail[at + 11]]);
    if entries != u16::MAX {
        return Ok(entries.into());
    }

    let eocd = tail_start + at as u64;
    if eocd < ZIP64_LOCATOR_LEN {
        return Err(invalid("no zip64 locator"));
    }
    r.seek(SeekFrom::Start(eocd - ZIP64_LOCATOR_LEN))?;
    let mut locator = [0; ZIP64_LOCATOR_LEN as usize];
    r.read_exact(&mut locator)?;
    if locator[..4] != ZIP64_LOCATOR_SIG {
        return Err(invalid("no zip64 locator"));
    }
    r.seek(SeekFrom::Start(u64_at(&locator, 8)))?;
    let mut record = [0; 40];
    r.read_exact(&mut record)?;
    if record[..4] != ZIP64_EOCD_SIG {
        return Err(invalid("no zip64 end of central directory"));
    }
    Ok(u64_at(&record, 32))
}

fn u64_at(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
