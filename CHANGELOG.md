# Changelog

All notable changes to link are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

- **Rust 1.98.0.** The pin in `rust-toolchain.toml` moves from 1.95.0 (the Tool Updates watcher's
  #29); rustfmt, clippy, the tests and rustdoc are clean under it without a change to the code.
  The guides say 1.98.0 where they said 1.95.0.
- **The servos on the wire: protocol v9, client ABI 9.** The owner's third ruling (the undulation
  must propel) and Master Control's Etape 8 make a chained body a row of servos at its pivots;
  the gait is the creature's, so the wire carries it. `REZ` declares a body's servos by bound -
  `max_joint_angle` (radians, what a servo is asked to hold at most) and `max_joint_torque`
  (newton-metres) - and a bound of zero is no such actuator, the Program ABI's own rule, so a
  worm declares servos and no velocity actuator while a point proxy declares the reverse;
  finite, non-negative, and zero or set together, refused by name otherwise both ways
  (`RezServoInvalid`). `ACTIONS` carries `joint_targets[7]` - the angle each servo is asked to
  hold, joint k between segments k and k + 1 - and their tick-1 resend beside the speed, turn
  and voice it carried (96 bytes; the reserved word moves to the end). Nothing is retired: a
  body honours only the actuators it declared. Companions: master-control reads the targets and
  the bounds (its gait bridge retires); the flagship's host relays them and its roster declares
  the bounds per body; rc-worm brings its own gait.
- **The copies beside a consumer no longer collide.** `lnk_copy_beside()` gives every consumer
  its own copy of the library, ordered after that consumer and after cargo - and consumers that
  share an output directory, a repository's test executables typically, therefore had two of
  those writing the very same destination file with nothing ordering them against each other. A
  build system worth having runs them at the same moment, and one of them fails: `Error copying
  file (if different)` on the flagship's verify leg (2026-08-27), where three test executables
  share `src/tests/Release`. The copies are now chained - each waits for the one before it - so
  only ever one is in flight; each is a file compare and costs nothing. The guide's account of
  the face is corrected with it: the face probes no compiler and nothing here compiles the
  headers - `cargo test` parses them.
- **The guides.** The owner's ask (2026-08-27): every repository of the organisation gets a
  development-environment guide a contributor can follow without struggling. Here:
  `docs/DEV_ENV_SETUP.md` - the short version, the pins (Rust 1.95.0 through `rust-toolchain.toml`,
  a linker, nothing else), Windows and Linux step by step, what CI runs and how to run every leg at
  home, the rules for changing the wire (bump, re-record the fingerprint, mirror, refuse both ways,
  open the companions), troubleshooting. CONTRIBUTING and the README no longer say 1.85 and point
  at the guide and at the flagship's `RUNNING_THE_GRID.md`.
- **A refusal, by name, on the wire: protocol v8.** Until now a host whose `REZ` the world
  refused learned of it only by never hearing its body relayed - the world refused by name in
  its own log and said nothing to the one it refused. `REFUSED` (type 12, sixteen bytes) is
  the server's letter to that one host: the tick it was judged at, the creature the `REZ`
  named, and the reason by name - `LNK_REFUSED_OWNED` (another host wears the identity),
  `FULL` (no room for one more row), `CROWDED` (no spot on the spawn lattice), `BOUNDS` (a
  bound or the mesh outside what the world allows). Zero and anything unnamed are refused by
  the codec both ways, and a refused encode writes nothing; a client that sends one is hung
  up on as the wrong way, like a letter. The C ABI gains `send_refused` on the vtable and
  `refused` in the message view, so `LNK_CLIENT_ABI_VERSION` is 8. Companions: master-control
  sends it where its roster refuses; the flagship's host says it out loud and stops waiting.
