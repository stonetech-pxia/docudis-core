# Authoritative data

This directory is the single editable source of truth for Core rule packs and
bundled lists. The files were migrated from the current
`packages/docudis_engine` working tree on 2026-09-30, including its uncommitted
rule-classification improvements.

Android/Dart consumers may contain a generated snapshot only when it records a
fixed Core revision and a deterministic content digest. Do not edit a generated
snapshot directly.

Rule packs and lists retain the DocCloak.Core attribution described by the
repository `NOTICE-DocCloak.Core` and `LICENSE-DocCloak.Core` files.

