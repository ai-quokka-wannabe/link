# Development Environment Setup

How to build and test Link - the wire of the Grid - from nothing, on Windows or Linux, exactly
as CI does, and how a change to the wire reaches the three repositories that load it. To run the
whole ecosystem, see the flagship's
[RUNNING_THE_GRID.md](https://github.com/ai-quokka-wannabe/tron-grid-lite/blob/main/docs/RUNNING_THE_GRID.md).

---

**The short version**, for someone who has done this before:

```text
git clone https://github.com/ai-quokka-wannabe/link.git
cd link
cargo test --locked
python tools/check_protocol_version.py check
```

That needs rustup and a C toolchain for the linker, nothing else. Everything below is the long
version.

---

## Prerequisites

| Tool | Version | Where to get it |
|------|---------|-----------------|
| rustup | any recent; **Rust 1.98.0** is pinned by `rust-toolchain.toml` and installed by rustup on first use | <https://rustup.rs/> |
| A linker | Windows: the Visual Studio "Desktop development with C++" workload or its Build Tools; Linux: `build-essential` | <https://visualstudio.microsoft.com/downloads/> · `sudo apt install build-essential` |
| Git | any recent | <https://git-scm.com/downloads> |
| Python | 3.10 or newer, for `tools/check_protocol_version.py` | <https://www.python.org/downloads/> |
| CMake + Ninja | 3.25+ and any Ninja, **only** to exercise the CMake face consumers use | <https://cmake.org/download/> |
| Node.js | 20 or newer, only for the markdown linter (`npm ci`) | <https://nodejs.org/> |

**There are no crates.** Link is `std` only: the protocol, the codec, the transport, the C ABI
and the Disk recorder are all in the standard library's vocabulary. There is no `cargo install`
step and no toolchain to choose - `rust-toolchain.toml` names `1.98.0` with `rustfmt` and
`clippy`, rustup installs exactly that the first time cargo runs here, and CI refuses any
workflow that installs a toolchain of its own. No submodules either: Link is the leaf every
other repository points at.

---

## Windows

Any shell after installing rustup and the Visual Studio C++ workload (or Build Tools):

```text
git clone https://github.com/ai-quokka-wannabe/link.git
cd link
cargo build --locked
cargo test --locked
python tools/check_protocol_version.py check
```

The first `cargo` run installs Rust 1.98.0. `cargo build --release` puts the library at
`target/release/link.dll` (`liblink.so` on Linux) beside the C headers in `include/lnk/`; nothing
runs it by itself - Master Control and the Grid load it from beside their own executables.

## Linux (Ubuntu / Debian)

```text
sudo apt update && sudo apt install -y build-essential git python3
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh     # then open a new shell
git clone https://github.com/ai-quokka-wannabe/link.git
cd link
cargo build --locked
cargo test --locked
python3 tools/check_protocol_version.py check
```

---

## Testing, exactly as CI does

On both platforms, on every pull request and on `main`:

| Step | Command | What it holds |
|------|---------|---------------|
| Format | `cargo fmt --check` | rustfmt, with `rustfmt.toml`'s 170-column limit |
| Lint | `cargo clippy --locked --all-targets -- --deny warnings` | `clippy::all` as errors |
| Docs | `cargo doc --locked --no-deps --document-private-items` | Every doc comment, warnings as errors |
| Tests | `cargo test --locked` | The codec both ways, the transport over real loopback sockets, the C ABI through its own vtable, the Disk written and replayed |
| The fingerprint | `python tools/check_protocol_version.py check` | The recorded fingerprint of `lnk_protocol.h` still describes the header - so a change to the wire cannot land without its version bump |
| The CMake face | `cmake --workflow --preset default` | What a consumer sees: cargo driven by CMake, on both platforms, so the face cannot rot. It is `LANGUAGES NONE` and probes no compiler; the copy-beside rule it offers is exercised by the consumers that call it |
| Toolchain pin | `.github/scripts/check-toolchain-pin.sh` | One pin, no workflow installing its own |
| Markdown | `npm ci && npm run lint:md` | The pinned markdownlint-cli2 |
| Links | lychee | Every link in the tree |
| CodeQL | Rust, C/C++ (the headers), workflows | Read an alert; close it by code |

Run the first six before opening a pull request.

---

## Changing the wire

Link is the contract every other repository is written against, so a change here has rules of
its own:

1. **Bump the version when the bytes change.** Any change to a message's layout or meaning bumps
   `LNK_PROTOCOL_VERSION` in `include/lnk/lnk_protocol.h` and its mirror `PROTOCOL_VERSION` in
   `src/protocol.rs`; any change to the vtable or the message view bumps
   `LNK_CLIENT_ABI_VERSION` in `lnk_client.h` and `src/abi.rs`. There is no compatibility to
   keep - 0.0.0 - only refusals to keep honest: two ends that disagree say both versions and hang
   up.
2. **Re-record the fingerprint.** `python tools/check_protocol_version.py update` re-records the
   hash of the header (with the version line removed) and refuses unless the version moved when
   the content did. CI's `check` compares.
3. **Mirror it in Rust, and pin it.** Every struct is `#[repr(C)]` with a `const` size assert in
   `src/protocol.rs`, and the header carries `LNK_STATIC_ASSERT`s for the same sizes and for the
   absence of padding; `cargo test` parses the headers themselves for the twin constants, so the
   two cannot drift apart unnoticed.
4. **Refuse by name, both ways.** The codec refuses on decode everything it refuses on encode,
   and a refused encode writes nothing; an unknown reason, kind, role or type is a named error,
   never a shrug. Add the test to the roundtrip list and to the refusals.
5. **Open the companions.** The pull-request template asks for them: master-control (its
   `src/link_dll.rs` mirrors the headers and the twin tests parse them; its submodule pointer
   moves), tron-grid-lite (the same), and rc-worm only if the Program ABI moved. Each consumer's
   submodule may point at this branch's commit before the merge - it is reachable from `main`
   after.

A change that adds a message is a good pattern to copy: protocol v8's `REFUSED` touched the
enum, the struct and its asserts, the payload-size table, decode and encode, the wrong-way
table in the transport, the C headers, the vtable and the view union, and the tests for each.

---

## Editing

VS Code with rust-analyzer (`.vscode/`), which reads the pinned toolchain from
`rust-toolchain.toml`. Format on save with rustfmt; set rust-analyzer's `checkOnSave` to
`clippy` to see the gate inline. Markdown is linted by the pinned markdownlint-cli2 (`npm ci`,
then `npm run lint:md`); British English throughout.

---

## Troubleshooting

### `cargo` says a version other than 1.98.0

Another Rust is in front of rustup on your `PATH`. Put rustup's `~/.cargo/bin` first, or remove
the other; never edit the pin to match a machine.

### `linker 'cc' not found` / `link.exe not found`

A C toolchain is needed to link the library: the Visual Studio C++ workload or Build Tools on
Windows, `build-essential` on Linux.

### `check_protocol_version.py` refuses: the fingerprint is stale

The header changed and the version did not, or the version moved and the fingerprint was not
re-recorded. Bump the version if the bytes changed, then `update`; the tool refuses an update
without a bump on purpose.

### A transport test times out

They use real loopback sockets with a 30-second budget. A firewall that blocks loopback, or a
machine under heavy load, is the usual cause; `cargo test -- --test-threads=1` helps a loaded
laptop.

### The CMake face cannot find cargo

`cmake --workflow --preset default` drives cargo from CMake; rustup's `~/.cargo/bin` must be on
the `PATH` of the shell that runs CMake.

---

*See `CONTRIBUTING.md` for the pull-request workflow and `README.md` for what the wire is.*
