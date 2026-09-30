// Copyright 2026 the Docudis contributors. Licensed under Apache-2.0.

use docudis_core::{parse_rule_pack, RegexDetector, BUNDLED_RULE_PACKS};
use serde::Deserialize;

#[test]
fn every_bundled_rule_compiles_and_matches_its_examples() {
    for (region, source) in BUNDLED_RULE_PACKS {
        let rules = parse_rule_pack(source).unwrap_or_else(|error| panic!("{region}: {error}"));
        for rule in rules {
            let detector = RegexDetector::new(vec![rule]);
            for example in &detector.rules[0].examples {
                let found = detector
                    .detect_sync(example)
                    .unwrap_or_else(|error| panic!("{region} {example:?}: {error}"));
                assert!(
                    !found.is_empty(),
                    "{} did not detect its example {example:?}; got {found:?}",
                    detector.rules[0].id
                );
            }
        }
    }
}

#[derive(Deserialize)]
struct Fixture {
    cases: Vec<FixtureCase>,
}
#[derive(Deserialize)]
struct FixtureCase {
    name: String,
    rule_id: String,
    text: String,
    expected: Vec<Expected>,
}
#[derive(Debug, Deserialize, PartialEq)]
struct Expected {
    #[serde(rename = "type")]
    entity_type: String,
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
fn rust_reproduces_shared_dart_rule_fixture() {
    let fixture: Fixture =
        serde_json::from_str(include_str!("../../../conformance/fixtures/v1/rules.json")).unwrap();
    let rules = BUNDLED_RULE_PACKS
        .iter()
        .flat_map(|(_, source)| parse_rule_pack(source).unwrap())
        .collect();
    let detector = RegexDetector::new(rules);
    for case in fixture.cases {
        let actual = detector
            .detect_sync(&case.text)
            .unwrap()
            .into_iter()
            .filter(|d| d.detector == case.rule_id)
            .map(|d| Expected {
                entity_type: d.entity_type.placeholder_name().into(),
                value: d.value,
                start_utf8: d.start,
                end_utf8: d.end,
                start_utf16: docudis_core::utf8_to_utf16_offset(&case.text, d.start).unwrap(),
                end_utf16: docudis_core::utf8_to_utf16_offset(&case.text, d.end).unwrap(),
                confidence: d.confidence,
                detector: d.detector,
                source: serde_json::to_value(d.source)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .into(),
                enabled: d.enabled,
            })
            .collect::<Vec<_>>();
        assert_eq!(actual, case.expected, "{}", case.name);
    }
}
