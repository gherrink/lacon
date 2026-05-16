# Phase 3: Claude Code adapter & `lacon init` - Pattern Map

**Mapped:** 2026-05-16
**Files analyzed:** 18 (10 new, 5 modified, 3 test files)
**Analogs found:** 17 / 18

## File Classification

### Production source files (new)

| New File | Role | Data Flow | Closest Analog | Match Quality |
|----------|------|-----------|----------------|---------------|
| `crates/lacon-adapter-claudecode/src/bin/hook.rs` | binary entry | request-response (one-shot) | `crates/lacon-cli/src/main.rs` + `bin/test_emitter/src/main.rs` | role-match (sync stdin/stdout binary) |
| `crates/lacon-adapter-claudecode/src/lib.rs` | service (orchestration) | transform | `crates/lacon-cli/src/commands/run.rs` | role-match (orchestrates load → match → run) |
| `crates/lacon-adapter-claudecode/src/protocol.rs` | model (typed structs) | transform | `crates/lacon-core/src/rules/schema.rs` (lines 119-133, `RewriteSpec`) | role-match (serde Deserialize struct) |
| `crates/lacon-adapter-claudecode/src/chain.rs` | utility (pure DFA) | transform | `crates/lacon-core/src/tracking/normalize.rs` | exact (pure fn + `#[cfg(test)] mod tests`) |
| `crates/lacon-adapter-claudecode/src/tui.rs` | utility (pure heuristic) | transform | `crates/lacon-core/src/tracking/normalize.rs` | exact (pure fn + const table + tests) |
| `crates/lacon-adapter-claudecode/src/quote.rs` | utility (pure transform) | transform | `crates/lacon-core/src/tracking/normalize.rs` | exact (pure fn + tests) |
| `crates/lacon-adapter-claudecode/src/rewrite.rs` *(orchestration helper)* | utility | transform | `crates/lacon-cli/src/commands/run.rs` lines 56-69 (`try_match_via_load_all`) | role-match (rule resolution glue) |
| `crates/lacon-core/src/rules/rewrite.rs` | service (pure fn) | transform | `crates/lacon-core/src/tracking/normalize.rs` | exact (pure fn over argv + `#[cfg(test)] mod tests`) |
| `crates/lacon-cli/src/commands/init.rs` | CLI command (FS+IO) | file-I/O | `crates/lacon-cli/src/commands/validate.rs` | role-match (file-touching CLI subcommand returning `Ok(i32)`) |
| `crates/lacon-cli/src/commands/init.rs` (`install_lacon_hook` helper) | utility (JSON walker) | transform | (no analog — first JSON walker in repo) | NO ANALOG (use RESEARCH.md Pattern + serde_json idioms) |

### Test files (new)

| New Test File | Role | Data Flow | Closest Analog | Match Quality |
|---------------|------|-----------|----------------|---------------|
| `crates/lacon-adapter-claudecode/tests/chain_split.rs` | test (table-driven) | transform | `crates/lacon-core/src/tracking/normalize.rs` lines 33-75 (`mod tests`) | exact (pure-fn unit tests via `s(&[...])` helper) |
| `crates/lacon-adapter-claudecode/tests/tui_heuristic.rs` | test (table-driven) | transform | `crates/lacon-core/src/tracking/normalize.rs` lines 33-75 | exact |
| `crates/lacon-adapter-claudecode/tests/hook_e2e.rs` | test (binary spawn) | request-response | `crates/lacon-cli/tests/cli_run.rs` lines 1-44 | exact (`assert_cmd::Command::cargo_bin` + tempdir) |
| `crates/lacon-cli/tests/cli_init.rs` | test (binary spawn + FS) | file-I/O | `crates/lacon-cli/tests/cli_validate.rs` lines 1-41 + `cli_run.rs` lines 9-13 | exact (cargo_bin + tempdir + fs::read_to_string assertions) |

### Modified files (Cargo + lib stubs replaced)

| Modified File | Change | Closest Analog | Match Quality |
|---------------|--------|----------------|---------------|
| `crates/lacon-adapter-claudecode/Cargo.toml` | Add `serde`/`serde_json` deps + `[[bin]]` | `bin/test_emitter/Cargo.toml` (lines 9-11 `[[bin]]`) + `crates/lacon-cli/Cargo.toml` (lines 8-10) | exact |
| `crates/lacon-cli/Cargo.toml` | Add `serde_json` + `tempfile` to `[dependencies]` | `crates/lacon-cli/Cargo.toml` lines 12-23 (existing dep block) | exact (extend in-place) |
| `Cargo.toml` (workspace root) | Add `serde_json` to `[workspace.dependencies]` | `Cargo.toml` lines 13-33 (workspace deps block) | exact |
| `crates/lacon-adapter-claudecode/src/lib.rs` | Replace stub with orchestration | (current stub is 12 lines; replaced wholesale) | n/a |
| `crates/lacon-cli/src/commands/init.rs` | Replace stub with implementation | `crates/lacon-cli/src/commands/validate.rs` | role-match |

