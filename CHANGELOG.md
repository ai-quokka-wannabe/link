# Changelog

All notable changes to link are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **The flagship's settings, mirrored.** Everything `tron-grid-lite` has settled about how a
  repository in this organisation behaves, copied here before any content: the CI shape
  (markdown lint, the stray-carriage-return check, warnings as errors, build and test on Windows
  and Linux, a `CI Success` gate matching the branch ruleset's required check), Dependabot on
  the pinned actions, the cache-cleanup workflow, CODEOWNERS, issue and pull-request templates,
  the code of conduct, the security policy, the contributor guide, editor and lint
  configuration, and the British-spelling `LICENCE` name for the GPL v3 text. Where the flagship
  speaks C++ and CMake, the mirror speaks Rust and Cargo: an edition-2024 `cdylib` crate with
  zero third-party crates, `cargo fmt` to the same 170-column limit, and clippy with warnings
  denied in the manifest. The GitHub-side settings (branch and tag rulesets, merge policy, the
  Actions lockdown, CodeQL default setup) were replicated through the API in the same sweep and
  verified byte-identical to the flagship's.

- **The wire has a face: README and TODO.** The README states the identity — one binary loaded
  by both ends, Rust behind a plain C ABI, `std` only — the doctrine behind each of those
  choices, and the family, with the flagship's `docs/TOPOLOGY.md` named as the design authority
  that code here implements and never extends. TODO.md stages the five etapes from wire contract
  to first consumer, each carrying its red-first obligation.
