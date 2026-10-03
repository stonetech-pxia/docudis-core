# Docudis Dart FFI

Apache-2.0 Dart bindings for the stable `docudis_v1_*` C ABI.

The package converts Dart UTF-16 detection offsets to Rust UTF-8 byte offsets,
copies every Rust-owned response before calling `docudis_v1_buffer_free`, and
maps stable status codes to `DocudisException`. `DocudisDifferentialRunner`
keeps the Dart implementation as the fallback while callers pass Dart-produced
NER detections to Rust for rule/list/dictionary merge and anonymization.

`DocudisCore` is the typed entry point apps use: `detect`, `process`,
`anonymize`, `merge`, `chunk`, `regions`, `replyCheck`, and `restore` take and
return the shared models (`EntityType`, `DetectionSource`, `Detection`,
`MappingEntry`, `PlaceholderMap`, `AnonymizedText`, `TextChunk`,
`ReplyCandidate`, `ReplyCheck`) with Dart UTF-16 offsets. Their JSON matches
what the Android app already stores, so existing records stay readable.
`PlaceholderMap` only holds entries; Rust issues placeholders and restores.

Build and test on macOS:

```sh
cargo build -p docudis-capi
cd bindings/dart
dart pub get
DOCUDIS_LIBRARY=../../target/debug/libdocudis_capi.dylib dart test
```

This binding does not load or execute an NER model. DocCloak.Core attribution
is preserved in `NOTICE` and in the repository's
`LICENSE-DocCloak.Core` and `NOTICE-DocCloak.Core` files.
