# link

**Link** — the wire of the Grid. Link is the library's official name, capitalised the way Master
Control is; `link`, lowercase, names only this repository.

One protocol library that Master Control and every TronGrid Lite instance load as the same
shared binary — Rust behind a plain C ABI — so the two ends of the wire cannot drift apart:
there is no second implementation to disagree with the first.

## The Four Repositories

link is one of four repositories in the [ai-quokka-wannabe](https://github.com/ai-quokka-wannabe)
organisation, and it is the contract between two of them.
[tron-grid-lite](https://github.com/ai-quokka-wannabe/tron-grid-lite) is the Grid — the renderer,
the senses and both client roles; [master-control](https://github.com/ai-quokka-wannabe/master-control)
is the world server the clients answer to; [rc-worm](https://github.com/ai-quokka-wannabe/rc-worm)
is the first Program. Who owns what, and why every delegation is the way it is, lives in the
flagship's [docs/TOPOLOGY.md](https://github.com/ai-quokka-wannabe/tron-grid-lite/blob/main/docs/TOPOLOGY.md)
— one table, kept in one place, pointed at from everywhere. That document is also the wire's
design authority: design changes land there before code lands here.

## The Doctrine

- **Rust, and `std` only — zero third-party crates.** The parser is the attack surface: the
  named disasters in the audit behind TOPOLOGY.md were memory-safety bugs in C++ packet parsers,
  so the one component that eats hostile bytes is the organisation's one memory-safe-language
  component. `std::net` carries TCP, and the framing is hand-written plain old data.
  Rust-the-language, not Rust-the-ecosystem.
- **A narrow C ABI.** Both consumers are C++, so the boundary is `#[repr(C)]` structs published
  as a C header, layout-asserted on both sides and fingerprinted in the flagship's `program-abi`
  manner.
- **No panic crosses the boundary.** Every exported function catches unwinding at the edge and
  returns an error code — the flagship's noexcept doctrine in Rust clothing.
- **One binary rather than one source.** Master Control, the creature host and the spectator all
  load the same `link` library, so a protocol drift between them is unrepresentable.

## Building

```bash
cargo build --release
cargo test
```

Rust stable 1.85 or later (edition 2024), via [rustup](https://rustup.rs/). Nothing else.

## Where It Stands

The contract is pinned: `include/lnk/lnk_protocol.h` carries the framing, nine of the ten
messages as no-padding PODs, and the fingerprint the handshake refuses mismatches with —
mirrored field for field and size-asserted again in `src/protocol.rs`, guarded in CI by
`tools/check_protocol_version.py`. The codec turns frames into messages by refusal, and the
transport carries them over TCP — a mismatched contract is refused at the handshake in words a
human can read. The built library exports one symbol, `lnkGetClientVTable`
(`include/lnk/lnk_client.h`), behind which the whole client lives; no panic ever crosses it.
REZ's payload layout is deliberately still open: it flattens
the flagship's creature descriptor and is designed against that validator, not guessed here. The
roadmap lives in [TODO.md](TODO.md), the history in [CHANGELOG.md](CHANGELOG.md).

## Licence

Copyright © 2026 Matej Gomboc <https://github.com/ai-quokka-wannabe/link>.

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
GNU General Public License for more details.

See the attached [LICENCE](LICENCE) file for more info.
