// Copyright 2026 the Docudis contributors. Licensed under Apache-2.0.

use crate::{Detection, DetectionSource, DictionaryDetector, EntityType};
use regex::Regex;
use std::{cmp::Reverse, collections::HashSet};

#[derive(Default)]
pub struct NeverHide {
    terms: DictionaryDetector,
}
impl NeverHide {
    pub fn new(terms: impl IntoIterator<Item = String>) -> Self {
        Self {
            terms: DictionaryDetector::new(terms),
        }
    }
    pub fn within(&self, text: &str) -> NeverHideSpans {
        NeverHideSpans {
            spans: self
                .terms
                .detect_sync(text)
                .into_iter()
                .map(|d| (d.start, d.end))
                .collect(),
        }
    }
}
impl Default for DictionaryDetector {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}
pub struct NeverHideSpans {
    spans: Vec<(usize, usize)>,
}
impl NeverHideSpans {
    pub fn covers(&self, d: &Detection) -> bool {
        if matches!(
            d.source,
            DetectionSource::Dictionary | DetectionSource::Manual
        ) {
            return false;
        }
        let Some((start, end)) = trimmed_range(&d.value).map(|(a, b)| (d.start + a, d.start + b))
        else {
            return false;
        };
        self.spans.iter().any(|(a, b)| *a <= start && end <= *b)
    }
}

pub struct DetectionPipeline {
    pub never_hide: NeverHide,
}
impl DetectionPipeline {
    pub fn new(never_hide: impl IntoIterator<Item = String>) -> Self {
        Self {
            never_hide: NeverHide::new(never_hide),
        }
    }
    pub fn process(&self, text: &str, candidates: Vec<Detection>) -> Vec<Detection> {
        if text.trim().is_empty() {
            return Vec::new();
        }
        let readable = self.never_hide.within(text);
        let candidates = candidates
            .into_iter()
            .flat_map(|d| without_title_lines(text, d))
            .collect();
        let fresh = with_defaults(text, candidates)
            .into_iter()
            .filter(|d| not_noise(d) && !readable.covers(d) && !public_product(text, d))
            .collect::<Vec<_>>();
        let kept = resolve_overlaps(&fresh);
        let repaired = repair_spans(text, &kept, &fresh);
        merge(text, &repaired, Some(&readable))
    }
}

pub fn with_defaults(text: &str, candidates: Vec<Detection>) -> Vec<Detection> {
    let births: HashSet<String> = candidates
        .iter()
        .filter(|d| d.entity_type == EntityType::Date && follows_birth_label(text, d.start))
        .map(|d| d.value.trim().to_owned())
        .collect();
    let rows = figure_rows(text);
    candidates
        .into_iter()
        .map(|mut d| {
            if d.entity_type == EntityType::Date && births.contains(d.value.trim()) {
                d.entity_type = EntityType::BirthDate
            } else if matches!(d.entity_type, EntityType::Date | EntityType::Amount)
                || d.entity_type == EntityType::Number
                    && matches!(d.source, DetectionSource::Model | DetectionSource::Rule)
                    && rows.iter().any(|(a, b)| *a <= d.start && d.end <= *b)
            {
                d.enabled = false
            }
            d
        })
        .collect()
}

pub fn follows_birth_label(text: &str, start: usize) -> bool {
    let lo = text[..start]
        .char_indices()
        .rev()
        .take(80)
        .last()
        .map_or(0, |(i, _)| i);
    let before = &text[lo..start];
    let lower = before.to_lowercase();
    let labels = [
        "born",
        "date of birth",
        "birth date",
        "birthdate",
        "birthday",
        "d.o.b.",
        "dob",
        "né(e) le",
        "née le",
        "né le",
        "naissance",
        "nacido",
        "nacida",
        "nacimiento",
        "f. nac.",
    ];
    labels.iter().any(|label| {
        lower.rfind(label).is_some_and(|i| {
            let rest = &lower[i + label.len()..];
            !rest.contains('\n')
                && rest.chars().filter(|c| c.is_ascii_digit()).count() == 0
                && rest.chars().count() <= 20
                || rest.trim_matches([' ', '\t', ':', '\r', '\n']).is_empty() && rest.contains('\n')
        })
    }) || before
        .rfind("NE LE")
        .is_some_and(|i| before[i + 5..].chars().count() <= 20)
}

