// Copyright 2026 the Docudis contributors. Licensed under Apache-2.0.

//! The review and restore helpers against fixtures generated from the Dart
//! reference engine (chunks, merge, regions, reply checks).

use docudis_core::{
    chunk_text, merge, regions_for_languages, utf8_to_utf16_offset, Detection, EntityType,
    ReplyCandidate, ReplyMatcher,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture<T> {
    cases: Vec<T>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct Range {
    start_utf8: usize,
    end_utf8: usize,
    start_utf16: usize,
    end_utf16: usize,
}

impl Range {
    fn of(text: &str, start: usize, end: usize) -> Self {
        Self {
            start_utf8: start,
            end_utf8: end,
            start_utf16: utf8_to_utf16_offset(text, start).unwrap(),
            end_utf16: utf8_to_utf16_offset(text, end).unwrap(),
        }
    }
}

#[derive(Deserialize)]
struct ChunkCase {
    name: String,
    text: String,
    taken: Vec<Range>,
    chunks: Vec<Range>,
}

#[test]
fn chunks_match_dart_reference() {
    let fixture: Fixture<ChunkCase> =
        serde_json::from_str(include_str!("../../../conformance/fixtures/v1/chunks.json")).unwrap();
    for case in fixture.cases {
        let taken: Vec<_> = case
            .taken
            .iter()
            .map(|r| (r.start_utf8, r.end_utf8))
            .collect();
        let actual: Vec<_> = chunk_text(&case.text, &taken)
            .into_iter()
            .map(|c| Range::of(&case.text, c.start, c.end))
            .collect();
        assert_eq!(actual, case.chunks, "{}", case.name);
    }
}

#[derive(Debug, Deserialize, PartialEq)]
struct Span {
    #[serde(rename = "type")]
    kind: String,
    value: String,
    start_utf8: usize,
    end_utf8: usize,
    start_utf16: usize,
    end_utf16: usize,
    confidence: f64,
    detector: String,
    source: String,
    enabled: bool,
}

#[derive(Deserialize)]
struct MergeCase {
    name: String,
    text: String,
    detections: Vec<Span>,
    merged: Vec<Span>,
}

#[test]
fn merge_matches_dart_reference() {
    let fixture: Fixture<MergeCase> =
        serde_json::from_str(include_str!("../../../conformance/fixtures/v1/merge.json")).unwrap();
    for case in fixture.cases {
        let detections: Vec<_> = case
            .detections
            .iter()
            .map(|s| Detection {
                entity_type: EntityType::from_name(&s.kind).unwrap(),
                value: s.value.clone(),
                start: s.start_utf8,
                end: s.end_utf8,
                confidence: s.confidence,
                detector: s.detector.clone(),
                source: serde_json::from_value(serde_json::Value::String(s.source.clone()))
                    .unwrap(),
                enabled: s.enabled,
            })
            .collect();
        let actual: Vec<_> = merge(&case.text, &detections, None)
            .into_iter()
            .map(|d| Span {
                kind: d.entity_type.placeholder_name().into(),
                start_utf16: utf8_to_utf16_offset(&case.text, d.start).unwrap(),
                end_utf16: utf8_to_utf16_offset(&case.text, d.end).unwrap(),
                value: d.value,
                start_utf8: d.start,
                end_utf8: d.end,
                confidence: d.confidence,
                detector: d.detector,
                source: serde_json::to_value(d.source)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_owned(),
                enabled: d.enabled,
            })
            .collect();
        assert_eq!(actual, case.merged, "{}", case.name);
    }
}

#[derive(Deserialize)]
struct RegionCase {
    name: String,
    languages: Vec<String>,
    text: String,
    regions: Vec<String>,
}

#[test]
fn regions_match_dart_reference() {
    let fixture: Fixture<RegionCase> = serde_json::from_str(include_str!(
        "../../../conformance/fixtures/v1/regions.json"
    ))
    .unwrap();
    for case in fixture.cases {
        let mut actual: Vec<_> =
            regions_for_languages(case.languages.iter().map(String::as_str), &case.text)
                .into_iter()
                .collect();
        actual.sort();
        assert_eq!(actual, case.regions, "{}", case.name);
    }
}

#[derive(Deserialize)]
struct ReplyFixture {
    candidates: Vec<ReplyDocument>,
    cases: Vec<ReplyCase>,
}

#[derive(Deserialize)]
struct ReplyDocument {
    id: String,
    text: String,
    placeholders: Vec<String>,
}

#[derive(Deserialize)]
struct ReplyCase {
    name: String,
    id: String,
    reply: String,
    unknown: Vec<String>,
    invented: Vec<String>,
    better_match: Option<String>,
}

#[test]
fn reply_checks_match_dart_reference() {
    let fixture: ReplyFixture = serde_json::from_str(include_str!(
        "../../../conformance/fixtures/v1/reply_check.json"
    ))
    .unwrap();
    let matcher = ReplyMatcher::new(fixture.candidates.iter().map(|d| {
        (
            d.id.clone(),
            ReplyCandidate::new(&d.text, d.placeholders.iter().map(String::as_str)),
        )
    }));
    for case in fixture.cases {
        let check = matcher.check(&case.reply, &case.id);
        assert_eq!(check.unknown, case.unknown, "{}", case.name);
        assert_eq!(check.invented, case.invented, "{}", case.name);
        assert_eq!(check.better_match, case.better_match, "{}", case.name);
    }
}
