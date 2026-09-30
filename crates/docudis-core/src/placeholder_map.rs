// Copyright 2026 the Docudis contributors.
//
// Licensed under Apache-2.0. This is a Rust migration of portions of the
// DocCloak.Core-derived Dart implementation. See ../NOTICE.

use crate::EntityType;
use serde::{Deserialize, Serialize};
use std::{
    cmp::Reverse,
    collections::{HashMap, HashSet},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersonGender {
    Female,
    Male,
}

/// One reversible mapping between an original value and its placeholder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MappingEntry {
    pub original: String,
    pub placeholder: String,
    #[serde(rename = "type")]
    pub entity_type: EntityType,
}

/// Assigns typed placeholders and restores exact or safely mangled tokens.
#[derive(Debug, Clone, Default)]
pub struct PlaceholderMap {
    forward: HashMap<String, String>,
    reverse: HashMap<String, String>,
    types: HashMap<String, EntityType>,
    genders: HashMap<String, PersonGender>,
    counters: HashMap<EntityType, usize>,
    insertion_order: Vec<String>,
}

impl PlaceholderMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.forward.is_empty()
    }

    /// Number of distinct placeholders, not aliases.
    pub fn len(&self) -> usize {
        self.reverse.len()
    }

    pub fn placeholder_for(&mut self, original: &str, entity_type: EntityType) -> String {
        self.placeholder_for_with_gender(original, entity_type, None)
    }

    pub fn placeholder_for_with_gender(
        &mut self,
        original: &str,
        entity_type: EntityType,
        gender: Option<PersonGender>,
    ) -> String {
        if let Some(existing) = self.forward.get(original) {
            return existing.clone();
        }

        if entity_type == EntityType::Person {
            if let Some(gender) = gender {
                self.genders.insert(original.to_owned(), gender);
            }
            if let Some(variant) = self.find_person_variant(original, gender) {
                let placeholder = self.forward[&variant].clone();
                self.insert_forward(original, &placeholder, entity_type);
                let replace_canonical = self
                    .reverse
                    .get(&placeholder)
                    .map(|canonical| utf16_len(original) > utf16_len(canonical))
                    .unwrap_or(true);
                if replace_canonical {
                    self.reverse
                        .insert(placeholder.clone(), original.to_owned());
                }
                return placeholder;
            }
        }

        let placeholder = self.next(entity_type);
        self.insert_forward(original, &placeholder, entity_type);
        self.reverse
            .insert(placeholder.clone(), original.to_owned());
        placeholder
    }

    fn insert_forward(&mut self, original: &str, placeholder: &str, entity_type: EntityType) {
        if !self.forward.contains_key(original) {
            self.insertion_order.push(original.to_owned());
        }
        self.forward
            .insert(original.to_owned(), placeholder.to_owned());
        self.types.insert(original.to_owned(), entity_type);
    }

    fn next(&mut self, entity_type: EntityType) -> String {
        let counter = self.counters.entry(entity_type).or_default();
        loop {
            *counter += 1;
            let placeholder = format!("[{}_{}]", entity_type.placeholder_name(), counter);
            if !self.reverse.contains_key(&placeholder) {
                return placeholder;
            }
        }
    }

    fn find_person_variant(&self, value: &str, gender: Option<PersonGender>) -> Option<String> {
        let tokens = person_tokens(value);
        if tokens.is_empty() {
            return None;
        }
        let mut best: Option<&str> = None;
        let mut best_shared = 0;
        let mut tie = false;
        for original in &self.insertion_order {
            if self.types.get(original) != Some(&EntityType::Person)
                || !is_person_variant(value, original)
            {
                continue;
            }
            if let (Some(gender), Some(other)) = (gender, self.genders.get(original)) {
                if gender != *other {
                    continue;
                }
            }
            let other_tokens = person_tokens(original);
            let shared = other_tokens
                .iter()
                .filter(|token| tokens.contains(token))
                .count();
            if shared > best_shared {
                best_shared = shared;
                best = Some(original);
                tie = false;
            } else if shared == best_shared {
                if let Some(best) = best {
                    if self.forward.get(original) != self.forward.get(best) {
                        tie = true;
                    }
                }
            }
        }
        if tie {
            None
        } else {
            best.map(ToOwned::to_owned)
        }
    }

    /// Restores exact tokens first, then tolerant variants only when their
    /// canonical form identifies one mapping unambiguously.
    pub fn restore(&self, text: &str) -> String {
        let mut result = text.to_owned();
        let mut exact: Vec<_> = self.reverse.iter().collect();
        exact.sort_by_key(|(placeholder, _)| Reverse(placeholder.len()));
        for (placeholder, original) in exact {
            result = result.replace(placeholder, original);
        }
        self.restore_mangled(&result)
    }

    fn restore_mangled(&self, text: &str) -> String {
        let index = self.tolerant_index();
        if index.is_empty() {
            return text.to_owned();
        }
        let candidates = mangled_candidates(text);
        if candidates.is_empty() {
            return text.to_owned();
        }
        let mut output = String::with_capacity(text.len());
        let mut cursor = 0;
        let mut changed = false;
        for (start, end) in candidates {
            let Some(canonical) = canonical_placeholder_form(&text[start..end]) else {
                continue;
            };
            let Some(key) = index.get(&canonical) else {
                continue;
            };
            let Some(original) = self.reverse.get(key) else {
                continue;
            };
            output.push_str(&text[cursor..start]);
            output.push_str(original);
            cursor = end;
            changed = true;
        }
        if !changed {
            return text.to_owned();
        }
        output.push_str(&text[cursor..]);
        output
    }

    fn tolerant_index(&self) -> HashMap<String, String> {
        let mut result = HashMap::new();
        let mut ambiguous = HashSet::new();
        for key in self.reverse.keys() {
            if !token_shaped_key(key) {
                continue;
            }
            let Some(canonical) = canonical_placeholder_form(key) else {
                continue;
            };
            if ambiguous.contains(&canonical) {
                continue;
            }
            if let Some(existing) = result.get(&canonical) {
                if existing != key {
                    result.remove(&canonical);
                    ambiguous.insert(canonical);
                }
            } else {
                result.insert(canonical, key.clone());
            }
        }
        result
    }

    pub fn entries(&self) -> Vec<MappingEntry> {
        self.insertion_order
            .iter()
            .filter_map(|original| {
                Some(MappingEntry {
                    original: original.clone(),
                    placeholder: self.forward.get(original)?.clone(),
                    entity_type: *self.types.get(original).unwrap_or(&EntityType::Other),
                })
            })
            .collect()
    }

    pub fn reverse(&self) -> &HashMap<String, String> {
        &self.reverse
    }

    pub fn import(&mut self, entries: impl IntoIterator<Item = MappingEntry>) {
        for entry in entries {
            let replace_canonical = self
                .reverse
                .get(&entry.placeholder)
                .map(|canonical| utf16_len(&entry.original) > utf16_len(canonical))
                .unwrap_or(true);
            self.insert_forward(&entry.original, &entry.placeholder, entry.entity_type);
            if replace_canonical {
                self.reverse
                    .insert(entry.placeholder.clone(), entry.original.clone());
            }
            if let Some((name, number)) = parse_typed_placeholder(&entry.placeholder) {
                if let Some(entity_type) = EntityType::from_name(name) {
                    let counter = self.counters.entry(entity_type).or_default();
                    *counter = (*counter).max(number);
                }
            }
        }
    }
}