## Pattern Assignments

### `crates/lacon-adapter-claudecode/src/bin/hook.rs` (binary entry)

**Analog A:** `crates/lacon-cli/src/main.rs` (the canonical workspace-binary main)
**Analog B:** `bin/test_emitter/src/main.rs` (precedent for `[[bin]]` outside the CLI crate)

**Imports / `main` shape pattern** — copy from `crates/lacon-cli/src/main.rs:1-20`:
```rust
//! lacon CLI entry point — clap derive surface, 6-subcommand dispatch.

mod cli;
mod commands;

use cli::{Cli, CliCommand};
use clap::Parser;

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let exit_code = match cli.command {
        CliCommand::Run { rule, argv } => commands::run::execute(rule, argv)?,
        // ...
    };
    std::process::exit(exit_code);
}
```

**Adapt for hook (no clap; stdin JSON instead):**
- Drop the `clap::Parser`; the hook reads `serde_json::from_reader(io::stdin().lock())` instead of CLI args.
- Keep the `anyhow::Result<()>` return type (Phase 1 D-03: `anyhow` at the binary boundary).
- Library code in `lacon-adapter-claudecode::lib.rs` returns a `HookOutcome` enum; `main` dispatches on it.

**Stdout-locking pattern** — copy from `bin/test_emitter/src/main.rs:34-40`:
```rust
let stdout = std::io::stdout();
let stderr = std::io::stderr();
let mut so = stdout.lock();
let mut se = stderr.lock();
```
Use the same lock-once-then-write idiom for the rewrite-path JSON emit.

**Process exit pattern** — copy from `crates/lacon-cli/src/main.rs:19`:
```rust
std::process::exit(exit_code);
```
For the hook: pass-through path returns `Ok(())` (exit 0, no stdout); rewrite path writes JSON to locked stdout then `Ok(())`.

---

### `crates/lacon-adapter-claudecode/src/lib.rs` (orchestration)

**Analog:** `crates/lacon-cli/src/commands/run.rs` lines 19-69 (the `execute` + `try_match_via_load_all` pair)

**Imports + execute-orchestration pattern** (lines 1-54):
```rust
use std::io::Write;
use std::path::PathBuf;

use lacon_core::config::{self, Config};
use lacon_core::error::{RuntimeError, TrackingError, ValidationError};
use lacon_core::rules::loader::{ResolvedRule, RuleLoader, RuleSource};

pub fn execute(rule: Option<String>, argv: Vec<String>) -> anyhow::Result<i32> {
    if argv.is_empty() {
        eprintln!("lacon run: no command provided after `--`");
        return Ok(2);
    }

    let project_path = std::env::current_dir().ok();
    let mut loader = RuleLoader::new(project_path.clone());
    // ... resolve, dispatch, eprintln on error, return Ok(exit_code) ...
}
```

**`load_all` + first-match-wins pattern** (lines 56-69 — the closest analog to per-segment rule resolution the adapter does):
```rust
fn try_match_via_load_all(
    loader: &mut RuleLoader,
    argv: &[String],
) -> Result<Option<ResolvedRule>, Vec<ValidationError>> {
    let candidates = loader.load_all()?;
    let prog_basename = argv[0].rsplit('/').next().unwrap_or(&argv[0]).to_owned();
    for r in candidates {
        match rule_matches_argv(&r, &prog_basename, &argv[1..]) {
            Ok(true) => return Ok(Some(r)),
            Ok(false) => continue,
            Err(e) => return Err(vec![e]),
        }
    }
    Ok(None)
}
```
**What to copy:** the `load_all()` → `for ... rule_matches_argv` → first-match-wins loop. The adapter does this PER SEGMENT after chain split.
**What to adapt:** the adapter operates on per-segment argv produced by `argv_for_resolution(seg.text)` (D-08), and on match it emits a wrapped `lacon run --rule <id> -- <quoted argv>` string instead of spawning a subprocess.

**Exit-code-with-eprintln-on-error idiom** (lines 30-44 — repeated throughout `run.rs`):
```rust
match loader.resolve(&rule_id) {
    Ok(r) => Some(r),
    Err(e) => {
        eprintln!("{}", e);
        return Ok(1);
    }
}
```
The adapter mirrors this at the top of `run_hook(input)` for any unrecoverable error, but the hook's normal failure mode is to fall through to pass-through (exit 0, empty stdout) — only structural errors (e.g., malformed stdin JSON) abort with non-zero.

