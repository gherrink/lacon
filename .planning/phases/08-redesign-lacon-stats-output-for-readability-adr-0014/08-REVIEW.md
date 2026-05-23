---
phase: 08-redesign-lacon-stats-output-for-readability-adr-0014
reviewed: 2026-05-23T14:35:00Z
depth: standard
files_reviewed: 6
files_reviewed_list:
  - crates/lacon-core/src/tracking/query.rs
  - crates/lacon-core/tests/tracking_query.rs
  - crates/lacon-cli/src/commands/stats.rs
  - crates/lacon-cli/src/cli.rs
  - crates/lacon-cli/src/main.rs
  - crates/lacon-cli/tests/cli_stats.rs
findings:
  critical: 2
  warning: 3
  info: 1
  total: 6
status: issues_found
---

# Phase 8: Code Review Report

**Reviewed:** 2026-05-23T14:35:00Z
**Depth:** standard
**Files Reviewed:** 6
**Status:** issues_found

## Summary

Phase 8 introduces the ADR 0014 read-time presentation layer for `lacon stats`: an overall
headline, four task-oriented sections, project canonicalization (ephemeral bucket + `.git`
ancestor walk), humanized byte counts, and `--since`/`--project`/`--rule`/`--bytes`/`--all`
filters. The design boundary is respected — SQL stays behind `lacon-core::tracking::query`,
no `rusqlite` dep leaks into `lacon-cli` production code, and no write-path code is touched.
The `.git` ancestor walk is genuinely bounded (one gitdir hop + one commondir hop, both
guarded by early-returns). Component-wise `Path::starts_with` is used correctly for ephemeral
detection. Division-by-zero on `raw_total == 0` is guarded.

Two blockers were found:

1. `parse_since` calls `s.split_at(s.len() - 1)` where `len()` is a byte offset. Passing
   any `--since` value whose last character is a multi-byte UTF-8 codepoint (e.g. `7é`, `30µ`)
   panics with "byte index N is not a char boundary" — a user-visible process abort, not a
   clean exit-2 error.

2. The Bypass rates section (Section 3) does not receive the `--project` filter. When
   `--project` is passed, sections 1, 2, and 4 are scoped to that project, but Section 3
   returns global bypass data. This also corrupts the "all-empty" hint at the end of
   `execute`: the hint fires only when all four result sets are empty, but `bypass` is
   populated from other projects, so the hint never fires even when the requested project
   genuinely has no data.

Three warnings cover: `is_bare` false-positive from substring `contains`, integer-division
truncation in saved-%, and inconsistent `.git` error handling between the dir and file
branches of the ancestor walk.

---

## Critical Issues

### CR-01: `parse_since` panics on multi-byte UTF-8 `--since` values

**File:** `crates/lacon-cli/src/commands/stats.rs:401`

**Issue:** `parse_since` computes the split point as `s.len() - 1`, which is a *byte* offset,
then calls `s.split_at(byte_offset)`. `str::split_at` panics if the offset does not fall on a
UTF-8 character boundary. For any `--since` argument whose last byte belongs to a multi-byte
codepoint — e.g. `7é` (3 bytes: `37 C3 A9`), `30µ` (4 bytes), `24î` — `split_at(len - 1)`
splits inside the codepoint and the binary aborts with a Rust panic rather than printing the
clean exit-2 error message that the rest of the function promises.

This is reachable from `lacon stats --since 7é` on any shell that allows non-ASCII input, or
from a script/alias that constructs a since value with a unit character from a non-ASCII
locale.

**Fix:** Use `char`-based splitting. The grammar requires a single-byte ASCII suffix (`d`, `h`,
`m`), so the correct approach is to match against the last *character*, which is stable even
on valid ASCII input:

```rust
fn parse_since(s: &str) -> Result<i64, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty value; use a form like 7d, 24h, or 30m".to_string());
    }
    // Split on the last character boundary (not byte offset), so multi-byte input
    // produces a clear error message instead of a panic.
    let (num_part, unit) = s
        .char_indices()
        .next_back()
        .map(|(i, _)| s.split_at(i))
        .ok_or_else(|| "empty value; use a form like 7d, 24h, or 30m".to_string())?;
    // ... rest unchanged
```

Alternatively, use `s.strip_suffix(|c: char| matches!(c, 'd' | 'h' | 'm'))` which also
provides a cleaner error message for unknown units (the suffix does not match, so the entire
input is the "unit"):

```rust
let (num_part, unit_str) = ["d", "h", "m"]
    .iter()
    .find_map(|u| s.strip_suffix(u).map(|n| (n, *u)))
    .ok_or_else(|| {
        let last = s.chars().next_back().unwrap_or(' ');
        format!("unknown unit `{last}`; use d (days), h (hours), or m (minutes)")
    })?;
```

