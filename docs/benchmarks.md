# Cold-start benchmarks

`lacon` runs on the Claude Code hook hot path — `lacon run` is invoked on every
matched command, thousands of times per session — so cold-start latency is a
load-bearing constraint. The budget is **≤ 10 ms** on the hook hot path. This
document records the measurements behind that claim and how to reproduce them.

Two things are measured, with different rigor:

- **`tracker_open` (in-process criterion bench)** — the **deterministic hard
  gate**. It asserts the steady-state `Tracker::open` mean stays under 3700 µs
  and runs on both CI OS lanes (`cargo bench -p lacon-core --bench tracker_open`).
- **`cold_start_probe` (wall-clock subprocess spawn)** — **soft-reported**
  (min-of-N, discarding 3 warm-ups over 50 samples). Wall-clock spawn is noisy on
  shared CI VMs, so it is a reported signal, not a build-breaker.

## Measurement protocol: first-ever DB creation vs steady-state `Tracker::open`

`Tracker::open` has two distinct regimes:

- **First-ever DB creation** — once per machine. The very first `Tracker::open`
  runs the `M0001_INITIAL` migration inside a `BEGIN IMMEDIATE`/`COMMIT`, and the
  `COMMIT` fsync dominates (an ext4 effect). This is **reported as a diagnostic
  but not gated** — the hook never pays it again after the DB exists.
- **Steady-state `Tracker::open`** — every subsequent invocation. `migrate()`
  early-returns once `PRAGMA user_version >= TARGET_VERSION`
  (`crates/lacon-core/src/tracking/migrations.rs:41-43`), so there is no migration
  `COMMIT` fsync, and `prune_if_due`'s 24h throttle also skips. This is the real
  hot path and the **deterministic hard gate**: the in-process
  `tracker_open_steady_state` criterion bench asserts the steady-state mean stays
  under the 3700 µs budget (1154 µs Phase 1 baseline + 2500 µs headroom).

## Results

### `tracker_open` — in-process criterion bench (the hard gate)

| Variant | Linux (criterion median) | Gated? |
|---------|--------------------------|--------|
| `tracker_open_steady_state` (hook hot path) | ~208 µs | **Yes** — `assert!(mean < 3700 µs)` |
| `tracker_open_first_run` (once-per-machine DB creation) | reported (fsync-dominated) | No — diagnostic only |

Measured 2026-05-22 on Linux (ext4): steady-state criterion median 208 µs, well
under the 3700 µs budget.

### `cold_start_probe` — wall-clock (soft-reported min-of-N)

**Phase 1 baseline.** Measured 2026-05-06 on Linux 6.8.0-111-generic (AMD Ryzen 7
5800X 8-Core). Sample size 50 per scenario, after a 3-run warm-up. Release build
with `opt-level = "z"` + `lto = "thin"` + `strip = "symbols"`.

| Command | min | median | p95 | max |
|---------|-----|--------|-----|-----|
| `lacon --version` | 982 µs | 1154 µs | 1301 µs | 1323 µs |
| `lacon validate <rule>` | 1082 µs | 1259 µs | 1401 µs | 1635 µs |

The dominant cost at these figures is process startup + dynamic linking; the clap
parse and loader paths add only ~100 µs on top of `--version`.

**Ship-gate measurements.** Linux numbers produced locally via
`./scripts/bench-cold-start.sh`; the macOS row is from the `macos-latest` CI lane.

| Command | Linux min | Linux median | macOS min | macOS median |
|---------|-----------|--------------|-----------|--------------|
| `lacon --version` | 1118 µs | 1474 µs | 1953 µs | 2009 µs |
| `lacon validate <rule>` | 1195 µs | 1414 µs | 2094 µs | 2172 µs |
| `lacon hook passthrough (no rule)` † | ~12 ms | ~13.6 ms | ~11 ms | ~11.9 ms |
| `lacon hook rewrite (matched)` † | ~12 ms | ~13.7 ms | ~11 ms | ~11.1 ms |

> † The hook wall-clock figure is **spawn-dominated measurement overhead, not
> hook work**, so it is *not* measured against the 10 ms budget. An `strace -c` of
> a single hook run shows the hook's own syscall work totals ~0.3 ms; the rest is
> `Command::spawn` + piped-stdio + scheduler latency under the probe's tight
> 50-iteration loop. Note the adapter hook (`lacon-claude-hook`) does **not**
> itself open the tracker — `Tracker::open` lives in `lacon run`, which the hook
> only rewrites the command to invoke.

The `--version`/`validate` lazy-open paths sit at ~1.1–1.5 ms on Linux and ~2 ms
on macOS. The macOS hook wall-clock (~11 ms median, passthrough p95 16.5 ms, max
39 ms) is exactly the shared-VM noise that keeps this a soft report rather than a
build-breaker; the deterministic gate is the in-process steady-state
`tracker_open` bench above, which passed on both `ubuntu-latest` and
`macos-latest`.

To regenerate the wall-clock table: `./scripts/bench-cold-start.sh`. To run the
hard gate: `cargo bench -p lacon-core --bench tracker_open`.

## Benchmark-informed decisions

| Item | Measured | Decision |
|------|----------|----------|
| Starlark cold-start | Per-test parse+run well under 1 ms in debug; negligible in release relative to the 10 ms budget. | Eager-init (parse the `AstModule` at rule-load time, store on the resolved rule) is correct; lazy-init not needed. |
| clap v4 vs pico-args | `lacon --version` median 1154 µs; `validate <rule>` median 1259 µs. | Keep clap derive; the full cold-start chain is well under budget. |
| os_pipe + threads vs duct vs raw nix | os_pipe + 1 reader thread + crossbeam-channel met the streaming and cold-start budgets on first implementation. | Keep os_pipe + crossbeam. |
| POSIX signal-forwarding, macOS vs Linux | Tested on Linux; `nix::sys::signal::kill` is portable with an identical macOS API. | Cross-platform behavior is equivalent (see the arch-doc's wrapper component). |
