// Copyright 2026 the Docudis contributors. Licensed under Apache-2.0.

use docudis_core::{
    anonymize, chunk_text, merge, regions_for_languages, BundledListDetector, Detection,
    DetectionPipeline, DictionaryDetector, MappingEntry, PlaceholderMap, RegexDetector,
    Replacement, ReplyCandidate, ReplyCheck, ReplyMatcher, RuleSelection, TextChunk,
};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::HashSet,
    ffi::{c_char, CString},
    panic::{catch_unwind, AssertUnwindSafe},
    ptr, slice,
};

pub const ABI_VERSION: u32 = 1;
const VERSION: &[u8] = concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes();

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocudisV1Status {
    Ok = 0,
    InvalidArgument = 1,
    InvalidUtf8 = 2,
    InvalidJson = 3,
    CoreError = 4,
    Panic = 255,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DocudisV1Buffer {
    pub ptr: *mut u8,
    pub len: usize,
    pub capacity: usize,
}

impl DocudisV1Buffer {
    const EMPTY: Self = Self {
        ptr: ptr::null_mut(),
        len: 0,
        capacity: 0,
    };

    fn from_vec(mut bytes: Vec<u8>) -> Self {
        let buffer = Self {
            ptr: bytes.as_mut_ptr(),
            len: bytes.len(),
            capacity: bytes.capacity(),
        };
        std::mem::forget(bytes);
        buffer
    }
}

#[derive(Debug, Deserialize)]
struct AnonymizeRequest {
    schema_version: u32,
    text: String,
    #[serde(default)]
    detections: Vec<Detection>,
    #[serde(default)]
    previous_map: Vec<MappingEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AnonymizeResponse {
    schema_version: u32,
    text: String,
    mappings: Vec<MappingEntry>,
    replacements: Vec<Replacement>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ProcessResponse {
    schema_version: u32,
    text: String,
    detections: Vec<Detection>,
    mappings: Vec<MappingEntry>,
    replacements: Vec<Replacement>,
}

#[derive(Debug, Deserialize)]
struct DetectRequest {
    schema_version: u32,
    text: String,
    #[serde(default)]
    regions: Option<HashSet<String>>,
    #[serde(default)]
    selection: Option<RuleSelection>,
    #[serde(default)]
    dictionary: Vec<String>,
    #[serde(default)]
    never_hide: Vec<String>,
    #[serde(default)]
    include_bundled_lists: bool,
    #[serde(default)]
    detections: Vec<Detection>,
    #[serde(default)]
    previous_map: Vec<MappingEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct DetectResponse {
    schema_version: u32,
    detections: Vec<Detection>,
}

#[derive(Debug, Deserialize)]
struct RestoreRequest {
    schema_version: u32,
    text: String,
    #[serde(default)]
    mappings: Vec<MappingEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RestoreResponse {
    schema_version: u32,
    text: String,
}

#[derive(Debug, Deserialize)]
struct ChunkRequest {
    schema_version: u32,
    text: String,
    #[serde(default)]
    taken: Vec<TextChunk>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ChunkResponse {
    schema_version: u32,
    chunks: Vec<TextChunk>,
}

#[derive(Debug, Deserialize)]
struct MergeRequest {
    schema_version: u32,
    text: String,
    #[serde(default)]
    detections: Vec<Detection>,
}

#[derive(Debug, Deserialize)]
struct RegionsRequest {
    schema_version: u32,
    #[serde(default)]
    languages: Vec<String>,
    #[serde(default)]
    text: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct RegionsResponse {
    schema_version: u32,
    regions: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct LanguagesRequest {
    schema_version: u32,
    text: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct LanguagesResponse {
    schema_version: u32,
    languages: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ReplyCheckRequest {
    schema_version: u32,
    reply: String,
    id: String,
    #[serde(default)]
    candidates: Vec<ReplyDocument>,
}

#[derive(Debug, Deserialize)]
struct ReplyDocument {
    id: String,
    text: String,
    #[serde(default)]
    placeholders: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ReplyCheckResponse {
    schema_version: u32,
    #[serde(flatten)]
    check: ReplyCheck,
}

struct ApiFailure(DocudisV1Status, String);

thread_local! {
    static LAST_ERROR: RefCell<CString> = RefCell::new(CString::default());
}

fn set_error(message: impl AsRef<str>) {
    let sanitized = message.as_ref().replace('\0', "�");
    LAST_ERROR.with(|slot| {
        *slot.borrow_mut() = CString::new(sanitized).unwrap_or_default();
    });
}

fn clear_error() {
    LAST_ERROR.with(|slot| *slot.borrow_mut() = CString::default());
}

#[no_mangle]
pub extern "C" fn docudis_v1_abi_version() -> u32 {
    ABI_VERSION
}

#[no_mangle]
pub extern "C" fn docudis_v1_version() -> *const c_char {
    VERSION.as_ptr().cast()
}

#[no_mangle]
pub extern "C" fn docudis_v1_last_error_message() -> *const c_char {
    LAST_ERROR.with(|slot| slot.borrow().as_ptr())
}

#[no_mangle]
/// Anonymizes a schema-v1 UTF-8 JSON request into a Rust-owned output buffer.
///
/// # Safety
///
/// `input` must point to `input_len` readable bytes and `out` must point to a
/// writable [`DocudisV1Buffer`] for the duration of the call. The returned
/// buffer must be released exactly once with [`docudis_v1_buffer_free`].
pub unsafe extern "C" fn docudis_v1_anonymize_json(
    input: *const u8,
    input_len: usize,
    out: *mut DocudisV1Buffer,
) -> DocudisV1Status {
    let result = catch_unwind(AssertUnwindSafe(|| {
        if out.is_null() {
            set_error("out must not be NULL");
            return DocudisV1Status::InvalidArgument;
        }
        // SAFETY: The pointer was checked above. The C contract requires it to
        // point to writable memory for one DocudisV1Buffer.
        unsafe { out.write(DocudisV1Buffer::EMPTY) };
        clear_error();

        if input.is_null() {
            set_error("input must not be NULL");
            return DocudisV1Status::InvalidArgument;
        }
        // SAFETY: The C contract requires `input` to be readable for
        // `input_len` bytes for the duration of this call.
        let input = unsafe { slice::from_raw_parts(input, input_len) };
        let input = match std::str::from_utf8(input) {
            Ok(input) => input,
            Err(error) => {
                set_error(format!("input is not valid UTF-8: {error}"));
                return DocudisV1Status::InvalidUtf8;
            }
        };
        let request: AnonymizeRequest = match serde_json::from_str(input) {
            Ok(request) => request,
            Err(error) => {
                set_error(format!("invalid request JSON: {error}"));
                return DocudisV1Status::InvalidJson;
            }
        };
        if request.schema_version != 1 {
            set_error(format!(
                "unsupported schema_version {}; expected 1",
                request.schema_version
            ));
            return DocudisV1Status::InvalidArgument;
        }
        let mut previous = PlaceholderMap::new();
        previous.import(request.previous_map);
        let previous = (!previous.is_empty()).then_some(&previous);
        let anonymized = match anonymize(&request.text, &request.detections, previous) {
            Ok(anonymized) => anonymized,
            Err(error) => {
                set_error(error.to_string());
                return DocudisV1Status::CoreError;
            }
        };
        let response = AnonymizeResponse {
            schema_version: 1,
            text: anonymized.text,
            mappings: anonymized.map.entries(),
            replacements: anonymized.replacements,
        };
        let bytes = match serde_json::to_vec(&response) {
            Ok(bytes) => bytes,
            Err(error) => {
                set_error(format!("could not serialize response JSON: {error}"));
                return DocudisV1Status::CoreError;
            }
        };
        // SAFETY: `out` is valid by the C contract and was checked above.
        unsafe { out.write(DocudisV1Buffer::from_vec(bytes)) };
        DocudisV1Status::Ok
    }));

    match result {
        Ok(status) => status,
        Err(_) => {
            if !out.is_null() {
                // SAFETY: Same argument contract as above. This best-effort
                // reset occurs after unwinding was caught at the ABI boundary.
                unsafe { out.write(DocudisV1Buffer::EMPTY) };
            }
            set_error("Rust panic caught at the Docudis C ABI boundary");
            DocudisV1Status::Panic
        }
    }
}

fn validate_schema(version: u32) -> Result<(), ApiFailure> {
    if version == 1 {
        Ok(())
    } else {
        Err(ApiFailure(
            DocudisV1Status::InvalidArgument,
            format!("unsupported schema_version {version}; expected 1"),
        ))
    }
}

fn validate_detections(text: &str, detections: &[Detection]) -> Result<(), ApiFailure> {
    for (index, detection) in detections.iter().enumerate() {
        let Some(value) = text.get(detection.start..detection.end) else {
            return Err(ApiFailure(
                DocudisV1Status::CoreError,
                format!("detection {index} is not on valid UTF-8 boundaries"),
            ));
        };
        if value != detection.value {
            return Err(ApiFailure(
                DocudisV1Status::CoreError,
                format!("detection {index} value does not match its text span"),
            ));
        }
    }
    Ok(())
}

fn detect_request(request: &DetectRequest) -> Result<Vec<Detection>, ApiFailure> {
    validate_schema(request.schema_version)?;
    validate_detections(&request.text, &request.detections)?;
    let regex = RegexDetector::bundled(request.regions.as_ref(), request.selection.as_ref())
        .map_err(|error| ApiFailure(DocudisV1Status::CoreError, error.to_string()))?;
    let mut candidates = regex
        .detect_sync(&request.text)
        .map_err(|error| ApiFailure(DocudisV1Status::CoreError, error.to_string()))?;
    candidates
        .extend(DictionaryDetector::new(request.dictionary.clone()).detect_sync(&request.text));
    if request.include_bundled_lists {
        candidates.extend(
            BundledListDetector::bundled()
                .map_err(|error| ApiFailure(DocudisV1Status::CoreError, error.to_string()))?
                .detect_sync(&request.text),
        );
    }
    candidates.extend(request.detections.iter().cloned());
    Ok(DetectionPipeline::new(request.never_hide.clone()).process(&request.text, candidates))
}

fn parse_request<T: for<'de> Deserialize<'de>>(input: &str) -> Result<T, ApiFailure> {
    serde_json::from_str(input).map_err(|error| {
        ApiFailure(
            DocudisV1Status::InvalidJson,
            format!("invalid request JSON: {error}"),
        )
    })
}

fn response_json<T: Serialize>(response: &T) -> Result<Vec<u8>, ApiFailure> {
    serde_json::to_vec(response).map_err(|error| {
        ApiFailure(
            DocudisV1Status::CoreError,
            format!("could not serialize response JSON: {error}"),
        )
    })
}

unsafe fn invoke_json(
    input: *const u8,
    input_len: usize,
    out: *mut DocudisV1Buffer,
    operation: impl FnOnce(&str) -> Result<Vec<u8>, ApiFailure>,
) -> DocudisV1Status {
    let result = catch_unwind(AssertUnwindSafe(|| {
        if out.is_null() {
            set_error("out must not be NULL");
            return DocudisV1Status::InvalidArgument;
        }
        // SAFETY: checked above and required by the public C contract.
        unsafe { out.write(DocudisV1Buffer::EMPTY) };
        clear_error();
        if input.is_null() {
            set_error("input must not be NULL");
            return DocudisV1Status::InvalidArgument;
        }
        // SAFETY: the public C contract requires a readable input range.
        let bytes = unsafe { slice::from_raw_parts(input, input_len) };
        let input = match std::str::from_utf8(bytes) {
            Ok(value) => value,
            Err(error) => {
                set_error(format!("input is not valid UTF-8: {error}"));
                return DocudisV1Status::InvalidUtf8;
            }
        };
        match operation(input) {
            Ok(bytes) => {
                // SAFETY: checked above and required by the public C contract.
                unsafe { out.write(DocudisV1Buffer::from_vec(bytes)) };
                DocudisV1Status::Ok
            }
            Err(ApiFailure(status, message)) => {
                set_error(message);
                status
            }
        }
    }));
    match result {
        Ok(status) => status,
        Err(_) => {
            if !out.is_null() {
                // SAFETY: best-effort reset under the same public contract.
                unsafe { out.write(DocudisV1Buffer::EMPTY) };
            }
            set_error("Rust panic caught at the Docudis C ABI boundary");
            DocudisV1Status::Panic
        }
    }
}

#[no_mangle]
/// Detects rules, dictionary/list terms, and merges caller-provided NER spans.
///
/// # Safety
/// Same pointer and ownership contract as [`docudis_v1_anonymize_json`].
pub unsafe extern "C" fn docudis_v1_detect_json(
    input: *const u8,
    input_len: usize,
    out: *mut DocudisV1Buffer,
) -> DocudisV1Status {
    // SAFETY: forwarded unchanged to the common checked ABI boundary.
    unsafe {
        invoke_json(input, input_len, out, |input| {
            let request: DetectRequest = parse_request(input)?;
            let detections = detect_request(&request)?;
            response_json(&DetectResponse {
                schema_version: 1,
                detections,
            })
        })
    }
}

#[no_mangle]
/// Runs detection and anonymization in one call.
///
/// # Safety
/// Same pointer and ownership contract as [`docudis_v1_anonymize_json`].
pub unsafe extern "C" fn docudis_v1_process_json(
    input: *const u8,
    input_len: usize,
    out: *mut DocudisV1Buffer,
) -> DocudisV1Status {
    // SAFETY: forwarded unchanged to the common checked ABI boundary.
    unsafe {
        invoke_json(input, input_len, out, |input| {
            let request: DetectRequest = parse_request(input)?;
            let detections = detect_request(&request)?;
            let mut previous = PlaceholderMap::new();
            previous.import(request.previous_map);
            let previous = (!previous.is_empty()).then_some(&previous);
            let result = anonymize(&request.text, &detections, previous)
                .map_err(|error| ApiFailure(DocudisV1Status::CoreError, error.to_string()))?;
            response_json(&ProcessResponse {
                schema_version: 1,
                text: result.text,
                detections,
                mappings: result.map.entries(),
                replacements: result.replacements,
            })
        })
    }
}

#[no_mangle]
/// Restores placeholders using caller-provided mappings.
///
/// # Safety
/// Same pointer and ownership contract as [`docudis_v1_anonymize_json`].
pub unsafe extern "C" fn docudis_v1_restore_json(
    input: *const u8,
    input_len: usize,
    out: *mut DocudisV1Buffer,
) -> DocudisV1Status {
    // SAFETY: forwarded unchanged to the common checked ABI boundary.
    unsafe {
        invoke_json(input, input_len, out, |input| {
            let request: RestoreRequest = parse_request(input)?;
            validate_schema(request.schema_version)?;
            let mut map = PlaceholderMap::new();
            map.import(request.mappings);
            response_json(&RestoreResponse {
                schema_version: 1,
                text: map.restore(&request.text),
            })
        })
    }
}

#[no_mangle]
/// Cuts text into the review page's one-tap chunks around taken spans.
///
/// # Safety
/// Same pointer and ownership contract as [`docudis_v1_anonymize_json`].
pub unsafe extern "C" fn docudis_v1_chunk_json(
    input: *const u8,
    input_len: usize,
    out: *mut DocudisV1Buffer,
) -> DocudisV1Status {
    // SAFETY: forwarded unchanged to the common checked ABI boundary.
    unsafe {
        invoke_json(input, input_len, out, |input| {
            let request: ChunkRequest = parse_request(input)?;
            validate_schema(request.schema_version)?;
            let mut taken = Vec::with_capacity(request.taken.len());
            for (index, range) in request.taken.iter().enumerate() {
                if range.start > range.end || request.text.get(range.start..range.end).is_none() {
                    return Err(ApiFailure(
                        DocudisV1Status::CoreError,
                        format!("taken range {index} is not on valid UTF-8 boundaries"),
                    ));
                }
                taken.push((range.start, range.end));
            }
            response_json(&ChunkResponse {
                schema_version: 1,
                chunks: chunk_text(&request.text, &taken),
            })
        })
    }
}

#[no_mangle]
/// Resolves overlaps and propagates values among the given detections
/// without detecting anything new (the review page's edits).
///
/// # Safety
/// Same pointer and ownership contract as [`docudis_v1_anonymize_json`].
pub unsafe extern "C" fn docudis_v1_merge_json(
    input: *const u8,
    input_len: usize,
    out: *mut DocudisV1Buffer,
) -> DocudisV1Status {
    // SAFETY: forwarded unchanged to the common checked ABI boundary.
    unsafe {
        invoke_json(input, input_len, out, |input| {
            let request: MergeRequest = parse_request(input)?;
            validate_schema(request.schema_version)?;
            validate_detections(&request.text, &request.detections)?;
            response_json(&DetectResponse {
                schema_version: 1,
                detections: merge(&request.text, &request.detections, None),
            })
        })
    }
}

#[no_mangle]
/// Picks the rule-pack regions for a text's detected language tags.
///
/// # Safety
/// Same pointer and ownership contract as [`docudis_v1_anonymize_json`].
pub unsafe extern "C" fn docudis_v1_regions_json(
    input: *const u8,
    input_len: usize,
    out: *mut DocudisV1Buffer,
) -> DocudisV1Status {
    // SAFETY: forwarded unchanged to the common checked ABI boundary.
    unsafe {
        invoke_json(input, input_len, out, |input| {
            let request: RegionsRequest = parse_request(input)?;
            validate_schema(request.schema_version)?;
            let mut regions: Vec<_> =
                regions_for_languages(request.languages.iter().map(String::as_str), &request.text)
                    .into_iter()
                    .collect();
            regions.sort();
            response_json(&RegionsResponse {
                schema_version: 1,
                regions,
            })
        })
    }
}

#[no_mangle]
/// Identifies the languages of a text, for hosts without language
/// identification of their own. Fails with `CoreError` when the library was
/// built without the `language-id` feature.
///
/// # Safety
/// Same pointer and ownership contract as [`docudis_v1_anonymize_json`].
pub unsafe extern "C" fn docudis_v1_languages_json(
    input: *const u8,
    input_len: usize,
    out: *mut DocudisV1Buffer,
) -> DocudisV1Status {
    // SAFETY: forwarded unchanged to the common checked ABI boundary.
    unsafe {
        invoke_json(input, input_len, out, |input| {
            let request: LanguagesRequest = parse_request(input)?;
            validate_schema(request.schema_version)?;
            response_json(&LanguagesResponse {
                schema_version: 1,
                languages: detect_languages(&request.text)?,
            })
        })
    }
}

#[cfg(feature = "language-id")]
fn detect_languages(text: &str) -> Result<Vec<String>, ApiFailure> {
    Ok(docudis_core::detect_languages(text))
}

#[cfg(not(feature = "language-id"))]
fn detect_languages(_: &str) -> Result<Vec<String>, ApiFailure> {
    Err(ApiFailure(
        DocudisV1Status::CoreError,
        "this Docudis library was built without language identification (feature language-id)"
            .to_owned(),
    ))
}

#[no_mangle]
/// Checks whether a pasted AI reply answers the given document.
///
/// # Safety
/// Same pointer and ownership contract as [`docudis_v1_anonymize_json`].
pub unsafe extern "C" fn docudis_v1_reply_check_json(
    input: *const u8,
    input_len: usize,
    out: *mut DocudisV1Buffer,
) -> DocudisV1Status {
    // SAFETY: forwarded unchanged to the common checked ABI boundary.
    unsafe {
        invoke_json(input, input_len, out, |input| {
            let request: ReplyCheckRequest = parse_request(input)?;
            validate_schema(request.schema_version)?;
            let matcher = ReplyMatcher::new(request.candidates.iter().map(|document| {
                (
                    document.id.clone(),
                    ReplyCandidate::new(
                        &document.text,
                        document.placeholders.iter().map(String::as_str),
                    ),
                )
            }));
            response_json(&ReplyCheckResponse {
                schema_version: 1,
                check: matcher.check(&request.reply, &request.id),
            })
        })
    }
}

#[no_mangle]
/// Releases a buffer allocated by [`docudis_v1_anonymize_json`] and zeroes it.
///
/// # Safety
///
/// `buffer` must be NULL or point to writable storage containing either an
/// empty buffer or a live buffer returned by this same loaded library. A live
/// allocation may be passed to this function only once.
pub unsafe extern "C" fn docudis_v1_buffer_free(buffer: *mut DocudisV1Buffer) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if buffer.is_null() {
            return;
        }
        // SAFETY: The caller promises this is a live buffer structure produced
        // by this library, or an empty one.
        let owned = unsafe { buffer.read() };
        if !owned.ptr.is_null() {
            // SAFETY: Successful outputs are created from a Vec with exactly
            // these allocation fields, and ownership crosses back only once.
            unsafe { drop(Vec::from_raw_parts(owned.ptr, owned.len, owned.capacity)) };
        }
        // SAFETY: The caller provided writable storage for the structure.
        unsafe { buffer.write(DocudisV1Buffer::EMPTY) };
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;

    #[test]
    fn round_trips_json_and_owned_buffer() {
        let request = r#"{"schema_version":1,"text":"Hi 张三","detections":[{"type":"PERSON","value":"张三","start":3,"end":9,"confidence":1.0,"detector":"test","source":"manual","enabled":true}]}"#.as_bytes();
        let mut output = DocudisV1Buffer::EMPTY;
        let status =
            unsafe { docudis_v1_anonymize_json(request.as_ptr(), request.len(), &mut output) };
        assert_eq!(status, DocudisV1Status::Ok);
        let bytes = unsafe { slice::from_raw_parts(output.ptr, output.len) };
        let response: AnonymizeResponse = serde_json::from_slice(bytes).unwrap();
        assert_eq!(response.text, "Hi [PERSON_1]");
        unsafe { docudis_v1_buffer_free(&mut output) };
        assert!(output.ptr.is_null());
        assert_eq!(output.len, 0);
    }

    #[test]
    fn invalid_json_has_stable_status_and_readable_error() {
        let request = b"not json";
        let mut output = DocudisV1Buffer::EMPTY;
        let status =
            unsafe { docudis_v1_anonymize_json(request.as_ptr(), request.len(), &mut output) };
        assert_eq!(status, DocudisV1Status::InvalidJson);
        let message = unsafe { CStr::from_ptr(docudis_v1_last_error_message()) };
        assert!(message.to_str().unwrap().contains("invalid request JSON"));
    }

    #[test]
    fn invalid_utf8_boundary_is_a_core_error_not_a_panic() {
        let request = r#"{"schema_version":1,"text":"😀","detections":[{"type":"CUSTOM","value":"😀","start":1,"end":4,"confidence":1.0,"detector":"test","source":"manual","enabled":true}]}"#.as_bytes();
        let mut output = DocudisV1Buffer::EMPTY;
        let status =
            unsafe { docudis_v1_anonymize_json(request.as_ptr(), request.len(), &mut output) };
        assert_eq!(status, DocudisV1Status::CoreError);
        assert!(output.ptr.is_null());
        let message = unsafe { CStr::from_ptr(docudis_v1_last_error_message()) };
        assert!(message
            .to_str()
            .unwrap()
            .contains("UTF-8 character boundary"));
    }

    #[test]
    fn detect_process_and_restore_share_schema_v1() {
        let request = br#"{"schema_version":1,"text":"Alice: alice@example.com","regions":[],"dictionary":["Alice"],"detections":[]}"#;
        let mut output = DocudisV1Buffer::EMPTY;
        assert_eq!(
            unsafe { docudis_v1_detect_json(request.as_ptr(), request.len(), &mut output) },
            DocudisV1Status::Ok
        );
        let detected: DetectResponse =
            serde_json::from_slice(unsafe { slice::from_raw_parts(output.ptr, output.len) })
                .unwrap();
        assert_eq!(detected.schema_version, 1);
        assert!(detected.detections.iter().any(|d| d.value == "Alice"));
        unsafe { docudis_v1_buffer_free(&mut output) };
        assert_eq!(
            unsafe { docudis_v1_process_json(request.as_ptr(), request.len(), &mut output) },
            DocudisV1Status::Ok
        );
        let processed: ProcessResponse =
            serde_json::from_slice(unsafe { slice::from_raw_parts(output.ptr, output.len) })
                .unwrap();
        assert_eq!(processed.detections.len(), detected.detections.len());
        unsafe { docudis_v1_buffer_free(&mut output) };
        let restore=serde_json::to_vec(&serde_json::json!({"schema_version":1,"text":processed.text,"mappings":processed.mappings})).unwrap();
        assert_eq!(
            unsafe { docudis_v1_restore_json(restore.as_ptr(), restore.len(), &mut output) },
            DocudisV1Status::Ok
        );
        let restored: RestoreResponse =
            serde_json::from_slice(unsafe { slice::from_raw_parts(output.ptr, output.len) })
                .unwrap();
        assert_eq!(restored.text, "Alice: alice@example.com");
        unsafe { docudis_v1_buffer_free(&mut output) };
    }

    type Call = unsafe extern "C" fn(*const u8, usize, *mut DocudisV1Buffer) -> DocudisV1Status;

    fn call_json(call: Call, request: serde_json::Value) -> (DocudisV1Status, serde_json::Value) {
        let request = serde_json::to_vec(&request).unwrap();
        let mut output = DocudisV1Buffer::EMPTY;
        let status = unsafe { call(request.as_ptr(), request.len(), &mut output) };
        let response = if output.ptr.is_null() {
            serde_json::Value::Null
        } else {
            serde_json::from_slice(unsafe { slice::from_raw_parts(output.ptr, output.len) })
                .unwrap()
        };
        unsafe { docudis_v1_buffer_free(&mut output) };
        (status, response)
    }

    #[test]
    fn chunk_json_returns_utf8_ranges_around_taken_spans() {
        let (status, response) = call_json(
            docudis_v1_chunk_json,
            serde_json::json!({"schema_version":1,"text":"张三 met Bob","taken":[{"start":0,"end":6}]}),
        );
        assert_eq!(status, DocudisV1Status::Ok);
        assert_eq!(
            response,
            serde_json::json!({"schema_version":1,"chunks":[{"start":7,"end":10},{"start":11,"end":14}]})
        );
        let (status, _) = call_json(
            docudis_v1_chunk_json,
            serde_json::json!({"schema_version":1,"text":"张三","taken":[{"start":0,"end":1}]}),
        );
        assert_eq!(status, DocudisV1Status::CoreError);
    }

    #[test]
    fn merge_json_propagates_without_detecting() {
        let (status, response) = call_json(
            docudis_v1_merge_json,
            serde_json::json!({"schema_version":1,"text":"Alice met Alice, call 06 12 34 56 78","detections":[
                {"type":"PERSON","value":"Alice","start":0,"end":5,"confidence":0.9,"detector":"ner","source":"model","enabled":true}
            ]}),
        );
        assert_eq!(status, DocudisV1Status::Ok);
        let values: Vec<_> = response["detections"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| {
                (
                    d["start"].as_u64().unwrap(),
                    d["source"].as_str().unwrap().to_owned(),
                )
            })
            .collect();
        // The phone is not detected: merge only works on what it is given.
        assert_eq!(
            values,
            [(0, "model".to_owned()), (10, "propagated".to_owned())]
        );
    }

    #[test]
    fn regions_json_is_sorted() {
        let (status, response) = call_json(
            docudis_v1_regions_json,
            serde_json::json!({"schema_version":1,"languages":["en-GB"],"text":"Invoice for 张三"}),
        );
        assert_eq!(status, DocudisV1Status::Ok);
        assert_eq!(
            response,
            serde_json::json!({"schema_version":1,"regions":["cn","gb","ie","us"]})
        );
    }

    #[test]
    fn languages_json_needs_the_feature() {
        let (status, response) = call_json(
            docudis_v1_languages_json,
            serde_json::json!({"schema_version":1,"text":"Le locataire paie le loyer chaque mois."}),
        );
        if cfg!(feature = "language-id") {
            assert_eq!(status, DocudisV1Status::Ok);
            assert_eq!(
                response,
                serde_json::json!({"schema_version":1,"languages":["fr"]})
            );
        } else {
            assert_eq!(status, DocudisV1Status::CoreError);
            let message = unsafe { CStr::from_ptr(docudis_v1_last_error_message()) };
            assert!(message.to_str().unwrap().contains("language-id"));
        }
    }

    #[test]
    fn reply_check_json_reports_invented_labels_and_a_better_match() {
        let candidates = serde_json::json!([
            {"id":"a","text":"Lease for [PERSON_1] at [ADDRESS_1], rent paid monthly.","placeholders":["[PERSON_1]","[ADDRESS_1]"]},
            {"id":"b","text":"Patient [PERSON_1] reported headaches and dizziness.","placeholders":["[PERSON_1]"]}
        ]);
        let (status, response) = call_json(
            docudis_v1_reply_check_json,
            serde_json::json!({"schema_version":1,"id":"b","candidates":candidates,
                "reply":"The lease binds [PERSON_1] and [PERSON_3]; rent for [ADDRESS_1] is paid monthly."}),
        );
        assert_eq!(status, DocudisV1Status::Ok);
        assert_eq!(
            response,
            serde_json::json!({"schema_version":1,"unknown":["[ADDRESS_1]"],"invented":["[PERSON_3]"],"better_match":"a"})
        );
    }

    #[test]
    fn caller_confidence_round_trips_bit_for_bit() {
        // serde_json's default float parser is not correctly rounded and
        // shifted these 17-digit NER confidences by one ULP.
        let request = br#"{"schema_version":1,"text":"Alice lives in Paris","regions":[],"detections":[{"type":"PERSON","value":"Alice","start":0,"end":5,"confidence":0.9998847145629489,"detector":"ner","source":"model","enabled":true},{"type":"ADDRESS","value":"Paris","start":15,"end":20,"confidence":0.9970184195601827,"detector":"ner","source":"model","enabled":true}]}"#;
        for call in [docudis_v1_detect_json as Call, docudis_v1_process_json] {
            let mut output = DocudisV1Buffer::EMPTY;
            assert_eq!(
                unsafe { call(request.as_ptr(), request.len(), &mut output) },
                DocudisV1Status::Ok
            );
            let json =
                std::str::from_utf8(unsafe { slice::from_raw_parts(output.ptr, output.len) })
                    .unwrap()
                    .to_owned();
            unsafe { docudis_v1_buffer_free(&mut output) };
            assert!(
                json.contains(r#""confidence":0.9998847145629489"#),
                "{json}"
            );
            assert!(
                json.contains(r#""confidence":0.9970184195601827"#),
                "{json}"
            );
        }
    }
}
