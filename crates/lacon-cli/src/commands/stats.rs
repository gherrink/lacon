//! lacon stats subcommand: summarize tracking data from the four reporting
//! views, with `--project`/`--since`/`--rule` filters (D-09, D-10) and a
//! graceful empty-DB path (D-03).
//!
//! # Design boundary (D-01)
//! All SQL lives in `lacon_core::tracking::query`. This command opens the DB
//! read-only via `tracking::open_readonly` (D-02) and calls the typed view
//! readers / filtered re-queries — it never inlines a query and keeps
//! `rusqlite` a dev-only dependency.
//!
//! # Filters (D-09 / D-10)
//! When any of `--project`/`--since`/`--rule` is set, the affected sections
//! read the base-table filtered re-queries; otherwise they read the views
//! directly. `--since` accepts relative forms only (`Nd`/`Nh`/`Nm`); a
//! malformed value errors with exit code 2 and no panic.
//!
//! # Output (ADR 0014 read-time presentation layer)
//! Plain text, no color dependency. An overall **headline** is printed first
//! (D-05): total runs, distinct projects (after canonicalization), `raw → kept`
//! bytes, and `saved` (absolute + percent), over `bypassed = 0` rows. Then four
//! task-oriented sections (D-15): "Commands with no rule", "Rule effectiveness",
//! "Bypass rates", "Savings by project". The project section re-aggregates the
//! per-`project_path` rows under a canonical key (D-06/D-07: ephemeral →
//! `(ephemeral)`, repo root via `.git`, else literal) and re-sorts by bytes
//! saved DESC. Every section caps at [`TOP_N`] rows with a `… M more` drill-in
//! hint (D-11); `--all` uncaps and suppresses the hint (D-12). Byte counts are
//! humanized (`22.8 KB`) by default; `--bytes` prints exact integers (D-14).
//!
//! The stored field names (`filtered_bytes`/`avg_keep_ratio`) and the four
//! `v_*` views are NOT renamed — relabeling is presentation-only (D-15 fence).

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use lacon_core::tracking::{self, query};

/// D-11: default per-section row cap before a `… M more` hint is printed.
/// `--all` uncaps every section (D-12).
const TOP_N: usize = 10;

