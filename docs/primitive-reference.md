# Primitive reference

A worked input→output example for every one of the ten native pipeline primitives.

`lacon` rules filter command output by running it through an ordered `pipeline` of
stages. Each stage is one *primitive*. With the deliberate exception of the Starlark
`post_process` stage (which runs on aggregated output, see [ADR 0008](decisions/0008-aggregated-post-process-starlark.md)),
every native primitive is a **streaming, line-by-line transformer** — it sees output
one line at a time and never buffers more than its own bounded state ([ADR 0005](decisions/0005-streaming-first-output-processing.md)).
Memory is bounded by the largest stateful primitive (typically `keep_tail N`) plus the
final `max_bytes` cap.

The contract for these primitives is [`docs/specs/filter-rule-schema.md`](specs/filter-rule-schema.md);
that spec is the source of truth and any change there is a breaking change for users.
Each example below is taken byte-for-byte from the project's golden test fixtures
(`tests/fixtures/primitives/<name>/{input.txt,expected.txt}`, exercised by
`crates/lacon-core/tests/primitives.rs`), so every example *is* the tested behavior —
the docs cannot drift from what the engine actually does.

> The inputs below are truncated for readability where a fixture is long (e.g. 50 or
> 200 lines); the truncation is marked with `…`. The kept/dropped *behavior* shown is
> exactly the fixture's behavior.

---

## `strip_ansi`

Removes ANSI color and control escape sequences. No arguments.

```yaml
- strip_ansi
```

**Input** (escape sequences shown as `\e`):

```
\e[31mred error\e[0m
\e[1mbold\e[0m heading
plain text
\e[2J\e[Hfullscreen
```

**Output:**

```
red error
bold heading
plain text
fullscreen
```

The color codes (`\e[31m`, `\e[0m`, `\e[1m`) and the screen-control codes (`\e[2J`,
`\e[H`) are stripped; the visible text is untouched.

---

## `drop_regex`

Drops any line that matches the regex. (Blacklist mode — the matched lines are removed,
everything else passes through.)

```yaml
- drop_regex: '^npm warn deprecated'
```

**Input:**

```
Installing dependencies...
npm warn deprecated har-validator@5.1.3: this library is no longer supported
npm warn deprecated uuid@3.4.0: Please upgrade to version 7
added 342 packages in 12.3s
npm warn deprecated request@2.88.2: request has been deprecated
Build successful.
npm warn deprecated node-forge@0.10.0: please upgrade
Starting development server...
Server listening on port 3000
```

**Output:**

```
Installing dependencies...
added 342 packages in 12.3s
Build successful.
Starting development server...
Server listening on port 3000
```

Every line beginning `npm warn deprecated` is dropped; the signal lines remain.

---

## `keep_regex`

Whitelist mode. If any `keep_regex` stage is present, **only** lines matching are kept;
all others are dropped. Multiple `keep_regex` stages are OR'd together (a line survives
if it matches any one of them).

```yaml
- keep_regex: '(error|ERROR|FAIL)'
```

**Input** (a 52-line test run; only the relevant lines shown):

```
test suite: unit tests
running 50 tests
test auth::login ... ok
…
test config::validate ... FAIL
…
test logger::error ... ok
…
test permission::revoke ... FAIL
…
test crypto::verify ... error: hash mismatch
test tls::handshake ... ERROR: certificate expired
test result: FAILED. 47 passed; 3 failed;
```

**Output:**

```
test config::validate ... FAIL
test logger::error ... ok
test permission::revoke ... FAIL
test crypto::verify ... error: hash mismatch
test tls::handshake ... ERROR: certificate expired
test result: FAILED. 47 passed; 3 failed;
```

Only the six lines containing `error`, `ERROR`, or `FAIL` survive — including
`test logger::error ... ok` (it matches `error`) and `test result: FAILED.` (it
matches `FAIL`). The passing `... ok` lines are dropped.

---

## `replace_regex`

Substitutes matched text in place. Takes a `pattern` and a `replacement`.

```yaml
- replace_regex:
    pattern: '/Users/[^/]+/'
    replacement: '~/'
```

**Input:**

```
Compiling /Users/alice/projects/myapp/src/main.rs
Compiling /Users/alice/projects/myapp/src/lib.rs
Compiling /Users/alice/projects/myapp/src/config.rs
Compiling /Users/alice/projects/myapp/src/db.rs
warning: unused variable in /Users/alice/projects/myapp/src/main.rs:42
error[E0382]: use of moved value at /Users/alice/projects/myapp/src/lib.rs:17
```

**Output:**

