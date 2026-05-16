# Branch Diffs

`aci diff` compares two Git references by indexing each ref in an isolated
detached worktree and diffing the resulting graph snapshots. It is intended for
review automation, release risk checks, and local branch review.

```sh
aci diff main feature
aci diff main feature --pretty
aci diff main feature --format json --pretty
aci diff v0.1.1 v0.1.2 --repo /path/to/repo
```

## What It Reports

- Changed files from `git diff --name-status -M`.
- Added, removed, and modified symbols using stable graph keys instead of
  worktree-local node IDs.
- Public API changes detected from export facts and language visibility syntax
  such as `export` and Rust `pub`.
- Import and package dependency additions/removals.
- Impacted files that changed directly or reference/import/call changed names.
- Parser and indexing diagnostics from each side of the comparison.

Text output is optimized for humans. JSON output is stable and serializes the
same report shape used by the `aci-diff` library crate.

## Worktree Safety

The command resolves both refs to commits, creates temporary detached worktrees,
indexes those worktrees, and removes them before returning. The current checkout
is not switched, staged changes are not touched, and branch names are never
checked out directly.

## Current Limits

Symbol identity is based on relative path, language, symbol kind, and
qualified/simple name. File renames are mapped through Git rename detection so
unchanged symbols in renamed files do not show as remove/add churn.

Modified symbols are detected from the extracted source span and fact quality.
This is precise for Tree-sitter-backed adapters whose spans cover declarations
or bodies. Scanner fallback spans may be narrower, so future semantic enrichment
can improve body-level detection for degraded parses.
