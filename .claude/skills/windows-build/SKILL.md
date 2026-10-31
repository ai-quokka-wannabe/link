---
name: windows-build
description: Build, test and format the org's code on the Windows desk - the Grid and rc-worm through CMake presets inside the Visual Studio developer environment, Master Control and link with cargo, clang-format and markdownlint at the pinned versions, the GPU checks on every device including lavapipe. Use before a commit, before a live world, or when a build or CI check fails on Windows only.
---

# Building on the Windows desk

Mirrored verbatim in link, tron-grid-lite, master-control and rc-worm (`.claude/skills/windows-build/`). Change all
four together. When a build trick is learned the hard way, add it to **Hard-won**.

## C++ (tron-grid-lite, rc-worm)

`scripts\build.bat <source dir> <preset> [install prefix]` finds Visual Studio with vswhere, calls `vcvars64.bat`
itself (a batch file is the reliable way: quoting vcvars through PowerShell or Git Bash breaks), configures, builds
Release, runs ctest and prints `BUILD-OK`.

- Build **both** Windows presets before a PR: `windows-msvc` and `windows-clang-cl`. Clang enforces
  `-Wmissing-field-initializers` under `-Werror` and MSVC does not: a new struct field must be named in every
  designated initializer, or CI's clang jobs fail where the desk passed.
- rc-worm into a Grid: `build.bat <rc-worm> windows-msvc <tron-grid-lite>\build\windows-msvc\src\Release`.
  The install runs `windeployqt` and `tools/check_deploy.py`; then `TronGridLite --list-programs` must say USABLE.
- Worktrees: build in a worktree under Temp, never in the clone the maintainer is working in; delete it after.

## Rust (master-control, link)

`cargo fmt --check`, `cargo clippy --locked --all-targets -- --deny warnings`, `cargo test --locked`. The toolchain
is pinned by `rust-toolchain.toml`; a bump is its own PR across both repos.

## Formatting and docs

- clang-format is the pip one at the major CI pins (`pip install "clang-format~=23.1"` in the workflows): find it with
  `python -c "import site;print(site.getuserbase())"` and its `Scripts\clang-format.exe`. Another clang-format on
  the PATH formats differently.
- `npx markdownlint-cli2 "**/*.md"` with the repo's own pinned version (`npm ci` first).
- Every new file - code, script, workflow, skill script - opens with the GPL notice naming
  "The AiQuokkaWannabe Authors", in the comment syntax of its language.

## Every GPU

Grid work must run on every device: the desk's integrated and dedicated GPUs, the modest cards in the GPU table of
tron-grid-lite's `.claude/CLAUDE.md`, and lavapipe. `TronGridLite --list-gpus`, `--gpu <index>`, then the three
`--verify-acoustics`, `--verify-scene`, `--verify-senses` on each. For lavapipe set `VK_ICD_FILENAMES` to its ICD
alone (tron-grid-lite `.claude/CLAUDE.md` names the variable and the path shape).

## Hard-won

- PowerShell 5.1 has no `&&`; Git Bash mangles `rev:path` and `/c/...` arguments - use PowerShell for those.
- An agent's builds and scratch go under `%TEMP%` and are deleted before reporting.
