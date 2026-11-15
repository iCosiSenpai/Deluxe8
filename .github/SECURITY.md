# Security policy

## Reporting a vulnerability

Please report security problems **privately** through
[GitHub's private vulnerability reporting](https://github.com/iCosiSenpai/Deluxe8/security/advisories/new), not in a public
issue. Include the version, what an attacker could do, and how to reproduce it. You will get an answer within a week.

Examples of what counts: a way to make the launcher write outside its data folder, run a program it should not, leak key
values or room passwords into logs, or accept a tampered download.

## Supported versions

Only the latest release gets fixes.

## What Deluxe8 does to stay safe

- Every component the launcher downloads is pinned by URL, size and SHA-256, written to a temporary file and renamed only
  after the check. Archives are unpacked with path checks (nothing outside the target folder).
- The launcher's web view runs with a strict content security policy, cannot open arbitrary files or folders, and can
  open web links only to the licence pages of downloaded components.
- Key files are parsed without ever printing their values; logs contain key names and paths only.
- Release executables are code-signed when a certificate is configured, and come with `SHA256SUMS.txt` and build
  provenance (`gh attestation verify <file> --repo iCosiSenpai/Deluxe8`).
