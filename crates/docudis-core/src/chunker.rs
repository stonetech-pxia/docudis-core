// Copyright 2026 the Docudis contributors. Licensed under Apache-2.0.

//! One-tap chunks for the review page, migrated from the Dart reference
//! `chunkText`. Lengths that the Dart code measured in UTF-16 code units
//! (the longest chunk, a bridged word) are still measured that way, so both
//! cut the same text the same way.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

/// A range of the text the user can redact with a single tap, as half-open
/// UTF-8 byte offsets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextChunk {
    pub start: usize,
    pub end: usize,
}

/// Cuts `text` into one-tap chunks. Whatever `taken` covers — the spans the
/// detectors found, as UTF-8 byte ranges — is left out, and is a hard
/// boundary: no chunk crosses a placeholder.
///
/// Three cuts and one merge:
///
/// 1. lines (`\n`), which for photos are the OCR lines;
/// 2. cells inside a line (tab, two or more spaces, `|`), which is what
///    splits `Label : value` and invoice columns;
/// 3. clauses inside a cell (`,` `;` `:` `。` `、` …), keeping the punctuation
///    that belongs to a value (`1,250.00`, `pierre.martin@exemple.fr`);
/// 4. atoms that read as one value — a run of capitalised words, a group of
///    digits, an e-mail, a CJK run — merge back into one chunk, so a full
///    name or a spaced-out phone number takes one tap.
///
/// Lower-case prose words stay one chunk each; lone punctuation is not a
/// chunk at all. CJK has no word boundaries to go by, so a CJK run is only
/// cut at punctuation: one tap there covers a clause.
///
/// Every `taken` range must lie on UTF-8 character boundaries of `text`.
pub fn chunk_text(text: &str, taken: &[(usize, usize)]) -> Vec<TextChunk> {
    let mut chunks = Vec::new();
    for (from, to) in outside(text, taken) {
        for (start, end) in segments(text, from, to) {
            merge_atoms(text, start, end, &mut chunks);
        }
    }
    chunks
}

/// The stretches of `text` that `taken` does not cover.
fn outside(text: &str, taken: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut ranges = taken.to_vec();
    ranges.sort_by_key(|&(start, _)| start);
    let mut out = Vec::new();
    let mut cursor = 0;
    for (start, end) in ranges {
        if end <= cursor {
            continue;
        }
        if start > cursor {
            out.push((cursor, start));
        }
        cursor = end;
    }
    if cursor < text.len() {
        out.push((cursor, text.len()));
    }
    out
}

/// Cuts 1-3: the ranges between hard breaks inside `text[from..to]`.
fn segments(text: &str, from: usize, to: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = from;
    let mut i = from;
    while i < to {
        let width = break_at(text, i);
        if width == 0 {
            i += char_at(text, i).len_utf8();
            continue;
        }
        if i > start {
            out.push((start, i));
        }
        // Every break character is one byte, except the CJK punctuation,
        // which is a single character: `width` counts characters.
        i = text[i..]
            .char_indices()
            .nth(width)
            .map_or(text.len(), |(offset, _)| i + offset);
        start = i;
    }
    if start < to {
        out.push((start, to));
    }
    out
}

/// How many characters the hard break at byte `i` spans, or 0 if there is
/// none.
fn break_at(text: &str, i: usize) -> usize {
    let c = char_at(text, i);
    if matches!(c, '\n' | '\t' | '|' | '\u{a0}') {
        return 1;
    }
    if c == ' ' {
        let n = text[i..].bytes().take_while(|&b| b == b' ').count();
        // A single space joins atoms; a run of them is a column gap.
        return if n >= 2 { n } else { 0 };
    }
    if CLAUSE_PUNCT.contains(&c) {
        return 1;
    }
    if c == ':' {
        return usize::from(i + 1 == text.len() || text.as_bytes()[i + 1] == b' ');
    }
    if c == ',' || c == '.' {
        // Not a break inside a number (1,250.00) or a host name.
        if digit_before(text, i) && digit_after(text, i + 1) {
            return 0;
        }
        if c == '.' {
            if letter_after(text, i + 1) {
                return 0; // exemple.fr
            }
            if abbreviation_before(text, i) {
                return 0; // Ms. Dr. No.
            }
        }
        return 1;
    }
    0
}

