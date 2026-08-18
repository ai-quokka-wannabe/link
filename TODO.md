# TODO

The wire's design authority is the flagship's
[docs/TOPOLOGY.md](https://github.com/ai-quokka-wannabe/tron-grid-lite/blob/main/docs/TOPOLOGY.md)
— the v1 protocol section and the four decisions taken. The etapes here implement it; design
changes go there first.

## Etape 1 — the wire contract

The `#[repr(C)]` message structs — HELLO, WELCOME, REZ, TICK_STATE, ACTIONS, EVENT, DEREZ, PING,
PONG, BYE — with the `u16 length | u8 type` little-endian framing constants and per-type length
caps, published as a C header with layout asserts on both sides and a fingerprint tool in the
flagship's `program-abi` manner: a changed layout at an unchanged version is refused by the
check, not by a reviewer's memory.

## Etape 2 — the codec

Encode and decode against hostile bytes: caps checked before any copy, every decode a total
function that returns refusal rather than trusting a field. Red-first, in the house manner: the
breakage round feeds the decoder truncated, oversized and misdeclared frames and watches it
refuse each one before the happy path is believed.

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
own TODO's business; the wire only has to be worth consuming.
