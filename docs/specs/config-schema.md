# Config schema

Reference for the YAML configuration loaded by `lacon` at three layers: bundled (compiled defaults), user (`~/.config/lacon/config.yaml`), and project (`<cwd>/.lacon/config.yaml`). Any change here is a breaking change for users.

For rule files, see [filter-rule-schema](filter-rule-schema.md). Config governs *engine and tracking behaviour*; rules govern *what gets filtered.*

## File locations

| Layer | Path | Use |
| --- | --- | --- |
| Bundled | embedded in binary | Defaults shown below. Always present, can't be edited. |
| User | `~/.config/lacon/config.yaml` | Personal settings. Overrides bundled. |
| Project | `<cwd>/.lacon/config.yaml` | Repo-specific settings. Overrides user. |

All three layers are optional in the sense that missing files fall back to the lower layer; bundled defaults are always available.

## v1 keys

```yaml
retention:                 # USER-ONLY
  invocations_days: 30     # default 30; also applies to suspected_regressions
  raw_outputs_days: 3      # default 3

defaults:                  # PROJECT-OR-USER
  max_bytes: 32768         # fallback final-stage cap for rules that omit max_bytes

store_raw_outputs: false   # PROJECT-OR-USER (project-level opt-in is the documented pattern)
```

All keys are optional. A missing key inherits from the layer below.

### `retention`

Pruning windows for the SQLite tables in [tracking-data-model](tracking-data-model.md). Pruning runs on `lacon` startup.

- `invocations_days` (integer, default 30) — also governs `suspected_regressions` (which is tied to `invocations` by foreign key).
- `raw_outputs_days` (integer, default 3) — independent of `invocations_days` because raw outputs are bulkier and warrant tighter retention.

**Scope: user-only.** The SQLite database is shared across projects on the user's machine; per-project retention overrides would create ambiguous semantics ("which project's retention wins for `raw_outputs` row X if it was captured in project A but pruned while running in project B?"). Project files that include a `retention` block fail validation.

### `defaults.max_bytes`

Integer, default 32768. Applied as the final-stage cap on any rule that does not declare its own `max_bytes` primitive (see [filter-rule-schema → max_bytes](filter-rule-schema.md#native-primitives)). The engine never returns more than this many bytes from a pipeline.

**Scope: project-or-user.** A repo with chattier-than-usual tooling can lower this for the project; a user can set their own preferred ceiling.

### `store_raw_outputs`

Boolean, default `false`. When true, `lacon run` stores merged stdout/stderr in the `raw_outputs` table for retrieval by `lacon explain`. See [tracking-data-model → Privacy](tracking-data-model.md#privacy) for the full v1 privacy contract.

**Scope: project-or-user.** Project-level opt-in is the documented pattern, but a user can default it on globally if they understand the privacy implications.

## Layer interaction

**Per-key deep merge.** When the engine resolves the effective config, it walks the layers from lowest priority (bundled) to highest (project). Each layer overrides scalar keys present in lower layers; sub-objects (`retention`, `defaults`) merge recursively rather than replacing wholesale.

Example. Given:

```yaml
# bundled (defaults)
retention:
  invocations_days: 30
  raw_outputs_days: 3
defaults:
  max_bytes: 32768
store_raw_outputs: false

# user
retention:
  invocations_days: 7
defaults:
  max_bytes: 16384

# project
store_raw_outputs: true
```

The effective config is:

```yaml
retention:
  invocations_days: 7        # from user
  raw_outputs_days: 3        # inherited from bundled
defaults:
  max_bytes: 16384           # from user
store_raw_outputs: true      # from project
```

User does not need to repeat `raw_outputs_days` to keep the bundled default.

## Per-key scope rules

| Key | Allowed layers | Rationale |
| --- | --- | --- |
| `retention.invocations_days` | user, bundled | DB-wide; per-project override would be ambiguous |
| `retention.raw_outputs_days` | user, bundled | Same as above |
| `defaults.max_bytes` | project, user, bundled | Reasonable per-project knob (chatty tools, etc.) |
| `store_raw_outputs` | project, user, bundled | Project opt-in is the documented use case |

A project config that includes a user-only key fails validation with an error pointing the user to `~/.config/lacon/config.yaml`. Example:

```
.lacon/config.yaml:1: key `retention` is user-only; move to ~/.config/lacon/config.yaml
```

Bundled config is the source of defaults; it cannot be edited at runtime. Modifying defaults requires a `lacon` release.

## Unknown keys

Unknown top-level or nested keys fail validation. We do not silently ignore them; a typo in a config file should produce a clear error rather than a silently-default behaviour. This is the same posture as [filter-rule-schema](filter-rule-schema.md).

## Validation

`lacon validate <path>` accepts both rule files and config files. The dispatcher detects file type by content:

- Top-level `id` + `match` → rule file → validate against [filter-rule-schema](filter-rule-schema.md)
- Anything else → config file → validate against this spec

`lacon doctor` runs config validation on every layer's `config.yaml` (if present) in addition to its rule sweep, and reports any layer-scope violations or unknown keys.

A config file that fails validation is rejected at load time; the previous (validated) layer is used in its place. `lacon` does not silently fall back to defaults when a config file is malformed — that would mask user mistakes.

## Worked example: monorepo project

```yaml
# .lacon/config.yaml — at the repo root
defaults:
  max_bytes: 8192          # this monorepo's tools are chatty; tighten the cap
store_raw_outputs: true    # team agreed; the dir is gitignored and machine-local
```

Combined with the user's `~/.config/lacon/config.yaml`:

```yaml
retention:
  invocations_days: 90     # the user wants longer trend history
```

The effective config when `lacon` runs in this project: 90-day invocation retention, 3-day raw-output retention (bundled), 8 KB default `max_bytes` (project), `store_raw_outputs: true` (project).
