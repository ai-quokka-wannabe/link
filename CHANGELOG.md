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

- **Etape 3: the transport — the socket the codec's refusals guard.** `src/transport.rs`:
  `std::net` TCP with `TCP_NODELAY` on both ends and no threads — each consumer owns its loop
  and turns a state machine. The handshake is blocking and timeout-bounded: magic, HELLO, then
  WELCOME — or a refusal in words, sent as UTF-8 text before the connection closes, because a
  refusal happens exactly when the two ends have not agreed they speak the same frames, so a
  frame could not carry it. The refusal names both protocol versions, travels to the client
  verbatim, and the convention is documented in the header (a comment-only change, which the
  fingerprint provably ignores). After the handshake the connection is non-blocking:
  `Connection::poll` judges type and length at the header — a hostile length hangs up before a
  single payload byte is read — and never reads past the frame it is assembling; `queue` and
  `flush` coalesce everything into one write per tick, carrying partial-write remainders. The
  build knows its own contract: the recorded fingerprint is compiled into the library,
  `local_hello` carries it and `accept` compares against it, so the repository guard, the
  handshake token and the binary are one thing. Seven transport tests run over real loopback
  sockets — refusals crossing the wire, a frame dribbled one byte at a time, ordered delivery
  of a coalesced burst — plus a deterministic slow-sink test for the write carry. Broken
  deliberately three times, each caught by exactly the test that guards it.

- **Etape 2: the codec — bytes to messages, by refusal.** `src/codec.rs` turns frames into
  messages and back with pure functions: no sockets, no `unsafe`, no allocation before
  validation, every field read and written in explicit little-endian so nothing ever
  reinterprets memory. The audit's gold-plated rule is an API shape — `payload_rule` and
  `check_length` answer from the three header bytes alone, so a hostile length is refused
  before a byte of payload is read — and `decode` re-checks rather than trusts, because "the
  caller surely validated" is how parsers die. Strictness is symmetric: `encode` refuses every
  frame `decode` would refuse (bad roles, unknown event kinds, nonzero reserved bytes, a
  TICK_STATE header lying about its own row count) and a refused encode writes nothing. The
  decoder never panics on any input, demonstrated by bombardment with deterministic junk at
  every legal length and type byte. Thirteen tests; three deliberate breakages, each caught by
  exactly the test that guards it — including an off-by-one at the 256-row cap boundary, which
  the round-trip suite holds at the boundary precisely so that mistake is representable only as
  a red.

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
