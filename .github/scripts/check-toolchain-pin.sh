#!/usr/bin/env bash
# The toolchain is pinned in exactly one place. rust-toolchain.toml names the one rustc every
# desk and every runner builds with; the day a workflow installs a toolchain of its own - a
# third-party action, a `rustup toolchain install`, a curl of rustup - CI has a second source of
# truth for the version, and the pin's guarantee is gone silently, because everything still
# builds. Adopted from the owner's setonix-os (`check-toolchain-pin.sh`).
#
# Exit 0 when the pin is exact and nothing else names a toolchain; 1 with the offending lines.

set -u

status=0

if ! grep -qE '^channel = "[0-9]+\.[0-9]+\.[0-9]+"$' rust-toolchain.toml; then
    echo "::error::rust-toolchain.toml must pin an exact version: channel = \"x.y.z\" (a floating channel turns a green build red on somebody else's timetable)."
    status=1
fi

# Only the directories that exist: grep answers 2 for a missing one even when it matched
# elsewhere, and a check that mistakes "error" for "no match" is the silent kind of green.
places=()
for place in .github/workflows .github/actions; do
    if [[ -d "$place" ]]; then
        places+=("$place")
    fi
done

# Un-anchored on purpose: the install idiom usually sits mid-line inside a `run: |` block.
# setup-node is not a toolchain here - it serves the pinned markdown linter - so it is not listed.
pattern='dtolnay/rust-toolchain|actions-rs/(toolchain|cargo)|actions/setup-(python|go|java|dotnet)|rustup toolchain install|rustup update|rustup (default|override set)|sh\.rustup\.rs|rustup-init'
if grep -rnE "$pattern" "${places[@]}"; then
    echo "::error::A workflow names a toolchain of its own (above). rust-toolchain.toml is the only place a compiler version lives; rustup reads it on first use."
    status=1
fi

if [[ $status -eq 0 ]]; then
    echo "The toolchain is pinned once: $(grep -E '^channel' rust-toolchain.toml)"
fi
exit $status