---

### `crates/lacon-adapter-claudecode/src/protocol.rs` (typed JSON structs)

**Analog:** `crates/lacon-core/src/rules/schema.rs` lines 118-133 (`RewriteSpec` — the closest existing serde-derived struct with `default`/`skip_serializing_if` patterns)

**Struct definition pattern** (schema.rs:118-133):
```rust
/// Pre-execution command rewrite specification.
#[derive(Deserialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct RewriteSpec {
    /// Flags to add (idempotent — won't add if already present).
    #[serde(default)]
    pub add_flags: Vec<String>,

    /// Flags to remove from argv.
    #[serde(default)]
    pub remove_flags: Vec<String>,

    /// Flag substitution map (old_flag → new_flag).
    #[serde(default)]
    pub replace_flags: BTreeMap<String, String>,
}
```

**Adapt for `BashToolInput` (RESEARCH.md Pattern 1, lines 296-340):**
- Use `#[derive(Deserialize, Serialize, Debug, Clone)]` (need both directions because we echo back).
- Use `#[serde(skip_serializing_if = "Option::is_none")]` instead of `#[serde(deny_unknown_fields)]` — Claude Code MAY add fields we don't know about; D-03 says we MUST carry them through. **Do NOT use `deny_unknown_fields`** on `BashToolInput` (anti-pattern for echo-back structs); consider `#[serde(flatten)] extra: Map<String, Value>` to capture+re-emit unknown fields losslessly.
- Use `Option<T>` for optional fields (`description`, `timeout`, `run_in_background`).

**Doc-comment pattern** (schema.rs:118):
```rust
/// Pre-execution command rewrite specification.
```
Single-line `///` doc above each struct/field. Match this style for protocol structs.

---

### `crates/lacon-adapter-claudecode/src/chain.rs` (DFA splitter — pure function)

**Analog:** `crates/lacon-core/src/tracking/normalize.rs` (the canonical pure-function-with-inline-tests module)

**Module header + doc pattern** (normalize.rs:1-11):
```rust
//! Pure command-normalization helper for `invocations.command_normalized`.
//!
//! Per CONTEXT D-18 + spec `docs/specs/tracking-data-model.md:68-72`:
//!   `<basename(argv[0])> [argv[1] if !starts_with('-')]`
//! else just `<basename(argv[0])>`.
//!
//! Normalization is implementation-defined — the spec says "may improve over time"
//! — so this fn is NOT a stable wire format. ...
```
**Copy structure:** module-level `//!` docblock citing the spec (`docs/specs/chained-commands.md:122-138`), then the pure function with `///` doc-examples, then `#[cfg(test)] mod tests`.

**Pure function + doctest pattern** (normalize.rs:13-31):
```rust
/// Derive a stable command-grouping key from `argv`.
///
/// # Examples
/// ```
/// use lacon_core::tracking::normalize;
/// assert_eq!(normalize(&["pnpm".into(), "install".into(), ...]), "pnpm install");
/// ```
pub fn normalize(argv: &[String]) -> String {
    let Some(prog) = argv.first() else {
        return String::new();
    };
    // ... pure transform, no I/O, no allocation beyond return value ...
}
```
**Copy structure:** `pub fn split_chain(input: &str) -> Vec<Segment>` with `///` doc + a few inline doctests for the simplest scenarios; full coverage moved to `tests/chain_split.rs`.

**Inline test module pattern** (normalize.rs:33-75):
```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn pnpm_install_with_flag_drops_flag() {
        assert_eq!(normalize(&s(&["pnpm", "install", "--frozen-lockfile"])), "pnpm install");
    }

    #[test]
    fn empty_argv_returns_empty_string() {
        assert_eq!(normalize(&[]), "");
    }
}
```
**Copy structure:** `#[cfg(test)] mod tests` with descriptive snake_case test names, one assertion per test, `s(&[...])` helper for `Vec<String>` construction.
**Adapt:** chain.rs needs only a handful of inline tests (smoke tests for the common path); the 13-scenario matrix lives in `tests/chain_split.rs` (separate test binary for compile isolation).

**No-existing-DFA precedent.** The DFA logic itself has no analog in the codebase — RESEARCH.md lines 466-510 give the full 7-state transition table. The pattern to copy here is *only* the module shape, not the algorithm.

---

### `crates/lacon-adapter-claudecode/src/tui.rs` (heuristic — pure function)

**Analog:** `crates/lacon-core/src/tracking/normalize.rs` (same shape: pure fn over argv-like input + const table + tests)

