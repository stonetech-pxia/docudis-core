// Copyright 2026 the Docudis contributors. Licensed under Apache-2.0.

use crate::Detection;

/// Extension boundary for rule, dictionary, or model-backed detection.
///
/// Pure core owns the `Detection` contract but not any concrete inference
/// runtime. A future `docudis-ort` adapter can implement this trait without
/// adding ONNX Runtime to `docudis-core`.
pub trait Detector {
    type Error;

    fn detect(&self, text: &str) -> Result<Vec<Detection>, Self::Error>;
}