/// Exit codes (documented for the SUMMARY): 0 success, 1 a query/open failure,
/// 2 bad CLI input (malformed `--since`). The empty-DB path is a success (0),
/// not an error.
///
/// `bytes` (D-14): print exact integers instead of humanized byte counts.
/// `all` (D-12): print every row uncapped and drop the `… M more` hint.
pub fn execute(
    project: Option<PathBuf>,
    since: Option<String>,
    rule: Option<String>,
    bytes: bool,
    all: bool,
) -> anyhow::Result<i32> {
    // ─── Resolve --since to an absolute cutoff in unix MILLISECONDS (D-10) ───
    // ts is unix ms (tracking-data-model.md); cutoff = now_ms - n*unit_ms.
    let cutoff_ms: Option<i64> = match since.as_deref() {
        None => None,
        Some(s) => match parse_since(s) {
            Ok(window_ms) => {
                let now_ms = match SystemTime::now().duration_since(UNIX_EPOCH) {
                    Ok(d) => d.as_millis() as i64,
                    Err(_) => {
                        eprintln!("lacon stats: system time is before the unix epoch");
                        return Ok(2);
                    }
                };
                Some(now_ms - window_ms)
            }
            Err(msg) => {
                eprintln!("lacon stats: invalid --since `{s}`: {msg}");
                return Ok(2);
            }
        },
    };

    // WR-03: the stored `project_path` is the absolute, logical cwd captured at
    // run time (`std::env::current_dir()`), and the query binds `--project` as a
    // byte-exact `project_path = ?N`. Without normalization, `--project .`,
    // `./`, a relative path, or a trailing slash silently mismatch and every
    // section reports "no data yet" with exit 0 — a quiet correctness trap.
    // Lexically absolutize + strip a trailing separator so these common forms
    // line up with the stored absolute path. We deliberately do NOT
    // `canonicalize` (resolve symlinks): the write side stores the *logical*
    // cwd, so symlink resolution here would diverge from it.
    let project_str: Option<String> = project.as_ref().map(|p| normalize_project(p));
    let project_ref: Option<&str> = project_str.as_deref();
    let rule_ref: Option<&str> = rule.as_deref();
    let filtered = cutoff_ms.is_some() || project_ref.is_some() || rule_ref.is_some();

    // ─── DB path resolve + graceful empty-DB skip (D-03, Pitfall 4) ─────────
    // Check existence BEFORE opening: open_readonly errors on an absent file
    // (it never CREATEs), so the fresh-machine state must be detected first.
    let db_path = match tracking::Tracker::xdg_db_path() {
        Some(p) => p,
        None => {
            eprintln!("lacon stats: could not resolve the XDG data directory");
            return Ok(2);
        }
    };

    if !db_path.exists() {
        print_empty();
        return Ok(0);
    }

    let conn = match tracking::open_readonly(&db_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("lacon stats: could not open history.db: {e}");
            return Ok(1);
        }
    };

    // BYTE RENDERER (D-14): exact integers under `--bytes`, else humanized.
    // A closure so every render site (headline + all four sections) is uniform.
    let render = |n: i64| -> String {
        if bytes {
            n.to_string()
        } else {
            humanize_bytes(n)
        }
    };

    // ─── Project rollup FIRST (D-06): the headline's distinct-projects count is
    // the rolled-up canonical-map length (Pitfall 7), NOT the SQL COUNT, so the
    // headline number always equals the visible "Savings by project" rows. We
    // therefore re-aggregate before printing the headline. ────────────────────
    let savings_res = if filtered {
        query::filtered_project_savings(&conn, cutoff_ms, project_ref)
    } else {
        query::project_savings(&conn)
    };
    let savings = match savings_res {
        Ok(rows) => rows,
        Err(e) => {
            eprintln!("lacon stats: query failed: {e}");
            return Ok(1);
        }
    };
    let rolled = rollup_project_savings(&savings);

    // ─── Headline (D-05, FIRST) ─────────────────────────────────────────────
    // WR-02 posture: map a reader Err to `lacon stats:` + exit 1 rather than
    // letting it escape via `?` -> anyhow (T-08-03). Spans matched + unmatched
    // runs over `bypassed = 0`; `distinct_projects` from the SQL aggregate is
    // PRE-canonicalization and is NOT displayed — we print `rolled.len()`.
    let totals_res = if filtered {
        query::filtered_overall_totals(&conn, cutoff_ms, project_ref)
    } else {
        query::overall_totals(&conn)
    };
    let totals = match totals_res {
        Ok(t) => t,
        Err(e) => {
            eprintln!("lacon stats: query failed: {e}");
            return Ok(1);
        }
    };
    let saved_pct = if totals.raw_total > 0 {
        // WR-02: compute in f64 with one decimal place. The old i64
        // `bytes_saved * 100 / raw_total` truncated sub-1% savings to `0%`
        // (e.g. 9 / 1000 → `0%`) and could overflow on extreme totals. One
        // decimal matches the precision used in the Rule effectiveness section.
        let pct = totals.bytes_saved as f64 * 100.0 / totals.raw_total as f64;
        format!("{pct:.1}%")
    } else {
        "—".to_string()
    };
    println!(
        "Overall: {} runs across {} projects  ·  raw {} → kept {}  ·  saved {} ({})",
        totals.total_runs,
        rolled.len(),
        render(totals.raw_total),
        render(totals.kept_total),
        render(totals.bytes_saved),
        saved_pct,
    );
    println!();

    // ─── Section 1: Commands with no rule (was "Unmatched offenders", D-15) ───
    // WR-02: map SELECT failures to `lacon stats:` + exit 1 rather than letting a
    // TrackingError::Sqlite escape via `?` -> anyhow (which prints the internal
    // "tracking: sqlite ..." text and bypasses the chosen exit code). Matches the
    // open-failure handling above and doctor's mapped posture (T-04-10).
    println!("Commands with no rule");
    let unmatched_res = if filtered {
        query::filtered_unmatched_offenders(&conn, cutoff_ms, project_ref)
    } else {
        query::unmatched_offenders(&conn)
    };
    let unmatched = match unmatched_res {
        Ok(rows) => rows,
        Err(e) => {
            eprintln!("lacon stats: query failed: {e}");
            return Ok(1);
        }
    };
    if unmatched.is_empty() {
        println!("  no data yet");
    } else {
        print_capped(&unmatched, all, |r| {
            format!(
                "  {}  runs={}  raw={}",
                r.command_normalized,
                r.runs,
                render(r.total_raw_bytes)
            )
        });
    }
    println!();

    // ─── Section 2: Rule effectiveness (was "Filtered offenders", D-15) ──────
    // Columns relabeled (D-15): the surviving-bytes column is `kept` (not
    // `filtered_bytes`), and effectiveness is `saved %` (higher is better) — the
    // inverse of the old `keep_ratio` (kept/raw, lower is better). We derive
    // `saved %` from the avg keep ratio: saved% = 100 - keep_ratio*100.
    println!("Rule effectiveness");
    let f_offenders_res = if filtered {
        query::filtered_filtered_offenders(&conn, cutoff_ms, project_ref, rule_ref)
    } else {
        query::filtered_offenders(&conn)
    };
    let f_offenders = match f_offenders_res {
        Ok(rows) => rows,
        Err(e) => {
            eprintln!("lacon stats: query failed: {e}");
            return Ok(1);
        }
    };
    if f_offenders.is_empty() {
        println!("  no data yet");
    } else {
        print_capped(&f_offenders, all, |r| {
            let saved = r
                .avg_keep_ratio
                .map(|v| format!("{:.0}%", (1.0 - v) * 100.0))
                .unwrap_or_else(|| "-".to_string());
            format!(
                "  {}  rule={}  runs={}  kept={}  saved %={}",
                r.command_normalized,
                r.rule_id.as_deref().unwrap_or("-"),
                r.runs,
                render(r.total_filtered_bytes),
                saved
            )
        });
    }
    println!();

    // ─── Section 3: Bypass rates ────────────────────────────────────────────
    println!("Bypass rates");
    let bypass_res = if filtered {
        // CR-02: thread `project_ref` so the bypass section is scoped to
        // `--project` like the other three sections (and the all-empty hint
        // below fires correctly when the project genuinely has no data).
        query::filtered_bypass_rate(&conn, cutoff_ms, project_ref, rule_ref)
    } else {
        query::bypass_rate(&conn)
    };
    let bypass = match bypass_res {
        Ok(rows) => rows,
        Err(e) => {
            eprintln!("lacon stats: query failed: {e}");
            return Ok(1);
        }
    };
    if bypass.is_empty() {
        println!("  no data yet");
    } else {
        print_capped(&bypass, all, |r| {
            format!(
                "  rule={}  total={}  bypassed={}  rate={:.2}",
                r.rule_id.as_deref().unwrap_or("-"),
                r.total,
                r.bypassed,
                r.bypass_rate
            )
        });
    }
    println!();

    // ─── Section 4: Savings by project (was "Per-project savings", D-15) ─────
    // Rolled up under the canonical key (D-06) and re-sorted by bytes_saved DESC
    // (the DB ORDER BY is destroyed by the Rust re-aggregation), then capped.
    println!("Savings by project");
    if rolled.is_empty() {
        println!("  no data yet");
    } else {
        print_capped(&rolled, all, |r| {
            format!(
                "  {}  runs={}  raw={}  kept={}  saved={}",
                r.key,
                r.total_runs,
                render(r.raw_total),
                render(r.filtered_total),
                render(r.bytes_saved)
            )
        });
    }

    // WR-03: when a `--project` filter matched nothing in any section, the
    // byte-exact match likely missed a path-form difference rather than there
    // being genuinely no data. Surface a hint (to stderr, so stdout stays
    // snapshot-stable) instead of a silent all-empty report. We compare against
    // the normalized value we actually bound so the user sees what was matched.
    if project_ref.is_some()
        && unmatched.is_empty()
        && f_offenders.is_empty()
        && bypass.is_empty()
        && rolled.is_empty()
    {
        if let Some(p) = project_ref {
            eprintln!(
                "lacon stats: hint: --project matched no rows. It must equal the \
                 stored absolute project path verbatim (matched against `{p}`). \
                 Try the absolute path printed under \"Savings by project\" when \
                 run without a filter."
            );
        }
    }

    Ok(0)
}