---

### CR-02: Bypass-rates section ignores `--project` filter; corrupts the "all-empty" hint

**File:** `crates/lacon-cli/src/commands/stats.rs:248-251` (bypass query call) and `299-313`
(all-empty hint)

**Issue:** `filtered_bypass_rate` accepts only `(since_cutoff_ms, rule)` — there is no
`project` parameter in either `query::filtered_bypass_rate` or `query::bypass_rate`. When
`--project` is set, the three other sections are scoped to that project but the Bypass rates
section silently returns *global* bypass data. This produces a misleading report: a user
running `lacon stats --project /my/proj` sees bypass rates from every other project in the DB
mixed into the output as if they belong to `/my/proj`.

The corruption propagates to the "all-empty" hint (lines 299–313). The hint is supposed to
warn the user when `--project` matched no rows anywhere, indicating a likely path mismatch.
It gates on all four result sets being empty: `unmatched.is_empty() && f_offenders.is_empty()
&& bypass.is_empty() && rolled.is_empty()`. Because `bypass` is drawn from the global table,
it is non-empty whenever *any* rule has more than 5 runs — making the hint dead code in any
populated DB, even when the requested project genuinely has no tracked data.

**Fix — two separate changes required:**

1. Add `project: Option<&str>` to `filtered_bypass_rate` in `query.rs` and thread it through
   as an additional `AND project_path = ?N` predicate (matching the pattern used by the other
   three filtered re-queries):

```rust
// crates/lacon-core/src/tracking/query.rs
pub fn filtered_bypass_rate(
    conn: &Connection,
    since_cutoff_ms: Option<i64>,
    project: Option<&str>,   // <-- add
    rule: Option<&str>,
) -> Result<Vec<BypassRate>, TrackingError> {
    // ...existing WHERE rule_id IS NOT NULL...
    if let Some(p) = project.as_ref() {
        n += 1;
        sql.push_str(&format!(" AND project_path = ?{n}"));
        binds.push(p);
    }
    // ...rest unchanged
```

2. Update the call site in `stats.rs` to pass `project_ref`:

```rust
// crates/lacon-cli/src/commands/stats.rs
let bypass_res = if filtered {
    query::filtered_bypass_rate(&conn, cutoff_ms, project_ref, rule_ref)
} else {
    query::bypass_rate(&conn)
};
```

The all-empty hint (lines 299–313) then becomes correct automatically, because `bypass` will
be empty when the project has no bypass-flagged invocations.

---

## Warnings

### WR-01: `is_bare` false-positive via substring `contains` and cross-section match

**File:** `crates/lacon-cli/src/commands/stats.rs:570`

**Issue:** The bare-repo detection line is:

```rust
.any(|l| l.starts_with("bare") && l.replace(' ', "").contains("bare=true"))
```

Two distinct false-positive patterns exist:

**(a) Substring match on value:** `"bare = trueblue"` passes both predicates —
`"bare=trueblue".contains("bare=true")` is `true` because `"bare=true"` is a prefix of the
value string. Any config key named `bare` with a value beginning with `true` (not limited to
the boolean literal) would trigger a false positive, causing a real worktree to be silently
treated as bare and produce a `None` (literal path fallback) instead of the correct repo root.

**(b) Cross-section match:** Git config is an INI file with sections (`[core]`, `[remote
"origin"]`, etc.). The code scans every trimmed line, so a key named `bare = true` in any
non-`[core]` section (hypothetical but valid per INI syntax) also triggers. In practice this
is unlikely, but the code does not scope its scan to `[core]`.

The consequence is that `resolve_repo_root` returns `None` for a legitimate worktree, and the
project falls back to the literal stored path instead of being rolled up under the repo root.
No panic, no crash — but the project canonicalization silently degrades.

**Fix:** Use an exact-value check after stripping spaces:

```rust
fn is_bare(git_dir: &Path) -> bool {
    match std::fs::read_to_string(git_dir.join("config")) {
        Ok(cfg) => cfg
            .lines()
            .map(str::trim)
            .any(|l| l.replace(' ', "") == "bare=true"),
        Err(_) => false,
    }
}
```

This still misses git config's other boolean spellings (`bare = yes`, `bare = on`, `bare = 1`)
but that is a pre-existing limitation documented in the design constraints (best-effort
heuristic). The fix eliminates the substring and cross-section false positives.

---

### WR-02: Saved-% uses integer division, silently rounds to `0%` for small savings