fn figure_rows(text: &str) -> Vec<(usize, usize)> {
    let measure = Regex::new(r"[+\-−]?[€$£]?\d+(?:[.,]\d+)+\s?%?").unwrap();
    let labels=Regex::new(r"(?i)\b(account|acct|compte|cuenta|iban|bic|swift|phone|tel|mobile|passport|client|customer|policy|member|ref|reference|contract|number|numero|id)\b").unwrap();
    let mut rows = Vec::new();
    let mut start = 0;
    for line in text.split('\n') {
        let end = start + line.len();
        if measure.find_iter(line).count() >= 2 && !labels.is_match(line) {
            rows.push((start, end))
        }
        start = end + 1
    }
    if rows.len() >= 3 {
        rows
    } else {
        Vec::new()
    }
}

pub fn resolve_overlaps(entities: &[Detection]) -> Vec<Detection> {
    fn covered(term: &Detection, by: &[Detection], skip: &HashSet<usize>) -> bool {
        by.iter().enumerate().any(|(i, c)| {
            !skip.contains(&i)
                && c.enabled
                && c.source != DetectionSource::Dictionary
                && c.start <= term.start
                && c.end >= term.end
                && c.len() > term.len()
        })
    }
    let mut yielding: HashSet<usize> = entities
        .iter()
        .enumerate()
        .filter(|(i, e)| {
            e.source == DetectionSource::Dictionary && covered(e, entities, &HashSet::from([*i]))
        })
        .map(|(i, _)| i)
        .collect();
    loop {
        let kept = resolve_inner(
            entities
                .iter()
                .enumerate()
                .filter(|(i, _)| !yielding.contains(i))
                .map(|(_, e)| e.clone())
                .collect(),
        );
        let exposed: Vec<_> = yielding
            .iter()
            .copied()
            .filter(|i| {
                !kept.iter().any(|c| {
                    c.enabled
                        && c.source != DetectionSource::Dictionary
                        && c.start <= entities[*i].start
                        && c.end >= entities[*i].end
                        && c.len() > entities[*i].len()
                })
            })
            .collect();
        if exposed.is_empty() {
            return kept;
        }
        for i in exposed {
            yielding.remove(&i);
        }
    }
}
fn resolve_inner(mut entities: Vec<Detection>) -> Vec<Detection> {
    entities.sort_by(|a, b| {
        (
            Reverse(a.source.priority()),
            Reverse(a.len()),
            Reverse((a.confidence * 1_000_000.) as i64),
            a.start,
        )
            .cmp(&(
                Reverse(b.source.priority()),
                Reverse(b.len()),
                Reverse((b.confidence * 1_000_000.) as i64),
                b.start,
            ))
    });
    let mut kept = Vec::new();
    for e in entities {
        if !kept.iter().any(|k: &Detection| k.overlaps(&e)) {
            kept.push(e)
        }
    }
    kept.sort_by_key(|d| d.start);
    kept
}

pub fn merge(
    text: &str,
    candidates: &[Detection],
    readable: Option<&NeverHideSpans>,
) -> Vec<Detection> {
    let allowed = |d: &Detection| readable.is_none_or(|r| !r.covers(d));
    let kept = resolve_overlaps(
        &candidates
            .iter()
            .filter(|d| not_noise(d) && allowed(d))
            .cloned()
            .collect::<Vec<_>>(),
    );
    let mut all = kept.clone();
    all.extend(propagate(text, &kept).into_iter().filter(allowed));
    resolve_overlaps(&all)
}

