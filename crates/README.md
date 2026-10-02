# Docudis Rust core

Core began as a Rust port of the Dart engine in docudis-android
(`packages/docudis_engine`). The Dart FFI package keeps a differential runner
that executes both implementations and reports where they disagree.

All Rust crates are licensed under Apache-2.0. The placeholder and restore
behavior was migrated from the Dart implementation that includes
DocCloak.Core-derived code. The required copyright and attribution are retained
in each crate's `NOTICE` and in
`NOTICE-DocCloak.Core`; the full Apache-2.0 text remains at
`LICENSE-DocCloak.Core`.

## Workspace

- `docudis-core`: platform-independent rules, validators, lists, dictionaries,
  overlap resolution, never-hide, repair, propagation, offset conversion,
  anonymization, and restoration. It contains no Flutter, PDF, OCR, ONNX
  Runtime, tokenizer, or other model dependency.
- `docudis-capi`: small versioned C ABI built as both `cdylib` and `staticlib`.
- `docudis-cli`: text/stdin interface over the same Rust core.
- `../conformance`: one language-neutral fixture suite consumed by Rust and
  Dart tests.

The `Detector` trait in `docudis-core` is the future adapter seam. Rule and
dictionary detection can remain pure; model-backed detection lives in the
separate `docudis-ner` repository, which implements or drives this boundary. OCR,
file selection, sharing, and platform UI remain outside the core. PDF/DOCX
processing will live in `docudis-documents` and consume core replacements.

## Build and test

Install a current stable Rust toolchain, then run from the repository root:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

cd bindings/dart
dart format --output=none --set-exit-if-changed lib test
dart analyze
dart test
```

The shared fixtures cover all rule packs and entity categories plus ASCII,
CJK, emoji, combining characters, NBSP, validators/hard negatives, overlap
priority, never-hide, repair, propagation, external NER spans, previous maps,
disabled detections, person variants, detection policies, and exact/tolerant
restoration.

## Offset contract

Rust and every public C function use half-open UTF-8 byte offsets. Dart strings
and the existing detector pipeline use half-open UTF-16 code-unit offsets. The
Dart FFI adapter must convert offsets at the boundary and reject an offset that
splits a surrogate pair or does not land on a UTF-8 scalar boundary.

`docudis-core` exports checked `utf8_to_utf16_offset` and
`utf16_to_utf8_offset` helpers for native-side verification. The Dart
conformance test demonstrates the binding-side conversion using `dart:convert`.
Do not reinterpret Dart offsets as Rust offsets, even when ASCII tests happen
to pass.

## CLI

The CLI runs non-NER rules by default. It accepts text or stdin, region packs,
user dictionary/never-hide terms, bundled lists, and external NER JSON:

```sh
cargo run -p docudis-cli -- \
  --regions fr --dictionary 'Élodie' --text 'Élodie: alice@example.com'

printf 'Call Alice' | cargo run -p docudis-cli -- \
  --json --detect-only --regions gb
```

`--ner-detections FILE` merges host-produced UTF-8 spans; without it the CLI
does not claim to run NER. `--type DATE=hide` (or `keep`, `off`) and
`--range START:END` (UTF-8 bytes) set the detection policy below; both repeat.
`--restore --map FILE` restores placeholders.

## C ABI

The authoritative declaration is `docudis-capi/include/docudis.h`.

- All exported names use the `docudis_v1_` prefix, and
  `docudis_v1_abi_version()` returns `1`.
- `docudis_v1_anonymize_json` accepts caller-provided detections unchanged.
  `docudis_v1_detect_json`, `docudis_v1_process_json`, and
  `docudis_v1_restore_json` add detection, combined processing, and restoration
  while preserving ABI v1. `docudis_v1_chunk_json`, `docudis_v1_merge_json`,
  `docudis_v1_regions_json`, and `docudis_v1_reply_check_json` serve the review
  and restore screens: one-tap chunks, merge without detection, rule-pack
  regions for detected languages, and the reply-to-document check.
  `docudis_v1_languages_json` identifies a text's languages for hosts without
  their own (Windows); it works only in a library built with the optional
  `language-id` feature (lingua, 30 languages, ~150 MB of models) and otherwise
  fails with `DOCUDIS_V1_CORE_ERROR`. Android builds leave the feature off and
  keep ML Kit. Complex
  data is versioned with `schema_version: 1`; detection and replacement offsets
  are UTF-8 bytes.
- Input memory remains owned by the caller. A successful output is allocated by
  Rust and **must** be passed exactly once to `docudis_v1_buffer_free` from the
  same loaded library. The free function zeroes the structure. The output is
  length-delimited and is not NUL-terminated.
- Status codes are stable in the v1 header. Failure detail is available from
  the thread-local `docudis_v1_last_error_message()` and must not be freed.
- Every operation that can execute Rust logic catches unwinding at the ABI
  boundary. No Rust panic is permitted to cross into C.
- The header exposes only the version, status enum, and owned byte buffer; it
  does not expose Rust layouts or core structs.

Example request:

```json
{
  "schema_version": 1,
  "text": "Hi 张三",
  "detections": [
    {
      "type": "PERSON",
      "value": "张三",
      "start": 3,
      "end": 9,
      "confidence": 1.0,
      "detector": "host",
      "source": "manual",
      "enabled": true
    }
  ]
}
```

### Detection policy

`docudis_v1_detect_json` and `docudis_v1_process_json` accept an optional
`policy` saying what to hide, whichever rule, list or model found it:

```json
{
  "schema_version": 1,
  "text": "...",
  "policy": {
    "types": { "DATE": "hide", "ADDRESS": "keep", "URL": "off" },
    "ranges": [[1200, 5400], [8000, 9100]]
  }
}
```

- `types` maps `EntityType` names to `hide` (enabled, overriding the default
  that leaves dates and amounts visible), `keep` (detected and still winning
  overlaps, but disabled) or `off` (dropped before overlaps are resolved, so
  it cannot displace a span inside it). `DATE` and `BIRTH_DATE` are separate
  keys. Dictionary terms and manual spans ignore `types`. `never_hide` still
  wins over `hide`.
- `ranges` are half-open UTF-8 byte ranges. A detection touching any range
  is kept whole; others are dropped, including values propagated from inside
  the ranges. Omit `ranges` to process the whole text; an empty list is an
  error.
- Unknown type names or actions and ranges that are empty, reversed, beyond
  the text or not on character boundaries fail with
  `DOCUDIS_V1_INVALID_ARGUMENT`.
- Without `policy` (or with `null`) output is unchanged. **The policy needs
  Core 0.2.0 or later** (`docudis_v1_version()`); older libraries silently
  ignore the field, so hosts must pin Core or check the version.

## Deliberate boundary

Core owns no model code. Tokenization, window construction, SentencePiece
realignment, softmax selection, BIO decoding, and inference live in
`docudis-ner`. Core only accepts the resulting detections, from one or more
models, through the v1 `Detection` contract. OCR, PDF/DOCX processing, and
platform UI remain outside Core.
