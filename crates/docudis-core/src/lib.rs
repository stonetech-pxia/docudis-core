// Copyright 2026 the Docudis contributors.
//
// Licensed under the Apache License, Version 2.0. This file is a Rust
// migration of portions of DocCloak.Core-derived Dart code. See NOTICE.

//! Platform-independent Docudis anonymization primitives.
//!
//! All public offsets are byte offsets into valid UTF-8. Language bindings
//! whose native strings use another indexing scheme must convert at their
//! boundary.

mod anonymizer;
mod detection;
mod detector;
mod entity_type;
mod list_detectors;
mod offsets;
mod pipeline;
mod placeholder_map;
mod regex_detector;
mod rules;
mod validators;

pub use anonymizer::{anonymize, AnonymizeError, AnonymizedText, Replacement};
pub use detection::{Detection, DetectionSource};
pub use detector::Detector;
pub use entity_type::EntityType;
pub use list_detectors::{BundledListDetector, DictionaryDetector};
pub use offsets::{utf16_to_utf8_offset, utf8_to_utf16_offset, OffsetError};
pub use pipeline::{
    follows_birth_label, merge, propagate, repair_spans, resolve_overlaps, with_defaults,
    DetectionPipeline, NeverHide, NeverHideSpans,
};
pub use placeholder_map::{MappingEntry, PersonGender, PlaceholderMap};
pub use regex_detector::{RegexDetectError, RegexDetector, STRONG_CONFIDENCE};
pub use rules::{
    bundled_rules, parse_rule_pack, regions_for_languages, RegexRule, RuleApplicability,
    RuleCategory, RuleClassification, RuleDefaultAction, RuleError, RulePackScope,
    RuleProtectionLevel, RuleProvenance, RuleSelection, RuleStatus, RuleVertical,
    BUNDLED_RULE_PACKS,
};
pub use validators::RuleValidator;
