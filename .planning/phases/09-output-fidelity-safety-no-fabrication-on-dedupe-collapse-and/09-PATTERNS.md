# Phase 9: Output-fidelity safety — no fabrication on dedupe/collapse and guaranteed LACON_DISABLE bypass - Pattern Map

**Mapped:** 2026-05-31
**Files analyzed:** 8 (6 MODIFY, 2 CREATE/EXTEND groups)
**Analogs found:** 8 / 8 — every touchpoint is a modification of an existing file or a new artifact with an in-repo analog. No greenfield files.

This phase is almost entirely "edit-in-place against a verified analog **inside the same file**." For most files the best pattern source is a sibling construct in the very file being edited (the `max_bytes` marker next to the `collapse_repeated` marker; the `!!` bypass branch next to the new env-prefix branch). The planner should treat the per-file excerpts below as the concrete shape to mirror.

## File Classification

| File | New/Mod | Role | Data Flow | Closest Analog | Match Quality |
|------|---------|------|-----------|----------------|---------------|
| `crates/lacon-core/src/pipeline/stages.rs` | MODIFY | engine primitive (streaming line transformer) | transform / streaming | `MaxBytes` marker in the **same file** (`stages.rs:450-457`) | exact (same file, same `[lacon: …]` convention) |
| `crates/lacon-adapter-claudecode/src/lib.rs` | MODIFY | adapter (hook decision fn) | request-response (PreToolUse) | the `!!` branch in `detect_bypass` **same file** (`lib.rs:46-48`) | exact (same fn, same return type) |
| `bundled-rules/git-status.yaml` | MODIFY | rule config (YAML) | transform pipeline config | `bundled-rules/tsc.yaml` (the "output IS signal" exempt pattern) | role-match (both bundled rules; tsc is the exempt template) |
| `docs/specs/filter-rule-schema.md` | MODIFY | spec / contract doc | documentation | `dedupe` / `max_bytes` primitive entries in the **same file** | exact (same doc, sibling primitive entries) |
| `tests/fixtures/git-status/many-untracked/{input,expected,meta}` | MODIFY (regenerate) | test fixture triple | transform fixture | `tests/fixtures/tsc/type-errors/` (exempt + must_keep_lines) | exact (sibling fixture, same triple structure) |
| `tests/fixtures/git-status/<new tabular scenario>/` | CREATE | test fixture triple | transform fixture | `tests/fixtures/git-status/many-untracked/` + `tsc/type-errors/meta.yaml` | exact (sibling fixture) |
| `crates/lacon-core/src/pipeline/stages.rs` (unit tests) | EXTEND | unit test | transform assertion | `collapse_repeated_*` tests **same file** (`stages.rs:593-658`) | exact (same test module) |
| `crates/lacon-adapter-claudecode/tests/hook_e2e.rs` + `lib.rs` unit tests | EXTEND | e2e + unit test | request-response assertion | `bypass_via_lacon_disable_env_emits_empty_stdout` (`hook_e2e.rs:188-201`) + `detect_bypass_*` (`lib.rs:383-406`) | exact (sibling tests) |

## Pattern Assignments

### `crates/lacon-core/src/pipeline/stages.rs` — `CollapseRepeated` marker (engine primitive, streaming transform)

**Analog:** the `MaxBytes` truncation marker in the **same file** — adopt its `[lacon: …]` convention and its flush-time emission shape (D-07).

**The marker convention to copy** (`stages.rs:450-457`, the model per D-07):
```rust
Stage::MaxBytes { truncated, dropped_bytes, .. } => {
    if *truncated {
        let marker = format!(
            "[lacon: truncated, {} more bytes dropped]",
            dropped_bytes
        );
        out.push(Cow::Owned(marker));
    }
}
```

**Fabrication surface #1 — in-run summary** (`stages.rs:288-296`, the line to change). Currently the free-form `summary_template` that *blends into* tool output:
```rust
} else {
    // Non-matching line: flush summary if we were in a run, then emit line.
    if *kept_so_far > 0 || *dropped > 0 {
        let summary = summary_template.replace("{count}", &dropped.to_string());
        out.push(Cow::Owned(summary));          // ← D-07: replace with [lacon: …] marker
        *kept_so_far = 0;
        *dropped = 0;
    }
    out.push(line);
}
```