const CLAUSE_PUNCT: [char; 31] = [
    ';', '?', '!', '，', '；', '：', '、', '。', '！', '？', '(', ')', '（', '）', '[', ']', '【',
    '】', '《', '》', '"', '“', '”', '「', '」', '『', '』', '<', '>', '{', '}',
];

/// True for `Ms.`, `Dr.`, `No.`: up to three letters, starting upper case.
fn abbreviation_before(text: &str, dot: usize) -> bool {
    let mut first = None;
    for (letters, c) in text[..dot].chars().rev().enumerate() {
        if !latin_letter(c) {
            break;
        }
        if letters == 3 {
            return false;
        }
        first = Some(c);
    }
    first.is_some_and(is_upper)
}

fn char_at(text: &str, i: usize) -> char {
    text[i..].chars().next().expect("offset is inside the text")
}

fn digit_before(text: &str, i: usize) -> bool {
    text[..i]
        .chars()
        .next_back()
        .is_some_and(|c| c.is_ascii_digit())
}

fn digit_after(text: &str, i: usize) -> bool {
    text.get(i..)
        .and_then(|rest| rest.chars().next())
        .is_some_and(|c| c.is_ascii_digit())
}

fn letter_after(text: &str, i: usize) -> bool {
    text.get(i..)
        .and_then(|rest| rest.chars().next())
        .is_some_and(latin_letter)
}

/// `[A-Za-zÀ-ɏ]`, the Dart reference's Latin letter class.
fn latin_letter(c: char) -> bool {
    c.is_ascii_alphabetic() || ('\u{c0}'..='\u{24f}').contains(&c)
}

/// Upper case the way Dart tests one character: it equals its upper-case
/// form and differs from its lower-case form.
fn is_upper(c: char) -> bool {
    let upper: String = c.to_uppercase().collect();
    let lower: String = c.to_lowercase().collect();
    upper.chars().eq([c]) && !lower.chars().eq([c])
}

fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

/// Cut 4: the atoms inside one segment, salient neighbours merged.
fn merge_atoms(text: &str, start: usize, end: usize, out: &mut Vec<TextChunk>) {
    let atoms: Vec<(usize, usize)> = ATOM
        .find_iter(&text[start..end])
        .map(|m| (start + m.start(), start + m.end()))
        .collect();
    let mut i = 0;
    while i < atoms.len() {
        let (from, to) = atoms[i];
        if !salient(&text[from..to]) {
            // A lower-case prose word is a chunk of its own; punctuation is
            // not.
            if latin_letter(char_at(text, from)) {
                out.push(TextChunk {
                    start: from,
                    end: to,
                });
            }
            i += 1;
            continue;
        }
        let mut last = i; // the last salient atom taken so far
        let mut bridged = 0; // short lower-case words waiting for another salient
        let mut j = i;
        while j + 1 < atoms.len() {
            let (next_from, next_to) = atoms[j + 1];
            let gap = &text[atoms[j].1..next_from];
            if !gap.is_empty() && gap != " " {
                break;
            }
            if utf16_len(&text[from..next_to]) > MAX_CHUNK {
                break;
            }
            if salient(&text[next_from..next_to]) {
                last = j + 1;
                bridged = 0;
            } else if bridged < MAX_BRIDGE
                && utf16_len(&text[next_from..next_to]) <= MAX_BRIDGE_WORD
                && latin_letter(char_at(text, next_from))
            {
                bridged += 1; // e.g. "14 rue de la Paix"
            } else {
                break;
            }
            j += 1;
        }
        out.push(TextChunk {
            start: from,
            end: atoms[last].1,
        });
        // Words bridged past the last salient atom are chunks of their own.
        i = last + 1;
    }
}

