// Copyright 2026 the Docudis contributors. Licensed under Apache-2.0.

use crate::{
    Detection, DetectionSource, Detector, EntityType, RegexRule, RuleError, RuleSelection,
};
use std::collections::HashSet;

pub const STRONG_CONFIDENCE: f64 = 0.8;

pub struct RegexDetector {
    pub rules: Vec<RegexRule>,
}

impl RegexDetector {
    pub fn new(rules: Vec<RegexRule>) -> Self {
        Self { rules }
    }
    pub fn bundled(
        regions: Option<&HashSet<String>>,
        selection: Option<&RuleSelection>,
    ) -> Result<Self, RuleError> {
        Ok(Self::new(crate::bundled_rules(regions, selection)?))
    }
    pub fn source_for(rule: &RegexRule) -> DetectionSource {
        if rule.is_validated() {
            DetectionSource::ValidatedRule
        } else if rule.confidence >= STRONG_CONFIDENCE {
            DetectionSource::StrongRule
        } else {
            DetectionSource::Rule
        }
    }
    pub fn type_for(rule: &RegexRule, value: &str) -> EntityType {
        let numeric = matches!(
            rule.entity_type,
            EntityType::Id | EntityType::Card | EntityType::Other | EntityType::Phone
        );
        let bare = !value.is_empty()
            && value
                .chars()
                .all(|c| c.is_ascii_digit() || matches!(c, ' ' | '.' | '/' | '-'));
        if Self::source_for(rule) == DetectionSource::Rule && numeric && bare {
            EntityType::Number
        } else {
            rule.entity_type
        }
    }
    pub fn detect_sync(&self, text: &str) -> Result<Vec<Detection>, RegexDetectError> {
        let view = zeros_for_os(text);
        let cells = view.contains('\t').then(|| view.replace('\t', "\u{1}"));
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for rule in &self.rules {
            for haystack in std::iter::once(view.as_str()).chain(cells.as_deref()) {
                let mut search_from = 0;
                while search_from <= haystack.len() {
                    let Some(found) =
                        rule.pattern
                            .find_from_pos(haystack, search_from)
                            .map_err(|e| RegexDetectError {
                                rule: rule.id.clone(),
                                message: e.to_string(),
                            })?
                    else {
                        break;
                    };
                    let prefix = &haystack[..found.start()];
                    let mut guards_match = true;
                    for guard in &rule.leading_guards {
                        let matched =
                            guard
                                .pattern
                                .is_match(prefix)
                                .map_err(|error| RegexDetectError {
                                    rule: rule.id.clone(),
                                    message: format!("lookbehind guard failed: {error}"),
                                })?;
                        if matched != guard.positive {
                            guards_match = false;
                            break;
                        }
                    }
                    if found.start() == found.end()
                        || !guards_match
                        || haystack[found.start()..found.end()].contains(['\t', '\u{1}'])
                        || !seen.insert((rule.id.as_str(), found.start(), found.end()))
                    {
                        search_from = next_char_boundary(haystack, found.start());
                        continue;
                    }
                    let value = &text[found.start()..found.end()];
                    if !rule.validate(value) {
                        search_from = next_char_boundary(haystack, found.start());
                        continue;
                    }
                    out.push(Detection {
                        entity_type: Self::type_for(rule, value),
                        value: value.to_owned(),
                        start: found.start(),
                        end: found.end(),
                        confidence: rule.confidence,
                        detector: rule.id.clone(),
                        source: Self::source_for(rule),
                        enabled: true,
                    });
                    search_from = found.end();
                }
            }
        }
        Ok(out)
    }
}

fn next_char_boundary(text: &str, offset: usize) -> usize {
    if offset >= text.len() {
        return text.len() + 1;
    }
    offset
        + text[offset..]
            .chars()
            .next()
            .expect("offset is a boundary")
            .len_utf8()
}

impl Detector for RegexDetector {
    type Error = RegexDetectError;
    fn detect(&self, text: &str) -> Result<Vec<Detection>, Self::Error> {
        self.detect_sync(text)
    }
}

#[derive(Debug)]
pub struct RegexDetectError {
    pub rule: String,
    pub message: String,
}
impl std::fmt::Display for RegexDetectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "rule {} failed while matching: {}",
            self.rule, self.message
        )
    }
}
impl std::error::Error for RegexDetectError {}

fn zeros_for_os(text: &str) -> String {
    let mut bytes = text.as_bytes().to_vec();
    let mut i = 0;
    while i < bytes.len() {
        if !(bytes[i].is_ascii_digit() || matches!(bytes[i], b'o' | b'O')) {
            i += 1;
            continue;
        }
        let start = i;
        let mut digit = false;
        while i < bytes.len() && (bytes[i].is_ascii_digit() || matches!(bytes[i], b'o' | b'O')) {
            digit |= bytes[i].is_ascii_digit();
            i += 1
        }
        let before = start
            .checked_sub(1)
            .and_then(|_| text[..start].chars().next_back());
        let after = text[i..].chars().next();
        let boundary = |c: Option<char>| c.is_none_or(|v| !v.is_alphanumeric());
        if digit && boundary(before) && boundary(after) {
            for b in &mut bytes[start..i] {
                if matches!(*b, b'o' | b'O') {
                    *b = b'0'
                }
            }
        }
    }
    String::from_utf8(bytes).expect("ASCII substitutions preserve UTF-8")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ocr_zero_only_changes_numeric_tokens() {
        assert_eq!(
            zeros_for_os("Good o113 496 o721 No"),
            "Good 0113 496 0721 No"
        )
    }
    #[test]
    fn variable_guard_detection() {
        let rule = crate::parse_rule_pack(crate::BUNDLED_RULE_PACKS[0].1)
            .unwrap()
            .into_iter()
            .find(|r| r.id.ends_with("named_national_id"))
            .unwrap();
        let view = zeros_for_os("DNI 48291736K");
        assert_eq!(view, "DNI 48291736K");
        let found = RegexDetector::new(vec![rule]).detect_sync(&view).unwrap();
        assert_eq!((found[0].start, found[0].end), (4, 13));
    }
}
