# Changelog

All notable changes to link are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

- **Protocol version 4: the body goes on the wire, and both ends must live in one world.**
  `REZ` is no longer a reserved number: it carries the creature's bounds (forward speed, turn
  rate, vocalisation strength, contact budget) and its render model as counted rows —
  `LnkRezVertex`, `LnkRezTriangle` (three indices and a material slot), `LnkRezMaterial` (the
  flagship's `TglRenderMaterial`, byte for byte) — the same payload a host sends at rez and
  Master Control relays to every spectator and late joiner. It is the one variable-size client
  input, and the Dark Souls III lesson shapes it: three named caps (1024 vertices, 2048
  triangles, 16 materials — the material cap being the one guarding the slot space every
  triangle indexes into), counts judged against them before a single row is read, the exact
  length judged before any copy, every triangle index bounded by the counts, every float
  finite, both when decoding a stranger's frame and when encoding our own caller's. Sensor
  layouts are deliberately not on this wire: they are host-local. The largest legal frame is
  now a full body rather than a full tick, and the receive buffer follows. **The world
  fingerprint**: `LnkWorldDefinition` (the floor's eight fields, the tick, the body's half
  height — what the simulated world is made of, materials and perception excluded) hashes by
  FNV-1a over its bytes in field order, through one implementation exposed as the new vtable
  function `world_fingerprint`; `HELLO` carries the client's (48 bytes) and `WELCOME` the
  server's (24 bytes), and the refusal bites both ways — the server refuses a citizen of
  another world at the door, and a client refuses a server whose `WELCOME` names a different
  world, each in words naming both fingerprints. `connect` and `listen` take the fingerprint;
  `send_rez` joins the table after `send_actions`, and refuses on a spectator connection exactly
  as `send_actions` does, while the server half treats a spectator's `REZ` frame as the same
  violation as its `ACTIONS`. ABI v4; versions 1 to 3 refused as history. Every rule above has a
  test, and every test was broken once before it was trusted.
- **Protocol version 3: rigour ruled, silence given authors, and the wire resends so nothing is
  lost.** The flagship's MMO-lessons audit (TOPOLOGY.md carries the rulings in full; the owner's
  principle: no information may be lost, because this is an embodied-AI project and
  repeatability is the product) lands on the wire as one version bump. **ACTIONS grows the
  previous tick's intent, resent whole** — Tribes repeated its moves across datagrams for
  exactly this reason; redundant on TCP, load-bearing the day the UDP trigger fires, adopted
  while a message layout still changes for free — plus a counted reserved word, because the
  alternative was four bytes of invisible alignment padding and invisible padding is exactly
  what the header refuses (40 bytes; ABI v3, versions 1 and 2 refused as history). The header
  now publishes the **keepalive contract** (PING after one second of silence, dead at ten —
  constants both ends compile in, the caller owning the clock) and `LNK_ACTIONS_REPEAT_TICKS`,
  the bounded repeat-last-intent window the silence rules name. **The spectator-ACTIONS rule the
  header always stated is now enforced by the library itself, on both ends**: `send_actions`
  refuses to stage them on a spectator connection (`LNK_BAD_ARGUMENT`), and the server half
  treats an ACTIONS frame arriving from a spectator as a protocol violation that shuts the
  connection (`LNK_FRAME_REFUSED`) — the CS:GO coaching lesson, enforced in the one shared
  implementation where it cannot drift. The write buffer gained a **high-water mark** (one
  megabyte — hundreds of full-world ticks — then `LNK_IO` and the connection is over, because
  an unbounded buffer is an allocation the peer controls). Two refusals grew honest words: a
  fingerprint mismatch at the *same* version now says "one of us carries a modified
  lnk_protocol.h" instead of naming two equal numbers at each other, and a TICK_STATE header
  disagreeing with its own rows is refused as the `CountRowsMismatch` it is rather than
  mislabelled as a cap violation. `listen`'s IPv4-only loopback is now stated in its
  documentation rather than discovered by an IPv6-preferring resolver. Thirty-five tests; four
  new refusals each broken deliberately once, red exactly where they should be.

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