**Same patterns as `chain.rs` above.** Specific structural touchpoints:

**Const table + basename extraction pattern** — adapt from normalize.rs:24-25:
```rust
// normalize.rs:24-25 — basename extraction via rsplit('/'):
let basename = prog.rsplit('/').next().unwrap_or(prog);
```
**Adapt** to use `std::path::Path::new(command).file_name().and_then(OsStr::to_str)` per RESEARCH.md line 577 (more correct on Windows-style paths even though v1 is Linux+macOS only).

**Const slice declaration** — there is NO existing `const &[&str]` table in the codebase. Closest approximation is the literal subcommand list at `crates/lacon-cli/tests/cli_surface.rs:6-8`:
```rust
const ALLOWED_SUBCOMMANDS: &[&str] = &[
    "run", "validate", "init", "stats", "explain", "doctor",
];
```
**Copy:** the `const NAME: &[&str] = &[...]` shape with one entry per line, grouped by category-comment per RESEARCH.md lines 554-567.

---

### `crates/lacon-adapter-claudecode/src/quote.rs` (POSIX shell-quote — pure function)

**Analog:** `crates/lacon-core/src/tracking/normalize.rs` (pure fn + tests)

**Module-shape patterns identical to chain.rs/tui.rs above.**

**Cow<str> return pattern** — no existing `Cow<str>` returner in the codebase. RESEARCH.md lines 707-725 give the full algorithm. The pattern to copy is the unit-test convention (one descriptive test per case) from normalize.rs lines 41-74.

**Round-trip-via-shell test pattern** — RESEARCH.md lines 743-770 give the full pattern (`std::process::Command::new("/bin/sh").arg("-c").arg(&cmd).output()`). No existing analog in the codebase — this is the first test that shells out for verification. Treat as a new pattern justified by the security property in D-22.

---

### `crates/lacon-core/src/rules/rewrite.rs` (`apply_rewrite` — pure function)

**Analog:** `crates/lacon-core/src/tracking/normalize.rs` (pure fn over argv + inline tests)

**File location precedent.** `rewrite.rs` lives next to `schema.rs` and `loader.rs` in `crates/lacon-core/src/rules/`. Add to the module list in `crates/lacon-core/src/rules/mod.rs` (currently lines 1-13):
```rust
//! Rule schema, loader, extends flatten — filled by PLAN-03.

pub mod bundled;
pub mod loader;
pub mod schema;

pub use loader::{RuleLoader, ResolvedRule, RuleSource};
pub use schema::{
    BypassWhen, MatchSpec, OnErrorSpec, RewriteSpec, RuleFile, ScriptSpec, StageSpec,
    // Arg types
    CollapseArgs, DedupeArgs, HeadTailArgs, KeepAroundArgs, ReplaceRegexArgs,
};
```
**Adapt:** add `pub mod rewrite;` and re-export `pub use rewrite::apply_rewrite;` so adapters can import as `lacon_core::rules::apply_rewrite` (matches the `use lacon_core::rules::loader::RuleLoader` style in `commands/run.rs:14`).

**Function signature + body pattern** — RESEARCH.md lines 631-663 give the full implementation. The shape mirrors normalize.rs:21-31 (early return on empty, build new Vec, no I/O, deterministic output).

**Test layout decision.** Per RESEARCH.md line 617 ("planner MUST require [10 regression tests] in `crates/lacon-core/tests/rewrite.rs` (new file) or as a unit test module in `rules/rewrite.rs`"). The codebase precedent is **inline tests for short pure functions** (normalize.rs has 7 tests inline, ~75 lines total). For 10 tests covering ≤200 lines, follow the inline pattern. If the planner wants integration test isolation (separate test binary), the analog is `crates/lacon-core/tests/tracking_normalize.rs` (file presence verified above).

---

### `crates/lacon-cli/src/commands/init.rs` (CLI command)

**Analog:** `crates/lacon-cli/src/commands/validate.rs` (the file-touching CLI command that returns `anyhow::Result<i32>`)