pub fn propagate(text: &str, entities: &[Detection]) -> Vec<Detection> {
    let mut seen: HashSet<(usize, usize)> = entities.iter().map(|d| (d.start, d.end)).collect();
    let mut added = HashSet::new();
    let mut terms = Vec::new();
    for e in entities {
        if !e.enabled {
            continue;
        }
        let val = e.value.trim();
        if term_ok(val) && added.insert(val.to_lowercase()) {
            terms.push((val.to_owned(), e.entity_type, false))
        }
        if e.entity_type == EntityType::Person && val.chars().any(char::is_whitespace) {
            for word in val.split_whitespace() {
                let w = word.trim_end_matches(|c: char| ".,;:!?()".contains(c));
                if w.chars().count() >= 4
                    && !starts_lower(w)
                    && !is_title(w)
                    && added.insert(w.to_lowercase())
                {
                    terms.push((w.to_owned(), e.entity_type, true))
                }
            }
        }
    }
    let mut out = Vec::new();
    for (term, kind, part) in terms {
        let re = Regex::new(&format!("(?i:{})", regex::escape(&term))).unwrap();
        for m in re.find_iter(text) {
            if seen.contains(&(m.start(), m.end()))
                || !boundary(text, m.start(), m.end())
                || part && text[m.start()..].chars().next() != term.chars().next()
            {
                continue;
            }
            seen.insert((m.start(), m.end()));
            out.push(Detection {
                entity_type: kind,
                value: text[m.start()..m.end()].to_owned(),
                start: m.start(),
                end: m.end(),
                confidence: 0.95,
                detector: "propagated".into(),
                source: DetectionSource::Propagated,
                enabled: true,
            });
        }
    }
    out
}
fn term_ok(v: &str) -> bool {
    let n = v.chars().count();
    n >= 3 || n == 2 && !v.is_ascii()
}
fn starts_lower(v: &str) -> bool {
    v.chars().next().is_some_and(|c| c.is_lowercase())
}
fn boundary(text: &str, start: usize, end: usize) -> bool {
    if text[start..end].chars().any(
        |c| matches!(c,'\u{3040}'..='\u{30ff}'|'\u{3400}'..='\u{9fff}'|'\u{ac00}'..='\u{d7af}'),
    ) {
        return true;
    }
    text[..start]
        .chars()
        .next_back()
        .is_none_or(|c| !c.is_alphanumeric())
        && text[end..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_alphanumeric())
}

fn not_noise(d: &Detection) -> bool {
    let v = d.value.trim();
    !v.is_empty()
        && (d.source != DetectionSource::Model || {
            let n = v.chars().count();
            n >= 2 && (n >= 3 || !v.is_ascii()) && !is_title(v) && !function_words_only(v)
        })
}
fn normalized(v: &str) -> String {
    v.trim_matches(|c: char| {
        c.is_whitespace()
            || c.is_ascii_punctuation()
            || matches!(c, '、' | '。' | '，' | '；' | '：' | '！' | '？')
    })
    .to_lowercase()
}
fn is_title(v: &str) -> bool {
    const WORDS: &[&str] = &[
        "chair",
        "chairman",
        "ceo",
        "cfo",
        "president",
        "director",
        "manager",
        "secretary",
        "head",
        "mr",
        "mrs",
        "ms",
        "miss",
        "dr",
        "prof",
        "operations",
        "human resources",
        "finance",
        "legal",
        "sales",
        "department",
        "lessor",
        "lessee",
        "landlord",
        "tenant",
        "buyer",
        "seller",
        "employer",
        "employee",
        "client",
        "customer",
        "monsieur",
        "madame",
        "président",
        "directeur",
        "service",
        "siret",
        "siren",
        "bailleur",
        "locataire",
        "señor",
        "señora",
        "presidente",
        "directora",
        "departamento",
        "dni",
        "nif",
        "先生",
        "女士",
        "小姐",
        "公司",
        "集团",
        "部门",
        "法务部",
        "销售部",
        "人事部",
        "财务部",
        "董事会",
        "董事长",
        "总经理",
        "经理",
        "总监",
        "主任",
        "主管",
        "秘书",
        "检测中心",
        "中心",
        "务部",
    ];
    let n = normalized(v);
    WORDS.contains(&n.as_str())
        || [
            "the ", "a ", "an ", "le ", "la ", "les ", "el ", "los ", "las ", "un ", "une ",
            "una ", "l'", "l’",
        ]
        .iter()
        .any(|p| n.strip_prefix(p).is_some_and(|x| WORDS.contains(&x)))
}
fn function_words_only(v: &str) -> bool {
    const W: &[&str] = &[
        "the", "a", "an", "of", "and", "or", "to", "in", "on", "at", "for", "by", "with", "from",
        "as", "le", "la", "les", "un", "une", "des", "du", "de", "et", "ou", "au", "en", "pour",
        "par", "avec", "el", "los", "las", "una", "y", "o", "del", "al", "con", "por", "para",
        "sin",
    ];
    let words: Vec<_> = v
        .split(|c: char| c.is_whitespace() || c.is_ascii_punctuation())
        .filter(|x| !x.is_empty())
        .collect();
    !words.is_empty() && words.iter().all(|x| W.contains(&x.to_lowercase().as_str()))
}
fn without_title_lines(text: &str, d: Detection) -> Vec<Detection> {
    if d.source != DetectionSource::Model || !d.value.contains('\n') {
        return vec![d];
    }
    let mut pieces = Vec::new();
    let mut offset = 0;
    for raw in d.value.split('\n') {
        if let Some((lead, tail)) = trimmed_range(raw) {
            let start = d.start + offset + lead;
            pieces.push(Detection {
                entity_type: d.entity_type,
                value: raw[lead..tail].to_owned(),
                start,
                end: d.start + offset + tail,
                confidence: d.confidence,
                detector: d.detector.clone(),
                source: d.source,
                enabled: d.enabled,
            });
        }
        offset += raw.len() + 1;
    }
    let table = pieces
        .iter()
        .any(|piece| line_at(text, piece.start).contains('\t'));
    let kept: Vec<_> = pieces
        .iter()
        .filter(|piece| not_noise(piece) && !(table && repeated_cell(text, &piece.value)))
        .cloned()
        .collect();
    if !table && kept.len() == pieces.len() {
        vec![d]
    } else {
        kept
    }
}

