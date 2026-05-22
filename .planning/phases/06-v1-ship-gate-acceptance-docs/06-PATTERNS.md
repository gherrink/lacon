# Phase 6: v1 ship gate — acceptance & docs - Pattern Map

**Mapped:** 2026-05-22
**Files analyzed:** 9 new/modified artifacts
**Analogs found:** 7 with in-repo analog / 9 total (1 green-field, 1 doc-source-only)

> Phase 6 is **validation + documentation, not new product code**. Almost every
> "new" file is a test, a CI workflow, a benchmark wrapper, or a Markdown doc.
> The dominant pattern is *copy an existing test/bench shape and parameterize it*,
> not author new machinery (per CONTEXT D-01..D-10 and RESEARCH "Don't Hand-Roll").

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `crates/lacon-cli/tests/pnpm_e2e.rs` (NEW, D-07) | test (integration, black-box) | request-response (init→hook→run) | `crates/lacon-adapter-claudecode/tests/hook_e2e.rs` + `crates/lacon-cli/tests/end_to_end.rs` | exact (hermetic half) + exact (`#[ignore]` real half via `runtime_signal.rs:47`) |
| `crates/lacon-cli/tests/hot_reload.rs` (NEW, D-06) | test (integration, black-box) OR loader unit | event-driven (file mtime → cache invalidation) | `crates/lacon-cli/tests/end_to_end.rs` (black-box) / `crates/lacon-core/src/rules/loader.rs:262-274` (unit) | exact (black-box) / role-match (unit) |
| `crates/lacon-cli/tests/cli_explain.rs` (MODIFY, D-03) | test (integration) | request-response (byte-replay) | itself — `cli_explain.rs:86-119` (existing 5 tests) | exact (extend in place) |
| `crates/lacon-core/benches/tracker_open.rs` (MODIFY, D-05) | bench (criterion gate) | batch (timed open loop) | itself — `tracker_open.rs:30-86` (existing first-run bench) | exact (add steady-state variant) |
| `.github/workflows/ci.yml` (NEW, D-08/D-09) | config (CI orchestration) | batch (build→test→bench per OS lane) | **none — green-field** (`.github/` absent) | NO ANALOG |
| benchmark entry point — `scripts/bench-cold-start.sh` OR `Makefile`/cargo alias (NEW, D-04) | utility (operator script) | batch (build then probe) | `benches/cold_start.rs:105-191` (the bin it wraps) | role-match (wrapper of existing bin) |
| `README.md` (REWRITE, D-10) | doc | — | itself `README.md:1-24` (Documentation section to keep) + `docs/specs/filter-rule-schema.md` (quickstart source) | exact (rewrite in place) |
| `docs/worked-example.md` (NEW, D-10) | doc | — | `docs/specs/filter-rule-schema.md:213-233` (source material) | doc-source (extract, no code analog) |
| `docs/primitive-reference.md` (NEW, D-10) | doc | — | `docs/specs/filter-rule-schema.md:98-152` (source) + `tests/fixtures/primitives/<name>/{input,expected}.txt` (verifiable examples) | doc-source (extract, no code analog) |

---

## Pattern Assignments

### `crates/lacon-cli/tests/pnpm_e2e.rs` (test, request-response) — NEW (D-07)

Two artifacts in one file: a **hermetic stub variant** (runs in default `cargo test`/CI) and a **`#[ignore]`-gated real variant** (manual only). Copy two distinct analogs.

**Analog A (hermetic half):** `crates/lacon-cli/tests/end_to_end.rs` — drives `lacon run` against the `test_emitter` stub.

**Stub-binary resolution + rule-writing helpers** (`end_to_end.rs:19-32`) — copy verbatim:
```rust
fn write_rule(dir: &std::path::Path, content: &str) {
    let rules_dir = dir.join(".lacon").join("rules");
    fs::create_dir_all(&rules_dir).unwrap();
    fs::write(rules_dir.join("test.yaml"), content).unwrap();
}

/// Resolves the cargo-built artifact, NOT a PATH lookup (anti-spoofing, T-07-04).
fn test_emitter_path() -> PathBuf {
    assert_cmd::cargo::cargo_bin("test_emitter")
}
```