**Fabrication surface #2 — flush path** (`stages.rs:432-444`, the SECOND place that must change — Pitfall 4). Note the CR-03 `*dropped > 0` guard that MUST be preserved:
```rust
Stage::CollapseRepeated {
    summary_template,
    kept_so_far,
    dropped,
    ..
} => {
    if *dropped > 0 {                                            // ← preserve CR-03 guard
        let summary = summary_template.replace("{count}", &dropped.to_string());
        out.push(Cow::Owned(summary));                           // ← D-07: same [lacon: …] change
        *kept_so_far = 0;
        *dropped = 0;
    }
}
```

**Target shape** (from RESEARCH Pattern 2, exact wording is D-07 discretion):
```rust
let marker = format!("[lacon: collapsed {} lines]", dropped);
out.push(Cow::Owned(marker));
```
Discretion (D-07): `summary_template` MAY be retained as an optional suffix *inside* the brackets (`[lacon: collapsed {count} lines — {summary}]`) or dropped for a fixed marker. Both surfaces (in-run + flush) must produce the **same** form.

**Dedupe — verify only, no change** (`stages.rs:256-270`, D-06). Every `out.push` is a verbatim input line; this is the fidelity-safe model the collapse fix is aligning to. Do not touch it.

---

### `crates/lacon-adapter-claudecode/src/lib.rs` — `detect_bypass` (adapter, request-response)

**Analog:** the existing `!!` branch in the **same function** — a leading-string scan that returns `true` (bypass) before any chain split. The new inline-env-prefix scan is a sibling branch with the same control-flow position (D-01, D-02).

**The function to extend** (`lib.rs:45-50`):
```rust
fn detect_bypass(command: &str) -> bool {
    if command.trim_start().starts_with("!!") {
        return true;
    }
    std::env::var("LACON_DISABLE").as_deref() == Ok("1")     // ← process-env check, kept
    // ← D-01: ADD a leading NAME=value scan of `command` that bypasses iff a
    //   leading LACON_DISABLE assignment unquotes to exactly "1" (D-03, D-04)
}
```

**Call site — already correct, no change** (`lib.rs:135-138`). The short-circuit point is exactly where D-02 wants it (before `split_chain` at line 141):
```rust
// 1. Bypass-detect (D-23/24/25): cheapest hot path — no split, no resolve.
if detect_bypass(command) {
    return Ok(HookOutcome::PassThrough);
}
```

**Value semantics to match** (`lib.rs:383-394`, the locked exact-`"1"` test — the new parser MUST agree with it after unquoting per D-03/D-04):
```rust
for v in ["", "0", "true", "yes", "2"] {
    std::env::set_var("LACON_DISABLE", v);
    assert!(!detect_bypass("echo hi"), "value {v:?} must NOT bypass");
}
std::env::set_var("LACON_DISABLE", "1");
assert!(detect_bypass("echo hi"), "value \"1\" must bypass");
```

**Parser shape** (RESEARCH Pattern 1; full grammar is over-engineering — cheap leading scan only, ≤10ms budget): scan whitespace-delimited leading tokens, accept `^[A-Za-z_][A-Za-z0-9_]*=…`, **break at the first non-assignment token** (the command word). Unquote one balanced layer of `'…'`/`"…"` before the `== "1"` compare so `LACON_DISABLE=1`, `="1"`, `='1'` all bypass; `echo LACON_DISABLE=1` breaks at `echo` → no bypass (D-04, Pitfall 3).

---

### `bundled-rules/git-status.yaml` — remove signal-collapsing stage (rule config)

**Analog:** `bundled-rules/tsc.yaml` — the canonical "the output IS the signal, so exempt the reduction floor and prove survival via `must_keep_lines`" pattern.

**The stage to remove/narrow** (`git-status.yaml:14-17`, D-08 — tab-indented file lines are signal):
```yaml
  - collapse_repeated:
      pattern: '^\t'
      max_kept: 5
      summary: "\t… {count} more changed/untracked files"   # ← tab-indented = blends into file lines (D-07 failure mode)
```

**tsc.yaml exempt pattern to mirror** (`tsc.yaml:11-18`):
```yaml
# RESEARCH Pattern 3 — the output IS the signal. … Reduction comes only
# from ANSI strip + collapsing accidental duplicates + a tail cap, NOT from
# dropping signal lines.
pipeline:
  - strip_ansi
  - dedupe: { max_kept: 1 }
  - keep_tail: { lines: 100 }
```

