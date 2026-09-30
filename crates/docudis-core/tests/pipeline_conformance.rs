// Copyright 2026 the Docudis contributors. Licensed under Apache-2.0.

use docudis_core::{
    utf8_to_utf16_offset, Detection, DetectionPipeline, DetectionSource, EntityType,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    text: String,
    never_hide: Vec<String>,
    candidates: Vec<Span>,
    expected: Vec<Span>,
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

#[test]
fn rust_pipeline_reproduces_shared_dart_fixture() {
    let fixture: Fixture = serde_json::from_str(include_str!(
        "../../../conformance/fixtures/v1/pipeline.json"
    ))
    .unwrap();
    for case in fixture.cases {
        let candidates = case
            .candidates
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
        let actual = DetectionPipeline::new(case.never_hide)
            .process(&case.text, candidates)
            .into_iter()
            .map(|d| Span {
                kind: d.entity_type.placeholder_name().into(),
                value: d.value,
                start_utf8: d.start,
                end_utf8: d.end,
                start_utf16: utf8_to_utf16_offset(&case.text, d.start).unwrap(),
                end_utf16: utf8_to_utf16_offset(&case.text, d.end).unwrap(),
                confidence: d.confidence,
                detector: d.detector,
                source: source_name(d.source),
                enabled: d.enabled,
            })
            .collect::<Vec<_>>();
        assert_eq!(actual, case.expected, "{}", case.name)
    }
}

fn source_name(source: DetectionSource) -> String {
    serde_json::to_value(source)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}
