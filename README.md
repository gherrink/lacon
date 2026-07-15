# lacon

A CLI tool that filters and rewrites bash command output to reduce token consumption by AI coding assistants.

When Claude Code (or another coding assistant) runs `pnpm install`, the tool burns thousands of tokens on progress bars, deprecation warnings, and other noise that doesn't help the model do its job. `lacon` sits between the assistant and the shell — via the assistant's hook system — and applies configurable filter rules to strip noise while preserving signal, especially on errors. It runs locally, makes no LLM calls, opens no network connections, and runs no daemon.

## Install

`lacon` runs on macOS and Linux (and WSL). Build from source with a recent Rust toolchain:

```sh
git clone <this-repo>
cd lacon
cargo build --release
```

This produces two binaries in `target/release/`:

- `lacon` — the CLI (`init`, `run`, `stats`, `explain`, `doctor`, `validate`)
- `lacon-claude-hook` — the Claude Code `PreToolUse` hook binary

Put both on your `PATH` (e.g. copy them into a directory like `~/.local/bin`).

## Quickstart

From inside a project directory, wire up the Claude Code integration:

```sh
lacon init
```

`lacon init` installs the Claude Code `PreToolUse` hook, creates a `.lacon/` skeleton for
your project rules, and adds a short instruction block to the project's `CLAUDE.md`. It is
idempotent — re-running it preserves your existing settings.

From then on, when Claude Code is about to run a Bash command, the hook checks whether a
filter rule matches. If one does, it rewrites the command to run through the wrapper:

```sh
lacon run --rule <id> -- <your command>
```

`lacon run` executes the command, merges its stderr into stdout, streams the output
through the rule's pipeline (or the rule's `on_error` pipeline when the command fails),
and writes the filtered result back as the tool output Claude Code sees — exiting with the
command's original exit code. Commands with no matching rule pass through unchanged. You
can bypass filtering for a single command with the `!!` prefix or for a session with
`LACON_DISABLE=1`.

The full CLI surface is six commands:

| Command | What it does |
|---------|--------------|
| `lacon init` | Wire up the Claude Code hook + `.lacon/` skeleton + CLAUDE.md note |
| `lacon run` | Run a command and filter its output (the production wrapper; also handy for manual testing) |
| `lacon validate` | Parse and type-check a rule or config file |
| `lacon stats` | Show byte-savings and offender stats from local tracking |
| `lacon explain` | Show raw vs. filtered output for a recorded invocation |
| `lacon doctor` | Check that the install, hook, rules, and tracking DB are healthy |

After installing, run `lacon doctor` to confirm everything is wired up correctly.

To filter your own commands, write a rule under `.lacon/rules/`. See the
[worked example](docs/worked-example.md) to get started and the
[primitive reference](docs/primitive-reference.md) for what each pipeline stage does.

## Performance

`lacon` sits on the hook hot path — `lacon run` is invoked on every matched
command — so cold start is a hard constraint, budgeted at **≤ 10 ms**. On Linux
the CLI cold-starts in ~1.1 ms (`lacon --version` / `validate`), and the hook hot
path's steady-state `Tracker::open` runs in **~208 µs**, enforced by a
deterministic in-process benchmark gate (`assert!(mean < 3700 µs)`) on every CI
run across both Linux and macOS. Full methodology and tables in
[benchmarks](docs/benchmarks.md).

## Documentation

- [Worked example](docs/worked-example.md) — writing a project-specific filter rule
- [Primitive reference](docs/primitive-reference.md) — one example per pipeline primitive
- [Vision](VISION.md) — what we're building and why
- [v1 scope](docs/prds/v1-scope.md) — what's in the first release
- [Backlog](docs/deferral-ledger.md) — deferred ideas
- [Architecture](docs/architecture/engine-and-claude-code-integration.md) — system shape
- [Open questions](docs/open-questions.md) — known risks and unknowns
- [Bundled rules roadmap](docs/roadmap.md) — which filters we plan to ship
- [Benchmarks](docs/benchmarks.md) — cold-start measurements and methodology
- Specs:
  - [Filter rule schema](docs/specs/filter-rule-schema.md)
  - [Tracking data model](docs/specs/tracking-data-model.md)
- [Decisions](docs/decisions/) — architecture decision records

## License

[MIT](LICENSE)
