# TODO

The wire's design authority is the flagship's
[docs/TOPOLOGY.md](https://github.com/ai-quokka-wannabe/tron-grid-lite/blob/main/docs/TOPOLOGY.md)
— the v1 protocol section and the four decisions taken. The etapes here implement it; design
changes go there first.

## Etape 1 — the wire contract

**Done.** `include/lnk/lnk_protocol.h` pins the framing (`u16 length | u8 type`, an
exact-size rule for fixed messages, caps checked before any copy), all eleven messages as
no-padding PODs (REZ and PROPRIOCEPTION joined later, at v4 and v5) with
sum-of-members asserts, and `LNK_PROTOCOL_VERSION` guarded by `tools/check_protocol_version.py`
in CI — the flagship's ABI discipline, wholesale. `src/protocol.rs` mirrors every struct and
pins the same sizes, so the two languages cannot drift without one refusing to build. Broken
deliberately once each, all discriminating: the Rust size pin, the unbumped-header refusal, the
same-version update refusal and the stale-fingerprint refusal. REZ's payload was Etape 6 below,
since landed.

## Etape 2 — the codec

**Done.** `src/codec.rs`: pure functions, no I/O, no `unsafe` — every field read and written in
explicit little-endian, so nothing reinterprets memory. The before-any-copy gate is an API shape
(`payload_rule` and `check_length` answer from the three header bytes alone), strictness is
symmetric (encode refuses everything decode refuses, and a refused encode writes nothing),
reserved bytes must be zero, and the decoder never panics on any input — a bombardment test
proves it with deterministic junk at every length and type byte. Broken deliberately three
times, each discriminating: a silenced reserved-zero check, a swapped field pair in encode, and
an off-by-one at the exact cap boundary.

## Etape 3 — the transport

**Done.** `src/transport.rs`: `std::net` TCP with `TCP_NODELAY` on both ends, a timeout-bounded
blocking handshake — magic, HELLO, WELCOME, or a refusal in words, text-then-close, documented
in the header — then a non-blocking framed phase with no threads: `Connection::poll` returns
whole frames or nothing, judges type and length at the header before any payload byte is read,
and hangs up on anything the contract refuses. Writes coalesce into one flush per tick with
partial-write remainders carried. The build knows its own contract: the recorded fingerprint is
compiled in, `local_hello` carries it, `accept` compares against it. Latest-wins stays the
consumer's step rule; the transport's obligation — ordered, whole, tick-stamped frames — is
tested over real loopback sockets, including a frame dribbled one byte at a time and a hostile
length hung up at the header. Broken deliberately three times, each discriminating: an inverted
fingerprint comparison, a dropped partial-write carry, a forgotten partial header.

## Etape 4 — the C ABI surface

**Done, both halves; ABI version 7 today.** `include/lnk/lnk_client.h` declares the surface and `src/abi.rs`
implements it: one exported symbol, `lnkGetClientVTable`, returning NULL for any version but its
own — the flagship's `tglGetProgramVTable` refusal, reproduced — with `vtable_bytes` and
`abi_version` as the table's first members. Behind it: connect (the whole handshake, refusals
arriving as words in the caller's buffer), poll (message views with TICK_STATE rows borrowed
until the next poll, the Program ABI's own borrow rules), send_actions/ping/pong, flush, close.
Every function wraps in `catch_unwind` and answers with a status code; null pointers earn
`LNK_BAD_ARGUMENT` rather than a dereference. `unsafe` entered the crate exactly as planned: the
manifest relaxed from forbid to deny, and `src/abi.rs` alone allows it. The header's status
constants are pinned to the Rust constants by a test that parses the header itself — the
cross-language twin check, no C compiler required — and the built `link.dll` was loaded through
`ctypes` to prove the export and the version refusal from a real foreign runtime. The
**server half** arrived earlier than planned — listen, accept, server_port and the four
server-side sends behind the same vtable, at ABI version 2 — because its first consumer turned
out to be the spectator's own test rather than Master Control: a spectator needs somebody to
talk to, and a hand-written test server would be the second implementation this organisation
forbids. The listener binds 127.0.0.1 only while the trust stance holds, and
`LNK_DEFAULT_PORT` (30702, Tron's designation guarding the doorway) landed in the protocol
header with it — the first real protocol bump, version 2, the fingerprint flow exercised in
anger.

## Etape 5 — the first consumer

**Done.** tron-grid-lite consumes the built library and header (its `--window` and `--program`
roles both dial through it), and so does Master Control, as the server half; the C header is
compiled by three C++ toolchains in the flagship's CI, where its static asserts run for real.

## Etape 6 — the REZ payload layout

**Done (protocol v4).** REZ carries the creature's identity, its bounds and its render model as
counted, capped, no-padding sections - vertices, triangles and materials - validated whole at
decode: counts under their caps before any copy, every index inside its array, every float
finite. The creature descriptor's eyes and ears stay the host's business, because only the host
renders them. The reasoning that shaped it:

REZ carries a creature's identity, descriptor and
render model, and the descriptor is the flagship's `TglCreatureDesc` — which carries pointers:
per-eye sample-direction and acceptance arrays, per-ear band edges. Flattening that into
counted, capped, no-padding wire sections is real design, and it is designed against the
flagship's validator as its first consumer rather than guessed here: the descriptor validation
and `copyValidatedModel` already state what a well-formed creature is, and the wire layout must
be exactly what they accept, or the refusal happens at the wrong end of the wire.
