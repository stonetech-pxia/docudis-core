# Changelog

## 0.2.0

First public release.

- Rules, validators, lists, dictionaries, the detection pipeline,
  anonymization, and restoration.
- The v1 C ABI, the CLI, and the Dart binding.
- Detection requests accept an optional `policy`: per-type `hide`, `keep`, or
  `off` for every source, and the text ranges to process. Libraries before
  0.2.0 ignore it.
- `docudis_v1_languages_json` identifies a text's languages, in builds with
  the optional `language-id` feature.