fn utf16_len(value: &str) -> usize {
    value.encode_utf16().count()
}

fn parse_typed_placeholder(value: &str) -> Option<(&str, usize)> {
    let inner = value.strip_prefix('[')?.strip_suffix(']')?;
    let (name, number) = inner.rsplit_once('_')?;
    if name.is_empty()
        || !name.chars().all(|ch| ch == '_' || ch.is_ascii_uppercase())
        || number.is_empty()
        || !number.chars().all(|ch| ch.is_ascii_digit())
    {
        return None;
    }
    Some((name, number.parse().ok()?))
}

fn contains_cjk(value: &str) -> bool {
    value.chars().any(|ch| {
        matches!(
            ch as u32,
            0x3040..=0x30ff | 0x3400..=0x9fff | 0xac00..=0xd7af
        )
    })
}

const HONORIFICS: [&str; 10] = [
    "先生", "女士", "小姐", "同学", "老师", "经理", "总", "医生", "律师", "さん",
];

fn person_tokens(value: &str) -> Vec<String> {
    if contains_cjk(value) {
        let mut core = value.trim();
        if let Some(honorific) = HONORIFICS
            .iter()
            .chain(["様", "씨"].iter())
            .find(|honorific| core.ends_with(**honorific))
        {
            core = core
                .strip_suffix(*honorific)
                .expect("ends_with was checked")
                .trim();
        }
        return if core.is_empty() {
            Vec::new()
        } else {
            vec![core.to_owned()]
        };
    }
    value
        .to_lowercase()
        .split_whitespace()
        .map(|token| {
            token
                .trim_matches(|ch| {
                    matches!(
                        ch,
                        '.' | ',' | ';' | ':' | '!' | '?' | '(' | ')' | '"' | '\''
                    )
                })
                .to_owned()
        })
        .filter(|token| !token.is_empty())
        .collect()
}

fn is_person_variant(left: &str, right: &str) -> bool {
    let left = person_tokens(left);
    let right = person_tokens(right);
    if left.is_empty() || right.is_empty() {
        return false;
    }
    let (small, big) = if left.len() <= right.len() {
        (&left, &right)
    } else {
        (&right, &left)
    };
    small.iter().all(|token| big.contains(token)) && small.iter().any(|token| utf16_len(token) >= 2)
}

