# Testing rules

How bundled rules are tested in v1, and what to do when a tool's output format changes and a fixture goes stale.

## Strategy: fixture-based, hermetic CI

Each bundled rule has captured representative output checked into the repo. The test runner asserts that the rule's pipeline transforms the captured input into the expected output, byte-for-byte. CI never installs `pnpm`, `cargo`, `vitest`, etc. — fixtures are static text files.

Trade-offs we accepted:

- **Drift over time:** captured output is a snapshot from one tool version on one machine. Tools change formats. Fixtures must be regenerated periodically (see [Regeneration](#regeneration)).
- **Coverage is bounded by the fixture set.** A rule that passes its three fixtures may still fail on output the fixtures don't represent. We mitigate this by capturing multiple scenarios per rule (success, failure, edge cases) and accepting that real-world drift will produce issues we then add fixtures for.

Rejected alternatives:

- **Live capture in CI.** Requires installing every tooled rule's dependencies. Slow, fragile, and varies by registry/network state.
- **Pure synthetic fixtures (hand-written).** Stable but cleaner-than-reality; rules that pass synthetic tests fail on real output. Real captures are the contract.

## Layout

Bundled rules live under `bundled-rules/` (embedded into the binary at build time). Their fixtures live under `tests/fixtures/`, kept out of the binary so the embedded rule set stays slim:

```
bundled-rules/
  <rule-id>.yaml          # the rule itself

tests/fixtures/
  <rule-id>/
    <scenario>/
      input.txt           # captured raw stdout+stderr (merged)
      expected.txt        # what the rule's pipeline must produce
      meta.yaml           # provenance metadata
```

Scenarios are short slugs describing the captured situation: `clean-install`, `with-warnings`, `compile-error`, `network-failure`, etc. Each rule should have at minimum one success-path fixture and one failure-path fixture.

### `meta.yaml` shape

```yaml
command: pnpm install
tool_version: "pnpm 9.4.0"
captured_at: 2026-04-12
os: linux
notes: clean install on a fresh node_modules; lockfile up to date
```

`os` is informational, not a test selector — fixtures are platform-agnostic in the sense that the test runs on every supported OS. If a rule needs to behave differently on macOS vs Linux, capture a fixture per OS and let the test runner assert against both.

## What each test verifies

Per fixture, the test asserts:

1. **Byte-exact match** of the rule's output against `expected.txt`. No leniency on whitespace or trailing newlines — those are part of the contract.
2. **Reduction threshold met.** `len(expected) / len(input) <= 0.5` for any fixture marked as a "primary success path" (the v1 ≥50% reduction acceptance criterion). Edge-case fixtures (e.g. failure-path output that's already small) are exempt via a flag in `meta.yaml`:

   ```yaml
   exempt_from_reduction_check: true
   ```

3. **No critical lines dropped** (optional, opt-in). A fixture can declare a list of substrings that must appear in the rule's output:

   ```yaml
   must_keep_lines:
     - "ERR_PNPM_PEER_DEP_ISSUES"
     - "exit code 1"
   ```

   The test fails if any listed substring is absent from the rule's output. This is the explicit way to encode "the error must survive filtering."

## Runner

A single Rust integration test file walks the `tests/fixtures/` tree, loads each rule (from `bundled-rules/<rule-id>.yaml`) and each of its scenarios, runs the rule's pipeline against `input.txt`, and asserts against `expected.txt`. Failures report the diff inline.

```
$ cargo test --test bundled_rules
running 32 tests
test pnpm-install::clean-install ... ok
test pnpm-install::with-warnings ... ok
test pnpm-install::peer-dep-error ... FAILED
...
```

`insta` is fine if we want snapshot ergonomics (`cargo insta review` to update). Plain `assert_eq!` works too. Either is acceptable — the contract is the fixture format, not the assertion library.

## Regeneration

When a tool changes its output format (and a fixture starts failing on real output, or a developer notices drift):

1. Run the canonical command on a clean machine and capture stdout+stderr merged:

   ```bash
   pnpm install 2>&1 > tests/fixtures/pnpm-install/clean-install/input.txt
   ```

2. Re-run the rule against the new input and inspect the output:

   ```bash
   lacon run --rule pnpm-install -- pnpm install 2>&1 \
     > tests/fixtures/pnpm-install/clean-install/expected.txt
   ```

   (Or use `cargo insta review` if running under `insta`.)

3. Update `meta.yaml`'s `tool_version` and `captured_at`.

4. Verify the test passes: `cargo test --test bundled_rules`.

5. Commit the fixture update and the rule changes (if any) together.

A helper script lives at `scripts/capture-fixtures.sh` for batch re-capture across all bundled rules. The script is best-effort — it requires every tool to be installed locally and skips ones that aren't, surfacing what was captured vs. what was missed.

## Out of scope for v1

Listed in [backlog](backlog.md):

- **User-facing fixture validation** — `lacon validate <rule.yaml> --fixtures <dir>` for users testing their own project rules. v1 users can manually run `lacon run --rule <id> -- <cmd>` to spot-check.
- **Automated drift detection** — a periodic CI job that re-captures all bundled-rule fixtures and opens an issue when output diffs from the committed snapshot. v1 relies on developer awareness and user issue reports.
