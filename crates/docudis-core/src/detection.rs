// Copyright 2026 the Docudis contributors. Licensed under Apache-2.0.

use crate::EntityType;
use serde::{Deserialize, Serialize};

/// Where a detection came from. The later overlap-resolution slice will use
/// the same priorities as the Dart reference implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DetectionSource {
    Dictionary,
    ValidatedRule,
    StrongRule,
    Model,
    BundledList,
    Rule,
    Propagated,
    Manual,
}

impl DetectionSource {
    pub const fn priority(self) -> u8 {
        match self {
            Self::Dictionary => 40,
            Self::ValidatedRule => 30,
            Self::StrongRule => 25,
            Self::Model | Self::BundledList => 20,
            Self::Rule => 10,
            Self::Propagated => 5,
            Self::Manual => 50,
        }
    }
}

/// A sensitive span in a UTF-8 string.
///
/// `start` and `end` are half-open UTF-8 byte offsets. `value` is retained
/// for detector diagnostics; anonymization uses the authoritative slice of
/// the input text, matching the Dart reference behavior.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Detection {
    #[serde(rename = "type")]
    pub entity_type: EntityType,
    pub value: String,
    pub start: usize,
    pub end: usize,
    pub confidence: f64,
    pub detector: String,
    pub source: DetectionSource,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

const fn default_enabled() -> bool {
    true
}

impl Detection {
    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    pub fn is_empty(&self) -> bool {
        self.start >= self.end
    }

    pub fn overlaps(&self, other: &Self) -> bool {
        self.start < other.end && self.end > other.start
    }
}