fn line_at(text: &str, at: usize) -> &str {
    let start = text[..at].rfind('\n').map_or(0, |value| value + 1);
    let end = text[at..].find('\n').map_or(text.len(), |value| at + value);
    &text[start..end]
}

fn repeated_cell(text: &str, value: &str) -> bool {
    text.split(['\t', '\n'])
        .filter(|cell| *cell == value)
        .count()
        >= 3
}

fn public_product(text: &str, d: &Detection) -> bool {
    if !matches!(d.entity_type, EntityType::Company | EntityType::Address)
        || matches!(
            d.source,
            DetectionSource::Dictionary | DetectionSource::Manual
        )
    {
        return false;
    }
    let following =
        Regex::new(r"^[\p{L}\p{N}&.'’-]*(?:[ \u{a0}]+[\p{Lu}\p{N}&][\p{L}\p{N}&.-]*){0,4}")
            .unwrap()
            .find(&text[d.end..])
            .map_or("", |m| m.as_str());
    let v = format!("{}{}", d.value, following).to_lowercase();
    [
        "etf",
        "etfs",
        "etp",
        "etn",
        "ucits",
        "sicav",
        "opcvm",
        "spdr",
        "ishares",
        "xtrackers",
        "lyxor",
        "msci",
        "ftse",
        "stoxx",
        "nasdaq",
        "nikkei",
        "s&p",
        "dow jones",
        "index fund",
        "cac 40",
        "ibex 35",
        "hang seng",
        "russell",
    ]
    .iter()
    .any(|m| {
        v.split(|c: char| !c.is_alphanumeric() && c != '&')
            .any(|w| w == *m)
            || v.contains(m)
    })
}

