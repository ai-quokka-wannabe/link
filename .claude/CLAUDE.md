# link

**Link** — the wire of the Grid: the protocol library that Master Control and every TronGrid
Lite instance load as the same shared binary, so the two ends of the wire cannot disagree about
what travels over it. Rust behind a plain C ABI. Link is the library's official name,
capitalised like Master Control; `link`, lowercase, names only this repository. The C prefix is
`Lnk`/`LNK_`, and the contract of record is `include/lnk/lnk_protocol.h`.

**Two facts that govern every decision in this repo:**

1. **This repository implements the wire; it never decides what the wire means.** The design
   authority for messages, framing, handshake and trust is `docs/TOPOLOGY.md` in
   `tron-grid-lite`, and design changes land there before code lands here. The sibling
   repositories (org `ai-quokka-wannabe`): `tron-grid-lite` — the Grid, the flagship whose
   conventions this repo mirrors; `master-control` — the world server; `rc-worm` — the first
   Program, paused until this repo and `master-control` have solid foundations. The being is
   **Master Control**; lower case names only its repository.
2. **The settings are mirrored from the flagship, deliberately.** Repository settings, rulesets,
   CI shape, lint configuration and governance files are copies of `tron-grid-lite`'s, kept as
   identical as the language difference allows — the owner wants them identical, not improved.
   When changing a mirrored setting, change it in the flagship too or not at all; a copy that
   drifts silently is the exact defect this organisation's static_assert culture exists to
   prevent.

## Rules

- **Language: Rust, edition 2024, stable toolchain only — and `std` only, zero third-party
  crates.** The parser is the attack surface, which is why this component is Rust at all;
  `std::net` carries TCP, and the framing is hand-written plain-old-data. A crate in
  `[dependencies]` is a design failure here. Rust-the-language, not Rust-the-ecosystem.
- **Warnings are errors**, denied in `Cargo.toml` `[lints]` rather than in CI flags, so local
  builds and CI agree. The flagship builds under `/WX` and `-Werror`; this is the same rule.
- **`unsafe` is denied in the manifest and allowed in exactly one module**: `src/abi.rs`, the C
  boundary, whose whole job is the unsafety. Every unsafe block there carries a SAFETY comment
  naming the contract that licenses it. Everywhere else, unsafe stays refused.
- **No panic crosses the C ABI, ever.** Every exported function catches unwinding at the
  boundary and returns an error code — the flagship's noexcept doctrine in Rust clothing, and
  its "the boundary is a `catch (...)` that nothing can be added outside of" pattern
  (`Logger::workerLoop`) is the shape to copy.
- **Spelling:** British English everywhere. The LICENCE file content is untouchable (legal
  document).
- **Vocabulary:** Tron terms, one word per concept — the Grid, Program, creature, User, Master
  Control, tick, senses, actions. The flagship's STYLE.md § Tron Naming is authoritative.
- **The CMake face is a published contract.** `CMakeLists.txt` exports exactly three things —
  the `lnk` header target, `lnk_copy_beside()`, and the two `LNK_*` path variables — and
  consumers build against them, so renaming or removing any is a breaking change to every
  consumer. Change them the way the wire is changed: deliberately, and with the flagship's
  consumption in view. The face stays `LANGUAGES NONE` and never grows compiler knowledge.
- **Formatting:** `cargo fmt` (`rustfmt.toml`, 170-column limit). Clippy clean.
- **Licence:** GPL v3-or-later.
- **Don't over-engineer.** Keep it simple. No abstractions until there's a concrete second use
  case.

## Building

```bash
cargo build --release
cargo test
cargo fmt --check
cargo clippy --all-targets
```

rustc 1.95.0 is installed on this machine via rustup. CI builds and tests on `ubuntu-latest` and
`windows-latest`; the runners' preinstalled toolchains are updated to current stable at the start
of every run, so a warning new to stable appears in CI at the same moment it appears locally.

## Process

- **Main is protected: PR + review, direct pushes rejected.** Branch, push, `gh pr create`; the
  owner merges. The `main` ruleset requires the `CI Success` check, signed commits, code-owner
  review and resolved threads — it is a byte-identical copy of the flagship's.
- **Actions policy: GitHub-owned actions only, SHA-pinned.** A single third-party action makes
  the workflow die with `startup_failure` and zero jobs. Never reintroduce one.
- **Red-first tests.** Build the check, watch it fail against the old code, then fix. Every new
  check gets broken deliberately once before it is trusted — a test that has never failed has
  not been tested.
- **Write the commit message to a scratchpad file and `git commit -F <file>`** — multi-line
  messages through PowerShell mangle quotes.
- The flagship's `.claude/CLAUDE.md` § Hard-won rules applies on this machine wholesale —
  especially: never edit files through PowerShell `Set-Content`/`Out-File`, use the editing
  tools rather than shell heredocs, and confirm the build succeeded before believing a test
  result.
