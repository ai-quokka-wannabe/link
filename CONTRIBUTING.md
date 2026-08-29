# Contributing to link

Thank you for your interest in contributing to link! This document provides guidelines and information for contributors.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [How to Contribute](#how-to-contribute)
    - [Reporting Bugs](#reporting-bugs)
    - [Suggesting Features](#suggesting-features)
    - [Pull Requests](#pull-requests)
- [Development Setup](#development-setup)
- [Coding Standards](#coding-standards)
- [Commit Messages](#commit-messages)
- [Documentation](#documentation)

---

## Code of Conduct

This project adheres to the Contributor Covenant Code of Conduct.
By participating, you are expected to uphold this code. Please see [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) for details.

---

## How to Contribute

### Reporting Bugs

Before submitting a bug report:

1. Check the [existing issues](https://github.com/ai-quokka-wannabe/link/issues) to avoid duplicates
2. Ensure you're using the latest version
3. Collect relevant information:
    - Operating system and version
    - Rust toolchain version (`rustc --version`)
    - Steps to reproduce
    - Expected vs actual behaviour

When submitting:

- Use the bug report template
- Provide a clear, descriptive title
- Include minimal reproduction steps
- Include logs from both ends of the wire if relevant

### Suggesting Features

We welcome feature suggestions! Before submitting:

1. Check [existing issues](https://github.com/ai-quokka-wannabe/link/issues) and
   [discussions](https://github.com/ai-quokka-wannabe/link/discussions) for similar ideas
2. Consider that the wire's design authority is
   [`docs/TOPOLOGY.md`](https://github.com/ai-quokka-wannabe/tron-grid-lite/blob/main/docs/TOPOLOGY.md)
   in the `tron-grid-lite` repository — protocol design changes belong there before code changes belong here
3. Prefer the simplest thing that works — the project is pre-1.0 and owes nobody compatibility

When submitting:

- Use the feature request template
- Explain the problem you're trying to solve
- Describe your proposed solution
- Consider alternatives you've thought about

### Pull Requests

#### Before You Start

1. Open an issue first to discuss significant changes
2. Fork the repository
3. Create a feature branch from `main`
4. Make your changes following our [coding standards](#coding-standards)

#### PR Requirements

- [ ] Code compiles without warnings (`cargo build`, warnings are denied in the manifest)
- [ ] Code is formatted (`cargo fmt`)
- [ ] Clippy reports nothing (`cargo clippy --all-targets`)
- [ ] Documentation is updated if needed
- [ ] An entry added to `CHANGELOG.md` § Unreleased for anything user-visible
- [ ] Commit messages follow [conventional commits](#commit-messages)

#### PR Process

1. Submit your PR against the `main` branch
2. Fill out the PR template completely
3. Wait for CI to pass
4. Address any review feedback
5. Once approved, a maintainer will merge

---

## Development Setup

### Prerequisites

- [rustup](https://rustup.rs/) - Rust 1.98.0 is pinned by `rust-toolchain.toml` and installed on first use
- A linker: the Visual Studio C++ workload (or Build Tools) on Windows, `build-essential` on Linux
- Nothing else — the crate uses the standard library only, with zero third-party crates

### Setup

Quick start (prerequisites already installed):

```bash
git clone https://github.com/ai-quokka-wannabe/link.git
cd link

cargo build
cargo test
python tools/check_protocol_version.py check
```

The complete guide - the pins, what CI runs and how to run every leg at home, the rules for
changing the wire and its companions - is [docs/DEV_ENV_SETUP.md](docs/DEV_ENV_SETUP.md).

---

## Coding Standards

### Rust Style

- Follow `cargo fmt` formatting (`rustfmt.toml` sets the organisation's 170-column limit)
- Use edition-2024 Rust; the standard library only, with zero third-party crates
- Keep clippy clean — `[lints]` in `Cargo.toml` denies all warnings
- 4-space indentation, 170-column limit

### Naming Conventions

Follow the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/naming.html):
`snake_case` functions and modules, `PascalCase` types, `SCREAMING_SNAKE_CASE` constants.

What things in this world are called — the Grid, Programs, creatures, the User, Master Control,
ticks, senses and actions — is settled in the flagship's
[STYLE.md](https://github.com/ai-quokka-wannabe/tron-grid-lite/blob/main/STYLE.md) § Tron Naming,
which also records why *Program* and *programme* are both correct here and neither is to be
"corrected" into the other.

### Code Comments

- Add comments for non-obvious logic
- Keep comments up to date with code changes
- Use British spelling in all documentation and comments

### British Spelling

Use British spelling in all documentation and user-facing text:

| American | British |
|----------|---------|
| color | colour |
| behavior | behaviour |
| organization | organisation |
| center | centre |
| license (noun) | licence |
| analyze | analyse |
| initialize | initialise |
| optimize | optimise |
| meter | metre |
| synchronize | synchronise |

**Note:** Code identifiers may use American spelling where it matches library/API conventions (e.g., Rust standard library names).

---

## Commit Messages

We use [Conventional Commits](https://www.conventionalcommits.org/). Format:

```text
<type>(<scope>): <description>

[optional body]

[optional footer(s)]
```

### Types

| Type | Description |
|------|-------------|
| `feat` | New feature |
| `fix` | Bug fix |
| `docs` | Documentation only |
| `style` | Formatting, no code change |
| `refactor` | Code change that neither fixes a bug nor adds a feature |
| `perf` | Performance improvement |
| `test` | Adding or updating tests |
| `chore` | Maintenance tasks |
| `ci` | CI/CD changes |

### Examples

```text
feat(codec): add frame length validation

fix(transport): handle half-closed connections on Windows

docs: update README with build instructions

chore: update pinned actions
```

### Rules

- Use imperative mood ("Add feature" not "Added feature")
- Don't capitalise the first letter of the description
- No period at the end of the subject line
- Keep the subject line under 72 characters
- Reference issues in the footer: `Fixes #123`

---

## Documentation

### Types of Documentation

| Location | Purpose |
|----------|---------|
| `README.md` | User-facing overview and quick start |
| `CONTRIBUTING.md` | This file — contributor guidelines |
| `SECURITY.md` | Security policy and vulnerability reporting |
| `CHANGELOG.md` | User-facing change history |
| `TODO.md` | Roadmap and open etapes |

### Updating Documentation

- Update `README.md` for user-facing changes
- Record *why* a non-trivial change was made in its `CHANGELOG.md` entry, not only what changed
- Update code comments when changing public APIs
- Keep examples up to date and working
- Add an entry to `CHANGELOG.md` § Unreleased for anything user-visible

---

## Questions?

- Open a [Discussion](https://github.com/ai-quokka-wannabe/link/discussions) for questions
- Check existing issues and discussions first
- Be patient — maintainers are volunteers

Thank you for contributing!
