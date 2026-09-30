// Copyright 2026 the Docudis contributors. Licensed under Apache-2.0.

use crate::{Detection, DetectionSource, Detector, EntityType};
use regex::Regex;
use serde::Deserialize;
use std::collections::{BTreeSet, HashMap, HashSet};

pub struct DictionaryDetector {
    terms: BTreeSet<String>,
}
impl DictionaryDetector {
    pub fn new(terms: impl IntoIterator<Item = String>) -> Self {
        Self {
            terms: terms
                .into_iter()
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty())
                .collect(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }
    pub fn detect_sync(&self, text: &str) -> Vec<Detection> {
        let mut out = Vec::new();
        for term in &self.terms {
            let wordy = !term.chars().any(is_cjk);
            let pattern = Regex::new(&format!("(?i:{})", loose_pattern(term)))
                .expect("escaped dictionary pattern");
            for m in pattern.find_iter(text) {
                let before = text[..m.start()].chars().next_back();
                let after = text[m.end()..].chars().next();
                if wordy
                    && (before.is_some_and(char::is_alphanumeric)
                        || after.is_some_and(char::is_alphanumeric))
                {
                    continue;
                }
                out.push(hit(
                    EntityType::Custom,
                    text,
                    m.start(),
                    m.end(),
                    1.0,
                    "dictionary",
                    DetectionSource::Dictionary,
                ));
            }
        }
        out
    }
}
impl Detector for DictionaryDetector {
    type Error = std::convert::Infallible;
    fn detect(&self, text: &str) -> Result<Vec<Detection>, Self::Error> {
        Ok(self.detect_sync(text))
    }
}

fn loose_pattern(term: &str) -> String {
    let mut out = String::new();
    for (i, word) in term.split_whitespace().enumerate() {
        if i > 0 {
            out.push_str(r"\s+")
        }
        for ch in word.chars() {
            let lower: String = ch.to_lowercase().collect();
            let family = [
                "aàáâãäå",
                "cç",
                "eèéêë",
                "iìíîï",
                "nñ",
                "oòóôõö",
                "uùúûü",
                "yýÿ",
            ]
            .into_iter()
            .find(|f| f.contains(&lower));
            if let Some(f) = family {
                out.push('[');
                out.push_str(f);
                out.push(']')
            } else {
                out.push_str(&regex::escape(&ch.to_string()))
            }
        }
    }
    out
}
fn is_cjk(c: char) -> bool {
    matches!(c,'\u{3040}'..='\u{30ff}'|'\u{3400}'..='\u{9fff}'|'\u{ac00}'..='\u{d7af}')
}

pub struct BundledListDetector {
    latin: HashMap<String, EntityType>,
    cjk: HashMap<String, EntityType>,
    latin_first: HashSet<String>,
    cjk_first: HashSet<char>,
    cjk_lengths: Vec<usize>,
    max_words: usize,
}
impl BundledListDetector {
    pub fn new(lists: impl IntoIterator<Item = (EntityType, Vec<String>)>) -> Self {
        let mut this = Self {
            latin: HashMap::new(),
            cjk: HashMap::new(),
            latin_first: HashSet::new(),
            cjk_first: HashSet::new(),
            cjk_lengths: Vec::new(),
            max_words: 0,
        };
        let mut lengths = BTreeSet::new();
        for (kind, values) in lists {
            for raw in values {
                let term = raw.trim_matches(|c: char| !c.is_alphanumeric()).to_owned();
                if term.is_empty() {
                    continue;
                }
                if term.chars().any(is_cjk) {
                    if term.chars().count() < 2 {
                        continue;
                    }
                    this.cjk_first.insert(term.chars().next().unwrap());
                    lengths.insert(term.chars().count());
                    this.cjk.insert(term, kind);
                } else {
                    let words: Vec<_> = word_ranges(&term);
                    if words.is_empty() || term.chars().count() < 3 {
                        continue;
                    }
                    this.latin_first
                        .insert(term[words[0].0..words[0].1].to_owned());
                    this.max_words = this.max_words.max(words.len());
                    this.latin.insert(term, kind);
                }
            }
        }
        this.cjk_lengths = lengths.into_iter().collect();
        this
    }
    pub fn bundled() -> Result<Self, serde_json::Error> {
        #[derive(Deserialize)]
        struct Entry {
            names: Vec<String>,
        }
        fn names(src: &str) -> Result<Vec<String>, serde_json::Error> {
            Ok(serde_json::from_str::<Vec<Entry>>(src)?
                .into_iter()
                .flat_map(|e| e.names)
                .collect())
        }
        Ok(Self::new([
            (
                EntityType::Company,
                names(include_str!("../../../data/lists/companies.json"))?,
            ),
            (
                EntityType::Address,
                names(include_str!("../../../data/lists/places_zh.json"))?,
            ),
        ]))
    }
    pub fn term_count(&self) -> usize {
        self.latin.len() + self.cjk.len()
    }
    pub fn detect_sync(&self, text: &str) -> Vec<Detection> {
        let mut out = Vec::new();
        let words = word_ranges(text);
        for i in 0..words.len() {
            if !self.latin_first.contains(&text[words[i].0..words[i].1]) {
                continue;
            }
            for n in 1..=self.max_words.min(words.len() - i) {
                let (start, end) = (words[i].0, words[i + n - 1].1);
                if let Some(kind) = self.latin.get(&text[start..end]) {
                    out.push(hit(
                        *kind,
                        text,
                        start,
                        end,
                        0.9,
                        &format!("list:{}", kind.placeholder_name().to_ascii_lowercase()),
                        DetectionSource::BundledList,
                    ));
                }
            }
        }
        let positions: Vec<_> = text.char_indices().collect();
        for (i, (start, ch)) in positions.iter().copied().enumerate() {
            if !self.cjk_first.contains(&ch) {
                continue;
            }
            for len in &self.cjk_lengths {
                if i + len > positions.len() {
                    break;
                }
                let end = positions.get(i + len).map_or(text.len(), |x| x.0);
                if let Some(kind) = self.cjk.get(&text[start..end]) {
                    out.push(hit(
                        *kind,
                        text,
                        start,
                        end,
                        0.9,
                        &format!("list:{}", kind.placeholder_name().to_ascii_lowercase()),
                        DetectionSource::BundledList,
                    ));
                }
            }
        }
        out
    }
}
impl Detector for BundledListDetector {
    type Error = std::convert::Infallible;
    fn detect(&self, text: &str) -> Result<Vec<Detection>, Self::Error> {
        Ok(self.detect_sync(text))
    }
}
fn word_ranges(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, ch) in text.char_indices() {
        if ch.is_alphanumeric() {
            start.get_or_insert(i);
        } else if let Some(s) = start.take() {
            out.push((s, i));
        }
    }
    if let Some(s) = start {
        out.push((s, text.len()))
    }
    out
}
fn hit(
    kind: EntityType,
    text: &str,
    start: usize,
    end: usize,
    confidence: f64,
    detector: &str,
    source: DetectionSource,
) -> Detection {
    Detection {
        entity_type: kind,
        value: text[start..end].to_owned(),
        start,
        end,
        confidence,
        detector: detector.to_owned(),
        source,
        enabled: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dictionary_accents_spaces_and_boundaries() {
        let d = DictionaryDetector::new(["Emilie Dupont".to_owned(), "张三".to_owned()]);
        let got = d.detect_sync("EMILIE\nDUPONT deliveryEmilie 张三丰");
        assert_eq!(
            got.iter().map(|d| d.value.as_str()).collect::<Vec<_>>(),
            ["EMILIE\nDUPONT", "张三"]
        )
    }
    #[test]
    fn bundled_case_and_cjk() {
        let d = BundledListDetector::new([(
            EntityType::Company,
            vec!["Apple".into(), "阿里巴巴".into()],
        )]);
        let got = d.detect_sync("Apple apple 阿里巴巴集团");
        assert_eq!(
            got.iter().map(|d| d.value.as_str()).collect::<Vec<_>>(),
            ["Apple", "阿里巴巴"]
        )
    }
}
