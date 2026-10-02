# Authoritative data

This directory is the single editable source of truth for Core rule packs and
bundled lists.

Android/Dart consumers may contain a generated snapshot only when it records a
fixed Core revision and a deterministic content digest. Do not edit a generated
snapshot directly.

## Sources

- `rules/*.json`: regional rule packs. Rules marked `"provenance": "doccloak"`
  (or `"doccloakModified"`) come from
  [DocCloak.Core](https://github.com/WLojek/DocCloak.Core) (Apache-2.0) and
  retain the attribution in the repository `NOTICE-DocCloak.Core` and
  `LICENSE-DocCloak.Core` files. Rules marked `"provenance": "docudis"` were
  written for Docudis.
- `lists/companies.json` and `lists/places_zh.json`: organization and Chinese
  place names taken from [Wikidata](https://www.wikidata.org/), which is
  available under CC0. Each entry keeps its Wikidata item ID (`Q…`) so it can
  be checked against the source.