**Core E2E shape** (`end_to_end.rs:34-78`) — match the stub command name, run `lacon run`, assert filtered output:
```rust
let dir = tempdir().unwrap();
let emitter_path = test_emitter_path();
let emitter_name = emitter_path.file_name().unwrap().to_str().unwrap();
write_rule(dir.path(), &format!("id: pnpm-stub\nmatch: {{ command: {} }}\npipeline:\n  - strip_ansi\n", emitter_name));
Command::cargo_bin("lacon").unwrap()
    .current_dir(dir.path())
    .args(["run", "--rule", "pnpm-stub", "--", emitter_path.to_str().unwrap(), "--stdout-lines", "3"])
    .assert().success()
    .stdout(predicate::str::contains("line 1"));
```

**Analog A2 (full init→hook→run chain):** `crates/lacon-adapter-claudecode/tests/hook_e2e.rs` — to exercise the *PreToolUse rewrite* step (not just `lacon run`), drive the hook binary via stdin JSON.

**Hook-driver + payload builder** (`hook_e2e.rs:22-71`):
```rust
fn run_hook_with_input(input_json: &str) -> Output {
    Command::cargo_bin("lacon-claude-hook").unwrap()
        .write_stdin(input_json).output().expect("hook binary runs")
}
fn bash_payload(cwd: &str, command: &str) -> String {
    serde_json::json!({
        "session_id": "s1", "transcript_path": "/t", "cwd": cwd,
        "permission_mode": "default", "hook_event_name": "PreToolUse",
        "tool_name": "Bash", "tool_input": { "command": command }, "tool_use_id": "u1"
    }).to_string()
}
/// Parse the rewrite response — assert it wraps as `lacon run --rule <id> -- <cmd>`.
fn updated_command(output: &Output) -> String {
    let value: Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    value["hookSpecificOutput"]["updatedInput"]["command"].as_str().unwrap().to_owned()
}
```

**Analog B (`#[ignore]` real half):** `crates/lacon-core/tests/runtime_signal.rs:46-48` — the project's OWN house style. The `#[ignore = "..."]` string IS the runbook line (it prints in test output). Match this verbatim:
```rust
#[test]
#[ignore = "requires pnpm — run via `cargo test -p lacon-cli --test pnpm_e2e -- --ignored`"]
fn pnpm_e2e_real() {
    // 1. `lacon init` in a fresh tempdir project.
    // 2. Drive lacon-claude-hook with a PreToolUse JSON payload for `pnpm install`.
    // 3. Execute the rewritten `lacon run --rule pkg-install -- pnpm install`.
    // 4. Assert filtered output is non-empty and reduced vs raw.
}
```

**Sandboxing (mandatory, Pitfall 4):** every test uses `tempdir()` for the project cwd; redirect `XDG_DATA_HOME`/`XDG_CONFIG_HOME` to a tempdir (see Shared Patterns → Test Sandboxing) so the test never touches the developer's real `~/.claude/settings.json` or `~/.local/share/lacon/history.db`.

---

### `crates/lacon-cli/tests/hot_reload.rs` (test, event-driven) — NEW (D-06)

Two acceptable shapes (CONTEXT "Claude's Discretion"). Prefer the black-box one for an end-to-end proof.

**Shape 1 — black-box (preferred). Analog:** `crates/lacon-cli/tests/end_to_end.rs` (same `write_rule` + `Command::cargo_bin("lacon")` + `current_dir` shape).
```
// 1. write_rule(dir, rule_v1) → run `lacon run --rule R -- <emitter>` → assert output_v1.
// 2. Overwrite the SAME rule file with rule_v2 (different pipeline).
//    NOTE: mtime resolution — touch/sleep or set an explicitly later mtime so the
//    cache key (path, mtime) changes (loader.rs:88 CacheKey = (PathBuf, SystemTime)).
// 3. Run `lacon run --rule R -- <emitter>` AGAIN (fresh process) → assert output_v2.
// Each invocation is a fresh OS process (no daemon, ADR-0013) so the second run
// re-reads the edited file. This proves hot reload with NO new mechanism.
```