**Imports + signature pattern** (validate.rs:1-19):
```rust
//! lacon validate subcommand: lint a rule or config file.
//!
//! Per D-17: dispatches by content (top-level `id` AND `match` -> rule;
//! else config). Per D-18: errors print one per line as ...

use std::path::Path;

pub fn execute(path: &Path) -> anyhow::Result<i32> {
    if !path.exists() {
        eprintln!("{}: file not found", path.display());
        return Ok(1);
    }
    // ... do work, eprintln on errors, return Ok(0) or Ok(1) ...
}
```
**Copy:** the `pub fn execute(...) -> anyhow::Result<i32>` signature, the `eprintln!` + `return Ok(1)` early-return on user-facing errors, the `Ok(0)` happy-path return.
**Adapt:** `init` takes no args (matches `cli.rs:33-35` `Init` variant has no fields). Signature is `pub fn execute() -> anyhow::Result<i32>` — replaces the current stub at `commands/init.rs:3-6`. If the planner adds `--force` or `--dry-run` (Claude's discretion per CONTEXT D-execution-discretion), update the `cli.rs:33-35` `Init` variant.

**Existing stub being replaced** (`commands/init.rs`):
```rust
//! Phase 3 implementation. Phase 1 stub: prints not-yet-implemented and exits 2.

pub fn execute() -> anyhow::Result<i32> {
    eprintln!("lacon init: not yet implemented (Phase 3 of v1 roadmap).");
    Ok(2)
}
```
Wholesale replace; do NOT keep the stub message.

**JSON walker (no analog).** The `install_lacon_hook(settings: &mut Value)` walker, `install_claude_md_block(existing, body)` string scanner, and `atomic_write_json(path, value)` helper are all FIRST-OF-KIND in the codebase. Copy implementations directly from RESEARCH.md:
- `install_lacon_hook` — RESEARCH.md lines 784-821
- `install_claude_md_block` + `append_fresh_block` — RESEARCH.md lines 836-888
- `atomic_write_json` (tempfile + persist) — RESEARCH.md lines 384-394

---

### `crates/lacon-adapter-claudecode/Cargo.toml` (modified)

**Current state** (12 lines — analog for the unchanged shape):
```toml
[package]
name = "lacon-adapter-claudecode"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
lacon-core = { path = "../lacon-core" }
```

**Analog A:** `bin/test_emitter/Cargo.toml:9-11` — for the `[[bin]]` block:
```toml
[[bin]]
name = "test_emitter"
path = "src/main.rs"
```
**Adapt:**
```toml
[[bin]]
name = "lacon-claude-hook"
path = "src/bin/hook.rs"
```

**Analog B:** `crates/lacon-cli/Cargo.toml:12-23` — for the `[dependencies]` + `[dev-dependencies]` block shape:
```toml
[dependencies]
lacon-core = { path = "../lacon-core" }
clap = { workspace = true }
anyhow = { workspace = true }
regex = { workspace = true }
etcetera = { workspace = true }

[dev-dependencies]
assert_cmd = { workspace = true }
predicates = { workspace = true }
tempfile = { workspace = true }
```
**Adapt:** add `serde = { workspace = true }`, `serde_json = { workspace = true }`, `anyhow = { workspace = true }` to `[dependencies]`. Add `assert_cmd`, `predicates`, `tempfile` to `[dev-dependencies]` (for `tests/hook_e2e.rs`). Do NOT add `clap`, `rusqlite`, `starlark`, `os_pipe`, `regex`, `etcetera` (D-02: minimal dep set for cold-start budget).

---

### `crates/lacon-cli/Cargo.toml` (modified)

**Analog:** the file itself, lines 12-23. Add to `[dependencies]`:
```toml
serde_json = { workspace = true }
tempfile = { workspace = true }
```
Note: `tempfile` is currently in `[dev-dependencies]` (line 22) — promote a duplicate entry to `[dependencies]` (Cargo allows the same crate in both sections; the runtime entry is what counts for the binary build).

---

### Workspace `Cargo.toml` (modified)

**Analog:** `Cargo.toml` lines 13-33 — the `[workspace.dependencies]` block shape:
```toml
[workspace.dependencies]
regex = "1"
serde = { version = "1", features = ["derive"] }
serde-saphyr = "0.0.26"
clap = { version = "4", features = ["derive"] }
# ...
tempfile = "3"
```
**Adapt:** add a single line per RESEARCH.md line 170:
```toml
serde_json = "1.0.149"
```
Place it next to `serde` (line 15) for grouping.

---

### `crates/lacon-adapter-claudecode/tests/chain_split.rs` (table-driven test)

**Analog:** `crates/lacon-core/src/tracking/normalize.rs` lines 33-75 (`mod tests` block)

**Copy:** the `s(&[...])` helper, descriptive test-function names, one assertion per test.

**Adapt for table-driven approach:**
```rust
// normalize.rs uses one #[test] per case — for 13 scenarios, do the same:
#[test] fn s1_single_command_no_chain() { /* ... */ }
#[test] fn s2a_two_segment_andand() { /* ... */ }
// ... 11 more ...
```
Avoid `proptest`/`rstest` macros (no existing precedent in the codebase; one `#[test]` per scenario is consistent with normalize.rs and the SC4 tests in `cli_validate.rs:177-285`).

---

### `crates/lacon-adapter-claudecode/tests/tui_heuristic.rs` (table-driven test)

**Analog:** `crates/lacon-core/src/tracking/normalize.rs` lines 33-75. Same shape as `chain_split.rs`.

---

### `crates/lacon-adapter-claudecode/tests/hook_e2e.rs` (binary-spawn integration)

**Analog:** `crates/lacon-cli/tests/cli_run.rs` lines 1-44 (the canonical `assert_cmd::Command::cargo_bin` + tempdir pattern)

**Imports pattern** (cli_run.rs:1-7):
```rust
//! Real-binary integration tests for `lacon run`. Use assert_cmd to spawn
//! the compiled `target/{debug|release}/lacon` binary.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;
```
**Adapt:** change docstring to reference `lacon-claude-hook`. Drop `predicates` if not asserting on stdout patterns (RESEARCH.md uses `serde_json::from_slice` for shape assertions instead).

**Test fixture-write helper pattern** (cli_run.rs:9-13):
```rust
fn write_rule(dir: &std::path::Path, rule_yaml: &str) {
    let rules_dir = dir.join(".lacon").join("rules");
    fs::create_dir_all(&rules_dir).unwrap();
    fs::write(rules_dir.join("test.yaml"), rule_yaml).unwrap();
}
```
**Copy verbatim** — RESEARCH.md hook_e2e.rs uses the identical helper inline.

**`Command::cargo_bin` invocation pattern** (cli_run.rs:28-43):
```rust
Command::cargo_bin("lacon")
    .unwrap()
    .current_dir(dir.path())
    .args([
        "run",
        "--rule", "filter-greet",
        "--", "/bin/sh", "-c", "echo skip me; echo keep me",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("keep me"))
    .stdout(predicate::str::contains("skip me").not());
```
**Adapt for hook (stdin-driven, not args-driven):**
```rust
Command::cargo_bin("lacon-claude-hook")
    .unwrap()
    .write_stdin(input_json)
    .output()  // not .assert() — we want to inspect both stdout and exit code
    .expect("hook binary runs");
```
RESEARCH.md lines 947-953 give the full helper (`run_hook_with_input`).

**Env-var test pattern** (cli_run.rs:122-141):
```rust
Command::cargo_bin("lacon")
    .unwrap()
    .current_dir(dir.path())
    .env("LACON_DISABLE", "1")
    .args([...])
    .assert()
    .success();
```
**Copy verbatim** for the `LACON_DISABLE=1` bypass test in `hook_e2e.rs`.

---

### `crates/lacon-cli/tests/cli_init.rs` (binary-spawn + filesystem integration)

**Analog A:** `crates/lacon-cli/tests/cli_validate.rs` lines 1-41 (file-touching + `cargo_bin("lacon")`)
**Analog B:** `crates/lacon-cli/tests/cli_run.rs` lines 9-13 (the `write_rule` tempdir helper)

**Imports pattern** (cli_validate.rs:1-4):
```rust
use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;
```
**Copy verbatim.**

**Tempdir + cargo_bin + assert.success pattern** (cli_validate.rs:19-41):
```rust
#[test]
fn validate_valid_rule_file_succeeds() {
    let dir = tempdir().unwrap();
    let rule = dir.path().join("rule.yaml");
    fs::write(&rule, r#" ... "#).unwrap();
    Command::cargo_bin("lacon")
        .unwrap()
        .args(["validate", rule.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::is_empty());
}
```
**Copy structure:** tempdir → write fixture → `Command::cargo_bin("lacon").current_dir(dir.path()).arg("init").assert().success()`. RESEARCH.md lines 1057-1086 give the full first-test pattern.

**Idempotency test pattern (run twice, compare bytes)** — no exact analog in the codebase. Closest precedent is the regression-guard test at `cli_validate.rs:273-285` (run a known-good fixture, assert success). For idempotency, run twice and assert `fs::read_to_string` equality across both runs (RESEARCH.md lines 1088-1104).

**Pre-populated user-state test** — analog is `cli_validate.rs:64-85` (write a project config file, then run `lacon validate` and assert error category). For `init`, write a pre-existing `.claude/settings.json` with user hooks, run init, assert preservation (RESEARCH.md lines 1106-1140).

---

## Shared Patterns

### Anyhow at binary boundary, thiserror inside (Phase 1 D-03)

**Source:** `crates/lacon-cli/src/main.rs` line 9 (`fn main() -> anyhow::Result<()>`) + `crates/lacon-core/src/error.rs` lines 15-126 (thiserror-derived `ValidationError`/`RuntimeError`)

**Apply to:** `crates/lacon-adapter-claudecode/src/bin/hook.rs` (anyhow), `crates/lacon-adapter-claudecode/src/lib.rs` (consider a `HookError` thiserror enum if structured errors are needed; otherwise re-use `lacon_core::error::ValidationError` from rule resolution and `anyhow::Error::from` everything else).

```rust
// error.rs:15-22 — pattern for module-internal error type
#[derive(thiserror::Error, Debug)]
pub enum ValidationError {
    #[error("{path}:{line}: InvalidRegex: {message}")]
    InvalidRegex {
        path: PathBuf,
        line: usize,
        message: String,
    },
    // ...
}
```

### Best-effort eprintln pattern (Phase 2 D-12)

**Source:** `crates/lacon-cli/src/commands/run.rs` lines 244-378 (`record_invocation` — every error becomes `eprintln!("lacon: ...")`, never alters exit code)

**Apply to:** all `lacon init` and `lacon-claude-hook` paths where "best-effort" is the right semantic. For `init`, this means: if `.claude/settings.json` write fails for IO reasons, eprintln + return Ok(1). For the hook, structural errors abort with non-zero (because the hook is on the hot path and silent failure would lose data); IO-style errors (e.g., transient stdin EOF) eprintln to stderr (which Claude Code captures and may surface to the model — be terse).

```rust
// run.rs:265-267 — example of the best-effort idiom:
Err(_) => {
    eprintln!("lacon: tracker skipped: system time before unix epoch");
    return;
}
```

### LACON_DISABLE=1 detection (Phase 1 D-24, runtime/mod.rs:175)

**Source:** `crates/lacon-core/src/runtime/mod.rs:175`:
```rust
if std::env::var("LACON_DISABLE").as_deref() == Ok("1") {
    return self.run_bypassed(argv, sink, started);
}
```

**Apply to:** `crates/lacon-adapter-claudecode/src/lib.rs` (the `detect_bypass` function — RESEARCH.md lines 911-922). **Mirror exactly** — the `as_deref() == Ok("1")` form is the locked precedent. Other values (empty, "0", "true") MUST NOT bypass.

### Subprocess argument injection mitigation (Phase 1 T-05-01)

**Source:** `crates/lacon-core/src/runtime/mod.rs:138-141`:
```rust
/// # Subprocess argument injection mitigation (T-05-01)
/// `argv` is passed as `Command::new(&argv[0]).args(&argv[1..])` — Rust's
/// `std::process::Command` never re-shell-interprets arguments. Do NOT
/// concatenate argv elements into a shell string.
```
**Apply to:** the SECURITY documentation on `quote_for_shell` in `crates/lacon-adapter-claudecode/src/quote.rs`. Per CONTEXT D-22, the adapter relies on this property to limit the trust scope of `quote_for_shell` to "must survive ONE shell parse" (Claude Code's bash invocation of `lacon run`); Phase 1's Runner downstream does not re-parse, so a quoting bug can only mis-execute the rewritten command, not propagate further.

### Argv basename extraction (commands/run.rs:61, normalize.rs:25)

**Source A:** `crates/lacon-cli/src/commands/run.rs:61`:
```rust
let prog_basename = argv[0].rsplit('/').next().unwrap_or(&argv[0]).to_owned();
```

**Source B:** `crates/lacon-core/src/tracking/normalize.rs:25`:
```rust
let basename = prog.rsplit('/').next().unwrap_or(prog);
```

**Apply to:** `crates/lacon-adapter-claudecode/src/tui.rs` (the `is_tui` basename extraction). RESEARCH.md line 577 prefers `std::path::Path::new(command).file_name()` over `rsplit('/')` for correctness on Windows-style paths — Phase 3 should follow RESEARCH.md's recommendation since v1 is macOS+Linux but the `Path` API is more semantically correct and no slower.

### `RuleLoader::new` + `load_all` invocation (Phase 1 D-14, loader.rs:110-212)

**Source:** `crates/lacon-cli/src/commands/run.rs` lines 25-26 + 60-69:
```rust
let project_path = std::env::current_dir().ok();
let mut loader = RuleLoader::new(project_path.clone());

// Later, in try_match_via_load_all:
let candidates = loader.load_all()?;
```

**Apply to:** `crates/lacon-adapter-claudecode/src/lib.rs` (the per-segment rule resolution loop). The adapter creates ONE loader for the whole hook invocation and reuses it across segments (cache amortization per Phase 1 D-15 mtime-based caching).

### CLI command stub-replacement (`commands::init::execute`)

**Source:** `crates/lacon-cli/src/commands/validate.rs` (the only non-stub CLI command file as of Phase 2)

**Apply to:** `crates/lacon-cli/src/commands/init.rs` — the wholesale replacement should mirror validate.rs's compact 29-line shape: imports → docstring → `pub fn execute(...) -> anyhow::Result<i32>`. Init is naturally larger because it does more work, but the entry-point shape is the same.

### `[[bin]]` outside the CLI crate (precedent: `bin/test_emitter/Cargo.toml`)

**Source:** `bin/test_emitter/Cargo.toml:9-11`:
```toml
[[bin]]
name = "test_emitter"
path = "src/main.rs"
```

**Apply to:** `crates/lacon-adapter-claudecode/Cargo.toml` for the new `lacon-claude-hook` binary. The `path = "src/bin/hook.rs"` form (vs. `src/main.rs`) is intentional — keeps `lib.rs` as the orchestration entry point and `bin/hook.rs` as the thin process-boundary wrapper, matching the adapter's role in the architecture diagram (RESEARCH.md lines 248-261).

### Cold-start probe extension (`benches/cold_start.rs`)

**Source:** `benches/cold_start.rs` lines 14-95 (full hand-rolled probe)

**Apply to:** add two new scenarios per RESEARCH.md lines 1146-1170:
- `lacon-claude-hook` pass-through (target ≤2ms median)
- `lacon-claude-hook` rewrite path (target ≤5ms median)

The existing `measure_one(args)` helper at line 20-24 needs an analog `measure_hook(stdin_json)` because the hook reads from stdin (RESEARCH.md lines 1156-1168 give the helper). Do NOT introduce hyperfine or criterion (Phase 1 RESEARCH.md line 825: hyperfine not installed; criterion is wrong for cold-start measurements).

## No Analog Found

| File | Role | Data Flow | Reason |
|------|------|-----------|--------|
| `install_lacon_hook` (helper inside `init.rs`) | utility (JSON walker) | transform | First serde_json::Value walker in the codebase (no existing JSON code anywhere). RESEARCH.md Pattern 3 + lines 784-821 give the implementation. |
| `install_claude_md_block` (helper inside `init.rs`) | utility (string scanner) | transform | First markdown-block-marker scanner in the codebase. RESEARCH.md lines 836-888. |
| `atomic_write_json` (helper inside `init.rs`) | utility (file I/O) | file-I/O | First `tempfile::NamedTempFile::persist` use in production code (tempfile is currently dev-only). RESEARCH.md lines 384-394. |
| `quote_for_shell` (in `quote.rs`) | utility (POSIX quoter) | transform | First shell-quoting code in the codebase. RESEARCH.md lines 707-725. The pure-fn module SHAPE has analogs (normalize.rs); the algorithm itself is novel. |
| Chain-splitter DFA (in `chain.rs`) | utility (state machine) | transform | First byte-iterating state machine in the codebase. RESEARCH.md lines 466-510 (full transition table). The MODULE shape has analogs (normalize.rs); the algorithm is novel. |

For each "no analog" file, the planner should reference the corresponding RESEARCH.md section directly in the plan's action steps. The novelty is bounded — each is a single pure function (or a single JSON walker) with a small test surface.

## Metadata

**Analog search scope:**
- `crates/lacon-core/src/` (rules, runtime, tracking, error, validate, config, pipeline, starlark_host)
- `crates/lacon-cli/src/` (main, cli, commands/{run,validate,init,doctor,explain,stats})
- `crates/lacon-cli/tests/` (cli_run, cli_validate, cli_surface, end_to_end, tracking_e2e, tracking_coldstart, tracking_best_effort)
- `crates/lacon-core/tests/` (runtime_bypass, normalize, primitives, ...)
- `crates/lacon-adapter-claudecode/src/` (current stub)
- `bin/test_emitter/` (Cargo.toml + src/main.rs)
- `benches/` (Cargo.toml + cold_start.rs)
- Workspace root `Cargo.toml`
- `.claude/settings.local.json` (existing repo precedent for the array-of-matchers shape — usable as a sanity-check fixture per RESEARCH.md line 462)

**Files scanned (read in full or in targeted ranges):** 18

**Pattern extraction date:** 2026-05-16

**Key insight for the planner:** Phase 3 has unusually high reuse for the *module shapes* (normalize.rs is the canonical pure-fn-with-tests pattern; cli_run.rs/cli_validate.rs are the canonical integration-test patterns; commands/run.rs is the canonical orchestration pattern). The *algorithms* (DFA, POSIX quoter, JSON walker) are novel — but RESEARCH.md gives concrete implementations for each, so the planner can cite RESEARCH.md line ranges rather than inventing pattern-from-spec text. The hook binary itself is a thin wrapper; nearly all the new code is pure-function library code with unit tests.
