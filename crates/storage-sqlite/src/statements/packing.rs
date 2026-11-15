//! The originals are kept compressed: a kept document is read rarely and there may be thousands of them.
//!
//! The compression is lossless: what is read back is the file as it was brought, byte for byte. A packed
//! content starts with a mark; a content without it is an original stored before (or one that compression does
//! not make smaller), and is read as it is.

/// What a stored content starts with: compressed…
const COMPRESSED: &[u8; 4] = b"WFZ1";
/// …or kept as it is, for the files compression does not make smaller.
const PLAIN: &[u8; 4] = b"WFZ0";
/// The hex of what both marks start with ("WFZ"), for SQL.
pub const MARK_HEX: &str = "57465A";
/// The strongest of the ordinary levels: a document is written once and read rarely.
const LEVEL: i32 = 19;

pub fn is_packed(stored: &[u8]) -> bool {
    stored.starts_with(COMPRESSED) || stored.starts_with(PLAIN)
}

/// The content as it is stored: compressed when that makes it smaller, and marked either way.
pub fn pack(content: &[u8]) -> Vec<u8> {
    let compressed = zstd::encode_all(content, LEVEL)
        .ok()
        .filter(|compressed| compressed.len() < content.len());
    let (mark, body) = match &compressed {
        Some(compressed) => (COMPRESSED, compressed.as_slice()),
        None => (PLAIN, content),
    };
    let mut out = Vec::with_capacity(mark.len() + body.len());
    out.extend_from_slice(mark);
    out.extend_from_slice(body);
    out
}

/// The original, from what is stored. A content with no mark is an original stored before compression existed.
pub fn unpack(mut stored: Vec<u8>) -> std::io::Result<Vec<u8>> {
    if stored.starts_with(COMPRESSED) {
        zstd::decode_all(&stored[COMPRESSED.len()..])
    } else if stored.starts_with(PLAIN) {
        stored.drain(..PLAIN.len());
        Ok(stored)
    } else {
        Ok(stored)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "test code")]
    use super::*;

    #[test]
    fn an_original_comes_back_byte_for_byte_and_takes_less_room() {
        let statement = "2026-01-05;PRLV SEPA ELECTRICITE;-104,00\n"
            .repeat(400)
            .into_bytes();
        let stored = pack(&statement);
        assert!(is_packed(&stored));
        assert!(stored.len() * 10 < statement.len());
        assert_eq!(unpack(stored).unwrap(), statement);
    }

    #[test]
    fn what_compression_does_not_shrink_is_kept_as_it_is() {
        // bytes with no pattern: compressing them makes them longer
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
        let noise: Vec<u8> = (0..4096)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                (seed >> 24) as u8
            })
            .collect();
        let stored = pack(&noise);
        assert_eq!(stored.len(), noise.len() + 4);
        assert!(is_packed(&stored));
        assert_eq!(unpack(stored).unwrap(), noise);
        assert_eq!(unpack(pack(b"")).unwrap(), b"");
    }

    #[test]
    fn an_original_stored_before_is_read_as_it_is() {
        let before = b"%PDF-1.4 an original stored uncompressed".to_vec();
        assert_eq!(unpack(before.clone()).unwrap(), before);
    }

    #[test]
    fn an_original_that_starts_like_the_mark_is_not_taken_for_a_packed_one() {
        let odd = b"WFZ1 is how this file happens to start".to_vec();
        assert_eq!(unpack(pack(&odd)).unwrap(), odd);
        let odd = b"WFZ0".to_vec();
        assert_eq!(unpack(pack(&odd)).unwrap(), odd);
    }

    #[test]
    fn the_mark_is_written_the_same_for_sql() {
        let hex: String = COMPRESSED[..3].iter().map(|b| format!("{b:02X}")).collect();
        assert_eq!(hex, MARK_HEX);
        assert_eq!(COMPRESSED[..3], PLAIN[..3]);
    }
}