**Shape 2 — loader unit test. Analog:** `crates/lacon-core/src/rules/loader.rs:262-274` (the mtime cache check) — drive `RuleLoader::resolve` twice across an mtime-changing edit and assert the second resolve reflects the new pipeline. The cache invalidation contract to exercise:
```rust
// loader.rs:262-274 — cache hit ONLY when (path, mtime) matches; an edit changes
// mtime → cache miss → full re-parse:
let cache_hit = if let Ok(meta) = std::fs::metadata(path) {
    meta.modified().ok().and_then(|mtime| {
        let key = (path.to_owned(), mtime);
        self.cache.get(&key).cloned().map(|v| (key, v))
    })
} else { None };
```

**Anti-pattern (RESEARCH):** do NOT add a file-watcher or daemon — that contradicts the locked no-daemon ADR-0013. The proof asserts existing behavior; it builds nothing.

---

### `crates/lacon-cli/tests/cli_explain.rs` (test, request-response) — MODIFY (D-03)

**Analog:** itself — extend the existing 5 tests in place. Reuse the existing seeding harness verbatim (`cli_explain.rs:22-84`): `SCHEMA_DDL`, `init_db`, `write_drop_noise_rule`, `db_path_under`, and the `lacon(xdg, proj)` builder (which already redirects `XDG_DATA_HOME`/`XDG_CONFIG_HOME`).

**Existing side-by-side test to copy structure from** (`cli_explain.rs:86-119`):
```rust
#[test]
fn explain_with_stored_raw_renders_side_by_side() {
    let xdg = tempdir().unwrap();
    let proj = tempdir().unwrap();
    write_drop_noise_rule(proj.path(), "cargo-rule");
    let conn = init_db(xdg.path());
    let raw = b"kept line one\nnoise dropped line\nkept line two\n";
    conn.execute("INSERT INTO raw_outputs ...", rusqlite::params![raw.to_vec()]).unwrap();
    conn.execute("INSERT INTO invocations ...", rusqlite::params![/* ... */]).unwrap();
    let assert = lacon(xdg.path(), proj.path()).args(["explain", "1"]).assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
    assert!(stdout.contains("kept line one"));
}
```

**D-03 gap to fill (only if absent):** the existing 5 tests assert *substring presence* in the rendered columns, NOT byte-for-byte equality of the re-derived filtered column vs the original `lacon run` output. Add ONE explicit byte-equality test: seed raw bytes, compute the expected filtered bytes by running the same rule's pipeline (or run `lacon run` once and capture stdout), then assert `explain`'s filtered column equals that byte-for-byte. Keep the RAW column verbatim and the filtered column's C0/C1/ESC neutralization intact (Security Domain note; do not regress WR-01).

---

### `crates/lacon-core/benches/tracker_open.rs` (bench, batch) — MODIFY (D-05)

**Analog:** itself — keep the existing `bench_tracker_open` (rename its function label to `tracker_open_first_run`, already labeled at line 34) as a NON-gating diagnostic, and ADD a steady-state variant that pays the migration cost ONCE outside the timed loop.

**Existing first-run loop to preserve** (`tracker_open.rs:30-60`) — fresh tempdir per iteration → includes migration COMMIT fsync (this is the ~25ms-on-ext4 number; demote it to reported-only):
```rust
c.bench_function("tracker_open_first_run", |b| {
    b.iter_custom(|iters| {
        let mut elapsed = Duration::ZERO;
        for _ in 0..iters {
            let tmp = tempfile::TempDir::new().unwrap();
            let db_path = tmp.path().join("lacon").join("history.db");
            let start = Instant::now();
            let tracker = Tracker::open(black_box(&db_path), black_box(&default_retention()), false, FIXED_NOW_MS).expect("open ok");
            elapsed += start.elapsed();
            drop(tracker); drop(tmp);
        }
        elapsed
    });
});
```

**New steady-state variant to add** (create DB once OUTSIDE the timed section; gate the budget on this) — pattern per RESEARCH §"Pattern 3", consistent with the existing `default_retention()`/`FIXED_NOW_MS`/`BUDGET_MICROS` constants (`tracker_open.rs:19-28`):
```rust
fn bench_tracker_open_steady_state(c: &mut Criterion) {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("lacon").join("history.db");
    // One-time creation OUTSIDE the timed section — migration fsync paid once:
    drop(Tracker::open(&db_path, &default_retention(), false, FIXED_NOW_MS).unwrap());
    c.bench_function("tracker_open_steady_state", |b| {
        b.iter_custom(|iters| {
            let start = Instant::now();
            for _ in 0..iters {
                let t = Tracker::open(&db_path, &default_retention(), false, FIXED_NOW_MS).unwrap();
                drop(t);
            }
            start.elapsed()
        });
    });
    // Re-target the assert!(mean < BUDGET_MICROS) gate (tracker_open.rs:80-85)
    // onto THIS steady-state number.
}
// Register both: criterion_group!(benches, bench_tracker_open, bench_tracker_open_steady_state);
```

