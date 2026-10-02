# Security policy

## Reporting a vulnerability

Report vulnerabilities privately through GitHub: open the repository's
**Security** tab and choose **Report a vulnerability**. Do not open a public
issue.

Use made-up data in the report. If the problem only shows with a real
document, describe its structure (language, layout, the kind of value that
leaks) instead of attaching it.

## What counts

Besides the usual memory-safety and C ABI issues, these are security issues
here:

- personal data that should be hidden but is left in the anonymized text;
- a restore that puts the wrong original value back, or a value from another
  document;
- a crash or hang on crafted input, including a Rust panic crossing the C ABI.

Only the latest release receives fixes.
