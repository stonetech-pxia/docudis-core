// Copyright 2026 the Docudis contributors. Licensed under Apache-2.0.

use docudis_core::{
    anonymize, utf16_to_utf8_offset, utf8_to_utf16_offset, Detection, DetectionSource, EntityType,
    MappingEntry,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    schema_version: u32,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    text: String,
    #[serde(default)]
    previous_map: Vec<MappingEntry>,
    detections: Vec<FixtureDetection>,
    expected: Expected,
}

#[derive(Deserialize)]
struct FixtureDetection {
    #[serde(rename = "type")]
    entity_type: EntityType,
    value: String,
    start_utf8: usize,
    end_utf8: usize,
    start_utf16: usize,
    end_utf16: usize,
    confidence: f64,
    detector: String,
    source: DetectionSource,
    enabled: bool,
}

#[derive(Deserialize)]
struct Expected {
    text: String,
    mappings: Vec<MappingEntry>,
    replacements: Vec<FixtureReplacement>,
    restore_cases: Vec<RestoreCase>,
}

#[derive(Deserialize)]
struct FixtureReplacement {
    start_utf8: usize,
    end_utf8: usize,
    start_utf16: usize,
    end_utf16: usize,
    placeholder: String,
}

#[derive(Deserialize)]
struct RestoreCase {
    input: String,
    output: String,
}

#[test]
fn rust_matches_shared_dart_fixtures() {
    let fixture: Fixture = serde_json::from_str(include_str!(
        "../../../conformance/fixtures/v1/anonymization.json"
    ))
    .expect("valid conformance fixture JSON");
    assert_eq!(fixture.schema_version, 1);

    for case in fixture.cases {
        let detections = case
            .detections
            .into_iter()
            .map(|detection| {
                assert_eq!(
                    utf8_to_utf16_offset(&case.text, detection.start_utf8),
                    Ok(detection.start_utf16),
                    "{} start offset",
                    case.name
                );
                assert_eq!(
                    utf8_to_utf16_offset(&case.text, detection.end_utf8),
                    Ok(detection.end_utf16),
                    "{} end offset",
                    case.name
                );
                assert_eq!(
                    utf16_to_utf8_offset(&case.text, detection.start_utf16),
                    Ok(detection.start_utf8),
                    "{} reverse start offset",
                    case.name
                );
                assert_eq!(
                    utf16_to_utf8_offset(&case.text, detection.end_utf16),
                    Ok(detection.end_utf8),
                    "{} reverse end offset",
                    case.name
                );
                assert_eq!(
                    &case.text[detection.start_utf8..detection.end_utf8],
                    detection.value,
                    "{} detection value",
                    case.name
                );
                Detection {
                    entity_type: detection.entity_type,
                    value: detection.value,
                    start: detection.start_utf8,
                    end: detection.end_utf8,
                    confidence: detection.confidence,
                    detector: detection.detector,
                    source: detection.source,
                    enabled: detection.enabled,
                }
            })
            .collect::<Vec<_>>();

        let mut previous = docudis_core::PlaceholderMap::new();
        previous.import(case.previous_map);
        let previous = (!previous.is_empty()).then_some(&previous);
        let actual =
            anonymize(&case.text, &detections, previous).expect("fixture offsets are valid");
        assert_eq!(actual.text, case.expected.text, "{} text", case.name);
        assert_eq!(
            actual.map.entries(),
            case.expected.mappings,
            "{} mappings",
            case.name
        );
        assert_eq!(
            actual.replacements.len(),
            case.expected.replacements.len(),
            "{} replacement count",
            case.name
        );
        for (actual, expected) in actual.replacements.iter().zip(&case.expected.replacements) {
            assert_eq!(
                actual.start, expected.start_utf8,
                "{} replacement start",
                case.name
            );
            assert_eq!(
                actual.end, expected.end_utf8,
                "{} replacement end",
                case.name
            );
            assert_eq!(
                actual.placeholder, expected.placeholder,
                "{} placeholder",
                case.name
            );
            assert_eq!(
                utf8_to_utf16_offset(&case.text, actual.start),
                Ok(expected.start_utf16),
                "{} replacement UTF-16 start",
                case.name
            );
            assert_eq!(
                utf8_to_utf16_offset(&case.text, actual.end),
                Ok(expected.end_utf16),
                "{} replacement UTF-16 end",
                case.name
            );
        }
        for restore in case.expected.restore_cases {
            assert_eq!(
                actual.map.restore(&restore.input),
                restore.output,
                "{} restore {:?}",
                case.name,
                restore.input
            );
        }
    }
}