- **The chain, on the wire: protocol v7.** The owner's ruling (2026-08-26): a worm is a chain
  of icosahedra joined spike to spike, and it undulates. The wire's part: a `REZ` names how
  many segments its creature has (`segment_count`, the head counted, one to
  `LNK_SEGMENTS_MAX` = 8) and how far apart their origins sit along the head's path
  (`segment_spacing`, zero for a single body, strictly positive and finite otherwise); a
  `TICK_STATE` row carries, after the head's pose, velocity and voice, the chain's count and a
  fixed array of seven `LnkSegmentPose` (position and yaw) - so a row is always 156 bytes and
  every consumer keeps copying rows by count, the Disk and its reader included. The slots
  beyond the chain are zero and a nonzero one is refused, because a tick's bytes are hashed
  and recorded; every float in a row, the head's included, must now be finite. Refused by name
  both ways: a chain of none or of nine, a spacing that lies, a NaN pose, a dirty slot. A full
  tick is 39,952 bytes - it fits one frame, and a maximal REZ still outweighs it. The client
  ABI moves to 7 with the structs it passes; the fingerprint moved with the version, so every
  Disk of v6 is history. The world's part (trailing segments placed along the head's recorded
  path), the Grid's (a model per segment pose) and the worm's (the two joint spikes) are the
  companion pull requests.
- **The toolchain is pinned in one place, and CI says so.** Adopted from the owner's
  `setonix-os`: `.github/scripts/check-toolchain-pin.sh` in quick-checks refuses a
  `rust-toolchain.toml` that floats and any workflow that installs a toolchain of its own - a
  second source of truth for the compiler version is the drift that leaves everything building.
  Its first run found one: the release workflow updated to floating stable before building -
  harmless only because rust-toolchain.toml overrides it inside the tree - and now installs the
  pin like every other job.
- **A wire change names its companions.** The pull request template asks, for every change to
  a message, a cap or `lnk_client.h`, for the companion pull requests in the three consumers
  (or the stated reason a consumer needs none) - the lightweight way to review a protocol change
  and its consumers together, adopted from the owner's `arm-cortex-mx-core-tests`.
- **`/check-coherence`.** A documentation audit for contradictions between clauses that were
  each right when written, orphaned claims about the tree, facts stated twice against the
  single-source-of-truth table, scope drift and stale "today" sections - and one that is willing
  to conclude the documents are coherent. Adopted from the owner's `setonix-os`; the same file
  in every repository of the organisation.
- **The pins Dependabot cannot see are watched weekly.** Adopted from the owner's `arm-dev-env`:
  `tool-updates.yml` reads each pinned tool version out of the tree, resolves the latest
  release from the tool's own feed, and opens one tracking issue per tool that is behind -
  edited on later runs, closed by itself when the pin catches up. An issue, not a pull
  request: a bump is installed on the desk and its checksum re-recorded, a decision rather
  than a merge button.
- **The markdown linter is pinned and every job has a timeout.** Adopted from the owner's
  `arm-cmake-toolchains` and `claude-chats-browser`: `package.json` + `package-lock.json` pin
  markdownlint-cli2 to the byte, `npm ci` installs exactly that, the cache is keyed on the lock
  file, and Dependabot proposes the bumps - a lint run is reproducible and a new linter release
  can no longer redden an unrelated pull request. Every job carries a `timeout-minutes`, so
  nothing can hang for the six-hour default.
- **A release can be rehearsed, is gated three ways, is checked as shipped, and lands as a
  draft.** Adopted from the owner's `altium-designer-mcp` and `claude-chats-browser`, mirrored
  from the flagship. A manual dispatch builds, packages and checks every artefact as a tag would,
  as `<version>-dryrun`, and publishes nothing. On a tag, `validate` refuses a tagged commit not
  on main, a CHANGELOG without a section for the version, and a release that already exists.
  Each archive is unpacked into a clean directory and every file the contract needs is present
  by name - which is how `lnk_client.h`, the vtable header a consumer compiles against, was
  found missing from the artefact: it ships now, beside `lnk_protocol.h` and the fingerprint,
  and the fingerprint in the archive must be the one this build recorded. The release job
  publishes only when both platforms' artefacts arrived and the checksums read back clean, as
  a draft with the publish command in the step summary.