/// A canonical-key-rolled project-savings row (D-06). Mirrors
/// [`query::ProjectSaving`]'s additive fields but keys on the resolved canonical
/// project key (`(ephemeral)` / repo root / literal) instead of the raw stored
/// `project_path`, with every additive field summed across the collapsed rows.
struct RolledSaving {
    key: String,
    total_runs: i64,
    raw_total: i64,
    filtered_total: i64,
    bytes_saved: i64,
}

/// D-06: re-aggregate per-`project_path` rows under [`canonical_project_key`],
/// summing every additive field (exact — runs/raw/filtered/saved are all sums),
/// then re-sort by `bytes_saved` DESC (the DB `ORDER BY` is destroyed by the
/// HashMap rollup). All temp-dir paths collapse to one `(ephemeral)` row;
/// worktrees/subdirs collapse to their repo root.
fn rollup_project_savings(rows: &[query::ProjectSaving]) -> Vec<RolledSaving> {
    let mut map: HashMap<String, RolledSaving> = HashMap::new();
    for r in rows {
        let key = canonical_project_key(r.project_path.as_deref().unwrap_or("-"));
        let acc = map.entry(key.clone()).or_insert(RolledSaving {
            key,
            total_runs: 0,
            raw_total: 0,
            filtered_total: 0,
            bytes_saved: 0,
        });
        acc.total_runs += r.total_runs;
        acc.raw_total += r.raw_total;
        acc.filtered_total += r.filtered_total;
        acc.bytes_saved += r.bytes_saved;
    }
    let mut out: Vec<RolledSaving> = map.into_values().collect();
    // DESC by bytes_saved (Reverse keeps it a clippy-clean sort_by_key).
    out.sort_by_key(|r| std::cmp::Reverse(r.bytes_saved));
    out
}

/// D-11/D-12: print up to [`TOP_N`] rows via `row_fmt`, and — unless `all` — a
/// `… M more` drill-in hint when more rows exist. `--all` uncaps and suppresses
/// the hint (T-08-08: bounds output regardless of history size by default).
fn print_capped<T>(rows: &[T], all: bool, row_fmt: impl Fn(&T) -> String) {
    let limit = if all { rows.len() } else { TOP_N };
    for r in rows.iter().take(limit) {
        println!("{}", row_fmt(r));
    }
    if !all && rows.len() > TOP_N {
        let more = rows.len() - TOP_N;
        println!("  … {more} more (use --project / --rule / --since / --all to drill in)");
    }
}