```
Compiling ~/projects/myapp/src/main.rs
Compiling ~/projects/myapp/src/lib.rs
Compiling ~/projects/myapp/src/config.rs
Compiling ~/projects/myapp/src/db.rs
warning: unused variable in ~/projects/myapp/src/main.rs:42
error[E0382]: use of moved value at ~/projects/myapp/src/lib.rs:17
```

Each `/Users/alice/` home-directory prefix is rewritten to `~/`. No lines are dropped;
only the matched span within each line changes.

---

## `dedupe`

Collapses **consecutive** duplicate lines. Optional `max_kept` (default `1`) controls
how many copies of a repeated run are kept. Non-adjacent duplicates are not affected.

```yaml
- dedupe: { max_kept: 1 }
```

**Input:**

```
done
done
done
start
done
done
```

**Output:**

```
done
start
done
```

The first run of three `done` lines collapses to one; `start` passes through; the
final run of two `done` lines collapses to one. Note the trailing `done` is **not**
merged with the earlier ones — only consecutive duplicates collapse.

---

## `collapse_repeated`

Collapses a consecutive run of lines that all match `pattern` into `max_kept` example
lines plus exactly one fixed elision marker of the form `[lacon: collapsed N lines]`,
where `N` is the number of dropped lines.

```yaml
- collapse_repeated:
    pattern: '^Progress: \d+%'
    max_kept: 1
```

> **Deprecated `summary` key:** earlier drafts accepted a free-form `summary:`
> template with a `{count}` placeholder. As of v1 (Phase 9) that template is **no
> longer emitted** — the dropped run is always replaced by the fixed
> `[lacon: collapsed N lines]` marker. The `summary` key is still accepted for
> backward compatibility (rules carrying it continue to parse) but its value is
> ignored; rules should drop it. See `docs/specs/filter-rule-schema.md`.

**Input** (200 consecutive progress lines, then a final line):

```
Progress: 1%
Progress: 2%
Progress: 3%
…
Progress: 200%
Done
```

**Output:**

```
Progress: 1%
[lacon: collapsed 199 lines]
Done
```

The first matching line (`Progress: 1%`) is kept (`max_kept: 1`); the remaining 199
matching lines are replaced by the fixed marker `[lacon: collapsed 199 lines]`; the
non-matching `Done` passes through unchanged.

---

## `keep_head`

Keeps only the first N lines (`lines: N`) or first N bytes (`bytes: N`), dropping the
rest of the stream.

```yaml
- keep_head: { lines: 5 }
```

**Input** (50 lines):

```
line 1
line 2
line 3
…
line 50
```

**Output:**

```
line 1
line 2
line 3
line 4
line 5
```

Only the first five lines survive; lines 6–50 are dropped.

---

## `keep_tail`

Keeps only the last N lines (`lines: N`) or last N bytes (`bytes: N`). Implemented as a
bounded ring buffer, so memory stays bounded even on very long streams.

```yaml
- keep_tail: { lines: 5 }
```

**Input** (50 lines):

```
line 1
line 2
…
line 49
line 50
```

**Output:**

```
line 46
line 47
line 48
line 49
line 50
```

Only the last five lines survive; lines 1–45 are dropped.

---

## `keep_around_match`

For each line matching `pattern`, keeps `before` preceding lines and `after` following
lines — the same windowing as `grep -B<before> -A<after>`. Lines outside every match's
window are dropped.

```yaml
- keep_around_match:
    pattern: '^FAIL '
    before: 0
    after: 15
```

**Input** (100 lines; line 50 is the only match):

```
line 1
line 2
…
line 49
FAIL critical test at line 50
line 51
…
line 100
```

**Output:**

```
FAIL critical test at line 50
line 51
line 52
line 53
line 54
line 55
line 56
line 57
line 58
line 59
line 60
line 61
line 62
line 63
line 64
line 65
```

The matching line plus the 15 lines after it are kept (`before: 0`, `after: 15`);
everything before the match and everything past line 65 is dropped.

---

## `max_bytes`

A hard cap on total output size. Once the cap is reached, output is truncated and a
`[lacon: truncated, N more bytes dropped]` marker is appended, where `N` is the number
of bytes dropped. This primitive **must be the last stage** in the pipeline (it is
auto-injected at the end if a rule omits it).

```yaml
- max_bytes: 200
```

**Input** (20 lines of `output line NN: some content here`):

```
output line 01: some content here
output line 02: some content here
…
output line 20: some content here
```

**Output:**

```
output line 01: some content here
output line 02: some content here
output line 03: some content here
output line 04: some content here
output line 05: some content here
[lacon: truncated, 510 more bytes dropped]
```

With a 200-byte cap, the first five lines fit; the remaining output (510 bytes) is
dropped and reported by the truncation marker. The marker's byte count is exact, so the
model can see how much was discarded.