pub fn repair_spans(text: &str, kept: &[Detection], candidates: &[Detection]) -> Vec<Detection> {
    let mut out = kept.to_vec();
    for i in 0..out.len() {
        if !out[i].enabled {
            continue;
        }
        let kind = repair_kind(out[i].entity_type);
        if kind.is_none() {
            continue;
        }
        let line_start = text[..out[i].start].rfind('\n').map_or(0, |x| x + 1);
        let line_end = text[out[i].end..]
            .find('\n')
            .map_or(text.len(), |x| out[i].end + x);
        let lo = if i == 0 {
            line_start
        } else {
            line_start.max(out[i - 1].end)
        };
        let hi = if i + 1 == out.len() {
            line_end
        } else {
            line_end.min(out[i + 1].start)
        };
        let mut start = out[i].start;
        let mut end = out[i].end;
        let previous = i.checked_sub(1).map(|j| out[j].clone());
        let next = out.get(i + 1).cloned();
        for c in candidates {
            if !c.enabled || repair_kind(c.entity_type) != kind || c.start >= end || c.end <= start
            {
                continue;
            }
            let mut candidate_start = start.min(c.start).max(lo);
            let mut candidate_end = end.max(c.end).min(hi);
            if c.end > hi
                && next.as_ref().is_some_and(|other| {
                    hi == other.start
                        && joins_repair(kind, &text[end..other.start], &text[start..other.end])
                })
            {
                candidate_end = end;
            }
            if c.start < lo
                && previous.as_ref().is_some_and(|other| {
                    lo == other.end
                        && joins_repair(kind, &text[other.end..start], &text[other.start..end])
                })
            {
                candidate_start = start;
            }
            trim_separators(text, start, end, &mut candidate_start, &mut candidate_end);
            let grown = format!(
                "{}{}",
                &text[candidate_start..start],
                &text[end..candidate_end]
            );
            if grown.is_empty() || !allowed_repair_text(kind.unwrap(), &grown) {
                continue;
            }
            if c.len() > out[i].len() && out[i].source != DetectionSource::ValidatedRule {
                out[i].entity_type = c.entity_type;
            }
            start = candidate_start;
            end = candidate_end;
        }
        match kind.unwrap() {
            0 | 1 => {
                let own = &text[start..end];
                if own.chars().any(char::is_alphabetic) && own == own.to_uppercase() {
                    start = start.saturating_sub(particle_before_length(&text[lo..start]));
                    end += capital_tail_length(&text[end..hi]);
                }
                if kind == Some(1) {
                    end += company_tail_length(&text[end..hi]);
                }
            }
            3 => {
                let left = if text[start..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_digit())
                {
                    digit_groups_before(&text[lo..start])
                } else {
                    0
                };
                let mut right = if text[..end]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_ascii_digit())
                {
                    digit_groups_after(&text[end..hi])
                } else {
                    0
                };
                if more_number(&text[end + right..hi]) {
                    right = 0;
                }
                if left > 0 || right > 0 {
                    out[i].entity_type = EntityType::Number;
                }
                start -= left;
                end += right;
            }
            2 => start = start.saturating_sub(unit_before_length(&text[lo..start])),
            _ => {}
        }
        if start != out[i].start || end != out[i].end {
            out[i].start = start;
            out[i].end = end;
            out[i].value = text[start..end].to_owned()
        }
    }
    let mut i = 0;
    while i + 1 < out.len() {
        let same = out[i].enabled
            && out[i + 1].enabled
            && repair_kind(out[i].entity_type) == repair_kind(out[i + 1].entity_type);
        let gap = &text[out[i].end..out[i + 1].start];
        let join = same
            && joins_repair(
                repair_kind(out[i].entity_type),
                gap,
                &text[out[i].start..out[i + 1].end],
            );
        if join {
            let end = out[i + 1].end;
            out[i].end = end;
            out[i].value = text[out[i].start..end].to_owned();
            out.remove(i + 1);
        } else {
            i += 1
        }
    }

    extend_address_lines(text, &mut out);
    let block_lines = address_block_lines(text, &out);
    out.extend(block_lines);
    out
}