**Source-path facts the planner needs** (verified in `crates/lacon-core/src/tracking/`):
- `Tracker::open` (`mod.rs:82-114`) always runs steps 1-5 every call: ensure dir → open conn → `apply_connection_pragmas` (incl. `journal_mode=WAL` write, `mod.rs:199-203`) → `migrate` → `prune_if_due`.
- `migrate` (`migrations.rs:38-53`) **early-returns** when `PRAGMA user_version >= TARGET_VERSION` (line 41-43) — so on an existing DB it does NO `BEGIN IMMEDIATE`/`COMMIT`, eliminating the migration-COMMIT fsync. **This early-return is what makes the steady-state vs first-ever split a pure measurement-protocol change, not a code change** — `Tracker::open` already costs less on the second-and-later call. The D-05 fix is therefore a NEW BENCH VARIANT (+ gate re-target), not a source edit. The `prune_if_due` 24h throttle similarly skips work after the first run.
- This matches RESEARCH Open Question 2 / Pitfall 2: gate on steady-state; keep first-ever as reported diagnostic; optionally re-measure first-ever on tmpfs.

**Wiring:** `[[bench]] harness = false` is already set (`crates/lacon-core/Cargo.toml:43-45`); `criterion = "0.5"` is already a dev-dependency (line 41). No manifest change needed to add a second `bench_function` in the same file.

---

### `.github/workflows/ci.yml` (config, batch) — NEW (D-08/D-09) — GREEN-FIELD

**No in-repo analog.** `.github/` does not exist (verified: `NO .github DIR`). The planner authors this fresh from RESEARCH §"Standard Stack" + §"Pitfall 1/3", not from a copied file. Key constraints (all from CONTEXT/RESEARCH, not invented):

- **Two lanes:** `ubuntu-latest` + `macos-latest` (D-09). macOS lane produces SC1's macOS cold-start number the Linux-only dev machine cannot.
- **Hermetic by construction (D-08, Pitfall 3):** steps are limited to `cargo build` + `cargo test` (NO `--ignored`) + the cold-start probe. NEVER `brew install pnpm` / `npm i -g` / `pip install` / system `libsqlite3`. `rusqlite[bundled]` (workspace `Cargo.toml:27`) removes the only system-library temptation.
- **Cold-start probe step** must `cargo build --release` first (the probe errors if `target/release/lacon` is absent — `cold_start.rs:107-110`), then `cargo run --release --bin cold_start_probe`.
- **macOS gate is SOFT** (Pitfall 1): report the **min** of N warmed samples (the probe already discards 3 warm-ups and computes min — `cold_start.rs:62-64,82`); do NOT hard-assert `<10ms` wall-clock on the shared macOS VM. Record the number into `docs/architecture.md`'s measurements table.
- **Action pinning (A1, Security V14):** the conservative baseline uses ONLY `actions/checkout@v4` + the runner's pre-installed Rust. If adding `dtolnay/rust-toolchain` / `Swatinem/rust-cache`, verify the slug and pin to a major-version tag or SHA. No `secrets.*`.

---

### benchmark entry point — `scripts/bench-cold-start.sh` (or Makefile/cargo alias) (utility, batch) — NEW (D-04)

**Analog:** the binary it wraps — `benches/cold_start.rs:105-191` (`cold_start_probe`). `scripts/` does not exist yet (verified: `NO scripts DIR`); creating it is fine. This is a thin wrapper, NOT a new harness (D-04 forbids authoring a new harness).

