# Contributing

Issues and pull requests are welcome. This project is maintained in spare
time, so replies can be slow.

## No real personal data

Never put real personal data in an issue, a commit, a test, or a fixture. Use
made-up names and numbers, and the documented example values for things like
API keys. If a bug only shows with a real document, describe its structure
instead.

## Rule packs

New or changed rules in `data/rules` are the most useful contributions. A rule
change should include:

- `examples` that match, and a hard negative in the tests where a near miss is
  likely;
- a validator (checksum or format check) when the value has one;
- the source of the format, such as the official specification.

## Checks

Run the full local verification from the [README](README.md#local-verification)
before opening a pull request. Changes to v1 behavior must keep every existing
conformance fixture passing; see [conformance/README.md](conformance/README.md).

## Sign-off

Contributions are accepted under Apache-2.0. Sign off each commit to certify
the [Developer Certificate of Origin](https://developercertificate.org/):

```sh
git commit -s
```

The app repositories (`docudis-android`, `docudis-desktop`) are licensed
differently and have their own contribution terms.