**Planner decision (RESEARCH Open Q2, recommendation: exempt):** removing the collapse stage makes git-status output ≈ input, breaching the `bundled_rules.rs` ≥50% floor (`bundled_rules.rs:120-135`). Recommended: drop the stage, set `exempt_from_reduction_check: true` on the regenerated fixture, add `must_keep_lines`, document in `meta.yaml` notes — exactly the tsc treatment. The `on_error` block (`git-status.yaml:19-25`) is untouched (ADR-0010, not affected by D-08).

---

### `docs/specs/filter-rule-schema.md` — marker contract (spec, D-12)

**Analog:** the `dedupe` and `max_bytes` primitive entries in the **same doc** (sibling format).

**The entry to update** (`filter-rule-schema.md:128-137`):
```markdown
**`collapse_repeated: { pattern, max_kept, summary }`** — collapses consecutive lines that all match `pattern` into `max_kept` examples plus a summary line.
...
The placeholder `{count}` in `summary` is replaced with the number of dropped lines.
```
Update to describe the standardized `[lacon: …]` elision marker (D-12). Keep additive — the primitive survives (D-07). If `summary` is retained as an optional suffix inside the marker, document that; if dropped, document its removal. This is a deliberate user-facing contract change — call it out as such.

---

### `tests/fixtures/git-status/many-untracked/` — regenerate triple (fixture, D-11)

**Analog:** `tests/fixtures/tsc/type-errors/meta.yaml` — the exempt + `must_keep_lines` triple.

**Current expected.txt** (`many-untracked/expected.txt`) keeps 5 file lines + the blending summary `… 118 more changed/untracked files`. After D-08 every surviving file line must be **byte-identical** to an input line; any elision must be the `[lacon: …]` marker.

**Current meta.yaml to regenerate** (`many-untracked/meta.yaml`):
```yaml
command: git status -uall
exit_code: 0
exempt_from_reduction_check: false      # ← likely flips to true (RESEARCH Open Q2)
notes: "…collapse_repeated on ^\t collapses the file block to 5 examples + a summary line"
```

**tsc exempt meta to mirror** (`tsc/type-errors/meta.yaml`):
```yaml
exit_code: 2
exempt_from_reduction_check: true
must_keep_lines:
  - "error TS"
notes: >-
  …Reduction is exempt because tsc output IS the signal… Error survival is
  proven via must_keep_lines instead.
```

**Fixture-walker contract every fixture is asserted against** (`bundled_rules.rs:104-143`): (1) byte-exact `actual == expected` (trailing-newline tolerant), (2) reduction ≤ 0.5 unless `exempt_from_reduction_check`, (3) every `must_keep_lines` substring survives.

---

### `tests/fixtures/git-status/<new tabular scenario>/` — CREATE (fixture, D-11)

