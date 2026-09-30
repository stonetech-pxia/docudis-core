// Copyright 2026 the Docudis contributors.
//
// Licensed under Apache-2.0. This is a Rust migration of portions of the
// DocCloak.Core-derived Dart implementation. See ../NOTICE.

use crate::{Detection, EntityType, PersonGender, PlaceholderMap};
use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Replacement {
    /// Inclusive UTF-8 byte offset in the original input.
    pub start: usize,
    /// Exclusive UTF-8 byte offset in the original input.
    pub end: usize,
    pub placeholder: String,
}

#[derive(Debug, Clone)]
pub struct AnonymizedText {
    pub text: String,
    pub map: PlaceholderMap,
    pub replacements: Vec<Replacement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnonymizeError {
    InvalidRange {
        index: usize,
        start: usize,
        end: usize,
    },
    OffsetOutOfBounds {
        index: usize,
        offset: usize,
    },
    OffsetNotCharBoundary {
        index: usize,
        offset: usize,
    },
}

impl fmt::Display for AnonymizeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRange { index, start, end } => {
                write!(
                    formatter,
                    "detection {index} has invalid range {start}..{end}"
                )
            }
            Self::OffsetOutOfBounds { index, offset } => {
                write!(
                    formatter,
                    "detection {index} offset {offset} is out of bounds"
                )
            }
            Self::OffsetNotCharBoundary { index, offset } => write!(
                formatter,
                "detection {index} offset {offset} is not a UTF-8 character boundary"
            ),
        }
    }
}

impl Error for AnonymizeError {}

/// Replaces enabled detections in reading order.
///
/// Detections use half-open UTF-8 byte offsets. Earlier spans win overlaps,
/// matching the current Dart anonymizer after its start-offset sort.
pub fn anonymize(
    original: &str,
    detections: &[Detection],
    previous: Option<&PlaceholderMap>,
) -> Result<AnonymizedText, AnonymizeError> {
    let mut active: Vec<(usize, &Detection)> = detections
        .iter()
        .enumerate()
        .filter(|(_, detection)| detection.enabled)
        .collect();
    active.sort_by_key(|(_, detection)| detection.start);

    for (index, detection) in &active {
        validate_detection(original, *index, detection)?;
    }

    let mut map = PlaceholderMap::new();
    if let Some(previous) = previous {
        map.import(previous.entries());
    }
    let mut output = String::with_capacity(original.len());
    let mut replacements = Vec::new();
    let mut cursor = 0;

    for (_, detection) in active {
        if detection.start < cursor {
            continue;
        }
        let raw = &original[detection.start..detection.end];
        let lead_len = raw
            .char_indices()
            .take_while(|(_, ch)| is_edge_separator(*ch))
            .map(|(_, ch)| ch.len_utf8())
            .sum::<usize>();
        let tail_len = raw
            .char_indices()
            .rev()
            .take_while(|(_, ch)| is_edge_separator(*ch))
            .map(|(_, ch)| ch.len_utf8())
            .sum::<usize>()
            .min(raw.len().saturating_sub(lead_len));
        let value_end = raw.len() - tail_len;
        let value = &raw[lead_len..value_end];
        if value.is_empty() {
            continue;
        }

        let gender = (detection.entity_type == EntityType::Person)
            .then(|| gender_before(original, detection.start))
            .flatten();
        let placeholder = map.placeholder_for_with_gender(value, detection.entity_type, gender);
        output.push_str(&original[cursor..detection.start]);
        output.push_str(&raw[..lead_len]);
        output.push_str(&placeholder);
        output.push_str(&raw[value_end..]);
        let start = detection.start + lead_len;
        replacements.push(Replacement {
            start,
            end: start + value.len(),
            placeholder,
        });
        cursor = detection.end;
    }

    output.push_str(&original[cursor..]);
    Ok(AnonymizedText {
        text: output,
        map,
        replacements,
    })
}

fn validate_detection(
    original: &str,
    index: usize,
    detection: &Detection,
) -> Result<(), AnonymizeError> {
    if detection.end <= detection.start {
        return Err(AnonymizeError::InvalidRange {
            index,
            start: detection.start,
            end: detection.end,
        });
    }
    for offset in [detection.start, detection.end] {
        if offset > original.len() {
            return Err(AnonymizeError::OffsetOutOfBounds { index, offset });
        }
        if !original.is_char_boundary(offset) {
            return Err(AnonymizeError::OffsetNotCharBoundary { index, offset });
        }
    }
    Ok(())
}

fn is_edge_separator(ch: char) -> bool {
    ch.is_whitespace() || ch == ',' || ch == ';'
}

fn gender_before(text: &str, start: usize) -> Option<PersonGender> {
    let before = &text[..start];
    let start_utf16 = before
        .char_indices()
        .rev()
        .scan(0, |units, (byte, ch)| {
            *units += ch.len_utf16();
            Some((byte, *units))
        })
        .take_while(|(_, units)| *units <= 16)
        .map(|(byte, _)| byte)
        .last()
        .unwrap_or(start);
    let context = &text[start_utf16..start];
    const FEMALE: [&str; 13] = [
        "Mme",
        "Madame",
        "Mlle",
        "Mademoiselle",
        "Mrs",
        "Ms",
        "Miss",
        "Sra",
        "Señora",
        "Srta",
        "Señorita",
        "Doña",
        "Dña",
    ];
    const MALE: [&str; 7] = ["M.", "Monsieur", "Mr", "Sr", "Señor", "Don", "D."];
    if has_honorific(context, &FEMALE) {
        Some(PersonGender::Female)
    } else if has_honorific(context, &MALE) {
        Some(PersonGender::Male)
    } else {
        None
    }
}

fn has_honorific(context: &str, honorifics: &[&str]) -> bool {
    let without_space = context.trim_end_matches([' ', '\u{a0}']);
    if without_space.len() == context.len() {
        return false;
    }
    honorifics.iter().any(|honorific| {
        [false, true].into_iter().any(|add_period| {
            let candidate = if add_period {
                format!("{honorific}.")
            } else {
                (*honorific).to_owned()
            };
            let Some(prefix) = without_space.strip_suffix(&candidate) else {
                return false;
            };
            prefix
                .chars()
                .next_back()
                .map(|ch| !ch.is_alphabetic() && ch != '.')
                .unwrap_or(true)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::gender_before;
    use crate::PersonGender;

    #[test]
    fn reads_reference_honorifics_without_crossing_word_boundaries() {
        for prefix in ["Mme ", "Dña.\u{a0}", "Mrs. "] {
            let text = format!("{prefix}Garnier");
            assert_eq!(
                gender_before(&text, text.len() - "Garnier".len()),
                Some(PersonGender::Female)
            );
        }
        for prefix in ["M. ", "Monsieur ", "Mr. ", "D.\u{a0}"] {
            let text = format!("{prefix}Garnier");
            assert_eq!(
                gender_before(&text, text.len() - "Garnier".len()),
                Some(PersonGender::Male)
            );
        }
        let text = "NotMr Garnier";
        assert_eq!(gender_before(text, text.len() - "Garnier".len()), None);
    }
}