/// WR-03: normalize a `--project` argument to line up with the stored
/// `project_path` (the absolute, logical cwd from `std::env::current_dir()`).
///
/// Makes the path absolute and lexically resolves `.`/`..` via
/// `std::path::absolute` (no filesystem access, no symlink resolution — matching
/// the write side's logical cwd), then strips a single trailing separator so
/// `/home/me/proj/` and `/home/me/proj` compare equal. On the rare error path
/// (`absolute` only fails on an empty path or unavailable cwd) we fall back to
/// the raw string so behavior never regresses below the pre-fix exact match.
fn normalize_project(p: &std::path::Path) -> String {
    let abs = std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    let s = abs.to_string_lossy();
    // Strip a single trailing path separator (but never reduce the root "/").
    let trimmed = s
        .strip_suffix(std::path::MAIN_SEPARATOR)
        .filter(|t| !t.is_empty())
        .unwrap_or(&s);
    trimmed.to_string()
}

/// Parse a relative `--since` value into a window in milliseconds.
///
/// Grammar (v1, D-10): an unsigned integer prefix followed by a single unit
/// suffix — `d` (days), `h` (hours), `m` (minutes). Combined forms like
/// `1d12h` are out of scope for v1 (left to discretion); reject them clearly.
fn parse_since(s: &str) -> Result<i64, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty value; use a form like 7d, 24h, or 30m".to_string());
    }
    // CR-01: split on the last *character*, not the last byte. `s.len()` is a
    // byte offset, so `split_at(s.len() - 1)` panics on a multi-byte UTF-8
    // suffix (e.g. `7é`, `30µ`). Match the unit via `strip_suffix` (a char
    // pattern, boundary-safe) so any non-ASCII or unknown suffix produces a
    // clean exit-2 error (D-03) instead of a process-aborting panic.
    let (num_part, unit_ms): (&str, i64) = if let Some(n) = s.strip_suffix('d') {
        (n, 86_400_000)
    } else if let Some(n) = s.strip_suffix('h') {
        (n, 3_600_000)
    } else if let Some(n) = s.strip_suffix('m') {
        (n, 60_000)
    } else {
        let last = s.chars().next_back().unwrap_or(' ');
        return Err(format!(
            "unknown unit `{last}`; use d (days), h (hours), or m (minutes)"
        ));
    };
    let n: i64 = num_part
        .parse()
        .map_err(|_| format!("`{num_part}` is not a whole number"))?;
    if n < 0 {
        return Err("the count must be non-negative".to_string());
    }
    n.checked_mul(unit_ms)
        .ok_or_else(|| "the window is too large".to_string())
}

/// Fresh-machine output: a "no data yet" line per section, exit 0 (D-03). The
/// four headers use the relabeled D-15 strings to match `execute`'s output.
fn print_empty() {
    for header in [
        "Commands with no rule",
        "Rule effectiveness",
        "Bypass rates",
        "Savings by project",
    ] {
        println!("{header}");
        println!("  no data yet");
        println!();
    }
}

/// D-13: humanize a byte count for the read-time presentation layer.
///
/// Decimal SI (1000-based, NOT 1024-based) per ADR 0014 §4 (whose literal
/// example is `22.8 KB`): below 1 KB we print the raw integer with a `B` suffix
/// (e.g. `512 B`, `0 B`); at and above 1 KB we divide by 1000 walking the SI
/// units and format with a single decimal place (e.g. `1.0 KB`, `22.8 KB`,
/// `1.0 MB`). All stored byte counts are `>= 0`, so the negative branch is not
/// expected, but it is handled defensively (sign-prefixed) rather than panicking.
///
/// Added by plan 08-02 as a standalone, fully unit-tested helper; wired into
/// `execute`'s `render` closure (every byte render site) by plan 08-03.
fn humanize_bytes(n: i64) -> String {
    const UNIT: f64 = 1000.0;
    let neg = n < 0;
    let bytes = n.unsigned_abs() as f64;
    if bytes < UNIT {
        // Raw integer below 1 KB, e.g. "512 B", "0 B", "999 B".
        return format!("{n} B");
    }
    let units = ["KB", "MB", "GB", "TB", "PB"];
    let mut value = bytes / UNIT;
    let mut idx = 0;
    while value >= UNIT && idx < units.len() - 1 {
        value /= UNIT;
        idx += 1;
    }
    let sign = if neg { "-" } else { "" };
    // One decimal place above 1 KB, e.g. "22.8 KB", "1.0 MB".
    format!("{sign}{value:.1} {}", units[idx])
}

/// D-08: the runtime-built set of filesystem prefixes that mark an ephemeral
/// (boot-/test-scoped) working directory. A stored `project_path` rooted under
/// any of these collapses into the single synthetic `(ephemeral)` bucket.
///
/// The static entries cover the conventional Linux (`/tmp`) and macOS
/// (`/var/folders`, plus its `/private`-prefixed spelling that `current_dir()`
/// can record) temp roots; `std::env::temp_dir()` and `$TMPDIR` add the
/// runtime-resolved temp dir so test harnesses that override `TMPDIR` are also
/// matched. We deliberately do NOT add `/var/tmp` (it is persistent across
/// reboots, not boot-ephemeral). `/dev/shm` is included as a Linux tmpfs root.
///
/// Added by plan 08-02; consumed via `is_ephemeral`/`canonical_project_key`,
/// which plan 08-03 wires into `execute`'s project rollup.
fn ephemeral_prefixes() -> Vec<std::path::PathBuf> {
    let mut v = vec![
        std::path::PathBuf::from("/tmp"),
        std::path::PathBuf::from("/var/folders"),
        std::path::PathBuf::from("/private/var/folders"),
        std::path::PathBuf::from("/dev/shm"),
        std::env::temp_dir(),
    ];
    if let Some(t) = std::env::var_os("TMPDIR") {
        v.push(std::path::PathBuf::from(t));
    }
    v
}

