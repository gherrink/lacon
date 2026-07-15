---
schema-version: 1
---

# Config schema

## Goal

Reference for the YAML configuration `lacon` loads at three layers (bundled, user, project). Config governs engine and tracking behavior; rules govern what gets filtered. Any change here is a breaking change for users.

## Context

Three layers, lowest to highest priority: bundled (compiled defaults, always present, not editable), user (`~/.config/lacon/config.yaml`), and project (`<cwd>/.lacon/config.yaml`). All layers are optional in that a missing file falls back to the layer below; bundled defaults are always available. Rule files are specified separately in the filter-rule-schema spec.

## Criteria

### File locations and precedence  {#file-locations-and-precedence}

Project (`<cwd>/.lacon/config.yaml`) overrides user (`~/.config/lacon/config.yaml`) overrides bundled (embedded). A missing file inherits from the lower layer.

### v1 keys and defaults  {#v1-keys-and-defaults}

The v1 keys are: `retention.invocations_days` (default 30), `retention.raw_outputs_days` (default 3), `defaults.max_bytes` (default 32768), and `store_raw_outputs` (default false). All keys are optional; a missing key inherits from the layer below.

### retention is user-only  {#retention-is-user-only}

The `retention` block may appear only in user or bundled config. `invocations_days` also governs the `suspected_regressions` table (tied by foreign key). Because the SQLite database is shared across all of a user's projects, per-project retention would be ambiguous — a project config containing a `retention` block fails validation.

### defaults.max_bytes scope and effect  {#defaults-max-bytes-scope-and}

`defaults.max_bytes` (integer, default 32768) is the final-stage cap applied to any rule that does not declare its own `max_bytes` primitive; the engine never returns more than this many bytes from a pipeline. Scope: project, user, or bundled.

### store_raw_outputs scope  {#store-raw-outputs-scope}

`store_raw_outputs` (boolean, default false) makes `lacon run` store merged stdout/stderr in the `raw_outputs` table for `lacon explain`. Scope: project, user, or bundled; project-level opt-in is the documented pattern.

### Per-key deep merge  {#per-key-deep-merge}

Resolving the effective config walks layers from bundled up to project. Each layer overrides scalar keys present in lower layers; sub-objects (`retention`, `defaults`) merge recursively rather than replacing wholesale, so a user need not repeat a bundled default to keep it.

### Per-key scope enforcement  {#per-key-scope-enforcement}

A config file that includes a key not allowed at its layer fails validation with an error pointing at the correct file — e.g. `.lacon/config.yaml:1: key \`retention\` is user-only; move to ~/.config/lacon/config.yaml`. Bundled config is the source of defaults and cannot be edited at runtime.

### Unknown keys fail validation  {#unknown-keys-fail-validation}

Unknown top-level or nested keys fail validation rather than being silently ignored, so a typo produces a clear error. Same posture as the filter-rule schema.

### Validation dispatch and malformed-config handling  {#validation-dispatch-and-malformed-config}

`lacon validate <path>` detects file type by content: top-level `id` + `match` is a rule file (validated against the filter-rule schema), anything else is a config file (validated against this spec). `lacon doctor` validates every layer's `config.yaml` in addition to its rule sweep, reporting layer-scope violations and unknown keys. A config file that fails validation is rejected at load time and the previous validated layer is used in its place — `lacon` does not silently fall back to defaults on a malformed file.
