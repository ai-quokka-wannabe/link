---
name: org-pr-flow
description: The ai-quokka-wannabe way from a change to a pull request - a branch, signed commits written from a message file, the attribution lines, review by two independent reviewer agents, findings fixed, this commit's CI watched to green, and the merge left to the maintainer. Use whenever work in any org repo is ready to become a PR, or when a PR's CI fails or a review comes back.
---

# From a change to a merged PR

Mirrored verbatim in link, tron-grid-lite, master-control and rc-worm (`.claude/skills/org-pr-flow/`). Change all
four together, and add every process lesson to **Hard-won**.

## The rules

- `main` is protected: every change goes through a PR. Never push to `main`, never tag, never publish a release;
  releases happen only on the maintainer's word.
- The maintainer merges. Do not merge your own PR unless they said so for that PR.
- Commits are signed (the desk's git config does it; never pass `--no-gpg-sign` or `--no-verify`).
- A commit message ends with the agent's `Co-Authored-By:` line; a PR body ends with the
  `Generated with Claude Code` line. No personal details, secrets or private project names in anything public.
- Actions in workflows are GitHub-owned only (`actions/*`, `github/*`), pinned by commit.
- A change that crosses repos (the link protocol, the Program ABI) lands in dependency order:
  link, then master-control, then tron-grid-lite, then rc-worm - each PR names the ones it waits for.

## Steps

1. Branch from a fresh `main`: `git switch -c <topic>` after `git pull --ff-only`.
2. Make the change with its tests and docs (CHANGELOG, TODO, the repo's docs). Run the **windows-build** checks.
3. Commit from a file: write the message to a Temp file and `git commit -F <file>`; PowerShell here-strings and
   Git Bash quoting both mangle long messages.
4. Push and open the PR with `gh pr create --body-file <file>`: what changed, why, how it was tested, what is
   left honest-gaps style.
5. Review: two reviewer agents in parallel, each on a different angle (correctness and physics/realism;
   coherence, docs and style), given the diff and not your conclusions. Fix every finding that survives your own
   check, push, and answer each finding on the PR.
6. Watch **this commit's** run: `gh run list --commit <sha>` then `gh run watch <id> --exit-status`.
   `gh pr checks --watch` can return before the new checks register.
7. Code scanning: a CodeQL alert on an operator-chosen or temp path is judged (no `..` reaches it) and dismissed
   with that reason. Alerts reopen when line numbers move - re-dismiss after the shift.
8. Tell the maintainer the PR is ready, with its link and anything that needs their ruling.

## Hard-won

- Chained shell commands committed past a failing linter: check the linter said "0 issues" before committing.
- A failed required check after dismissals needs a re-run (an empty commit, or `gh run rerun`).
- Conflicts after a sibling PR merged: rebase on `main`, rebuild, rerun the tests - do not resolve by eye.
  Parallel PRs nearly always meet in `CHANGELOG.md` at the top of `### Fixed`: keep both entries, a blank line
  between, and lint. A PR the ruleset reports as BEHIND but not conflicting: `gh api -X PUT
  repos/<org>/<repo>/pulls/<n>/update-branch` (GitHub's merge commit is signed).
- **A dismissal request filed from the maintainer's account cannot be approved by that account** (the button
  greys out), and link and rc-worm require approval. Fix the code so the alert closes as fixed instead.
- **Tests never build paths from `std::env::temp_dir()`:** it reads `TMP`, CodeQL counts that as user input,
  and every production `File::create` the path reaches is flagged too (link#53, master-control#78). Use a
  test-only scratch under the crate's `target/` from `env!("CARGO_MANIFEST_DIR")` (`src/scratch.rs` in link and
  master-control) or `env!("CARGO_TARGET_TMPDIR")` in integration tests. Never print read-back file contents
  in an assertion message either (`rust/log-injection`).
- Parallel deep-dive agents cost about 3k tokens a fix; brief them to put the CHANGELOG entry in its own last
  commit, and never to use `temp_dir()` in tests.