/// D-08: is the stored project path rooted under an ephemeral prefix?
///
/// Uses component-wise `Path::starts_with` — NOT `str::starts_with`, which would
/// false-match `/tmpfoo` against `/tmp` and wrongly collapse a real project into
/// the `(ephemeral)` bucket. Matches against the STORED string and never
/// `canonicalize`s: ephemeral directories are frequently already deleted, and
/// the write side stored the *logical* cwd, so symlink resolution would diverge.
///
/// Added by plan 08-02; the canonical-key resolver and `execute`'s project
/// rollup (plan 08-03) call it.
fn is_ephemeral(stored: &str) -> bool {
    let p = std::path::Path::new(stored);
    ephemeral_prefixes()
        .iter()
        .any(|prefix| p.starts_with(prefix))
}

/// D-09/D-10: resolve a stored project path to its repository root via a bounded
/// sequence of read-only filesystem reads — NO `git` subprocess, NO
/// `canonicalize` (the stored dir may already be gone, and we must not resolve
/// symlinks away from the logical cwd the write side recorded).
///
/// Walks `path.ancestors()` looking for a `.git` entry:
///   - `.git` is a **directory** → that ancestor is the repo root, unless its
///     `config` declares `core.bare = true` (a bare repo has no working tree, so
///     there is nothing to roll a project path up to) → `None` (D-10).
///   - `.git` is a **file** → it is a gitlink of the form `gitdir: <path>`. Strip
///     the prefix and `trim_end`; a relative value is resolved against the
///     gitfile's own directory (git submodules write relative; `git worktree`
///     writes absolute). Then read `<gitdir>/commondir` (conventionally `../..`,
///     relative to that admin gitdir; an absolute value is legal too) to locate
///     the main `.git` directory; the repo root is the **parent** of that main
///     `.git`. A single gitdir hop then a single commondir hop — no unbounded
///     chain following (T-08-04).
///
/// Every IO error, missing `.git`, malformed gitlink, bare repo, or absent
/// directory yields `None`; the caller falls back to the literal stored path
/// (D-10). `core.worktree`/`GIT_WORK_TREE` are not honored in v1 — parent-of-
/// `.git` is the documented best-effort heuristic.
///
/// Added by plan 08-02; wired into `execute`'s project rollup (via
/// `canonical_project_key`) by plan 08-03.
fn resolve_repo_root(path: &std::path::Path) -> Option<String> {
    use std::path::{Path, PathBuf};

    // Lexically join `base` with a possibly-relative `value`, then pop any
    // `..`/`.` components WITHOUT touching the filesystem (no canonicalize). An
    // absolute `value` replaces `base` entirely.
    fn lexical_join(base: &Path, value: &str) -> PathBuf {
        let raw = Path::new(value);
        let joined = if raw.is_absolute() {
            raw.to_path_buf()
        } else {
            base.join(raw)
        };
        let mut out = PathBuf::new();
        for comp in joined.components() {
            use std::path::Component;
            match comp {
                Component::ParentDir => {
                    out.pop();
                }
                Component::CurDir => {}
                other => out.push(other.as_os_str()),
            }
        }
        out
    }

    // Returns true iff `<git_dir>/config` declares `core.bare = true` (D-10).
    fn is_bare(git_dir: &Path) -> bool {
        match std::fs::read_to_string(git_dir.join("config")) {
            Ok(cfg) => cfg
                .lines()
                .map(str::trim)
                // WR-01: exact-match the whitespace-stripped line. `contains`
                // false-matched `bare = trueblue` (value PREFIXED by `true`);
                // an exact `== "bare=true"` only fires on the boolean literal.
                .any(|l| l.replace(' ', "") == "bare=true"),
            Err(_) => false,
        }
    }

    for ancestor in path.ancestors() {
        let dot_git = ancestor.join(".git");
        let meta = match std::fs::metadata(&dot_git) {
            Ok(m) => m,
            Err(_) => continue, // no .git at this ancestor; keep walking up.
        };

        if meta.is_dir() {
            // Normal repo (or a run from a subdirectory). A bare repo has no
            // working tree → literal fallback.
            if is_bare(&dot_git) {
                return None;
            }
            return Some(ancestor.to_string_lossy().into_owned());
        }

        if meta.is_file() {
            // Gitlink: `gitdir: <path>` (relative resolved against the gitfile's
            // own directory; absolute used as-is). One gitdir hop.
            //
            // WR-03: an unreadable gitfile or a malformed gitlink (no `gitdir:`
            // line) must `continue` to keep walking ancestors — NOT `?`-return
            // None from the whole function, which would skip a parent repo and
            // mismatch the directory branch's error posture (any failure → keep
            // walking → ultimately the literal fallback, D-10).
            let contents = match std::fs::read_to_string(&dot_git) {
                Ok(c) => c,
                Err(_) => continue, // unreadable gitlink → keep walking ancestors.
            };
            let gitdir_value = match contents
                .lines()
                .find_map(|l| l.trim().strip_prefix("gitdir:"))
                .map(str::trim)
            {
                Some(v) => v,
                None => continue, // malformed gitlink (no `gitdir:`) → keep walking.
            };
            let gitfile_dir = dot_git.parent().unwrap_or(ancestor);
            let admin_git_dir = lexical_join(gitfile_dir, gitdir_value);

            // commondir locates the main `.git` (one commondir hop). Default to
            // the admin gitdir itself if commondir is absent (a plain gitlink
            // pointing straight at the main .git).
            let main_git_dir = match std::fs::read_to_string(admin_git_dir.join("commondir")) {
                Ok(common) => lexical_join(&admin_git_dir, common.trim()),
                Err(_) => admin_git_dir.clone(),
            };

            if is_bare(&main_git_dir) {
                return None;
            }
            // Repo root is the parent of the main `.git` directory.
            return main_git_dir
                .parent()
                .map(|p| p.to_string_lossy().into_owned());
        }

        // A `.git` that is neither dir nor file (symlink to nothing, device,
        // ...) → treat as absent and keep walking.
    }

    None
}

