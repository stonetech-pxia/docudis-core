// Copyright 2026 the Docudis contributors. Licensed under Apache-2.0.

//! Tells which document an AI reply answers, migrated from the Dart
//! reference `ReplyMatcher`. The label contexts are cut at the same UTF-16
//! distances as the Dart code, and evidence is summed in the same order, so
//! both reach the same verdict.

use crate::{
    placeholder_map::{canonical_placeholder_form, mangled_candidates},
    EntityType,
};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    sync::LazyLock,
};

/// One document an AI reply may answer: the anonymized text that was sent
/// and the placeholders its key restores. Its words and label contexts are
/// indexed once.
#[derive(Debug, Clone)]
pub struct ReplyCandidate {
    labels: HashSet<String>,
    words: HashSet<String>,
    contexts: HashMap<String, HashSet<String>>,
}

impl ReplyCandidate {
    pub fn new<'a>(output: &str, placeholders: impl IntoIterator<Item = &'a str>) -> Self {
        let indexed = Indexed::new(output);
        Self {
            labels: placeholders.into_iter().filter_map(label).collect(),
            words: indexed.words.into_iter().collect(),
            contexts: indexed
                .contexts
                .into_iter()
                .map(|(label, around)| (label, around.into_iter().collect()))
                .collect(),
        }
    }

    /// Highest number this document gave a label type ("person" -> 3).
    fn highest(&self, kind: &str) -> u64 {
        self.labels
            .iter()
            .map(|label| split(label))
            .filter(|(t, _)| *t == kind)
            .map(|(_, n)| n)
            .max()
            .unwrap_or(0)
    }
}

/// What a pasted reply says about the document it is restored with.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplyCheck {
    /// Labels in the reply the document never issued, as `[PERSON_4]`, in
    /// reading order, except the `invented` ones.
    pub unknown: Vec<String>,
    /// Labels an AI plausibly made up: a higher number of a type the
    /// document has (`[PERSON_4]` next to `[PERSON_1]`..`[PERSON_3]`), or a
    /// type no document ever has (`[EXPEDIENTE_1]`).
    pub invented: Vec<String>,
    /// Id of another document the reply fits clearly better.
    pub better_match: Option<String>,
}

/// Tells which document an AI reply answers, so a reply is not restored
/// with another document's key: every document numbers its labels from 1,
/// so `[PERSON_1]` and `[IBAN_1]` exist in most of them and the labels alone
/// cannot tell.
///
/// Evidence, per document: the words around each label in the reply that
/// also stand around that label in the document ("DNI [ID_1]" against
/// "Nº de referencia: [ID_1]"), plus the reply's words the document shares,
/// weighted by how few documents share them. A document fits clearly better
/// when it misses fewer of the reply's labels and has no less evidence, or
/// misses no more and has at least twice the evidence. A short reply with
/// no words of its own gives no evidence either way.
#[derive(Debug, Clone, Default)]
pub struct ReplyMatcher {
    /// Record id and document, in the caller's order: the first of two
    /// equally good other documents wins.
    candidates: Vec<(String, ReplyCandidate)>,
}

impl ReplyMatcher {
    pub fn new(candidates: impl IntoIterator<Item = (String, ReplyCandidate)>) -> Self {
        Self {
            candidates: candidates.into_iter().collect(),
        }
    }

    pub fn check(&self, reply: &str, id: &str) -> ReplyCheck {
        let Some(current) = self.candidate(id) else {
            return ReplyCheck::default();
        };
        let indexed = Indexed::new(reply);
        if indexed.labels.is_empty() {
            return ReplyCheck::default();
        }

        let n = self.candidates.len() as f64;
        let idf: Vec<f64> = indexed
            .words
            .iter()
            .map(|w| {
                let df = self
                    .candidates
                    .iter()
                    .filter(|(_, c)| c.words.contains(w))
                    .count() as f64;
                ((n + 1.0) / (df + 1.0)).ln()
            })
            .collect();
        let evidence = |c: &ReplyCandidate| {
            let mut score = 0.0;
            for (label, around) in &indexed.contexts {
                if let Some(there) = c.contexts.get(label) {
                    score += around.iter().filter(|w| there.contains(*w)).count() as f64;
                }
            }
            for (w, weight) in indexed.words.iter().zip(&idf) {
                if c.words.contains(w) {
                    score += weight;
                }
            }
            score
        };
        let missing = |c: &ReplyCandidate| {
            indexed
                .labels
                .iter()
                .filter(|l| !c.labels.contains(*l))
                .count()
        };

        let own_score = evidence(current);
        let own_missing = missing(current);
        let mut better: Option<&str> = None;
        let mut better_score = 0.0;
        for (other_id, other) in &self.candidates {
            if other_id == id {
                continue;
            }
            let score = evidence(other);
            let misses = missing(other);
            let fits = (misses < own_missing && score >= own_score)
                || (misses <= own_missing && score >= 2.0 * own_score && score - own_score >= 3.0);
            if fits && (better.is_none() || score > better_score) {
                better = Some(other_id);
                better_score = score;
            }
        }

        let mut check = ReplyCheck {
            better_match: better.map(str::to_owned),
            ..ReplyCheck::default()
        };
        for label in indexed
            .labels
            .iter()
            .filter(|l| !current.labels.contains(*l))
        {
            let (kind, number) = split(label);
            let highest = current.highest(kind);
            let issued = EntityType::from_name(&kind.to_uppercase()).is_some();
            let shown = format!("[{}]", label.to_uppercase());
            if !issued || (highest > 0 && number > highest) {
                check.invented.push(shown);
            } else {
                check.unknown.push(shown);
            }
        }
        check
    }