fn token_shaped_key(value: &str) -> bool {
    (value.starts_with('[')
        && value.ends_with(']')
        && value[1..value.len() - 1].find(['[', ']', '\n']).is_none())
        || (value.starts_with("<<")
            && value.ends_with(">>")
            && value[2..value.len() - 2].find(['<', '>', '\n']).is_none())
}

fn canonical_placeholder_form(candidate: &str) -> Option<String> {
    let stripped: String = candidate
        .chars()
        .filter(|ch| !matches!(ch, '*' | '`' | '~'))
        .collect();
    let stripped = stripped.trim_matches(|ch: char| !ch.is_alphanumeric());
    if stripped.is_empty() {
        return None;
    }
    let parts: Vec<_> = stripped
        .split(|ch: char| ch.is_whitespace() || ch == '_' || ch == '-')
        .filter(|part| !part.is_empty())
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("_").to_lowercase())
    }
}

fn mangled_candidates(text: &str) -> Vec<(usize, usize)> {
    let mut result = Vec::new();
    let mut cursor = 0;
    while cursor < text.len() {
        if text[cursor..].starts_with('[') {
            if let Some(relative_end) = text[cursor + 1..].find(']') {
                let end = cursor + 1 + relative_end + 1;
                let body = &text[cursor + 1..end - 1];
                if !body.contains(['[', '\n']) {
                    result.push((cursor, end));
                    cursor = end;
                    continue;
                }
            }
        } else if text[cursor..].starts_with("<<") {
            if let Some(relative_end) = text[cursor + 2..].find(">>") {
                let end = cursor + 2 + relative_end + 2;
                let body = &text[cursor + 2..end - 2];
                if !body.contains(['<', '\n']) {
                    result.push((cursor, end));
                    cursor = end;
                    continue;
                }
            }
        }

        let ch = text[cursor..].chars().next().expect("cursor is in bounds");
        if ch.is_ascii_uppercase() && bare_start_boundary(text, cursor) {
            let mut end = cursor + ch.len_utf8();
            while end < text.len() {
                let next = text[end..].chars().next().expect("end is in bounds");
                if next.is_ascii_uppercase() || next.is_ascii_digit() || next == '_' {
                    end += next.len_utf8();
                } else {
                    break;
                }
            }
            let candidate = &text[cursor..end];
            if is_bare_typed_token(candidate) && bare_end_boundary(text, end) {
                result.push((cursor, end));
                cursor = end;
                continue;
            }
        }
        cursor += ch.len_utf8();
    }
    result
}

fn is_word_or_underscore(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

fn bare_start_boundary(text: &str, start: usize) -> bool {
    text[..start]
        .chars()
        .next_back()
        .map(|ch| !is_word_or_underscore(ch) && ch != '[' && ch != '<')
        .unwrap_or(true)
}

fn bare_end_boundary(text: &str, end: usize) -> bool {
    text[end..]
        .chars()
        .next()
        .map(|ch| !is_word_or_underscore(ch))
        .unwrap_or(true)
}

fn is_bare_typed_token(value: &str) -> bool {
    let Some((prefix, digits)) = value.rsplit_once('_') else {
        return false;
    };
    !prefix.is_empty()
        && !digits.is_empty()
        && digits.chars().all(|ch| ch.is_ascii_digit())
        && prefix.split('_').all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reuses_values_and_person_variants() {
        let mut map = PlaceholderMap::new();
        assert_eq!(
            map.placeholder_for("John Smith", EntityType::Person),
            "[PERSON_1]"
        );
        assert_eq!(
            map.placeholder_for("John", EntityType::Person),
            "[PERSON_1]"
        );
        assert_eq!(
            map.placeholder_for("张三先生", EntityType::Person),
            "[PERSON_2]"
        );
        assert_eq!(
            map.placeholder_for("张三", EntityType::Person),
            "[PERSON_2]"
        );
        assert_eq!(map.restore("[PERSON_1]/[PERSON_2]"), "John Smith/张三先生");
    }

    #[test]
    fn restores_unambiguous_mangled_tokens() {
        let mut map = PlaceholderMap::new();
        map.placeholder_for("张三", EntityType::Person);
        map.placeholder_for("13812345678", EntityType::Phone);
        assert_eq!(
            map.restore("**[person 1]** and PERSON_1 and [PHONE-1.]"),
            "**张三** and 张三 and 13812345678"
        );
        assert_eq!(map.restore("[PERSON_9]"), "[PERSON_9]");
    }

    #[test]
    fn imported_counters_continue() {
        let mut map = PlaceholderMap::new();
        map.import([MappingEntry {
            original: "张三".to_owned(),
            placeholder: "[PERSON_4]".to_owned(),
            entity_type: EntityType::Person,
        }]);
        assert_eq!(
            map.placeholder_for("李四", EntityType::Person),
            "[PERSON_5]"
        );
    }
}