**Analog:** the `many-untracked/` triple structure (`input.txt` / `expected.txt` / `meta.yaml`) + the tsc exempt meta. New fixture reproduces the *class* the bug belongs to (aligned/tabular columns, repeated-prefix loop rows, grep hits per success-criteria #1/#3) and asserts every survivor is byte-identical to an input line. Do **not** gate on reproducing the literal `table table table` string (RESEARCH Pitfall 1 / Open Q1 — that came from a non-git-status loop).

---

### `crates/lacon-core/src/pipeline/stages.rs` (unit tests) — EXTEND (unit test)

**Analog:** the three `collapse_repeated_*` tests in the **same module** (`stages.rs:593-658`). They use `Stage::CollapseRepeated { … summary_template … }` + `run_stage(&mut s, &[lines])` and assert the exact `out` vec.

**Test to update** (`stages.rs:594-618`) — the assertion must move from the free-form summary to the new marker:
```rust
let mut s = Stage::CollapseRepeated {
    pattern,
    max_kept: 1,
    summary_template: "… {count} progress lines".to_owned(),
    kept_so_far: 0,
    dropped: 0,
};
let out = run_stage(&mut s, &["Progress: 10%", /* … */ "Done"]);
assert_eq!(out, vec!["Progress: 10%", "… 3 progress lines", "Done"]);
//                                     ^^^^^^^^^^^^^^^^^^^ ← becomes "[lacon: collapsed 3 lines]"
```

**Preserve the CR-03 regression test** (`stages.rs:638-658`, `collapse_repeated_no_spurious_summary_when_nothing_dropped`) — it must still assert NO marker is emitted when `dropped == 0`. **Add** a repeated-prefix / tabular case proving every non-marker survivor equals an input line verbatim (D-09).

---

### `crates/lacon-adapter-claudecode/tests/hook_e2e.rs` + `lib.rs` unit tests — EXTEND (e2e + unit)

**e2e analog** (`hook_e2e.rs:188-201`, today tests only process-env bypass):
```rust
#[test]
fn bypass_via_lacon_disable_env_emits_empty_stdout() {
    let dir = tempfile::tempdir().unwrap();
    write_rule(dir.path(), ECHO_RULE);
    let payload = bash_payload(&dir.path().to_string_lossy(), "echo hi");
    let output = run_hook_with_input_and_env(&payload, &[("LACON_DISABLE", "1")]);
    assert!(output.status.success());
    assert!(output.stdout.is_empty(), "empty stdout expected on LACON_DISABLE=1 bypass");
}
```
New e2e: `inline_lacon_disable_prefix_passes_through` — same shape but the prefix is **in the command string** via `bash_payload(cwd, "LACON_DISABLE=1 echo hi")`, asserting empty stdout (= no rewrite). Add a quoting variant and a chain variant. Note (RESEARCH Open Q3): the hook never emits the command's stdout on bypass, so "empty stdout = pass-through" is the hook-level proof; byte-exact execution is the D-05 engine backstop, already tested.

**e2e helpers available** (`hook_e2e.rs:23-71`): `run_hook_with_input`, `run_hook_with_input_and_env`, `write_rule`, `bash_payload`, `updated_command`, `ECHO_RULE`.

**Unit analog** (`lib.rs:383-406`, the `detect_bypass_*` tests). Extend with: quoting variants (`LACON_DISABLE=1` / `="1"` / `='1'` all bypass), leading-position-only, and the negative `echo LACON_DISABLE=1` (must NOT bypass). Reuse the `ENV_LOCK` guard the existing tests use to serialize env mutation.

## Shared Patterns

### The `[lacon: …]` elision-marker namespace
**Source:** `crates/lacon-core/src/pipeline/stages.rs:450-457` (`[lacon: truncated, N more bytes dropped]`) and `crates/lacon-core/src/runtime/mod.rs:288` (`<line> [lacon: line truncated]`).
**Apply to:** the `collapse_repeated` summary at BOTH emission sites (`stages.rs:288-296` in-run, `432-444` flush), and the spec entry (`filter-rule-schema.md:128-137`).
A marker is recognizable as tool-*injected* (not tool-*emitted*) iff it carries the leading `[lacon:` token and does NOT inherit the formatting of the lines it replaces (the tab-indent of the old git-status template is the anti-pattern, D-07).

### Bypass-before-wrap (whole-command)
**Source:** `crates/lacon-adapter-claudecode/src/lib.rs:45-50` (`detect_bypass`) returning into `lib.rs:136-138` (`PassThrough` before `split_chain`).
**Apply to:** the new inline-env-prefix branch. Bypass must short-circuit here, never after wrap (Pitfall 2). Whole-command granularity only — no per-segment (CON-chained-bypass-whole-command, deferred).

### Fixture triple + walker contract
**Source:** `crates/lacon-core/tests/bundled_rules.rs:104-143` (byte-exact, reduction-floor, must_keep_lines) + `tests/fixtures/tsc/type-errors/meta.yaml` (the exempt template).
**Apply to:** every fixture this phase regenerates or adds. `exempt_from_reduction_check: true` + `must_keep_lines` is the standard escape when "the output is the signal."

### Build-before-test for bundled-rule edits
**Source:** CLAUDE.md load-bearing note + RESEARCH Pitfall 5.
**Apply to:** any `git-status.yaml` change — `cargo build --workspace && cargo test --workspace` (the YAML is rust-embed'd into `lacon-core` at build time; bare `cargo test` runs stale embedded bytes and also panics on unresolved helper bins).

## No Analog Found

None. Every file is a modification of an existing file or a new artifact (fixture/test) that has a direct sibling analog in the repo. RESEARCH confirms zero new dependencies and zero new modules — the bypass fix is one helper + one branch; the fabrication fix is a marker change at two call sites plus a YAML edit.

## Metadata

**Analog search scope:** `crates/lacon-core/src/pipeline/stages.rs`, `crates/lacon-adapter-claudecode/src/{lib.rs,tests/hook_e2e.rs}`, `crates/lacon-core/tests/bundled_rules.rs`, `bundled-rules/{git-status,tsc}.yaml`, `tests/fixtures/{git-status,tsc}/`, `docs/specs/filter-rule-schema.md`.
**Files scanned:** 9 (all verified against source this session; line numbers re-confirmed, not assumed from RESEARCH).
**Pattern extraction date:** 2026-05-31