fn trim_separators(
    text: &str,
    base_start: usize,
    base_end: usize,
    start: &mut usize,
    end: &mut usize,
) {
    while *start < base_start
        && text[*start..]
            .chars()
            .next()
            .is_some_and(|c| c.is_whitespace() || matches!(c, ',' | ';'))
    {
        *start += text[*start..].chars().next().unwrap().len_utf8()
    }
    while *end > base_end
        && text[..*end]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_whitespace() || matches!(c, ',' | ';'))
    {
        *end -= text[..*end].chars().next_back().unwrap().len_utf8()
    }
}
fn allowed_repair_text(kind: u8, value: &str) -> bool {
    if value.chars().count() > 40 {
        return false;
    }
    match kind {
        0 | 1 => value.chars().all(|c| {
            c.is_alphabetic() || matches!(c, ' ' | '.' | '\'' | '’' | '&' | '(' | ')' | '-')
        }),
        2 => value.chars().all(|c| {
            c.is_alphanumeric()
                || matches!(
                    c,
                    ' ' | '.' | ',' | '\'' | '’' | 'º' | 'ª' | '°' | '/' | '(' | ')' | '-'
                )
        }),
        3 => value
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, ' ' | '.' | '/' | '-')),
        _ => false,
    }
}
fn particle_before_length(before: &str) -> usize {
    const P: &[&str] = &[
        "DE LAS ", "DE LOS ", "DE LA ", "DEL ", "DE ", "DU ", "DES ", "VAN DER ", "VAN DEN ",
        "VAN ", "VON ", "DA ", "DOS ", "DO ", "DI ",
    ];
    P.iter()
        .find(|p| before.ends_with(**p))
        .map_or(0, |p| p.len())
}
fn capital_tail_length(after: &str) -> usize {
    let mut used = 0;
    for _ in 0..3 {
        let rest = &after[used..];
        if !rest.starts_with(' ') {
            break;
        }
        let word_len = rest[1..]
            .char_indices()
            .take_while(|(_, c)| c.is_uppercase() || matches!(c, '\'' | '’' | '-'))
            .map(|(at, c)| at + c.len_utf8())
            .last()
            .unwrap_or(0);
        let word = &rest[1..1 + word_len];
        if word.chars().count() < 2
            || !word.chars().next().is_some_and(char::is_uppercase)
            || is_title(word)
            || rest[1 + word_len..]
                .chars()
                .next()
                .is_some_and(char::is_alphanumeric)
        {
            break;
        }
        used += 1 + word_len;
    }
    used
}
fn company_tail_length(after: &str) -> usize {
    let expression = Regex::new(
        r"^(?: (?:of|de|del|da|du|des|la|las|los|le|les|y|e|et|and|the|für|van))?(?: \p{Lu}[\p{L}\p{N}'’&.\-]*){1,3}",
    )
    .unwrap();
    let Some(value) = expression.find(after).map(|m| m.as_str()) else {
        let lower = after.to_lowercase();
        return [
            "of", "de", "del", "da", "du", "des", "la", "las", "los", "le", "les", "y", "e", "et",
            "and", "the", "für", "van",
        ]
        .iter()
        .find_map(|particle| (lower == format!(" {particle} ")).then_some(1 + particle.len()))
        .unwrap_or(0);
    };
    let mut used = 0;
    for word in value.trim_start().split(' ') {
        if word.is_empty() {
            continue;
        }
        if is_title(word) {
            break;
        }
        used += word.len() + 1;
    }
    used
}
fn digit_groups_after(after: &str) -> usize {
    let b = after.as_bytes();
    let mut i = 0;
    loop {
        let mark = i;
        if i < b.len() && matches!(b[i], b' ' | b'.' | b'-') {
            i += 1
        } else {
            break;
        }
        let begin = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1
        }
        if i - begin < 2 {
            i = mark;
            break;
        }
    }
    i
}
fn digit_groups_before(before: &str) -> usize {
    let b = before.as_bytes();
    let mut i = b.len();
    let end = i;
    let mut groups = 0;
    while i > 0 {
        let digits_end = i;
        while i > 0 && b[i - 1].is_ascii_digit() {
            i -= 1
        }
        if digits_end - i < 2 {
            break;
        }
        groups += 1;
        if i == 0 || !matches!(b[i - 1], b' ' | b'.' | b'-') {
            break;
        }
        i -= 1;
    }
    if groups > 0
        && before[..i]
            .chars()
            .next_back()
            .is_some_and(|c| matches!(c, '/' | ':' | '.' | ',' | '-') || c.is_ascii_digit())
    {
        0
    } else {
        end - i
    }
}
fn more_number(rest: &str) -> bool {
    let mut chars = rest.chars();
    match chars.next() {
        Some(c) if c.is_ascii_digit() => true,
        Some(' ' | '.' | ',' | '/' | ':' | '-') => chars.next().is_some_and(|c| c.is_ascii_digit()),
        _ => false,
    }
}

fn joins_repair(kind: Option<u8>, gap: &str, whole: &str) -> bool {
    match kind {
        Some(0) | Some(1) => {
            gap == " "
                || [
                    " de ",
                    " del ",
                    " de la ",
                    " de los ",
                    " de las ",
                    " du ",
                    " des ",
                    " van ",
                    " van der ",
                    " van den ",
                    " von ",
                    " da ",
                    " do ",
                    " dos ",
                    " di ",
                ]
                .contains(&gap.to_lowercase().as_str())
        }
        Some(2) => {
            gap.len() <= 30
                && !gap.contains('\n')
                && gap.chars().any(char::is_alphanumeric)
                && !contains_prose_word(gap)
                && whole.chars().any(|c| c.is_ascii_digit())
        }
        _ => false,
    }
}

fn contains_prose_word(value: &str) -> bool {
    const ALLOWED: &[&str] = &[
        "de", "del", "la", "las", "los", "el", "du", "des", "le", "les", "bis", "ter", "sur",
        "sous", "en", "of", "the", "upon", "on", "cedex",
    ];
    value
        .split(|c: char| !c.is_alphanumeric() && c != '\'' && c != '’')
        .filter(|word| !word.is_empty())
        .any(|word| {
            let lower = word.to_lowercase();
            !ALLOWED.contains(&lower.as_str())
                && word.chars().take(2).count() == 2
                && word.chars().take(2).all(char::is_lowercase)
        })
}

