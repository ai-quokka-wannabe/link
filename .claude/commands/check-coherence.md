Audit the documentation for coherence: `README.md`, `CONTRIBUTING.md`, `STYLE.md`, `TODO.md`,
`CHANGELOG.md`, everything under `docs/` and `.claude/CLAUDE.md`, against each other and
against the tree as it is. Adopted from the owner's `setonix-os`.

Run the numbered checks in order. Each returns exactly one of: **pass**, **finding** (quote the
two clauses side by side, each with file and line), or **ask the maintainer** (when the tree
cannot say which clause is right).

## 1. Contradictions between clauses that were each right when written

A number, a name, a rule or a status stated differently in two places: a version, a count of
tests or presets, a port, a cap, an etape marked done in one file and pending in another, a
job name the ruleset requires by its exact name spelled differently in a document.

## 2. Orphaned claims about the tree

A file, target, preset, flag, script, workflow, job, test or command a document names that does
not exist, or exists under another name. Verify by listing, not by memory.

## 3. Facts stated in two places when one owner is named

`STYLE.md` § Single Source of Truth names one file per kind of information. A second copy of
such a fact is a finding, whether or not it still agrees; propose the reference
(`FILE § Section`) that replaces the copy.

## 4. Scope drift

A document describing what the project was going to be rather than what it is: an etape's
"will" that is now "does", or the reverse; a "Later, with triggers" item that has quietly been
built; a limitation stated as current that a CHANGELOG entry lifted.

## 5. Stale "today" sections

`.claude/CLAUDE.md` § CI today, `README.md` § What Lives Here Today and any paragraph shaped
like a handoff, against the workflows and the tree as they are now.

## 6. References lychee cannot judge

A section referenced by name (`FILE § Section Name`) whose heading has since been renamed, or
a claim of the form "see X" where X no longer says that.

## Rules of the audit

- Report only what you verified in the tree. A finding names files and lines.
- Do not restate a rule's content in the report; cite where it lives.
- Be willing to conclude that the documents are coherent. A coherence audit that manufactures
  findings to look thorough is worse than useless, because it trains the reader to ignore the
  next one.
- Findings are reported, not fixed, unless the maintainer asked for fixes in the same breath.
