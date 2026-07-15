---
status: accepted
schema-version: 2
---

# 0012: Append-only inheritance via extends

## Context

Rules can build on each other via `extends`. The question is how much control the child rule has over the parent's pipeline — options range from "completely override" through "append-only" to "fine-grained insert/remove of specific stages."

## Options

- **Append-only inheritance (chosen).** Inherit scalar fields the child omits, and *prepend* the parent's pipeline stages; no remove/reorder/insert. Anything more complex, the child copies the parent and edits.
- **Stage-IDs with insert/remove operations.** Each stage gets a name; children `insert_after: <id>`, `remove: <id>`, etc. More powerful, but the rule file becomes a patch instruction rather than a description, and the effective pipeline isn't visible without a merge tool. Rejected.
- **Full override-only (no inheritance).** Children replace parent fields but never compose — too restrictive; users would copy-paste bundled rules constantly, then drift as bundled rules evolve. Rejected.
- **Configurable merge mode (`extends_mode: append | prepend | replace`).** A possible future extension, but a rarely-needed schema knob. Backlogged.

## Decision

`extends` inherits scalar fields (`description`, `match`, `bypass_when`, `rewrite`, `on_error`, `post_process`) where the child doesn't define them, and *prepends* the parent's `pipeline` stages — the child's pipeline runs after the parent's. There is no mechanism to remove, reorder, or insert into the parent's pipeline; a child needing more copies the parent's content and edits it.

## Consequences

- Simple, predictable behavior: the effective pipeline is parent's stages + child's stages, in that order.
- Rule files stay readable — a child's effective behavior is understood by reading the child plus its parent, with no merge engine to mentally simulate.
- Some rules will be partially copy-pasted from bundled — an acceptable cost for clarity.
- Avoids the debugging nightmare of multi-layer pipeline merging where the effective pipeline isn't visible without tooling.