const ADDRESS_PARTICLES: &[&str] = &[
    "de la", "upon", "cedex", "del", "las", "los", "des", "les", "bis", "ter", "s/n", "de", "la",
    "el", "du", "le", "na", "an", "of", "the", "on", "c/",
];

fn address_word(value: &str) -> Option<(usize, bool)> {
    for particle in ADDRESS_PARTICLES {
        if value.starts_with(particle)
            && value[particle.len()..]
                .chars()
                .next()
                .is_none_or(|c| matches!(c, ' ' | ','))
        {
            return Some((particle.len(), true));
        }
    }
    let mut chars = value.char_indices();
    let (_, first) = chars.next()?;
    let valid = if first.is_uppercase() {
        |c: char| c.is_alphabetic() || matches!(c, '\'' | '’' | '.' | '-')
    } else if first.is_ascii_digit() {
        |c: char| c.is_alphanumeric() || matches!(c, 'º' | '°' | 'ª' | '.' | '-')
    } else {
        return None;
    };
    let mut end = first.len_utf8();
    for (at, c) in chars {
        if !valid(c) {
            break;
        }
        end = at + c.len_utf8();
    }
    Some((end, false))
}

fn address_separator(value: &str, at: usize) -> usize {
    value[at..]
        .bytes()
        .take(2)
        .take_while(|b| matches!(b, b' ' | b','))
        .count()
}

fn address_run_after_length(value: &str) -> usize {
    let mut at = 0;
    let mut words = Vec::new();
    for _ in 0..8 {
        let separator = address_separator(value, at);
        if separator == 0 {
            break;
        }
        let word_start = at + separator;
        let Some((length, particle)) = address_word(&value[word_start..]) else {
            break;
        };
        at = word_start + length;
        words.push((at, particle));
    }
    words
        .iter()
        .rfind(|(_, particle)| !particle)
        .map_or(0, |(end, _)| *end)
}

fn address_run_before_length(value: &str) -> usize {
    for (start, _) in value.char_indices() {
        let suffix = &value[start..];
        let mut at = 0;
        let mut words = Vec::new();
        for _ in 0..8 {
            let word_start = at;
            let Some((length, particle)) = address_word(&suffix[word_start..]) else {
                break;
            };
            at += length;
            let separator = address_separator(suffix, at);
            if separator == 0 {
                break;
            }
            words.push((word_start, particle));
            at += separator;
            if at == suffix.len() {
                if let Some((first, _)) = words.iter().find(|(_, particle)| !particle) {
                    return suffix.len() - *first;
                }
                break;
            }
        }
    }
    0
}

fn extend_address_lines(text: &str, spans: &mut [Detection]) {
    for i in 0..spans.len() {
        if !spans[i].enabled || repair_kind(spans[i].entity_type) != Some(2) {
            continue;
        }
        let line_start = text[..spans[i].start].rfind('\n').map_or(0, |x| x + 1);
        let line_end = text[spans[i].end..]
            .find('\n')
            .map_or(text.len(), |x| spans[i].end + x);
        let lo = if i == 0 {
            line_start
        } else {
            line_start.max(spans[i - 1].end)
        };
        let hi = if i + 1 == spans.len() {
            line_end
        } else {
            line_end.min(spans[i + 1].start)
        };
        if lo > spans[i].start || hi < spans[i].end {
            continue;
        }
        let left = address_run_before_length(&text[lo..spans[i].start]);
        let right = address_run_after_length(&text[spans[i].end..hi]);
        if left > 0 || right > 0 {
            spans[i].start -= left;
            spans[i].end += right;
            spans[i].value = text[spans[i].start..spans[i].end].to_owned();
        }
    }
}

fn line_ranges(text: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut start = 0;
    loop {
        let end = text[start..].find('\n').map_or(text.len(), |at| start + at);
        ranges.push((start, end));
        if end == text.len() {
            return ranges;
        }
        start = end + 1;
    }
}