- **Every internal link and anchor is checked per pull request, the external ones weekly.**
  Adopted from the owner's `altium-designer-mcp`: `lychee --offline --include-fragments` in
  quick-checks, installed from its pinned release with a checksum rather than through a
  third-party action, so a dead anchor is a red pull request; and `links.yml`, a scheduled
  workflow that follows the external links too, never blocking a merge on a site elsewhere.
- **The toolchain is pinned, the lock file is honoured, the docs must build clean, and main
  caches the build.** Adopted from the owner's `altium-designer-mcp`: `rust-toolchain.toml`
  pins rustc 1.95.0 with rustfmt and clippy, locally and in CI alike, so a new release never
  turns a green build red on its own timetable; every cargo step runs `--locked`; `cargo doc
  --document-private-items` runs with warnings as errors in quick-checks - and found two doc
  links to private items at once; and main saves a cargo cache that pull requests restore.
- **Nine findings of a bug hunt, fixed, the low and the cosmetic included.** The TCP connect
  itself is now bounded by the handshake timeout (`connect_timeout` per resolved address), as
  the header always claimed; a zero timeout is `LNK_BAD_ARGUMENT` at both ends instead of an
  I/O error that, at the server, dropped an already-accepted client without a word. The
  write-buffer limit judges what the peer left *unread* after the socket took what it would,
  so a late joiner told every body at once is a big tick and not a dead peer - before, a
  batch over a megabyte failed at once and forever. `close` gives the farewell the whole
  window, blocking, rather than one non-blocking try that could cut half a frame and the BYE
  and turn a leave into a crash. A refusal drains what a wrong client already sent before it
  closes, so the words are not reset away under an HTTP request. A frame the codec refuses
  shuts the socket, as the header says. The refusal read in `connect` is bounded by the
  timeout as a whole, not per byte. The Disk's byte count is what actually reached the file,
  error or not. The detail line never cuts a UTF-8 sequence. The header's words follow:
  which messages flow which way, what `LNK_REFUSED` also means for a Disk path, a Disk that
  could not take its BYE, the three variable-size messages, and the creature host hearing
  EVENTs.
- **A flake pinned and fixed: the handshake no longer times out before it waited.** The
  listener is non-blocking and, on Windows, an accepted socket inherits that; a server whose
  `accept` ran before the client's HELLO bytes had landed read WouldBlock and reported a
  handshake timeout that never elapsed - once in a dozen runs. The accepted socket now blocks
  for the handshake, bounded by the timeout as promised, and a test with a client that
  hesitates 200 ms after dialling makes the old behaviour fail every time. Beside it, the
  bounded connect retries a refused or reset dial within its budget, so a client dialling a
  server still standing up waits its timeout rather than failing at the first SYN.

- **Every message flows its own way only.** The letter already did; now the rule is whole. A
  WELCOME, TICK_STATE, EVENT or PROPRIOCEPTION arriving at the server, or a HELLO or ACTIONS
  arriving at a client, ends the connection before the consumer sees it -
  `TransportError::WrongWay(name)`, `LNK_FRAME_REFUSED` with the name in the detail - because
  an interval refuses both directions of nonsense and an end that speaks the other's words is
  not a peer. Master Control's adversarial suite found the gap: a raw citizen could send the
  world a WELCOME and be quietly ignored. No wire change; the fingerprint stands. Tested at
  both ends for every one of the six; one breakage round (EVENT let through) caught.

- **Protocol version 6: the contact knows its face, and a slide makes a sound.** `LnkContact`
  grows what the exact-contacts ruling named: the face's unit normal (world frame), the depth
  the body stood past it, and the slip - the body's velocity along the face, body frame -
  fifty-two bytes, thirteen floats all judged finite both ways. `LNK_EVENT_SCRATCH` (2) is the
  new event kind: a body sliding along a face - the floor, a riser, another body - sounds from
  the contact point, its strength the slip against the normal impulse; footsteps are scratches.
  The message shapes and the ABI are otherwise unchanged (ABI stays 6); versions 1 to 5 are
  refused as history.
