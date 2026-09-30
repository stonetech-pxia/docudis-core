# Docudis Dart FFI

Apache-2.0 Dart bindings for the stable `docudis_v1_*` C ABI.

The package converts Dart UTF-16 detection offsets to Rust UTF-8 byte offsets,
copies every Rust-owned response before calling `docudis_v1_buffer_free`, and
maps stable status codes to `DocudisException`. `DocudisDifferentialRunner`
keeps the Dart implementation as the fallback while callers pass Dart-produced
NER detections to Rust for rule/list/dictionary merge and anonymization.

Build and test on macOS:

```sh
cargo build -p docudis-capi
cd bindings/dart
dart pub get
DOCUDIS_LIBRARY=../../target/debug/libdocudis_capi.dylib dart test
```

This binding does not load or execute an NER model. DocCloak.Core attribution
is preserved in `NOTICE` and in the repository's
the repository `LICENSE-DocCloak.Core` and `NOTICE-DocCloak.Core` files.