fn exact_address_line(value: &str) -> bool {
    let value = value.trim();
    if !(2..=40).contains(&value.chars().count()) || !value.chars().any(char::is_alphabetic) {
        return false;
    }
    if value
        .split([' ', ','])
        .filter(|word| !word.is_empty())
        .any(is_title)
    {
        return false;
    }
    let mut at = 0;
    loop {
        let Some((length, _)) = address_word(&value[at..]) else {
            return false;
        };
        at += length;
        if at == value.len() {
            return true;
        }
        let separator = address_separator(value, at);
        if separator == 0 {
            return false;
        }
        at += separator;
        if at == value.len() {
            return false;
        }
    }
}

fn address_block_lines(text: &str, spans: &[Detection]) -> Vec<Detection> {
    let lines = line_ranges(text);
    let address_line = |index: usize| -> bool {
        let Some(&(start, end)) = lines.get(index) else {
            return false;
        };
        let mut ink = 0;
        let mut hidden = 0;
        for (offset, c) in text[start..end].char_indices() {
            if matches!(c, ' ' | '\t') {
                continue;
            }
            ink += 1;
            let at = start + offset;
            if spans.iter().any(|d| {
                d.enabled && d.entity_type == EntityType::Address && d.start <= at && at < d.end
            }) {
                hidden += 1;
            }
        }
        ink > 0 && hidden * 5 >= ink * 3
    };
    let untouched = |index: usize| -> bool {
        let (start, end) = lines[index];
        !spans.iter().any(|d| d.start < end && d.end > start)
    };

    let mut added = Vec::new();
    let mut i = 1;
    while i < lines.len() {
        if !address_line(i - 1) || !untouched(i) {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < lines.len()
            && j - i < 2
            && untouched(j)
            && exact_address_line(&text[lines[j].0..lines[j].1])
        {
            j += 1;
        }
        if j == i || !address_line(j) {
            i += 1;
            continue;
        }
        for &(line_start, line_end) in &lines[i..j] {
            let line = &text[line_start..line_end];
            let value = line.trim();
            let start = line_start + line.len() - line.trim_start().len();
            let end = line_end - (line.len() - line.trim_end().len());
            added.push(Detection {
                entity_type: EntityType::Address,
                value: value.to_owned(),
                start,
                end,
                confidence: 0.7,
                detector: "repair:address_line".into(),
                source: DetectionSource::Rule,
                enabled: true,
            });
        }
        i = j + 1;
    }
    added
}

fn unit_before_length(before: &str) -> usize {
    let expression = Regex::new(
        r"(?i)(?:(?:(?:Appartement|Appt?\.?|Apt\.?|Bâtiment|Bât\.?|Étage|Unit|Suite|Flat|Floor|Room|Piso|Planta|Puerta|Local) [\p{L}\p{N}.º\-]{1,6},? )+(?:\d{1,4}[A-Za-z]?(?: bis| ter)?,? )?|\d{1,4}[A-Za-z]?(?: bis| ter)?,? )$",
    )
    .unwrap();
    expression
        .find(before)
        .filter(|m| {
            before[..m.start()]
                .chars()
                .next_back()
                .is_none_or(|c| !c.is_alphanumeric())
        })
        .map_or(0, |m| m.len())
}
fn repair_kind(t: EntityType) -> Option<u8> {
    match t {
        EntityType::Person => Some(0),
        EntityType::Company => Some(1),
        EntityType::Address => Some(2),
        EntityType::Phone
        | EntityType::Id
        | EntityType::Number
        | EntityType::Card
        | EntityType::Other => Some(3),
        _ => None,
    }
}
fn trimmed_range(value: &str) -> Option<(usize, usize)> {
    let start = value.find(|c: char| !c.is_whitespace() && !c.is_ascii_punctuation())?;
    let end = value
        .rfind(|c: char| !c.is_whitespace() && !c.is_ascii_punctuation())
        .map(|i| i + value[i..].chars().next().unwrap().len_utf8())?;
    Some((start, end))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn d(value: &str, start: usize, source: DetectionSource) -> Detection {
        Detection {
            entity_type: EntityType::Person,
            value: value.into(),
            start,
            end: start + value.len(),
            confidence: 0.9,
            detector: "test".into(),
            source,
            enabled: true,
        }
    }
    #[test]
    fn priority_and_propagation() {
        let r = resolve_overlaps(&[
            d("Alice", 0, DetectionSource::Model),
            d("Ali", 0, DetectionSource::Dictionary),
        ]);
        assert_eq!(r[0].value, "Alice");
        let p = propagate("Alice met Alice", &r);
        assert_eq!(p[0].start, 10)
    }
}
