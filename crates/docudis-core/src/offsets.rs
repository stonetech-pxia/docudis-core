// Copyright 2026 the Docudis contributors. Licensed under Apache-2.0.

use std::{error::Error, fmt};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OffsetError {
    Utf8OutOfBounds,
    NotUtf8Boundary,
    Utf16OutOfBounds,
    SplitsSurrogatePair,
}

impl fmt::Display for OffsetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Utf8OutOfBounds => "UTF-8 byte offset is out of bounds",
            Self::NotUtf8Boundary => "UTF-8 byte offset is not a character boundary",
            Self::Utf16OutOfBounds => "UTF-16 code-unit offset is out of bounds",
            Self::SplitsSurrogatePair => "UTF-16 code-unit offset splits a surrogate pair",
        })
    }
}

impl Error for OffsetError {}

/// Converts a UTF-8 byte offset to a UTF-16 code-unit offset.
pub fn utf8_to_utf16_offset(text: &str, byte_offset: usize) -> Result<usize, OffsetError> {
    if byte_offset > text.len() {
        return Err(OffsetError::Utf8OutOfBounds);
    }
    if !text.is_char_boundary(byte_offset) {
        return Err(OffsetError::NotUtf8Boundary);
    }
    Ok(text[..byte_offset].encode_utf16().count())
}

/// Converts a UTF-16 code-unit offset to a UTF-8 byte offset.
///
/// An offset between the high and low surrogate of a supplementary scalar is
/// rejected because it cannot name a Rust string boundary.
pub fn utf16_to_utf8_offset(text: &str, code_unit_offset: usize) -> Result<usize, OffsetError> {
    let mut utf16_cursor = 0;
    for (byte_offset, ch) in text.char_indices() {
        if utf16_cursor == code_unit_offset {
            return Ok(byte_offset);
        }
        let next = utf16_cursor + ch.len_utf16();
        if code_unit_offset < next {
            return Err(OffsetError::SplitsSurrogatePair);
        }
        utf16_cursor = next;
    }
    if utf16_cursor == code_unit_offset {
        Ok(text.len())
    } else {
        Err(OffsetError::Utf16OutOfBounds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_ascii_cjk_emoji_and_combining_sequences() {
        let text = "A张😀e\u{301}Z";
        let boundaries = [(0, 0), (1, 1), (4, 2), (8, 4), (9, 5), (11, 6), (12, 7)];
        for (utf8, utf16) in boundaries {
            assert_eq!(utf8_to_utf16_offset(text, utf8), Ok(utf16));
            assert_eq!(utf16_to_utf8_offset(text, utf16), Ok(utf8));
        }
        assert_eq!(
            utf16_to_utf8_offset(text, 3),
            Err(OffsetError::SplitsSurrogatePair)
        );
        assert_eq!(
            utf8_to_utf16_offset(text, 2),
            Err(OffsetError::NotUtf8Boundary)
        );
    }
}