- **The server half, and the first real protocol bump.** The vtable grows Master Control's end
  of the wire — `listen` (127.0.0.1 only while the trust stance holds; port 0 asks the
  operating system and `server_port` answers which), `accept` (one knock if somebody knocked,
  the whole handshake walked with refusals in words, `LNK_NOTHING_YET` when nobody is waiting),
  `send_welcome`, `send_tick_state`, `send_event` and `send_derez` — at client ABI version 2,
  with version 1 refused as history. It arrived earlier than the plan said because its first
  consumer turned out to be the flagship spectator's own test rather than Master Control: a
  spectator needs somebody to talk to, and a hand-written test server would be the second
  implementation this organisation forbids; Master Control simply inherits the half ready-made.
  `send_tick_state` judges the caller's declared count against the cap *before reading a single
  row* — the wire's validate-before-copy rule applied to our own caller with exactly the trust
  a stranger would get. And `LNK_DEFAULT_PORT` landed in the protocol header: **30702**, from
  JA-307020 — Tron's designation, the security program guarding the doorway into the Grid —
  taking the protocol to **version 2**, the fingerprint flow's first exercise in anger, with a
  new twin test pinning version and port between the header and the Rust mirror. Thirty tests;
  three discriminating breakage rounds (a no-knock that answered OK, an inverted count gate, a
  version gate that let history in) — plus one breakage round of my own restoration discipline,
  when an unasserted restore left a round applied and the suite caught it immediately.

- **The CMake face: a consumer never learns cargo exists.** `CMakeLists.txt` at the root is the
  face Link shows a CMake consumer — `add_subdirectory()` it and receive exactly three things:
  the header target `lnk`, the residence-rule function `lnk_copy_beside(<target>)` (an ALL
  target per consumer rather than POST_BUILD, which only fires on relink and would leave a
  fresh Link beside a stale executable), and the `LNK_LIBRARY_FILE`/`LNK_FINGERPRINT_FILE`
  paths for tests. `project(Link LANGUAGES NONE)`, deliberately: consuming Link enables no
  compiler in the consumer's build, and the owner's aim is exact — the flagship consumes the
  wire as though it were just another shared library, oblivious to how it is made. The face
  carries its own `CMakePresets.json` in the flagship's manner, sized to what the face is: one
  configure preset, one build preset, one workflow, so `cmake --workflow --preset default` is
  the whole standalone ceremony — and it is not a side check but the pipeline itself: the CI
  build jobs on both platforms and the release workflow all build Link through the face, the
  direct cargo build step retired, so what CI proves and what a consumer runs are one path.

- **Etape 4: the C ABI surface — the library a foreign runtime loads.**
  `include/lnk/lnk_client.h` declares it and `src/abi.rs` implements it: one exported symbol,
  `lnkGetClientVTable`, returning the table for its own version and NULL for any other — the
  flagship's `tglGetProgramVTable` refusal reproduced, with `vtable_bytes` and `abi_version` as
  the first two members. Behind the table: connect (the whole handshake; a server's refusal
  arrives as words in the caller's buffer), poll (message views, with TICK_STATE rows borrowed
  until the next poll — the Program ABI's borrow rules), send_actions, send_ping, send_pong,
  flush, close. Every function wraps in `catch_unwind` and answers with a status code — no
  panic crosses the boundary, and `connect` pre-writes `LNK_PANIC` into the status so even a
  caught unwind leaves the truth behind. Null pointers earn `LNK_BAD_ARGUMENT` rather than a
  dereference. `unsafe` entered the crate exactly as the plan said it would: the manifest
  relaxed from forbid to deny, `src/abi.rs` alone allows it, and every unsafe block carries a
  SAFETY comment naming the contract that licenses it. The header's constants are pinned to the
  Rust constants by a test that parses the header itself — cross-language twinning with no C
  compiler — and the built `link.dll` was loaded through Python's ctypes to prove the export,
  the vtable's 80 bytes and the version refusal from a genuinely foreign runtime. Twenty-five
  tests; three discriminating breakage rounds (a drifted status constant, a version gate that
  accepted strangers, a refusal laundered into an io error) — plus one stale-binary catch by
  the house rule itself, when an Etape-1-era DLL nearly stood in for the fresh one. The server
  half of the surface — listen and accept — waits for Master Control's consumer etape.

- **The flagship's release workflow, adopted.** Tag-triggered in the same shape:
  the tag's version must match `Cargo.toml` before anything builds; the release build runs the
  full test suite and the fingerprint check before anything is signed, because a tag can be
  pushed from any commit and an attestation must never vouch for an untested artefact; the
  artefact is the loaded contract itself — `link.dll`/`liblink.so`, the C header, the recorded
  fingerprint, plus README, LICENCE and CHANGELOG — with SHA-256 checksums, build provenance
  attested, and release notes extracted from this file. Same pinned GitHub-owned actions, same
  gh-CLI publishing.

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