/// The longest chunk one tap may cover, in UTF-16 code units.
const MAX_CHUNK: usize = 60;

/// How many short lower-case words may sit between two salient atoms, and
/// how short they have to be.
///
/// Three characters at most: enough for the particles that hold a name or an
/// address together ("14 rue de la Paix", "Ludwig van Beethoven"), short
/// enough to leave ordinary verbs out ("Please call Sarah Meyer" is three
/// chunks, not one).
const MAX_BRIDGE: usize = 3;
const MAX_BRIDGE_WORD: usize = 3;

/// Whether the atom reads as a value on its own, and so merges with its
/// neighbours: e-mails and URLs, abbreviations, capitalised words, digit
/// groups, CJK runs, currency signs.
fn salient(atom: &str) -> bool {
    if atom.contains('@') || atom.contains("://") {
        return true;
    }
    if atom.chars().any(cjk) {
        return true;
    }
    let first = atom.chars().next().expect("atoms are not empty");
    if first.is_ascii_digit() {
        return true;
    }
    if CURRENCY.contains(&atom) {
        return true;
    }
    latin_letter(first) && is_upper(first)
}

const CURRENCY: [&str; 8] = ["€", "£", "$", "¥", "%", "₽", "₹", "¢"];

fn cjk(c: char) -> bool {
    matches!(c, '\u{3040}'..='\u{30ff}' | '\u{3400}'..='\u{9fff}' | '\u{ac00}'..='\u{d7af}' | '\u{f900}'..='\u{faff}')
}

/// The Dart reference's atoms. Its `\s` is the JavaScript whitespace set,
/// spelled out here because Rust's `\s` is Unicode `White_Space`.
static ATOM: LazyLock<Regex> = LazyLock::new(|| {
    let not_space = r"[^\t\n\x0B\x0C\r \x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}]";
    let pattern = [
        format!("{not_space}+@{not_space}+"), // e-mail
        format!("[A-Za-z][A-Za-z0-9+.\\-]*://{not_space}+|www\\.{not_space}+"), // URL
        r"[\x{3040}-\x{30FF}\x{3400}-\x{9FFF}\x{AC00}-\x{D7AF}\x{F900}-\x{FAFF}]+".to_owned(), // CJK run
        r"[A-Za-z\x{C0}-\x{24F}]{1,3}\.".to_owned(), // Ms. Dr. No.
        r"[0-9](?:[0-9.,/\-+'’]*[0-9])?".to_owned(), // digit group
        r"[A-Za-z\x{C0}-\x{24F}][A-Za-z\x{C0}-\x{24F}'’\-]*".to_owned(), // word
        not_space.to_owned(),                        // anything else, one character
    ]
    .join("|");
    Regex::new(&pattern).expect("atom pattern compiles")
});

#[cfg(test)]
mod tests {
    use super::*;

    fn chunks<'a>(text: &'a str, taken: &[(usize, usize)]) -> Vec<&'a str> {
        chunk_text(text, taken)
            .into_iter()
            .map(|c| &text[c.start..c.end])
            .collect()
    }

    #[test]
    fn names_and_spaced_numbers_take_one_tap() {
        assert_eq!(
            chunks("Please call Sarah Meyer on 06 12 34 56 78, today.", &[]),
            ["Please", "call", "Sarah Meyer on 06 12 34 56 78", "today"]
        );
    }

    #[test]
    fn taken_spans_are_hard_boundaries() {
        let text = "Alice Martin met Bob";
        assert_eq!(chunks(text, &[(6, 12)]), ["Alice", "met", "Bob"]);
    }

    #[test]
    fn cjk_runs_are_cut_at_punctuation_only() {
        assert_eq!(chunks("客户：张三，电话", &[]), ["客户", "张三", "电话"]);
    }
}