- **A Disk: a client whose socket is a file (ABI v6).** The state log the topology owes is not a
  second encoder. `record_open` opens a recording - a server-held end with no peer; every
  `send_*` stages a frame, `flush` writes them all, `poll` hears nothing, `close` writes `BYE` -
  and Master Control feeds it from the same per-subscriber loop as every citizen, so the file
  holds what was said in the wire's own bytes. `replay_open` opens it as a client-held end whose
  `poll` yields the frames in order and whose end of file is `LNK_PEER_CLOSED`; its header -
  protocol fingerprint, world fingerprint, start tick, dt, start time - is judged as a handshake
  is, in the same words (another contract, another world), and `send_*` on a replay is refused
  because a replay has nobody to talk to. The owner named it: a recording is a *Disk* (`.disk`,
  after the identity disc that holds everything a program has done and seen), and the program
  that reads Disks back is *Clu*. The wire itself is unchanged - protocol v5, fingerprint kept -
  so a v5 world and a v6 library agree on every frame. Three breakage rounds discriminated.
- **The letter speaks body frame - the header now says so.** `LnkProprioception`'s specific
  force and `LnkContact`'s position and impulse were documented as world space; the physics
  produces them in the body frame, exactly as the Program ABI's `TglSenses` and `TglContact`
  hand them to a brain, and that is what Master Control relays. Words only - no byte moved -
  so the fingerprint is re-recorded at version 5 rather than bumped.
- **An orderly release: `close` lets the farewell land.** `close` used to send `BYE` and slam
  both halves of the socket. A peer with a tick in flight then answered that tick with a TCP
  reset, and a reset discards the peer's unread receive buffer - `BYE` included - so a polite
  leave arrived at Master Control as a crash, and the creature was orphaned instead of
  derezzed. `close` now shuts only the writing half, drains what the peer still sends for
  `FAREWELL_WINDOW` (250 ms) or until the peer closes too, then releases. Found the honest way:
  the first per-owner letter made the server write often enough to lose the farewell. A test
  writes a full tick at a closing peer and requires the `BYE` to survive it.
- **Protocol version 5: proprioception is a letter, not a broadcast.** A creature host is owed
  what a spectator has no use for - the specific force its otolith reads, whether its feet
  touch the ground, and the tick's contacts. Rather than grow every `TICK_STATE` row, the
  ruling (TOPOLOGY.md § The protocol, 2026-08-22) adds `PROPRIOCEPTION`: a 32-byte header
  (`LnkProprioception`) and up to `LNK_CONTACTS_MAX` (16) `LnkContact` rows, sent by Master
  Control only to the connection that owns the creature, every tick after that tick's
  `TICK_STATE` - the first message composed per subscriber. The letter flows one way, and the
  library enforces it at both ends: `send_proprioception` refuses on a connection this end
  dialled, and the server half treats the frame arriving *at* the server as the same violation
  as a spectator's `ACTIONS` (`LNK_FRAME_REFUSED`, connection closed). Decode and encode refuse
  by name: a ragged length or a count over the cap at header time, a count disagreeing with
  the length, a grounded byte that is neither 0 nor 1, reserved bytes, any non-finite float.
  ABI v5 (`send_proprioception`, `LnkProprioceptionView`); versions 1 to 4 refused as history.
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

### Fixed

- **The cache cleanup runs itself.** `cleanup_caches.yml` was manual-only and had never been
  dispatched here, and its script knew only the Vulkan, markdownlint and CodeQL key families, so
  every `cargo-*`, `npm-*-lock-*` and `qt-*` generation fell into the never-delete fail-safe: a
  changed lock file, toolchain pin or SDK version left the old entry behind until GitHub's own
  eviction. Now, after every green `CI - Main Branch` run, the script prunes `refs/heads/main`
  to the newest entry per family - cargo, npm, npm-markdownlint, Qt (per kit), Vulkan SDK (per
  platform), CodeQL overlay base - and only there, so an open pull request's newer cache can
  never evict main's live one; a closed pull request's caches are reclaimed by a second job;
  the manual dry run stays. The pattern is the owner's arm-dev-env workflow. Verified by a dry
  run on the branch before merging; the same files land in all four repositories.
