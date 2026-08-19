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

- **Etape 1: the Link protocol contract.** `include/lnk/lnk_protocol.h` is the contract of
  record — the `LNK1` magic, the three-byte `u16 length | u8 type` little-endian framing
  (deliberately no header struct: C would pad it), an exact-size rule for every fixed message
  checked before any copy, and nine of the ten messages as no-padding PODs: HELLO (version,
  fingerprint, role), WELCOME (tick, dt, client id), TICK_STATE (header plus forty-byte
  creature rows, capped at 256 to fit one frame four times over), ACTIONS (the ABI's twelve
  bytes plus tick and address), EVENT, DEREZ, PING/PONG, BYE. Zero is never a valid type, role
  or kind, so a zeroed buffer refuses instead of meaning something. REZ's number is reserved
  and its layout deferred to its own etape: it flattens the flagship's pointer-carrying
  creature descriptor and is designed against that validator, not guessed. The header is
  fingerprinted by `tools/check_protocol_version.py` — the flagship's ABI tool, adapted — and
  checked in CI; the same fingerprint is what HELLO carries, so the repository guard and the
  handshake refusal are one mechanism. `src/protocol.rs` mirrors every struct with the same
  sizes pinned by const asserts and refuses big-endian hosts outright. Broken deliberately
  once each, all discriminating: the Rust size pin, the unbumped-header refusal, the
  same-version update refusal, the stale-fingerprint refusal. And the library's name is
  official: **Link**, capitalised like Master Control — `link` names only the repository.

- **The wire has a face: README and TODO.** The README states the identity — one binary loaded
  by both ends, Rust behind a plain C ABI, `std` only — the doctrine behind each of those
  choices, and the family, with the flagship's `docs/TOPOLOGY.md` named as the design authority
  that code here implements and never extends. TODO.md stages the five etapes from wire contract
  to first consumer, each carrying its red-first obligation.
