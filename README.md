# Docudis Core

Platform-independent detection, anonymization, restoration, and stable C ABI
for Docudis. This repository intentionally has no dependency on Flutter,
Android application code, OCR, PDF processing, ML Kit, ONNX Runtime, or
model tokenizers.

Docudis replaces personal data in a document with numbered placeholders on the
user's device, so the text can be given to an AI service, and puts the
original values back into the AI's reply. Core is the part that decides what
is hidden and how it is restored. It runs fully offline and makes no network
requests; whether anything leaves the device is decided by the apps below,
which are open source as well.

## Related repositories

| Repository | Role | License |
|---|---|---|
| [docudis-core](https://github.com/stonetech-pxia/docudis-core) (this one) | Rules, lists, pipeline, anonymization, restoration, C ABI | Apache-2.0 |
| [docudis-ner](https://github.com/stonetech-pxia/docudis-ner) | Tokenizers, windowing, decoding, and inference for NER models | Apache-2.0 |
| [docudis-android](https://github.com/stonetech-pxia/docudis-android) | Android app | AGPL-3.0, commercial license available |
| [docudis-desktop](https://github.com/stonetech-pxia/docudis-desktop) | Windows and macOS app | AGPL-3.0, commercial license available |

Dependencies point one way: ner depends on Core, and the apps depend on both.

## Repository boundary

- `crates/docudis-core`: rules, lists, dictionaries, pipeline/repair, offsets,
  anonymization, and restoration.
- `crates/docudis-capi`: versioned `docudis_v1_*` C ABI and public header.
- `crates/docudis-cli`: command-line adapter.
- `bindings/dart`: Dart FFI adapter with ABI validation and safe buffer ownership.
- `data`: authoritative rule packs and bundled lists.
- `conformance`: shared v1 fixtures (rules, pipeline, anonymization, and the
  review/restore helpers: chunks, merge, regions, reply checks).

The Android application consumes versioned Core artifacts. It must not copy or
independently edit `data/rules`; generated Dart snapshots are verified against a
specific Core revision and content digest.

Model-specific code (tokenizers, windowing, BIO decoding, inference) lives in
the separate `docudis-ner` repository, which depends on Core and never the
other way round. Core accepts detections from any number of models through the
`detections` field of the v1 JSON requests (`source: "model"`, with the model
identified by `detector`) and merges them with rule and list detections.

## Local verification

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release --workspace
./scripts/test-c-header.sh

cd bindings/dart
dart pub get
dart format --output=none --set-exit-if-changed lib test
dart analyze
DOCUDIS_LIBRARY=../../target/debug/libdocudis_capi.dylib dart test
```

On Linux, use `libdocudis_capi.so`; on Windows, use
`docudis_capi.dll`. Native tests fail rather than silently mock when
`DOCUDIS_LIBRARY` is set.

## Android artifacts

Install the Android NDK, `cargo-ndk`, and the required Rust targets, then run:

```sh
./scripts/build-android.sh release
```

The default release matrix matches Flutter's Android device targets:
`arm64-v8a`, `armeabi-v7a`, and `x86_64`. Output is written under
`dist/android/release/jniLibs/<abi>/libdocudis_capi.so`. The script validates
that every requested ABI exists and exports the versioned ABI symbols. CI may
set `DOCUDIS_ANDROID_ABIS=x86_64` for a focused emulator artifact.

## Compatibility contract

`docudis_v1_abi_version()` and JSON `schema_version: 1` are stable. A breaking
change requires a new symbol namespace and schema version; v1 behavior must not
change in place. All C ABI offsets are half-open UTF-8 byte offsets. The Dart
adapter converts to and from UTF-16 code-unit offsets and refuses to load a
library with a different ABI version.

Optional request fields are added within v1. Older libraries ignore fields
they do not know, so a host that relies on one must pin Core or check
`docudis_v1_version()`: the detection `policy` (types to hide, keep or
ignore, and ranges to process) needs 0.2.0 or later.

See [crates/README.md](crates/README.md) and
[conformance/README.md](conformance/README.md) for detailed behavior and
compatibility notes.

## Limitations

Detection is automatic, and no automatic detection finds all personal data.
Review the result before sharing a document.

- Rules and lists find structured values (emails, phone numbers, IDs, IBANs,
  cards, and so on) and known names. Most personal names and free-form
  addresses need an NER model from `docudis-ner`; Core alone does not find
  them unless they are in the user's dictionary.
- Dates and amounts are detected but left visible unless a policy says to hide
  them.
- Restoration relies on the AI keeping the placeholders recognizable. It
  tolerates common changes to their formatting, but a placeholder that is
  rewritten beyond that is not restored.

## Security

Please report vulnerabilities privately as described in
[SECURITY.md](SECURITY.md). Missed detections that expose data and restores
that put the wrong value back count as security issues.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Never put real personal data in an
issue, a test, or a fixture.

## License

Apache-2.0. The original DocCloak.Core copyright and attribution are retained
in `LICENSE-DocCloak.Core` and `NOTICE-DocCloak.Core`.

The Apache-2.0 license does not grant rights to the Docudis name or logo. A
modified version must not be distributed under the Docudis name.