    fn candidate(&self, id: &str) -> Option<&ReplyCandidate> {
        self.candidates
            .iter()
            .find(|(candidate_id, _)| candidate_id == id)
            .map(|(_, c)| c)
    }
}

/// A text's labels (canonical, "person_1"), its words outside labels, and
/// the words around each label, all in reading order.
struct Indexed {
    labels: Vec<String>,
    words: Vec<String>,
    contexts: Vec<(String, Vec<String>)>,
}

impl Indexed {
    fn new(text: &str) -> Self {
        // Labels are blanked with one space per UTF-16 code unit, so UTF-16
        // positions in `plain` are those of `text`.
        let mut plain = String::with_capacity(text.len());
        let mut spans = Vec::new();
        let mut cursor = 0;
        let mut cursor16 = 0;
        for (start, end) in mangled_candidates(text) {
            let Some(found) = label(&text[start..end]) else {
                continue;
            };
            let start16 = cursor16 + utf16_len(&text[cursor..start]);
            let end16 = start16 + utf16_len(&text[start..end]);
            plain.push_str(&text[cursor..start]);
            plain.extend(std::iter::repeat_n(' ', end16 - start16));
            spans.push((found, start16, end16));
            cursor = end;
            cursor16 = end16;
        }
        plain.push_str(&text[cursor..]);

        let mut indexed = Self {
            labels: Vec::new(),
            words: unique(words_of(&plain)),
            contexts: Vec::new(),
        };
        let length16 = utf16_len(&plain);
        for (found, start, end) in spans {
            let before = words_of(utf16_slice(&plain, start.saturating_sub(40), start));
            let after = words_of(utf16_slice(&plain, end, (end + 25).min(length16)));
            let around: Vec<String> = before[before.len().saturating_sub(3)..]
                .iter()
                .chain(after.iter().take(2))
                .cloned()
                .collect();
            if !indexed.labels.contains(&found) {
                indexed.labels.push(found.clone());
            }
            match indexed.contexts.iter_mut().find(|(l, _)| *l == found) {
                Some((_, existing)) => {
                    for w in around {
                        if !existing.contains(&w) {
                            existing.push(w);
                        }
                    }
                }
                None => indexed.contexts.push((found, unique(around))),
            }
        }
        indexed
    }
}

static WORD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\p{L}\p{N}]{3,}").expect("word pattern compiles"));

fn words_of(text: &str) -> Vec<String> {
    WORD.find_iter(text)
        .map(|m| m.as_str().to_lowercase())
        .collect()
}

fn unique(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .filter(|v| seen.insert(v.clone()))
        .collect()
}

/// "[PERSON_1]", "[person 1]" or a bare "PERSON_1" as "person_1"; None for
/// anything that is not a typed, numbered label ("[12/9/26, 9:14]").
fn label(candidate: &str) -> Option<String> {
    let canonical = canonical_placeholder_form(candidate)?;
    let (kind, digits) = canonical.rsplit_once('_')?;
    let typed = kind
        .split('_')
        .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_lowercase()));
    let numbered = !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit());
    (typed && numbered).then_some(canonical)
}

/// "person_12" as ("person", 12). Numbers too long to count are the highest.
fn split(label: &str) -> (&str, u64) {
    let (kind, digits) = label.rsplit_once('_').expect("labels are numbered");
    (kind, digits.parse().unwrap_or(u64::MAX))
}

fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

/// `text[start..end)` in UTF-16 code units. A bound inside a surrogate pair
/// leaves that character out, as the lone surrogate Dart would keep there
/// is never part of a word.
fn utf16_slice(text: &str, start: usize, end: usize) -> &str {
    let mut from = text.len();
    let mut to = text.len();
    let mut at = 0;
    for (byte, c) in text.char_indices() {
        if at >= start && from == text.len() {
            from = byte;
        }
        if at + c.len_utf16() > end {
            to = byte;
            break;
        }
        at += c.len_utf16();
    }
    &text[from.min(to)..to]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_slice_drops_split_surrogate_pairs() {
        let text = "a😀b";
        assert_eq!(utf16_slice(text, 0, 2), "a");
        assert_eq!(utf16_slice(text, 2, 4), "b");
        assert_eq!(utf16_slice(text, 1, 3), "😀");
        assert_eq!(utf16_slice(text, 0, 4), text);
    }

    #[test]
    fn labels_need_a_type_and_a_number() {
        assert_eq!(label("**[person 1]**").as_deref(), Some("person_1"));
        assert_eq!(label("[12/9/26, 9:14]"), None);
        assert_eq!(label("[note]"), None);
    }
}
