# TODO

The wire's design authority is the flagship's
[docs/TOPOLOGY.md](https://github.com/ai-quokka-wannabe/tron-grid-lite/blob/main/docs/TOPOLOGY.md)
— the v1 protocol section and the four decisions taken. The etapes here implement it; design
changes go there first.

## Etape 1 — the wire contract

**Done, except the one message that needs a consumer to design against.**
`include/lnk/lnk_protocol.h` pins the framing (`u16 length | u8 type`, an exact-size rule for
fixed messages, caps checked before any copy), nine of the ten messages as no-padding PODs with
sum-of-members asserts, and `LNK_PROTOCOL_VERSION` guarded by `tools/check_protocol_version.py`
in CI — the flagship's ABI discipline, wholesale. `src/protocol.rs` mirrors every struct and
pins the same sizes, so the two languages cannot drift without one refusing to build. Broken
deliberately once each, all discriminating: the Rust size pin, the unbumped-header refusal, the
same-version update refusal and the stale-fingerprint refusal. REZ's number is reserved and its
payload is Etape 6 below.

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

`std::net` TCP with `TCP_NODELAY`, the fingerprint handshake refusal, and the tick-stamped
latest-wins delivery semantics.

## Etape 4 — the C ABI surface

The narrow exported surface — open, poll, send, close, over byte buffers — with `catch_unwind`
at every boundary and error codes rather than unwinding, packaged as the `cdylib` both consumers
load. This is the etape where `unsafe` enters the crate, narrowly, and where the manifest's
`unsafe_code = "forbid"` relaxes to a per-module allowance.

## Etape 5 — the first consumer

tron-grid-lite consumes the built library and header. The seam extractions on its side are its
own TODO's business; the wire only has to be worth consuming. This is also the etape where the C
header is first compiled by a C++ toolchain, which is when its static asserts first actually
run — until then the Rust mirrors carry the layout claims alone, which is a known, deliberate
gap.

## Etape 6 — the REZ payload layout

The one message the contract still owes. REZ carries a creature's identity, descriptor and
render model, and the descriptor is the flagship's `TglCreatureDesc` — which carries pointers:
per-eye sample-direction and acceptance arrays, per-ear band edges. Flattening that into
counted, capped, no-padding wire sections is real design, and it is designed against the
flagship's validator as its first consumer rather than guessed here: the descriptor validation
and `copyValidatedModel` already state what a well-formed creature is, and the wire layout must
be exactly what they accept, or the refusal happens at the wrong end of the wire. Until it
lands, an end that receives REZ refuses it as unknown, which is honest.
