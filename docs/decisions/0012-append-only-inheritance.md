# 0012: Append-only inheritance via extends

**Status:** Accepted

## Context

Rules can build on each other via `extends`. The question is how much control the child rule should have over the parent's pipeline. Options range from "completely override" through "append-only" to "fine-grained insert/remove of specific stages."

## Decision

`extends` inherits scalar fields (`description`, `match`, `bypass_when`, `rewrite`, `on_error`, `post_process`) where the child doesn't define them, and *prepends* the parent's `pipeline` stages. The child's pipeline runs after the parent's. There is no mechanism to remove, reorder, or insert into the parent's pipeline.

If a child rule needs anything more complex, it copies the parent rule's content and edits it.

## Consequences

- Simple, predictable behavior: the effective pipeline is parent's stages + child's stages, in that order
- Rule files stay readable — the effective behavior of a child rule can be understood by reading the child plus its parent, no merge engine to mentally simulate
- Some rules will be partially copy-pasted from bundled. Acceptable cost for clarity.
- Avoids the debugging nightmare of multi-layer pipeline merging where the effective pipeline isn't visible without tooling

## Alternatives considered

**Stage-IDs with insert/remove operations.** Each stage gets a name; child rules can `insert_after: <id>`, `remove: <id>`, etc. More powerful, but the rule file becomes a patch instruction rather than a description, and the effective pipeline isn't visible without running a merge tool.

**Full override-only (no inheritance).** Children can replace parent fields but never compose with them. Rejected: too restrictive; users would copy-paste bundled rules constantly, then drift over time as the bundled rules evolve.

**Configurable merge mode (`extends_mode: append | prepend | replace`).** Possible future extension, but adds a rarely-needed knob to the schema. Filed in the backlog.