**What the wrapper must do** (derived from the probe's own contract):
- `cargo build --release` for ALL bins first — the probe checks `target/release/lacon` (`cold_start.rs:107-110`) AND `target/release/lacon-claude-hook` (`cold_start.rs:138`, warns + skips hook scenarios if absent). Both must exist for the hook hot-path number (the load-bearing one per D-04/D-05).
- Then `cargo run --release --bin cold_start_probe`.
- The probe already emits a Markdown table labeled with `std::env::consts::OS` (`cold_start.rs:112-118`) — the wrapper just needs to capture/redirect that output for pasting into `docs/architecture.md`.

Exact form (shell vs Makefile vs cargo alias) is Claude's Discretion (D-04).

---

### `README.md` (doc) — REWRITE (D-10)

**Analog:** itself. KEEP the existing `## Documentation` link list (`README.md:9-21`) and `## License` (`README.md:22-24`); ADD the new `docs/worked-example.md` + `docs/primitive-reference.md` links to that section. REPLACE the design-status stub:
- Line 5 (`> **Status:** in design. No installable artifact yet.`) → flip to install + quickstart (D-10, "State of the Art" deprecation note).
- Quickstart content sources: `lacon init` → hook wiring (Phase 3), the `lacon run`/`validate`/`stats`/`explain`/`doctor` surface, and a minimal rule example. The worked-example/primitive docs are the deep links.

---

### `docs/worked-example.md` (doc) — NEW (D-10)

**No code analog — doc-source extraction.** Source material: `docs/specs/filter-rule-schema.md:213-233` (the existing "Worked example" block). Extract and polish — do NOT green-field (Pitfall 5: doc drift). The canonical example is the `our-monorepo-pnpm` rule that `extends: bundled/pkg-install` and adds two `drop_regex` stages:
```yaml
# .lacon/rules/our-monorepo-pnpm.yaml  (source: filter-rule-schema.md:216-225)
id: our-monorepo-pnpm
description: pnpm install in our monorepo (verbose lockfile output we want to strip)
extends: bundled/pkg-install
pipeline:
  - drop_regex: '^Lockfile is up to date'
  - drop_regex: '^Already up to date'
```
Preserve the three explanatory bullets (`filter-rule-schema.md:227-231`) about inheritance/prepend/resolution-precedence so the doc stays consistent with the schema contract (ADR-0012, CLAUDE.md inheritance semantics).

---

### `docs/primitive-reference.md` (doc) — NEW (D-10)

**No code analog — doc-source extraction + fixture-verifiable examples.** Source canonical behavior from `docs/specs/filter-rule-schema.md:98-152` (all 10 primitives: `strip_ansi`, `drop_regex`, `keep_regex`, `replace_regex`, `dedupe`, `collapse_repeated`, `keep_head`, `keep_tail`, `keep_around_match`, `max_bytes`). One worked input→output example per primitive (REQ-docs-primitive-reference).

**Drift-prevention (Pitfall 5):** derive each example's input→output from the existing golden fixtures at `tests/fixtures/primitives/<name>/{input.txt,expected.txt}` (the byte-exact tested behavior — driven by `crates/lacon-core/tests/primitives.rs:16-44`). This makes every doc example literally the tested behavior. Note the spec semantics that must be reproduced exactly: `keep_regex` whitelist/OR mode, `dedupe` `max_kept` default 1, `collapse_repeated` `{count}` placeholder, `keep_around_match` grep -B/-A semantics, `max_bytes` `[lacon: truncated, N more bytes dropped]` marker (must be last stage).

---

## Shared Patterns

### Test Sandboxing (XDG redirect + tempdir cwd)
**Source:** `crates/lacon-cli/tests/cli_explain.rs:78-84` and `crates/lacon-cli/tests/tracking_coldstart.rs:34-49`
**Apply to:** every new/modified test (`pnpm_e2e.rs`, `hot_reload.rs`, `cli_explain.rs` additions) — mandatory (RESEARCH Pitfall 4: tests must never mutate the developer's real `~/.claude/settings.json` or `~/.local/share/lacon/history.db`).
```rust
fn lacon(xdg: &Path, proj: &Path) -> Command {
    let mut cmd = Command::cargo_bin("lacon").unwrap();
    cmd.current_dir(proj)
        .env("XDG_DATA_HOME", xdg)
        .env("XDG_CONFIG_HOME", xdg.join("config"));
    cmd
}
// Caller: let xdg = tempdir().unwrap(); let proj = tempdir().unwrap();
```

### Anti-spoofing stub-binary resolution
**Source:** `crates/lacon-cli/tests/end_to_end.rs:30-32`
**Apply to:** `pnpm_e2e.rs` (and any test invoking a workspace binary). Resolve the cargo artifact, never PATH.
```rust
fn test_emitter_path() -> PathBuf { assert_cmd::cargo::cargo_bin("test_emitter") }
// CLI under test: Command::cargo_bin("lacon")  /  Command::cargo_bin("lacon-claude-hook")
```

### Project-rule writer
**Source:** `crates/lacon-cli/tests/end_to_end.rs:19-23` (identical helper in `hook_e2e.rs:42-46`)
**Apply to:** `pnpm_e2e.rs`, `hot_reload.rs`.
```rust
fn write_rule(dir: &std::path::Path, content: &str) {
    let rules_dir = dir.join(".lacon").join("rules");
    fs::create_dir_all(&rules_dir).unwrap();
    fs::write(rules_dir.join("test.yaml"), content).unwrap();
}
```

### `#[ignore = "<runbook line>"]` for tool/interactive-dependent tests
**Source:** `crates/lacon-core/tests/runtime_signal.rs:47` (house style)
**Apply to:** the real-pnpm half of `pnpm_e2e.rs`. The string is the runbook — it prints in test output and keeps CI hermetic (default `cargo test` skips it; CI must NEVER pass `--ignored`).
```rust
#[ignore = "requires pnpm — run via `cargo test -p lacon-cli --test pnpm_e2e -- --ignored`"]
```

### Plain `assert_eq!` / `assert!` — do NOT introduce `insta`
**Source:** whole suite (`primitives.rs:49`, `end_to_end.rs`, `cli_explain.rs`)
**Apply to:** all new tests. `insta` is declared (`Cargo.toml:34`, `lacon-core/Cargo.toml:38`) but **unused** — keep it that way (CONTEXT "Established Patterns", RESEARCH Anti-Patterns).

### Markdown-table cold-start reporting with per-OS label
**Source:** `benches/cold_start.rs:80-118` (`run_scenario`/`run_hook_scenario`/`std::env::consts::OS` header)
**Apply to:** the D-04 entry point and the CI macOS-number capture — the probe already emits the table; the wrapper/CI just capture it.

---

## No Analog Found

| File | Role | Data Flow | Reason |
|------|------|-----------|--------|
| `.github/workflows/ci.yml` | config | batch | `.github/` is absent (verified) — green-field. Author from RESEARCH Standard Stack (GitHub Actions: `actions/checkout@v4`, ubuntu+macos `runs-on`, pinned actions per A1) + Pitfalls 1/3, not from a copied in-repo file. |

**Docs with no *code* analog (source material exists, so not "green-field"):**
- `docs/worked-example.md` → extract from `docs/specs/filter-rule-schema.md:213-233`.
- `docs/primitive-reference.md` → extract from `docs/specs/filter-rule-schema.md:98-152` + verify against `tests/fixtures/primitives/`.

---

## Metadata

**Analog search scope:** `crates/lacon-cli/tests/`, `crates/lacon-core/tests/`, `crates/lacon-adapter-claudecode/tests/`, `crates/lacon-core/src/tracking/`, `crates/lacon-core/src/rules/`, `benches/`, `bin/test_emitter/`, `docs/specs/`, repo root (`README.md`, `Cargo.toml`, `.github/`, `scripts/`).
**Files scanned/read:** 14 (end_to_end.rs, test_emitter/main.rs, runtime_signal.rs, tracker_open.rs, cold_start.rs, cli_explain.rs, bundled_rules.rs, loader.rs, tracking/mod.rs, tracking_coldstart.rs, hook_e2e.rs, migrations.rs, primitives.rs, README.md + filter-rule-schema.md + Cargo manifests).
**Key verifications:** `.github/` and `scripts/` confirmed absent; `migrate()` early-return (`migrations.rs:41-43`) confirms D-05 split is a measurement/bench-variant change, not a `Tracker::open` source edit; `[[bench]] harness=false` + `criterion 0.5` already wired for `tracker_open`.
**Pattern extraction date:** 2026-05-22
