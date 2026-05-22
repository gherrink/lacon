# Deferred Items — quick task 260522-v4a

Out-of-scope discoveries logged during execution (NOT fixed — they are pre-existing
clippy warnings in files this task does not touch). The scope boundary limits auto-fixes
to issues directly caused by this task's changes (doctor.rs / cli_doctor.rs / cli.rs).

## Pre-existing clippy warnings (lacon-core + unrelated lacon-cli test)

- `crates/lacon-core/src/pipeline/stages.rs:438` — this `if` can be collapsed into the outer `match`
- `crates/lacon-core/src/pipeline/stages.rs:451` — this `if` can be collapsed into the outer `match`
- `crates/lacon-core/src/tracking/record.rs:8` — doc list item overindented
- `crates/lacon-core/src/tracking/mod.rs:201` — manual case-insensitive ASCII comparison
- `crates/lacon-cli/tests/tracking_e2e.rs:43` — writing `&PathBuf` instead of `&Path`

All five predate this task and are unrelated to the doctor rework. `cargo clippy -p lacon-cli
--all-targets` produces zero warnings attributable to doctor.rs, cli_doctor.rs, or cli.rs.
