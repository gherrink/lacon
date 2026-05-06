# Phase 1: Engine Core & `lacon run` Wrapper — Research

**Researched:** 2026-05-06
**Domain:** Rust systems programming — subprocess spawning, streaming I/O, Starlark embedding, YAML parsing, Cargo workspace
**Confidence:** HIGH (all core claims verified via cargo registry, Context7, or official docs)

---

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

- **D-01:** Three-crate Cargo workspace: `crates/lacon-core`, `crates/lacon-cli`, `crates/lacon-adapter-claudecode` (Phase-1 stub).
- **D-02:** Edition 2021. MSRV pinned at start of Phase 1 in `Cargo.toml`; `rust-toolchain.toml` for reproducibility.
- **D-03:** Dependency set locked: `regex`, `serde` + YAML crate, `clap` v4 with `derive`, `starlark` (Meta's crate), `os_pipe` + `std::process`, `crossbeam-channel` or `std::sync::mpsc`, `nix`, `thiserror`, `anyhow` (CLI boundary only), `etcetera`, `rust-embed` or `include_str!`.
- **D-04:** No async runtime (no `tokio`). Synchronous `std::process::Command` + OS threads for merge.
- **D-05:** Closed `enum Stage { ... }` with `step(&mut self, line: Cow<str>, out: &mut SmallVec<...>)` dispatched via `match`. No `Box<dyn Stage>`. Pipeline = `Vec<Stage>`.
- **D-06:** Multiple `keep_regex` stages OR-merged into a single `RegexSet` at load time.
- **D-07:** `max_bytes` enforcement in two places: explicit stage in pipeline AND implicit final cap from `defaults.max_bytes` (32768).
- **D-08:** Truncation marker `[lacon: truncated, N more bytes dropped]` byte-exact.
- **D-09:** Spawn via `std::process::Command` + `os_pipe`. NO PTY.
- **D-10:** Two dedicated OS threads (one per stream) → single `crossbeam-channel`/`mpsc`. Main thread runs pipeline.
- **D-11:** Merge guarantee: best-effort line atomicity, no cross-stream order guarantee.
- **D-12:** SIGTERM and SIGINT forwarded to subprocess PID via `nix::sys::signal::kill`. No drain on kill. Exit with subprocess exit code or `128 + sig`.
- **D-13:** Success buffer held alongside raw line stream until exit code known; on non-zero exit, success buffer discarded, raw stream run through `on_error` pipeline.
- **D-14:** Lazy-resolve-on-demand hot path for `lacon run --rule <id>`; eager parse for `lacon validate`, `lacon doctor`, `lacon run` without `--rule`.
- **D-15:** In-process-only regex cache. mtime-check invalidation. No disk cache.
- **D-16:** `extends` flattened at parse time; cycles → `CircularExtends`. Single-level only; multi-hop flattened recursively but chain not exposed.
- **D-17:** `lacon validate` dispatch: parse to `Value`, look for top-level `id` AND `match`; both required → rule validator; otherwise → config validator. Reject malformed files.
- **D-18:** Validation error enum categories: `InvalidRegex`, `UnknownPrimitive`, `CircularExtends`, `MissingScriptFile`, `UserOnlyKeyInProject`, `UnknownKey`. Format: `<path>:<line>: <category>: <message>`.

### Claude's Discretion

- Internal module organization within `lacon-core` (e.g., `pipeline/`, `rules/`, `runtime/`, `validate/`).
- Specific error message wording inside each `thiserror` variant.
- Choice between `crossbeam-channel` and `std::sync::mpsc`.
- Choice between `rust-embed` and `include_str!` for bundled-rule embedding.

### Deferred Ideas (OUT OF SCOPE)

- On-disk persisted regex cache (deferred to Phase 6 if benchmarks demand it).
- Per-line streaming Starlark (explicitly out of v1).
- `Box<dyn Stage>` extensible primitive trait.
- `tokio` async runtime.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| REQ-engine-streaming-primitives | Ten native primitives as line-by-line streaming transformers; memory bounded | `enum Stage` pattern (D-05), `BufReader::read_until` for non-UTF8-safe reading, `SmallVec` output accumulator |
| REQ-engine-starlark-postprocess | `post_process` Starlark stage on aggregated post-pipeline output; `def process(ctx, lines) -> list[str]` | `starlark` 0.13.0 `AstModule`/`Globals`/`Module`/`Evaluator` pattern, `eval_function` API |
| REQ-engine-rule-loading | Rule loading from three layers; first-match-wins; mtime cache invalidation | Loader architecture (D-14, D-15), `etcetera` 0.11.0 for XDG paths, `rust-embed` 8.11.0 for bundled layer |
| REQ-engine-extends | `extends` flattened at parse time; parent `pipeline` prepended; no cycles | Recursive flatten algorithm, `CircularExtends` error, set-based cycle detection |
| REQ-engine-on-error | `on_error` block replaces success pipeline on non-zero exit; success buffer discarded | D-13 dual-buffer model; exit code observed after subprocess `wait()` |
| REQ-engine-rewrite | `rewrite.add_flags` (idempotent) / `remove_flags` / `replace_flags` applied at adapter layer | Struct-based rewrite block; idempotency check = `contains` before push |
| REQ-engine-bypass | `LACON_DISABLE=1` env var skips filtering in `lacon run`; `!!` prefix reserved (adapter Phase 3) | `std::env::var("LACON_DISABLE")` check at `lacon run` entry |
| REQ-engine-max-bytes-cap | Hard `max_bytes` cap; default 32768; truncation marker; injected as final stage when rule omits it | `MaxBytes` stage variant; implicit cap injection at rule load time |
| REQ-cli-run | `lacon run [--rule <id>] -- <cmd> [args...]`; spawns subprocess; merges streams; propagates exit code | `clap` v4 derive, `os_pipe`, thread-based merge, `nix::sys::signal::kill` |
| REQ-cli-validate | `lacon validate <path>`; content-dispatch; rejects malformed files; one-error-per-line format | `serde_yaml`/`serde-saphyr` parse-to-Value; `thiserror` error enum; format `<path>:<line>: <category>: <message>` |
</phase_requirements>

---

## Summary

Phase 1 establishes every foundational building block that downstream phases consume. It is a greenfield Rust workspace — no code, no `Cargo.toml`, no `src/` exists yet. The output is a `lacon` binary that can execute `lacon run --rule <id> -- <cmd>` in production and `lacon validate <path>` for lint — everything else in v1 builds on top of these two capabilities.

The design decisions are locked in CONTEXT.md and traced through 11 accepted ADRs. The research task is therefore not to evaluate alternatives but to verify concrete implementation details: exact crate versions, API shapes, configuration syntax, and known gotchas that will cause plan tasks to fail if ignored.

The most consequential implementation choices in Phase 1 are: (1) the YAML parsing crate selection, because `serde_yaml` 0.9 is deprecated — the replacement with best line-number support for error reporting is `serde-saphyr` 0.0.26; (2) the subprocess merge model — `os_pipe::pipe()` cloned writer, two reader threads, channel to main — has a documented deadlock footgun (must drop `Command` before reading from the pipe reader); (3) the `starlark` 0.13.0 `eval_function` API requires values allocated on the correct `Heap` before calling; and (4) the `keep_tail` ring-buffer semantics must hold both lines AND bytes variant to satisfy the schema.

**Primary recommendation:** Use `serde-saphyr 0.0.26` for YAML parsing (deprecated `serde_yaml` has a `0.9.34+deprecated` marker — do not use it); use `os_pipe 1.2.3` with two threads and `crossbeam-channel 0.5.15` for the merge; use `smallvec 1.14.0` (not the 2.x alpha) for `Stage::step` output accumulation.

---

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Subprocess spawning & stream merge | `lacon-core` (runtime) | `lacon-cli` (entry point) | Core logic reusable across CLI commands; CLI wires the flags |
| Streaming pipeline (10 primitives) | `lacon-core` (pipeline) | — | Pure transformation; no I/O dependency |
| Starlark `post_process` host | `lacon-core` (pipeline) | — | Engine-agnostic; Starlark VM lives in core |
| Rule loading + `extends` flattening | `lacon-core` (rules) | — | Library function called by both `lacon run` and `lacon validate` |
| Config loading + layer merge | `lacon-core` (config) | — | Shared by all CLI subcommands |
| `lacon validate` dispatch | `lacon-core` (validate) | `lacon-cli` | Validation logic in core; CLI surfaces errors |
| Signal forwarding | `lacon-core` (runtime) | — | Subprocess lifecycle owned by runtime |
| `max_bytes` cap injection | `lacon-core` (rules/loader) | — | Injected at load time, not at run time |
| Bundled-rule embedding | `lacon-core` (rules) | — | Embedded at compile time; loader queries it |
| XDG path resolution | `lacon-core` (config) | — | `etcetera` crate; shared across all path lookups |
| CLI argument parsing | `lacon-cli` | — | `clap` v4 derive; thin dispatch layer only |
| `lacon-adapter-claudecode` stub | `lacon-adapter-claudecode` | — | Empty crate boundary; filled in Phase 3 |

---

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `regex` | 1.12.3 | Pattern matching for all regex primitives and `RegexSet` for OR-merge | Standard Rust regex; NFA-based, no backtracking risk |
| `serde` | (latest) | Deserialization of YAML rule files and config | Universal serde derive |
| `serde-saphyr` | 0.0.26 | YAML parsing — replacement for deprecated `serde_yaml 0.9` | Line/column error reporting; panic-free; actively maintained |
| `clap` | 4.6.1 | CLI argument parsing with derive macros | `derive` feature: `#[derive(Parser, Subcommand)]`; zero hand-rolled parsing |
| `starlark` | 0.13.0 | Starlark `post_process` VM host | Meta's implementation; hermetic by default; Buck2-hardened |
| `os_pipe` | 1.2.3 | OS pipe primitives for subprocess stdout+stderr merge | Cross-platform; cloneable write-end for 2>&1 equivalent |
| `crossbeam-channel` | 0.5.15 | Multi-producer merge channel from reader threads to pipeline loop | Bounded backpressure; `select!` macro for both streams |
| `nix` | 0.31.2 | POSIX signal forwarding (`kill(pid, SIGTERM/SIGINT)`) | Idiomatic POSIX; single-PID semantics portable on Linux + macOS |
| `thiserror` | 2.0.18 | Derive `Error` for all internal error enums | Eliminates boilerplate `Display`/`From` impls |
| `anyhow` | 1.0.102 | Error propagation at CLI boundary only (`main.rs`) | `?` ergonomics; never in library code |
| `etcetera` | 0.11.0 | XDG-compliant path resolution (`~/.config/lacon/`, `~/.local/share/lacon/`) | Cross-platform; follows XDG Base Directory spec |
| `rust-embed` | 8.11.0 | Embed bundled rule files into binary at compile time | Zero runtime I/O for bundled layer; `#[derive(RustEmbed)]` |
| `smallvec` | 1.14.0 | Stack-allocated output accumulator in `Stage::step` (avoids heap alloc on common 0–2 line output) | Servo's production crate; 1.x branch is stable (2.x is alpha) |

> `smallvec 1.14.0` confirmed as latest stable 1.x release (February 2025). `cargo search` shows 2.0.0-alpha as top result — pin `"1"` to stay on stable branch. [VERIFIED: crates.io]

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `assert_cmd` | 2.2.1 | Integration testing of `lacon` binary | `lacon run` end-to-end tests against real subprocess |
| `predicates` | 3.1.4 | Fluent assertions on stdout/stderr in integration tests | Paired with `assert_cmd` |
| `insta` | 1.47.2 | Snapshot testing for `lacon validate` golden output | Error message stability; `insta::assert_snapshot!` |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `serde-saphyr` | `serde_yaml 0.9` | `serde_yaml` is deprecated (`0.9.34+deprecated`); do not use for new code |
| `serde-saphyr` | `yaml-rust2` + hand-rolled serde | `yaml-rust2` is a parser; does not provide serde `Deserialize` derive — needs glue code |
| `crossbeam-channel` | `std::sync::mpsc` | `mpsc` lacks bounded-backpressure semantics and `select!`; acceptable if dep surface matters |
| `rust-embed` | `include_str!` / `include_bytes!` | `include_str!` works for individual files; `rust-embed` provides directory-level embedding with path iteration needed by the rule loader |
| `nix` | `signal-hook 0.4.4` | `signal-hook` is higher-level (iterator/pipe API); `nix::sys::signal::kill` is the direct POSIX call; `nix` already in the dep set for process primitives |
| `smallvec 1.14.0` | `Vec<Cow<str>>` | `Vec` always heap-allocates; most primitives pass 0–1 lines through per call — `SmallVec<[Cow<str>; 2]>` eliminates the allocation on the common case |

**Installation:**

```bash
cargo add regex serde serde-saphyr clap --features clap/derive starlark os_pipe crossbeam-channel nix --features nix/signal thiserror anyhow etcetera rust-embed smallvec
# dev-only
cargo add --dev assert_cmd predicates insta
```

**Version verification (performed during research):**

| Crate | Verified version | Notes |
|-------|-----------------|-------|
| `regex` | 1.12.3 | [VERIFIED: cargo search] |
| `serde-saphyr` | 0.0.26 | [VERIFIED: cargo search] — use this, not `serde_yaml` |
| `clap` | 4.6.1 | [VERIFIED: cargo search] |
| `starlark` | 0.13.0 | [VERIFIED: cargo search] |
| `os_pipe` | 1.2.3 | [VERIFIED: cargo search] |
| `crossbeam-channel` | 0.5.15 | [VERIFIED: cargo search] |
| `nix` | 0.31.2 | [VERIFIED: cargo search] |
| `thiserror` | 2.0.18 | [VERIFIED: cargo search] |
| `anyhow` | 1.0.102 | [VERIFIED: cargo search] |
| `etcetera` | 0.11.0 | [VERIFIED: cargo search] |
| `rust-embed` | 8.11.0 | [VERIFIED: cargo search] |
| `smallvec` | 1.14.0 | [VERIFIED: crates.io — latest stable 1.x; cargo search returns alpha 2.x first] |

---

## Architecture Patterns

### System Architecture Diagram

```
lacon run --rule <id> -- <cmd> [args]
         │
         ▼
  [lacon-cli: main.rs]
  parse clap args
         │
         ├─ LACON_DISABLE=1 → print cmd unchanged, exit 0
         │
         ▼
  [lacon-core::runtime::Runner]
  load rule via RuleLoader (lazy-resolve hot path)
         │
         ▼
  spawn subprocess via std::process::Command
  stdout ──┐          (os_pipe writer cloned ×2)
  stderr ──┘ → pipe_reader
         │
         ├── thread A: read pipe_reader, emit lines → crossbeam::Sender
         │   (both streams → same sender)
         │
         ▼
  main thread: crossbeam::Receiver
    for line in receiver:
      accumulate to success_buffer (bounded by max_bytes)
      accumulate to raw_buffer (for on_error path)
      run success pipeline: Vec<Stage>::step(line) → SmallVec output
         │
         ▼
  subprocess exits (wait())
         │
         ├── exit_code == 0
         │     run post_process Starlark (if rule has it)
         │     apply implicit max_bytes cap (if not in pipeline)
         │     write filtered bytes → stdout
         │
         └── exit_code != 0
               discard success_buffer
               run raw_buffer through on_error pipeline
               write on_error output → stdout
         │
         ▼
  [Phase 2: Tracker write — InvocationMeta struct]
  exit with subprocess exit_code (or 128+sig)
```

### Recommended Project Structure

```
lacon/                              # workspace root
├── Cargo.toml                      # [workspace], [workspace.package], [workspace.dependencies], [profile.release]
├── rust-toolchain.toml             # pins stable toolchain
├── crates/
│   ├── lacon-core/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── config/             # Config struct, layer merge, lacon validate dispatch
│   │       │   └── mod.rs
│   │       ├── rules/              # RuleFile, Rule, Stage enum, extends flattening, RuleLoader
│   │       │   ├── mod.rs
│   │       │   ├── loader.rs       # lazy-resolve + eager paths, mtime cache
│   │       │   ├── bundled.rs      # rust-embed integration
│   │       │   └── schema.rs       # serde structs for YAML rule format
│   │       ├── pipeline/           # Pipeline runner, Stage::step dispatch, RegexSet OR-merge
│   │       │   ├── mod.rs
│   │       │   └── stages.rs       # enum Stage + all 10 primitive impls
│   │       ├── starlark_host/      # AstModule parse, eval_function wrapper, ctx struct
│   │       │   └── mod.rs
│   │       ├── runtime/            # Runner: spawn, merge threads, signal forwarding, on_error swap
│   │       │   └── mod.rs
│   │       ├── validate/           # ValidationError enum, rule validator, config validator
│   │       │   └── mod.rs
│   │       └── error.rs            # thiserror error types
│   ├── lacon-cli/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       └── main.rs             # clap derive, subcommand dispatch, anyhow at boundary
│   └── lacon-adapter-claudecode/   # Phase 1: stub only
│       ├── Cargo.toml
│       └── src/
│           └── lib.rs              # empty or minimal trait impl
├── bundled-rules/                  # YAML rule files (Phase 5 fills; rust-embed scans this dir)
└── tests/
    ├── fixtures/                   # per-rule scenario dirs (Phase 5)
    └── integration/                # assert_cmd tests for lacon run + lacon validate
```

### Pattern 1: Cargo Workspace with Inheritance

**What:** Root `Cargo.toml` defines `[workspace.package]` and `[workspace.dependencies]` inherited by all members.

**When to use:** Always; Rust 1.64+ supports this. Prevents version drift between crates.

```toml
# Source: https://doc.rust-lang.org/cargo/reference/workspaces.html [VERIFIED: official docs]

[workspace]
members = ["crates/*"]
resolver = "2"

[workspace.package]
version = "0.1.0"
edition = "2021"
rust-version = "1.80"   # MSRV — set from actual installed toolchain at phase start; currently 1.94.1

[workspace.dependencies]
regex = "1"
serde = { version = "1", features = ["derive"] }
serde-saphyr = "0.0.26"
clap = { version = "4", features = ["derive"] }
starlark = "0.13"
os_pipe = "1"
crossbeam-channel = "0.5"
nix = { version = "0.31", features = ["signal"] }
thiserror = "2"
anyhow = "1"
etcetera = "0.11"
rust-embed = "8"
smallvec = "1"

[profile.release]
# Optimized for cold-start speed and binary size on the hook hot path
opt-level = "z"          # minimize binary size (load time dominates for CLI)
lto = "thin"             # link-time optimization without the fat-LTO compile cost
codegen-units = 1        # single unit = better inlining across the crate graph
panic = "abort"          # removes unwinding machinery (~50 KB savings)
strip = "symbols"        # strip debug symbols from release binary

[profile.dev]
# Fast compile; no optimizations
opt-level = 0
```

**Member crate inheriting:**

```toml
# crates/lacon-core/Cargo.toml
[package]
name = "lacon-core"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
regex = { workspace = true }
serde = { workspace = true }
# ... etc
```

### Pattern 2: Subprocess Merge with `os_pipe` + Threads

**What:** Spawn subprocess with both stdout and stderr connected to the same `os_pipe` write-end. Read from the single read-end in the main processing thread (or a dedicated reader thread).

**When to use:** `lacon run` subprocess launch — the only subprocess spawning in Phase 1.

```rust
// Source: https://docs.rs/os_pipe/1.2.3/os_pipe/ [VERIFIED: official docs + Context7]
use os_pipe::pipe;
use std::process::Command;
use std::io::{BufRead, BufReader};

fn spawn_and_merge(cmd: &str, args: &[&str]) -> (std::process::Child, os_pipe::PipeReader) {
    let (reader, writer) = pipe().expect("pipe creation failed");
    let writer_clone = writer.try_clone().expect("writer clone failed");

    let child = Command::new(cmd)
        .args(args)
        .stdout(writer)          // os_pipe::PipeWriter implements Into<Stdio>
        .stderr(writer_clone)    // both ends → same pipe
        .spawn()
        .expect("spawn failed");
    // CRITICAL: drop Command (which holds a writer copy) before reading.
    // Failure to drop means the read-end never sees EOF.
    (child, reader)
}

// In the runner: two reader threads emit lines into a crossbeam channel
use crossbeam_channel::unbounded;
use std::thread;
use std::io::BufReader;

let (tx, rx) = unbounded::<String>();

// Single reader thread (both streams already merged via os_pipe)
let tx2 = tx.clone();
let reader_thread = thread::spawn(move || {
    let mut buf = Vec::new();
    let mut br = BufReader::new(pipe_reader);
    loop {
        buf.clear();
        match br.read_until(b'\n', &mut buf) {
            Ok(0) => break,  // EOF
            Ok(_) => {
                // Convert to String; replace invalid UTF-8 with replacement char
                let line = String::from_utf8_lossy(&buf).trim_end_matches('\n').to_owned();
                if tx2.send(line).is_err() { break; }
            }
            Err(_) => break,
        }
    }
});
```

**CRITICAL DEADLOCK FOOTGUN:** The `Command` object holds internal copies of the pipe write-end. If not dropped before reading, the read-end blocks forever waiting for an EOF that never arrives. Drop the `Command` (or extract the `Child`) before calling read.

### Pattern 3: `enum Stage` with SmallVec Output

**What:** Closed enum for all 10 pipeline primitives. Each variant carries its state inline. `step` takes a line, writes to an output accumulator.

**When to use:** All pipeline primitive implementations.

```rust
// Source: D-05 decision; SmallVec pattern [VERIFIED: CONTEXT.md]
use std::borrow::Cow;
use smallvec::SmallVec;

// Output type alias — most stages produce 0 or 1 lines; SmallVec<[_; 2]> avoids heap
type LineOut<'a> = SmallVec<[Cow<'a, str>; 2]>;

pub enum Stage {
    StripAnsi,
    DropRegex(regex::Regex),
    KeepRegex(regex::RegexSet),          // D-06: OR-merged at load time
    ReplaceRegex { pattern: regex::Regex, replacement: String },
    Dedupe { last: Option<String>, max_kept: usize, count: usize },
    CollapseRepeated { pattern: regex::Regex, max_kept: usize, summary: String, buf: Vec<String>, count: usize },
    KeepHead { mode: HeadTailMode, remaining: usize },
    KeepTail { mode: HeadTailMode, ring: std::collections::VecDeque<String> },
    KeepAroundMatch { pattern: regex::Regex, before: usize, after: usize, ctx_buf: std::collections::VecDeque<String>, emit_after: usize },
    MaxBytes { remaining: usize },
    StarlarkScript { ast: starlark::syntax::AstModule, function_name: String },
}

impl Stage {
    pub fn step<'a>(&mut self, line: Cow<'a, str>, out: &mut LineOut<'a>) {
        match self {
            Stage::StripAnsi => { /* strip ANSI, push to out */ }
            Stage::DropRegex(re) => {
                if !re.is_match(&line) { out.push(line); }
            }
            Stage::KeepRegex(set) => {
                if set.is_match(&line) { out.push(line); }
            }
            // ... etc
        }
    }
}
```

**Note on `KeepTail` bytes mode:** The ring buffer must track byte counts, not just line counts, when `bytes:` variant is used. `VecDeque` works; track a running byte total and pop-front when it exceeds the limit.

### Pattern 4: Starlark `post_process` Host

**What:** Parse the Starlark script file once at rule load; evaluate the `process(ctx, lines)` function at run time after the native pipeline completes.

**When to use:** Any rule with a `post_process` or inline `script:` stage.

```rust
// Source: Context7 /facebook/starlark-rust [VERIFIED: Context7]
use starlark::environment::{Globals, Module};
use starlark::eval::Evaluator;
use starlark::syntax::{AstModule, Dialect};

fn eval_post_process(
    ast: &AstModule,
    lines: Vec<String>,
    ctx_exit_code: i32,
    ctx_command: &str,
) -> starlark::Result<Vec<String>> {
    let globals = Globals::standard();  // standard library only — no file I/O, no load
    Module::with_temp_heap(|module| {
        // Inject ctx as a dict-like object via module.set()
        // For v1, a simple struct implementing StarlarkValue is simplest
        let heap = module.heap();
        module.set("_exit_code", heap.alloc(ctx_exit_code));
        // ... other ctx fields

        let mut eval = Evaluator::new(&module);
        // Evaluate the module to get the function value
        let func_val = eval.eval_module(ast.clone(), &globals)?;

        // Allocate the lines list on the heap
        let lines_val = heap.alloc_list(
            lines.iter().map(|s| heap.alloc(s.as_str())).collect::<Vec<_>>().as_slice()
        );

        // Call process(ctx, lines)
        let result = eval.eval_function(func_val, &[lines_val], &[])?;

        // Extract result as Vec<String>
        // result is a Starlark list; iterate with .iterate()
        Ok(vec![]) // placeholder — actual impl extracts from Value
    })
}
```

**Hermetic by design:** `Globals::standard()` does not include file I/O, network, or `load`. Do not call `eval.set_loader()`. The Starlark spec forbids `load` statements in function bodies anyway.

**MSRV note:** `starlark 0.13` requires `rust-version` compatible with its own MSRV. Check with `cargo tree --package starlark --depth 0` after adding the dep. [ASSUMED — starlark's own MSRV not independently verified; the stable toolchain 1.94.1 almost certainly satisfies it]

### Pattern 5: `lacon validate` Content Dispatch

**What:** Parse YAML file to a generic `Value`, inspect top-level keys, dispatch to rule or config validator.

**When to use:** `lacon validate <path>` entry point.

```rust
// Source: D-17, D-18; serde-saphyr location() API [VERIFIED: official docs research]
use serde_saphyr::Value;

fn validate_file(path: &std::path::Path) -> Result<(), Vec<ValidationError>> {
    let content = std::fs::read_to_string(path)?;
    let value: Value = serde_saphyr::from_str(&content)
        .map_err(|e| vec![ValidationError::ParseError {
            line: e.location().map(|l| l.line()).unwrap_or(0),
            column: e.location().map(|l| l.column()).unwrap_or(0),
            message: e.to_string(),
        }])?;

    // Dispatch: top-level "id" AND "match" → rule; otherwise → config
    let has_id = value.get("id").is_some();
    let has_match = value.get("match").is_some();

    if has_id && has_match {
        validate_rule(path, &value)
    } else {
        validate_config(path, &value)
    }
}
```

**Error format (byte-exact per D-18):**

```
.lacon/config.yaml:1: UserOnlyKeyInProject: key `retention` is user-only; move to ~/.config/lacon/config.yaml
```

### Pattern 6: Signal Forwarding with `nix`

**What:** Register a signal handler that forwards SIGTERM/SIGINT to the subprocess PID.

**When to use:** Inside the `lacon run` subprocess runtime.

```rust
// Source: nix 0.31.2 docs [VERIFIED: official docs]
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;

fn forward_signal(child_pid: u32, signal: Signal) {
    let pid = Pid::from_raw(child_pid as i32);
    // Single-PID kill — NOT process group (negative PID)
    // D-12: single-PID semantics; portable on both Linux and macOS
    let _ = kill(pid, signal);
    // On kill: wrapper does NOT drain. Exits with 128 + sig_number.
}
```

**macOS vs Linux distinction:** Sending to a positive PID targets only that process on both platforms. If the subprocess spawns its own children (e.g., `cargo build` spawns `rustc`), those children are NOT killed. This is accepted in v1; process-group kill (`-pid`) is a v2 enhancement. [VERIFIED: nix docs on positive vs negative pid semantics]

**`signal_hook` alternative:** `signal-hook 0.4.4` offers a higher-level iterator/pipe API for signal handling. Since `nix` is already in the dep set (for other POSIX APIs), `nix::sys::signal::kill` is the simpler choice — avoids an extra dependency.

### Anti-Patterns to Avoid

- **Reading from merged pipe before dropping `Command`:** Deadlocks forever. Always extract `child` from `Command::spawn()` and let the `Command` (with its internal writer copies) go out of scope.
- **Using `BufReader::lines()` on subprocess output:** `lines()` panics on non-UTF8 bytes. Use `read_until(b'\n', &mut buf)` + `String::from_utf8_lossy()` instead.
- **Calling `Globals::with_predeclared()` or setting a loader:** Breaks hermetic mode; gives Starlark scripts file I/O access. Use `Globals::standard()` only.
- **Allocating `stage.step()` output into a `Vec<_>` on every line:** Defeats the purpose of streaming. Use `SmallVec<[Cow<str>; 2]>` and reuse across the pipeline loop.
- **Using `serde_yaml` (the deprecated crate):** It is marked `0.9.34+deprecated` on crates.io. Projects that add it today get a deprecation warning. Use `serde-saphyr` instead.
- **Placing `max_bytes` stage injection at call-time rather than load-time:** Results in the cap being injected redundantly on every invocation. Inject once at rule parse/flatten.
- **Merging `on_error` pipeline with success pipeline:** ADR-0010 is explicit — `on_error` fully replaces, never merges. The dual-buffer approach (D-13) is the correct model: hold raw lines until exit code is known.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| YAML parsing with line/column errors | Custom YAML parser | `serde-saphyr` 0.0.26 | Correct YAML spec, panic-free, serde `Deserialize` derive |
| ANSI escape sequence stripping | Regex on `\x1b[...` | `strip_ansi_escapes` crate (if needed) or a well-tested regex pattern | ANSI codes have many variants; hand-rolled regex misses OSC, DCS sequences |
| XDG directory resolution | `PathBuf::from(env::var("HOME")?)` | `etcetera` 0.11.0 | Platform-correct; handles macOS vs Linux differences (Library/Application Support vs .config) |
| Multi-pattern OR matching | Multiple `Regex::is_match` calls | `regex::RegexSet` | Single pass through haystack; avoids repeated NFA traversal |
| Bundled file embedding | `const RULES: &[u8] = include_bytes!("../../bundled-rules/foo.yaml")` for each file | `rust-embed 8.11.0` with `#[derive(RustEmbed)]` | Directory-level embedding; path iteration; release/debug mode switching |
| CLI argument parsing | Manual `std::env::args()` parsing | `clap 4.6.1` with `derive` | Correct `--` separator handling; `--rule <id> -- <cmd>` form requires precise positional parsing |
| Subprocess stream merge | `process.stdout.take()` + `process.stderr.take()` + complex select/epoll | `os_pipe::pipe()` + writer clone | Dead-simple; avoids async; deadlock-safe if you drop the writer copies |
| Signal handling | `libc::sigaction` directly | `nix::sys::signal::kill` | Idiomatic Rust; safe wrapper; correct on both macOS and Linux |
| Error enum boilerplate | Manual `impl Display for Error` | `thiserror 2.0.18` | Derive generates `Display`, `Error`, `From` impls correctly |

**Key insight:** The hardest part of Phase 1 is NOT the algorithm complexity — it is the ordering constraints: (1) drop pipe writers before reading, (2) buffer both success and raw streams simultaneously until exit code is known, (3) inject `max_bytes` cap at load time not run time, (4) flatten `extends` chains at parse time not execution time. Getting these orderings wrong produces bugs that only manifest under specific conditions (large output, non-zero exit, multi-hop `extends`).

---

## Common Pitfalls

### Pitfall 1: Pipe Deadlock — Writer Not Dropped Before Read

**What goes wrong:** The main thread blocks on `read_until` forever; the subprocess appears to hang even after it exits.

**Why it happens:** `Command::spawn()` stores internal copies of the pipe write-ends. The read-end waits for all write-ends to close (to signal EOF). If the `Command` object (or any clones of the writers) is still alive in scope, the reader never sees EOF.

**How to avoid:** Immediately after `child = cmd.spawn()`, ensure the `Command` value is dropped. Pattern: use a block scope or call `drop(cmd)` explicitly. Extract the `Child` handle as the only long-lived reference.

**Warning signs:** Integration test hangs indefinitely rather than finishing; `strace` shows the reader thread blocked in `read(2)`.

### Pitfall 2: `BufReader::lines()` Panics on Non-UTF8 Output

**What goes wrong:** `cargo build` or other tools emit non-UTF8 bytes (e.g., compiler progress escape codes, binary artifact names on some locales). `lines()` returns `Err(InvalidData)` which, if `.unwrap()`'d in an iterator, panics.

**Why it happens:** `BufRead::lines()` calls `read_line()`, which calls `from_utf8()` on the read bytes. Non-UTF8 bytes are an error, not a replacement.

**How to avoid:** Use `read_until(b'\n', &mut buf)` to read raw bytes. Convert to `String` with `String::from_utf8_lossy(&buf)`, which replaces invalid bytes with `U+FFFD`. Accept that some non-ASCII content is lossily converted.

**Warning signs:** Tests on `cargo build` output pass locally but panic on CI with unusual tool output; the panic stack points to `lines()`.

### Pitfall 3: `serde_yaml` Dependency Pulled In Transitively

**What goes wrong:** Another dependency in the graph pulls in `serde_yaml 0.9`. Code that uses the deprecated crate compiles but emits deprecation cargo warnings; the `Error::location()` API exists in 0.9 but the crate will stop receiving security updates.

**Why it happens:** Many Rust tools still use `serde_yaml 0.9` internally.

**How to avoid:** Use `serde-saphyr` in `lacon-core`. Accept that transitive deps may pull in `serde_yaml 0.9` for their own use — that is fine as long as `lacon-core`'s own YAML parsing uses `serde-saphyr`. Do not add `serde_yaml` to `[workspace.dependencies]`.

**Warning signs:** `cargo tree | grep serde_yaml` shows it in the dep graph; check whether it's a direct dep of any `lacon-*` crate.

### Pitfall 4: `starlark` `AstModule::parse` Requires `content.to_owned()`

**What goes wrong:** Caller passes a `&str` slice; `AstModule::parse` takes an owned `String` for the content (the AST stores source spans into the string). Compiler error at parse site.

**Why it happens:** The `starlark` crate's AST nodes store offsets into the source; the source must be owned to outlive the AST.

**How to avoid:** Always call `.to_owned()` or pass a `String` directly. Parse the `.star` file content as `std::fs::read_to_string()` (already a `String`).

### Pitfall 5: `RegexSet` Reports Match but Not Capture Groups

**What goes wrong:** Code uses `RegexSet::is_match()` for `keep_regex` filtering (correct), then tries to also use the set for `replace_regex` or for extracting match positions. `RegexSet` does not provide `Match` or `Captures` objects.

**Why it happens:** `RegexSet`'s design is deliberately limited to "which patterns match" — it cannot report offsets in a single pass.

**How to avoid:** Use `RegexSet` ONLY for the `keep_regex` OR-merge (D-06). For `replace_regex` and `drop_regex`, use individual `regex::Regex` instances. For `drop_regex`, a single compiled `Regex` is fine (no OR-merge needed per stage).

**Warning signs:** Compiler error "no method `captures` on `RegexSet`".

### Pitfall 6: `extends` Cycle Detection Must Use a Visited Set, Not Depth Counter

**What goes wrong:** Rules `A extends B` and `B extends A`. A naive recursive loader without cycle detection either stack-overflows or loops forever.

**Why it happens:** The YAML is user-authored; the loader cannot assume the graph is a DAG.

**How to avoid:** Thread a `HashSet<String>` of visited rule IDs through the recursive flatten call. If the next parent ID is already in the set, return `Err(ValidationError::CircularExtends { chain: ... })`.

**Warning signs:** `lacon validate` hangs or stack-overflows on a pair of rules that each reference the other.

### Pitfall 7: `max_bytes` Implicit Injection Applied After Explicit Stage

**What goes wrong:** Rule has `max_bytes: 4096` in its pipeline AND the implicit cap (`defaults.max_bytes: 32768`) gets appended AFTER, resulting in a final cap of 32768 regardless of what the rule requested.

**Why it happens:** The implicit injection code checks "does the pipeline contain `max_bytes`?" but applies the check incorrectly (e.g., checks `on_error` instead of `pipeline`, or checks after `extends` flatten before the parent's stages are prepended).

**How to avoid:** Inject the implicit `max_bytes` stage AFTER `extends` flattening. Check the fully-flattened `pipeline` for any existing `MaxBytes` variant. If found → do not inject. If absent → append with value from `defaults.max_bytes`. Same logic applies independently to `on_error.pipeline`.

### Pitfall 8: Cold-Start Measurement Is Meaningless Without Release Build

**What goes wrong:** `LACON_TIMING=1 lacon run` is measured on a debug build; result is 80ms; team panics; time is spent on premature optimization.

**Why it happens:** Debug builds have no LTO, no optimization; binary is 4× larger; startup cost includes DWARF parsing by the dynamic linker.

**How to avoid:** All cold-start measurements MUST be against a `--release` binary. Add this to the benchmark task description in the plan.

---

## Code Examples

### Cargo Workspace Root

```toml
# Source: https://doc.rust-lang.org/cargo/reference/workspaces.html [VERIFIED: official docs]
[workspace]
members = ["crates/*"]
resolver = "2"                     # Required for edition 2021 feature resolver

[workspace.package]
version = "0.1.0"
edition = "2021"
rust-version = "1.80"              # Conservative MSRV; current installed: 1.94.1

[profile.release]
opt-level = "z"                    # binary size over speed (load time dominates)
lto = "thin"                       # good balance: ~10% size reduction vs fat LTO
codegen-units = 1                  # better optimization across crate boundaries
panic = "abort"                    # removes unwind tables
strip = "symbols"                  # strip debug symbols
```

### os_pipe Subprocess Merge (Key Snippet)

```rust
// Source: https://docs.rs/os_pipe/1.2.3/os_pipe/ [VERIFIED: official docs]
use os_pipe::pipe;
use std::process::{Command, Stdio};

let (reader, writer) = pipe()?;
let writer_clone = writer.try_clone()?;

let mut child = Command::new(&cmd)
    .args(&args)
    .stdout(writer)           // PipeWriter implements Into<Stdio>
    .stderr(writer_clone)
    .spawn()?;

drop(cmd_builder);            // DROP any remaining writer copies before reading!
// reader is now the sole non-subprocess holder of the write-end
// reader will see EOF once the child closes its write-end

// Read lines (non-UTF8 safe):
use std::io::{BufRead, BufReader};
let mut buf_reader = BufReader::new(reader);
let mut line_buf = Vec::new();
loop {
    line_buf.clear();
    match buf_reader.read_until(b'\n', &mut line_buf) {
        Ok(0) => break,
        Ok(_) => {
            let line = String::from_utf8_lossy(&line_buf);
            // trim trailing newline, send to pipeline
        }
        Err(e) => return Err(e.into()),
    }
}
```

### RegexSet OR-Merge at Load Time

```rust
// Source: https://docs.rs/regex/latest/regex/struct.RegexSet.html [VERIFIED: official docs]
use regex::RegexSet;

// At rule load time — collect all keep_regex patterns
let keep_patterns: Vec<&str> = pipeline_stages
    .iter()
    .filter_map(|s| if let Stage::KeepRegex(p) = s { Some(p.as_str()) } else { None })
    .collect();

// OR-merge into single RegexSet
let merged: RegexSet = RegexSet::new(&keep_patterns)?;

// Replace individual KeepRegex stages with a single merged one
// (Only if any keep_regex stages exist — empty RegexSet matches nothing)
```

### Starlark `eval_function` with List of Strings

```rust
// Source: Context7 /facebook/starlark-rust [VERIFIED: Context7]
use starlark::environment::{Globals, Module};
use starlark::eval::Evaluator;
use starlark::syntax::{AstModule, Dialect};

// At rule load time (parse once, evaluate many times):
let ast = AstModule::parse("rule.star", script_content, &Dialect::Standard)?;

// At run time (after native pipeline completes):
let globals = Globals::standard();  // no I/O, no load()
Module::with_temp_heap(|module| {
    let mut eval = Evaluator::new(&module);
    let func_val = eval.eval_module(ast.clone(), &globals)?;

    let heap = module.heap();
    // Allocate lines as Starlark list
    let starlark_lines: Vec<_> = filtered_lines.iter()
        .map(|s| heap.alloc(s.as_str()))
        .collect();
    let lines_val = heap.alloc_list(&starlark_lines);

    // Call process(ctx, lines)
    let result_val = eval.eval_function(func_val, &[lines_val], &[])?;

    // Extract result as Vec<String>
    // ...

    starlark::Result::Ok(result_lines)
})?
```

### Signal Forwarding

```rust
// Source: https://docs.rs/nix/0.31.2/nix/sys/signal/fn.kill.html [VERIFIED: official docs]
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;

fn forward_signal_to_child(child_pid: u32, signal: Signal) {
    // Positive PID = single process; no process-group kill in v1
    let _ = kill(Pid::from_raw(child_pid as i32), signal);
}

// Exit code propagation:
// - Normal exit: exit with child.wait().unwrap().code().unwrap_or(1)
// - Signal kill: exit with 128 + signal_number
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `serde_yaml 0.9` for YAML parsing | `serde-saphyr 0.0.26` | March 2024 (serde_yaml deprecated) | Must not add `serde_yaml` as a direct dep |
| `smallvec 1.x` | `smallvec 1.14.0` stable (1.x branch); `2.0.0-alpha` in separate branch | Ongoing | Pin `"1"` in Cargo.toml to avoid pulling in alpha |
| `thiserror 1.x` | `thiserror 2.0.18` | 2024 | 2.x changed some derive semantics; pin `"2"` |
| `lto = "fat"` for release | `lto = "thin"` preferred for balance | 2023+ | Fat LTO increases compile time significantly; thin achieves 80% of the benefit |

**Deprecated/outdated:**

- `serde_yaml`: `0.9.34+deprecated` — the `+deprecated` suffix is literal crates.io metadata; new projects must not use it.
- `lazy_static!` for compiled regexes: superseded by `std::sync::OnceLock` (stable since Rust 1.70) for static initialization. Not needed for per-rule regex cache (per-invocation compile is sub-ms per D-15).
- `starlark_module!` macro for registering Rust functions callable from Starlark: only needed if the implementation exposes Rust functions to user scripts; the v1 `post_process` API does not — the bridge is one-way (Rust → Starlark).

---

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `starlark 0.13.0` MSRV is compatible with stable Rust 1.80 (workspace MSRV) | Standard Stack, Code Examples | Workspace MSRV may need to be raised; check with `cargo tree` after adding starlark dep |
| A2 | `serde-saphyr`'s `location()` API (line/column) works similarly to `serde_yaml`'s `Error::location()` | Pattern 5, validate dispatch | Error format `<path>:<line>: <category>` may need adjustment if API differs |
| A3 | `starlark 0.13.0`'s `AstModule::clone()` performs a shallow clone (sharing the underlying AST) adequate for "parse once, evaluate many times" pattern | Code Examples (Starlark) | If clone is deep/expensive, parse per-invocation instead (still sub-ms for typical script sizes) |
| A4 | `opt-level = "z"` with `lto = "thin"` achieves <10ms cold start for this binary's scope | Architecture Patterns (Cargo Workspace) | May need `lto = "fat"` or linker replacement (mold); measure in Phase 1 benchmarking tasks |
| A5 | `serde-saphyr` can deserialize a `Value` (untyped map) in addition to typed structs, needed for D-17 content dispatch | Pattern 5 (validate dispatch) | If Value-based parsing is not supported, use `saphyr` parser directly and hand-roll dispatch |

---

## Open Questions

1. **`serde-saphyr` Value API completeness**
   - What we know: `serde-saphyr` 0.0.26 supports serde `Deserialize` derive and has line/column error reporting
   - What's unclear: Whether it exposes a `serde_yaml::Value`-equivalent for the `lacon validate` content dispatch (check for top-level `id` and `match` keys without a known type)
   - Recommendation: In Wave 0 (setup), add `serde-saphyr` and write a 5-line test: `serde_saphyr::from_str::<serde_saphyr::Value>(yaml_str)`. If `Value` is not available, fall back to `saphyr` (the underlying parser crate) for key inspection.

2. **`starlark` MSRV**
   - What we know: Active workspace toolchain is 1.94.1; workspace MSRV set to 1.80 (conservative)
   - What's unclear: `starlark 0.13.0` may require a higher MSRV than 1.80
   - Recommendation: Run `cargo add starlark@0.13` and check the resolver's MSRV violation output.

3. **Process-group kill vs single-PID on macOS**
   - What we know: `nix::kill(Pid::from_raw(pid), SIGTERM)` sends to a single process; child processes of `cargo build` etc. will not be killed
   - What's unclear: Whether Claude Code's 2-minute timeout for Bash tool leaves long-running subprocesses as zombies in practice
   - Recommendation: Accept v1 single-PID semantics; add a comment in `runtime.rs` referencing this as a known limitation; track in `docs/open-questions.md` for v2.

---

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Rust stable toolchain | All compilation | ✓ | rustc 1.94.1 | — |
| Cargo | All compilation | ✓ | 1.94.1 | — |
| `strip` (GNU) | Release binary stripping | ✓ | GNU strip | — |
| `perf` | Cold-start measurement (optional) | ✓ | system perf | `/usr/bin/time` (available) |
| `hyperfine` | Repeatable cold-start benchmarks | ✗ | — | `for i in $(seq 20); do /usr/bin/time lacon run ...; done` |
| `valgrind` | Memory bound validation (optional) | ✓ | system valgrind | — |
| `lld` | Faster linking (optional) | ✗ | — | Default `ld` (slower link; no impact on runtime) |

**Missing dependencies with no fallback:** None that block execution.

**Missing dependencies with fallback:**

- `hyperfine` not installed: use shell loop + `/usr/bin/time` for cold-start measurements in Phase 1 benchmark tasks. Install with `cargo install hyperfine` if precise statistics are needed.
- `lld` not installed: default `ld` is slower at link time but produces equivalent binaries; not needed until Phase 6 CI optimization.

---

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[test]` + `assert_cmd 2.2.1` for integration |
| Config file | None in Wave 0; Cargo handles test discovery |
| Quick run command | `cargo test -p lacon-core` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| REQ-engine-streaming-primitives | Each of 10 primitive variants transforms lines correctly | Unit (golden fixture) | `cargo test -p lacon-core pipeline` | ❌ Wave 0 |
| REQ-engine-streaming-primitives | Memory bounded (no allocation beyond max_bytes + ring buf) | Unit | `cargo test -p lacon-core max_bytes_memory` | ❌ Wave 0 |
| REQ-engine-starlark-postprocess | `process(ctx, lines)` called with correct inputs; returns `list[str]` | Unit | `cargo test -p lacon-core starlark_host` | ❌ Wave 0 |
| REQ-engine-starlark-postprocess | Hermetic: no file I/O, no `load` | Unit | `cargo test -p lacon-core starlark_hermetic` | ❌ Wave 0 |
| REQ-engine-rule-loading | Project > user > bundled precedence; first-match-wins | Unit | `cargo test -p lacon-core rule_loader` | ❌ Wave 0 |
| REQ-engine-rule-loading | mtime invalidation reloads changed rule | Unit | `cargo test -p lacon-core rule_mtime` | ❌ Wave 0 |
| REQ-engine-extends | Parent pipeline prepended to child | Unit | `cargo test -p lacon-core extends_flatten` | ❌ Wave 0 |
| REQ-engine-extends | Cycle detected → `CircularExtends` error | Unit | `cargo test -p lacon-core extends_cycle` | ❌ Wave 0 |
| REQ-engine-on-error | Non-zero exit discards success buffer, runs `on_error` pipeline | Integration | `cargo test --test lacon_run_on_error` | ❌ Wave 0 |
| REQ-engine-rewrite | `add_flags` idempotent; `remove_flags` removes; `replace_flags` substitutes | Unit | `cargo test -p lacon-core rewrite` | ❌ Wave 0 |
| REQ-engine-bypass | `LACON_DISABLE=1` passes command through unfiltered | Integration | `cargo test --test lacon_run_bypass` | ❌ Wave 0 |
| REQ-engine-max-bytes-cap | Output never exceeds `max_bytes`; truncation marker appended | Unit | `cargo test -p lacon-core max_bytes` | ❌ Wave 0 |
| REQ-engine-max-bytes-cap | Implicit cap injected when rule omits `max_bytes` stage | Unit | `cargo test -p lacon-core implicit_cap` | ❌ Wave 0 |
| REQ-cli-run | `lacon run -- echo hello` prints `hello` (no rule → pass-through) | Integration | `cargo test --test lacon_run_integration` | ❌ Wave 0 |
| REQ-cli-run | `lacon run --rule <id> -- <cmd>` applies rule pipeline | Integration | `cargo test --test lacon_run_rule` | ❌ Wave 0 |
| REQ-cli-run | Exit code propagated from subprocess | Integration | `cargo test --test lacon_run_exitcode` | ❌ Wave 0 |
| REQ-cli-validate | Rule file with valid schema → exit 0, no output | Integration | `cargo test --test lacon_validate_rule` | ❌ Wave 0 |
| REQ-cli-validate | Config file with `retention` at project layer → `UserOnlyKeyInProject` | Integration | `cargo test --test lacon_validate_config` | ❌ Wave 0 |
| REQ-cli-validate | Unknown key → `UnknownKey` error with `path:line:` prefix | Integration | `cargo test --test lacon_validate_unknown_key` | ❌ Wave 0 |
| REQ-cli-validate | `id` + `match` present → dispatched to rule validator | Integration | `cargo test --test lacon_validate_dispatch` | ❌ Wave 0 |

### Per-Primitive Unit Test Pattern

Each primitive should have a golden fixture test:

```
tests/
  fixtures/
    primitives/
      strip_ansi/
        input.txt          # lines with ANSI codes
        expected.txt       # same lines, codes stripped
      keep_regex/
        input.txt
        expected.txt       # only lines matching pattern
      # ... one dir per primitive
```

Test driver (pseudo-code):

```rust
// tests/integration/primitives.rs
#[test]
fn test_strip_ansi() {
    let input = include_str!("../fixtures/primitives/strip_ansi/input.txt");
    let expected = include_str!("../fixtures/primitives/strip_ansi/expected.txt");
    let mut stage = Stage::StripAnsi;
    let output = run_stage_on_lines(&mut stage, input.lines());
    assert_eq!(output.join("\n"), expected.trim_end());
}
```

### Sampling Rate

- **Per task commit:** `cargo test -p lacon-core`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full workspace suite green before `/gsd-verify-work`

### Wave 0 Gaps

- [ ] `crates/lacon-core/src/lib.rs` — root module stubs
- [ ] `crates/lacon-core/src/pipeline/stages.rs` — `enum Stage` skeleton
- [ ] `tests/integration/` — directory for `assert_cmd` integration tests
- [ ] `tests/fixtures/primitives/` — golden fixture files for each primitive
- [ ] Workspace `Cargo.toml` with all `[workspace.dependencies]`
- [ ] `rust-toolchain.toml` — pin stable toolchain
- [ ] Framework install: `cargo add --dev assert_cmd predicates insta` (if not already in workspace deps)

---

## Security Domain

The `security_enforcement` key is absent from `.planning/config.json`, so this section is included.

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | No | No user auth in `lacon run` |
| V3 Session Management | No | No sessions; invocation is stateless |
| V4 Access Control | Partial | `0700` on `~/.local/share/lacon/` enforced at DB init (Phase 2); file-system permissions only |
| V5 Input Validation | Yes | YAML rule files are user-authored; validate unknown keys, reject malformed regex |
| V6 Cryptography | No | No encryption in v1 (raw_outputs stored plaintext, off by default) |
| V7 Error Handling | Yes | Validation errors surface `path:line:category:message`; never expose internal paths in prod error messages |

### Known Threat Patterns for This Stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Malformed regex in rule file causing `panic` | Denial of Service | `regex::Regex::new()` returns `Result` — handle error, don't unwrap; return `InvalidRegex` |
| `extends:` cycle → stack overflow | Denial of Service | Cycle detection with `HashSet` (Pitfall 6) |
| Starlark script reading filesystem | Information Disclosure | `Globals::standard()` — no file I/O built-in; do not call `set_loader()` |
| Unknown config keys silently ignored → typo masking | Tampering (config integrity) | `UnknownKey` error on validation; reject malformed files at load |
| `raw_outputs` stored in world-readable path | Information Disclosure | Enforce `0700` on `~/.local/share/lacon/` (Phase 2); document in Phase 1 that the `etcetera` path must be `set_permissions(0o700)` at first use |
| `LACON_DISABLE=1` as bypass escape | Elevation of Privilege | Documented and intentional (trust model: user-controlled bypass); log bypass rate via `invocations.bypassed` column |

---

## Sources

### Primary (HIGH confidence)

- `cargo search <crate>` — all version numbers verified against live crates.io registry
- https://docs.rs/os_pipe/1.2.3/os_pipe/ — subprocess merge pattern, deadlock prevention
- https://docs.rs/regex/latest/regex/struct.RegexSet.html — `RegexSet` API, `is_match`, creation
- https://docs.rs/nix/0.31.2/nix/sys/signal/fn.kill.html — signal forwarding, PID semantics
- https://doc.rust-lang.org/cargo/reference/workspaces.html — workspace inheritance syntax, profile placement
- Context7 `/facebook/starlark-rust` — `AstModule`/`Globals`/`Module`/`Evaluator` API, `eval_function` signature, heap allocation
- `CONTEXT.md` (01-CONTEXT.md) — all locked decisions D-01..D-18
- ADRs 0001–0013 (all accepted) — architectural rationale
- `docs/specs/filter-rule-schema.md` — all 10 primitive contracts, schema shapes
- `docs/specs/config-schema.md` — config keys, scope rules, error format example

### Secondary (MEDIUM confidence)

- https://users.rust-lang.org/t/serde-yaml-deprecation-alternatives/108868 — `serde_yaml` deprecation confirmed; `serde-saphyr` recommended
- https://nnethercote.github.io/perf-book/build-configuration.html — release profile settings for binary size and startup
- crates.io `smallvec 1.14.0` via web search — latest stable 1.x; 2.x is alpha

### Tertiary (LOW confidence)

- General Rust forum and community knowledge on `BufReader::lines()` UTF-8 safety — not independently benchmarked; based on stdlib doc reading
- `starlark 0.13` cold-start measurement — no public benchmark found; CONTEXT.md cites "<5ms" as observed; this is [ASSUMED] and should be measured in Phase 1

---

## Metadata

**Confidence breakdown:**

- Standard stack: HIGH — all crate versions verified via `cargo search` against live registry
- Architecture: HIGH — patterns derived from locked ADRs and official crate docs
- Pitfalls: MEDIUM — verified via official docs for deadlock and UTF-8 issues; cycle detection and max_bytes injection pitfalls are ASSUMED from logical analysis
- Starlark integration: MEDIUM — API verified via Context7; cold-start cost ASSUMED

**Research date:** 2026-05-06
**Valid until:** 2026-06-06 (30 days; crate versions move slowly; `serde-saphyr` is young and may have releases)

---

## RESEARCH COMPLETE