/// D-07: resolve a stored `project_path` to its canonical rollup key, in
/// precedence order:
///   (a) **ephemeral** — a path under a temp root collapses to `(ephemeral)`,
///       and this WINS over `.git` so a throwaway repo created under `/tmp`
///       still buckets as ephemeral (D-07/D-08);
///   (b) **repo root** via `.git` resolution (D-09);
///   (c) **literal fallback** — the stored string verbatim, so the key never
///       regresses below the exact recorded path (D-10).
///
/// PURE: no `canonicalize`, no subprocess. Added by plan 08-02; called by
/// `rollup_project_savings` (plan 08-03).
fn canonical_project_key(stored: &str) -> String {
    if is_ephemeral(stored) {
        return "(ephemeral)".to_string();
    }
    if let Some(root) = resolve_repo_root(std::path::Path::new(stored)) {
        return root;
    }
    stored.to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        canonical_project_key, humanize_bytes, is_ephemeral, normalize_project, parse_since,
        resolve_repo_root,
    };
    use std::path::{Path, MAIN_SEPARATOR};

    // WR-03: an already-absolute path is returned unchanged (the common case
    // must keep matching the stored absolute project_path).
    #[test]
    fn normalize_project_absolute_unchanged() {
        let abs = if MAIN_SEPARATOR == '/' {
            "/home/me/proj"
        } else {
            r"C:\home\me\proj"
        };
        assert_eq!(normalize_project(Path::new(abs)), abs);
    }

    // WR-03: a single trailing separator is stripped so `proj/` == `proj`.
    #[test]
    fn normalize_project_strips_trailing_separator() {
        let with_slash = format!(
            "{}home{}me{}proj{}",
            MAIN_SEPARATOR, MAIN_SEPARATOR, MAIN_SEPARATOR, MAIN_SEPARATOR
        );
        let without = format!(
            "{}home{}me{}proj",
            MAIN_SEPARATOR, MAIN_SEPARATOR, MAIN_SEPARATOR
        );
        assert_eq!(normalize_project(Path::new(&with_slash)), without);
    }

    // WR-03: `.` resolves to the current dir (absolute), so `--project .` lines
    // up with the stored cwd instead of silently mismatching.
    #[test]
    fn normalize_project_dot_becomes_absolute_cwd() {
        let cwd = std::env::current_dir().unwrap();
        let expected = cwd.to_string_lossy().to_string();
        // current_dir() has no trailing separator, so no stripping is involved.
        assert_eq!(normalize_project(Path::new(".")), expected);
    }

    // WR-03: the root path must not be reduced to empty by the trailing-strip.
    #[cfg(unix)]
    #[test]
    fn normalize_project_root_preserved() {
        assert_eq!(normalize_project(Path::new("/")), "/");
    }

    #[test]
    fn parse_since_days_hours_minutes() {
        assert_eq!(parse_since("7d").unwrap(), 7 * 86_400_000);
        assert_eq!(parse_since("24h").unwrap(), 24 * 3_600_000);
        assert_eq!(parse_since("30m").unwrap(), 30 * 60_000);
    }

    #[test]
    fn parse_since_rejects_bad_unit() {
        assert!(parse_since("7x").is_err());
        assert!(parse_since("abc").is_err());
    }

    #[test]
    fn parse_since_rejects_empty() {
        assert!(parse_since("").is_err());
    }

    // CR-01: a `--since` value whose last character is a multi-byte UTF-8
    // codepoint (e.g. `7é` = bytes 37 C3 A9) must return Err — NOT panic from a
    // byte-offset `split_at` slicing inside the codepoint. The grammar requires
    // a single ASCII unit suffix, so a non-ASCII suffix is an unknown unit.
    #[test]
    fn parse_since_multibyte_suffix_errors_no_panic() {
        for bad in ["7é", "30µ", "24î", "é", "12£"] {
            assert!(
                parse_since(bad).is_err(),
                "{bad:?} must be a clean Err, not a panic"
            );
        }
    }

    // D-13: decimal-SI humanization. These six points are the Nyquist boundary
    // set — they pin the B↔KB threshold (999/1000), prove the scheme is
    // 1000-based not 1024-based (1024 → "1.0 KB", a binary scheme would print a
    // different value / "KiB"), reproduce the ADR §4 literal ("22.8 KB"), cross
    // the KB↔MB boundary (1_000_000 → "1.0 MB"), and pin the zero case. Interior
    // points add nothing.
    #[test]
    fn humanize_bytes_decimal_si_boundaries() {
        assert_eq!(humanize_bytes(0), "0 B");
        assert_eq!(humanize_bytes(999), "999 B");
        assert_eq!(humanize_bytes(1000), "1.0 KB");
        // Decimal SI, NOT binary: 1024 bytes is still ~1.0 KB (1.024 → "1.0").
        assert_eq!(humanize_bytes(1024), "1.0 KB");
        // ADR §4 literal example.
        assert_eq!(humanize_bytes(22_800), "22.8 KB");
        assert_eq!(humanize_bytes(1_000_000), "1.0 MB");
    }

    // D-08: ephemeral detection must use component-wise `Path::starts_with`, not
    // `str::starts_with`. The mandatory `/tmpfoo/x` negative is the regression
    // guard for that distinction: as raw strings `"/tmpfoo/x".starts_with("/tmp")`
    // is true, but `/tmpfoo` is a real (non-ephemeral) directory whose data must
    // NOT collapse into the synthetic `(ephemeral)` bucket.
    #[test]
    fn is_ephemeral_matches_temp_roots_but_not_tmpfoo() {
        assert!(is_ephemeral("/tmp/x"), "/tmp/x should be ephemeral");
        assert!(
            !is_ephemeral("/tmpfoo/x"),
            "/tmpfoo/x must NOT be ephemeral (Path::starts_with is component-wise)"
        );

        // A path rooted at the runtime temp dir is ephemeral. `temp_dir()`
        // reflects `$TMPDIR` when set, so build the candidate from it rather than
        // mutating process env (which would race other tests).
        let tmp = std::env::temp_dir();
        let candidate = tmp.join("lacon-ephemeral-probe");
        assert!(
            is_ephemeral(&candidate.to_string_lossy()),
            "a path under temp_dir() ({tmp:?}) should be ephemeral"
        );
    }

    // ─── resolve_repo_root + canonical_project_key (.git resolution, D-09/D-10) ──
    //
    // All fixtures are built with std::fs under a tempdir — no `git` binary is
    // needed and the production code never shells out to git (D-09). To avoid
    // the ephemeral prefix collapsing the tempdir-rooted paths into
    // `(ephemeral)`, these resolve_repo_root tests assert against the tail of
    // the resolved path rather than full equality (the tempdir lives under
    // /tmp). The `canonical_project_key` precedence test uses an OS tempdir on
    // purpose (it MUST collapse to `(ephemeral)`).

    // D-09: a `.git` DIRECTORY means the containing dir is the repo root, and a
    // run from a subdirectory rolls up to that same root (the ancestor walk).
    #[test]
    fn resolve_repo_root_git_directory_rollup() {
        let base = tempfile::tempdir().unwrap();
        let repo = base.path().join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        let sub = repo.join("sub");
        std::fs::create_dir_all(&sub).unwrap();

        let from_root = resolve_repo_root(&repo).expect("repo root resolves");
        let from_sub = resolve_repo_root(&sub).expect("subdir resolves to repo root");
        assert!(
            from_root.ends_with("repo"),
            "repo root should end with repo: {from_root}"
        );
        assert_eq!(from_root, from_sub, "subdir must roll up to the repo root");
    }

    // D-09: a `.git` FILE with an ABSOLUTE gitdir (the `git worktree` layout):
    // wt/.git -> gitdir: <abs>/repo/.git/worktrees/wt ; commondir = "../.." ;
    // the repo root is the parent of the main `.git` (i.e. `repo`).
    #[test]
    fn resolve_repo_root_worktree_absolute_gitdir() {
        let base = tempfile::tempdir().unwrap();
        let repo = base.path().join("repo");
        let main_git = repo.join(".git");
        let admin = main_git.join("worktrees").join("wt");
        std::fs::create_dir_all(&admin).unwrap();
        // commondir points back to the main .git (conventionally "../..").
        std::fs::write(admin.join("commondir"), "../..\n").unwrap();

        let wt = base.path().join("wt");
        std::fs::create_dir_all(&wt).unwrap();
        // Absolute gitdir, as `git worktree add` writes it.
        std::fs::write(wt.join(".git"), format!("gitdir: {}\n", admin.display())).unwrap();

        let root = resolve_repo_root(&wt).expect("worktree resolves to repo root");
        assert!(
            root.ends_with("repo"),
            "worktree must resolve to the main repo root: {root}"
        );
    }

    // D-09: a `.git` FILE with a RELATIVE gitdir (the git submodule layout) must
    // resolve the relative value against the gitfile's OWN directory — the branch
    // the absolute-worktree case does not exercise.
    #[test]
    fn resolve_repo_root_submodule_relative_gitdir() {
        let base = tempfile::tempdir().unwrap();
        let repo = base.path().join("super");
        // The superproject keeps the submodule's admin dir under .git/modules/sub.
        let admin = repo.join(".git").join("modules").join("sub");
        std::fs::create_dir_all(&admin).unwrap();
        // commondir locates the MAIN .git relative to this admin gitdir: from
        // super/.git/modules/sub up to super/.git is "../..".
        std::fs::write(admin.join("commondir"), "../..\n").unwrap();

        // The submodule working dir sits at super/sub with a RELATIVE gitdir.
        let sub = repo.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        // Relative gitdir from super/sub to super/.git/modules/sub.
        std::fs::write(sub.join(".git"), "gitdir: ../.git/modules/sub\n").unwrap();

        let root = resolve_repo_root(&sub).expect("submodule resolves to superproject root");
        assert!(
            root.ends_with("super"),
            "relative gitdir must resolve against the gitfile dir: {root}"
        );
    }

    // WR-03: a malformed `.git` FILE (a gitlink with no `gitdir:` line) at a
    // child must NOT short-circuit the whole ancestor walk to None. The walk
    // must `continue` past it and resolve the PARENT repo (a real `.git/` dir).
    // Before the fix, `find_map(...)?` returned None from the function and the
    // parent repo was never reached. Both fixtures live under the SAME parent so
    // the resolved root is shared (asserted via tail-match, since tempdirs sit
    // under /tmp which is otherwise ephemeral).
    #[test]
    fn resolve_repo_root_malformed_gitlink_child_falls_through_to_parent() {
        let base = tempfile::tempdir().unwrap();
        let repo = base.path().join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        // A nested dir whose `.git` is a FILE with no `gitdir:` line (malformed).
        let child = repo.join("child");
        std::fs::create_dir_all(&child).unwrap();
        std::fs::write(child.join(".git"), "not a gitlink at all\n").unwrap();

        let root = resolve_repo_root(&child)
            .expect("a malformed child gitlink must fall through to the parent repo");
        assert!(
            root.ends_with("repo"),
            "malformed gitlink must keep walking to the parent repo root, not return None: {root}"
        );
    }

    // WR-03: a malformed `.git` FILE with no readable/usable gitlink and NO
    // parent repo above it resolves to None (the caller's literal fallback) —
    // never a panic, and never a value that skips a (here absent) parent repo.
    #[test]
    fn resolve_repo_root_malformed_gitlink_no_parent_returns_none() {
        let base = tempfile::tempdir().unwrap();
        let leaf = base.path().join("leaf");
        std::fs::create_dir_all(&leaf).unwrap();
        std::fs::write(leaf.join(".git"), "garbage with no gitdir line\n").unwrap();
        assert!(
            resolve_repo_root(&leaf).is_none(),
            "a malformed gitlink with no parent repo must be a clean None (literal fallback)"
        );
    }

    // D-10: literal fallback branch 1 — no `.git` anywhere → None (caller keeps
    // the literal path). No panic.
    #[test]
    fn resolve_repo_root_no_git_returns_none() {
        let base = tempfile::tempdir().unwrap();
        let plain = base.path().join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        assert!(resolve_repo_root(&plain).is_none());
    }

    // D-10: literal fallback branch 2 — a bare repo (`core.bare = true` in
    // <.git>/config) has no working tree → None. No panic.
    #[test]
    fn resolve_repo_root_bare_repo_returns_none() {
        let base = tempfile::tempdir().unwrap();
        let repo = base.path().join("bare");
        let git_dir = repo.join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        std::fs::write(git_dir.join("config"), "[core]\n\tbare = true\n").unwrap();
        assert!(resolve_repo_root(&repo).is_none());
    }

    // D-10: literal fallback branch 3 — a path that does not exist on disk → None
    // (no canonicalize, no panic).
    #[test]
    fn resolve_repo_root_nonexistent_path_returns_none() {
        let missing = Path::new("/this/path/should/not/exist/lacon-test");
        assert!(resolve_repo_root(missing).is_none());
    }

    // D-07: canonical_project_key precedence — ephemeral wins over .git. A repo
    // created under an OS temp root still collapses to `(ephemeral)`, never a
    // repo-root key.
    #[test]
    fn canonical_project_key_ephemeral_beats_git() {
        let base = tempfile::tempdir().unwrap(); // lives under an ephemeral root
        let repo = base.path().join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();

        let key = canonical_project_key(&repo.to_string_lossy());
        assert_eq!(
            key, "(ephemeral)",
            "an ephemeral-rooted repo must collapse to (ephemeral), not a repo root"
        );
    }

    // D-10: canonical_project_key literal fallback — a non-ephemeral path with no
    // resolvable .git returns the stored string verbatim (never regresses below
    // the exact path).
    #[test]
    fn canonical_project_key_literal_fallback() {
        let stored = "/this/path/should/not/exist/lacon-proj";
        assert_eq!(canonical_project_key(stored), stored);
    }
}