**File:** `crates/lacon-cli/src/commands/stats.rs:161`

**Issue:**

```rust
let saved_pct = if totals.raw_total > 0 {
    format!("{}%", totals.bytes_saved * 100 / totals.raw_total)
```

Both `bytes_saved` and `raw_total` are `i64`. The expression `bytes_saved * 100 /
raw_total` performs integer division, which truncates toward zero. For small savings ratios
(below 1%), the headline will display `0%`. For example: `bytes_saved = 9`, `raw_total =
1000` → `9 * 100 / 1000 = 0`, displayed as `0%`. A user who has saved 0.9% of bytes across a
session will see `saved 9 B (0%)` — an actively misleading figure.

Additionally, `bytes_saved * 100` can theoretically overflow `i64` for extreme values (> 92
petabytes saved), though in practice unreachable with the current DB size limits.

**Fix:** Use floating-point for the percentage calculation:

```rust
let saved_pct = if totals.raw_total > 0 {
    let pct = totals.bytes_saved as f64 * 100.0 / totals.raw_total as f64;
    format!("{pct:.1}%")
} else {
    "—".to_string()
};
```

This also eliminates the overflow concern and produces output consistent with the one-decimal
precision used throughout the `Rule effectiveness` section.

---

### WR-03: `.git` file read failure short-circuits the ancestor walk; `.git` dir I/O errors continue

**File:** `crates/lacon-cli/src/commands/stats.rs:594`

**Issue:** There is an asymmetry between the two `.git` branches inside the ancestor loop:

- **Directory branch** (line 577–579): `std::fs::metadata` failure on `.git` triggers
  `continue`, which moves to the next ancestor. Correct.
- **File branch** (line 594): `std::fs::read_to_string(&dot_git).ok()?` uses `?` on the
  `Option`. The `?` operator inside a function returning `Option<String>` exits the **entire
  function** with `None`, not just the current loop iteration.

Consequence: if a stored `project_path` contains a `.git` *file* that exists on disk but is
unreadable (permission error, device, FIFO, etc.), `resolve_repo_root` returns `None`
immediately. This prevents the ancestor walk from ever reaching a parent directory that might
have a readable `.git` directory. The fallback to the literal stored path is correct in
principle, but the asymmetry is surprising and silently discards potentially valid parent-repo
resolution.

The same early-exit applies to a malformed gitlink (no `gitdir:` line) because the
`find_map(...).map(str::trim)?` also returns `None` from the function via `?`.

**Fix:** Replace the `?` short-circuit with `continue` where appropriate to maintain walk
symmetry:

```rust
if meta.is_file() {
    let contents = match std::fs::read_to_string(&dot_git) {
        Ok(c) => c,
        Err(_) => continue, // unreadable gitlink → keep walking ancestors
    };
    let gitdir_value = match contents
        .lines()
        .find_map(|l| l.trim().strip_prefix("gitdir:"))
        .map(str::trim)
    {
        Some(v) => v,
        None => continue, // malformed gitlink (no "gitdir:" line) → keep walking
    };
    // ... rest of file branch unchanged, removing .ok()? forms
```

This makes the error posture consistent: any unreadable or malformed `.git` entry at any
ancestor level is silently skipped and the walk continues, exactly as the directory branch
does.

---

## Info

### IN-01: `print_empty()` omits the headline, diverging from the non-empty output structure

**File:** `crates/lacon-cli/src/commands/stats.rs:424-435`

**Issue:** When the DB does not exist (`print_empty` path, D-03), the output contains four
section headers but no `Overall:` headline line. The non-empty path always prints the headline
first, so tools, tests, or humans that parse the output by looking for `Overall:` would find
it absent on a fresh machine. The test `stats_empty_db_prints_no_data_yet_and_succeeds` only
checks for `"no data yet"` and does not assert the absence or presence of `Overall:`, so this
divergence is undetected.

This is not a bug in the runtime path (a DB with data always has a headline), but it is an
observable output-format inconsistency that could trip callers scripting against `lacon stats`.

**Fix:** Add an `Overall:` zero-value headline to `print_empty`:

```rust
fn print_empty() {
    println!("Overall: 0 runs across 0 projects  ·  raw 0 B → kept 0 B  ·  saved 0 B (—)");
    println!();
    for header in [/* ... */] { /* ... */ }
}
```

Or, preferably, treat the zero-row case uniformly by letting `execute` reach the headline
print path even when `overall_totals` returns all-zero (which it does correctly per the
`COALESCE` guards), and reserving `print_empty` only for the "no DB file" case where a
headline would mislead.

---

_Reviewed: 2026-05-23T14:35:00Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
