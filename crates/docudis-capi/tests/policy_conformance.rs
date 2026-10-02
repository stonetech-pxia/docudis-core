// Copyright 2026 the Docudis contributors. Licensed under Apache-2.0.

use docudis_capi::{
    docudis_v1_buffer_free, docudis_v1_detect_json, DocudisV1Buffer, DocudisV1Status,
};
use serde_json::{json, Value};
use std::slice;

const KEYS: [&str; 8] = [
    "type",
    "value",
    "start_utf8",
    "end_utf8",
    "confidence",
    "detector",
    "source",
    "enabled",
];

/// Builds the same detect request the Dart binding sends, in UTF-8 offsets.
fn request(case: &Value) -> Value {
    let mut request = json!({
        "schema_version": 1,
        "text": case["text"],
        "regions": [],
        "dictionary": case["dictionary"],
        "detections": case["candidates"].as_array().unwrap().iter().map(|c| json!({
            "type": c["type"], "value": c["value"], "start": c["start_utf8"],
            "end": c["end_utf8"], "confidence": c["confidence"],
            "detector": c["detector"], "source": c["source"], "enabled": c["enabled"],
        })).collect::<Vec<_>>(),
    });
    if let Some(policy) = case.get("policy") {
        let mut sent = json!({});
        if let Some(types) = policy.get("types") {
            sent["types"] = types.clone();
        }
        if let Some(ranges) = policy.get("ranges_utf8") {
            sent["ranges"] = ranges.clone();
        }
        request["policy"] = sent;
    }
    request
}

#[test]
fn detect_json_reproduces_policy_fixture() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../../conformance/fixtures/v1/policy.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let input = serde_json::to_vec(&request(case)).unwrap();
        let mut output = DocudisV1Buffer {
            ptr: std::ptr::null_mut(),
            len: 0,
            capacity: 0,
        };
        let status = unsafe { docudis_v1_detect_json(input.as_ptr(), input.len(), &mut output) };
        if case.get("error").is_some() {
            assert_eq!(status, DocudisV1Status::InvalidArgument, "{name}");
            assert!(output.ptr.is_null(), "{name}");
            continue;
        }
        assert_eq!(status, DocudisV1Status::Ok, "{name}");
        let response: Value =
            serde_json::from_slice(unsafe { slice::from_raw_parts(output.ptr, output.len) })
                .unwrap();
        unsafe { docudis_v1_buffer_free(&mut output) };
        let actual: Vec<Value> = response["detections"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| {
                let mut d = d.clone();
                d["start_utf8"] = d["start"].take();
                d["end_utf8"] = d["end"].take();
                KEYS.iter().map(|k| (k.to_string(), d[k].clone())).collect()
            })
            .collect();
        let expected: Vec<Value> = case["expected"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| KEYS.iter().map(|k| (k.to_string(), d[k].clone())).collect())
            .collect();
        assert_eq!(actual, expected, "{name}");
    }
}
