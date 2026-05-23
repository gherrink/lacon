# Phase 8: redesign-lacon-stats-output-for-readability-adr-0014 - Discussion Log (Assumptions Mode)

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions captured in CONTEXT.md — this log preserves the analysis.

**Date:** 2026-05-23
**Phase:** 08-redesign-lacon-stats-output-for-readability-adr-0014
**Mode:** assumptions
**Areas analyzed:** Helper code placement; Byte humanization + `--bytes`; Top-N capping/ordering + `--all`; `.git` resolution + ephemeral detection; Test restructuring + relabeling

## Assumptions Presented

### Helper code placement
| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| Presentation helpers inline in `commands/stats.rs`; one new SQL aggregate `query::overall_totals` in `lacon-core/.../query.rs` | Likely | one-module-per-command convention (`explain.rs`, `doctor.rs`, existing `stats.rs` helpers); no shared util module; ADR "all of this in lacon-cli" + D-01 |

### Byte humanization + `--bytes`
| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| Decimal SI (`KB`/`MB`/`GB`, 1000-based, 1 decimal, `512 B` below 1 KB); `--bytes` bool prints exact integers; new `humanize_bytes` helper | Likely | ADR §4 literal `22.8 KB`; `init` `#[arg(long)] bool` flag shape; `cli_surface.rs` caps subcommands not flags |

### Top-N capping/ordering + `--all`
| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| N=10 per section by primary metric; project capped AFTER Rust rollup; `… M more` hint | Confident (ordering/rollup) | `query.rs` `ORDER BY … DESC`; canonical-key rollup destroys DB order for project section |
| Ship `--all` now | Unclear → resolved | ADR §3 lists `--all` as a drill-in option but parenthesizes "a future `--all`" |

### `.git` resolution + ephemeral detection
| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| Ephemeral-first → `.git` walk → literal fallback; component-wise `Path::starts_with`; gitfile may be relative; `commondir` `../..`; `core.bare` guard; literal fallback on any I/O error | Confident (research-firmed) | git 2.53 byte-level verification; submodule gitfile is relative; `current_dir()` stores logical cwd (macOS dual `/var/folders` spelling) |

### Test restructuring + relabeling
| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| `cli_stats.rs` targeted edits (substring assertions, not golden files); update 4 header assertions; add temp/worktree/cap/`--all` test; column relabel low-risk | Confident | every CLI-suite assertion is `contains(...)`; no test asserts `filtered_bytes`/`keep_ratio` tokens |

## Corrections Made

No corrections — all assumptions confirmed. Two open choices were resolved via AskUserQuestion:

### Byte units
- **Question:** Byte humanization format?
- **User choice:** Decimal SI (`22.8 KB`) — confirms assumption B as written.

### `--all` flag
- **Question:** Ship `--all` (uncapped output) in this phase?
- **User choice:** Ship `--all` now — resolves the Unclear in the top-N area.

## External Research

- **Git linked-worktree resolution (git 2.53, byte-verified):** `.git` file =
  `gitdir: <path>\n`; path absolute for `git worktree`, **relative for submodules**
  (resolve against the gitfile dir). `commondir` = path relative to the admin gitdir
  (conventionally `../..`); `parent(main .git)` is the working-tree root for the
  normal non-bare case. Guard `core.bare = true` → no working tree. Source:
  `gitrepository-layout(5)`, `git-worktree(1)`, live `xxd`/`git rev-parse`. → Firms
  up D-09/D-10 (relative-gitdir branch + bare guard + literal fallback).
- **Platform temp prefixes:** macOS `temp_dir()`/`$TMPDIR` = `/var/folders/.../T`;
  `/var` symlinks to `/private/var`, so a logical cwd can carry either spelling →
  match **both** `/var/folders` and `/private/var/folders`. Use `Path::starts_with`
  (component-wise) to avoid `/tmpfoo`. Don't `canonicalize` (ephemeral paths often
  deleted). Minimal robust set = `/tmp`, `/var/folders`, `/private/var/folders`,
  `temp_dir()`, `$TMPDIR`. Sources: Rust `std::env::temp_dir` docs, nodejs/node
  #11422, `std::path::Path::starts_with` docs. → Firms up D-08.
