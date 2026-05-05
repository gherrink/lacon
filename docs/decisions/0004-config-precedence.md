# 0004: Project > User > Bundled config precedence

**Status:** Accepted

## Context

`lacon` has three layers of configuration: bundled (shipped with the binary), user (`~/.config/lacon/`), and project (`<cwd>/.lacon/`). When more than one layer defines a rule for the same command, we need a deterministic precedence rule.

## Decision

Project rules win over user rules, which win over bundled rules. Closer to the work has higher priority. Resolution is first-match-wins; rules from different layers do not merge.

## Consequences

- Matches the convention every other dev tool uses (git, npm, eslint, prettier, etc.) — no surprises for users
- Simple mental model: "the rule closest to my code wins"
- A project can't strengthen a user rule, only override it; users who want to layer use `extends` explicitly
- A too-broad project rule can shadow more specific user/bundled rules, but this is debuggable via `lacon doctor` and the `v_filtered_offenders` view

## Alternatives considered

**Merge stages from all matching layers.** More flexible — a project could add stages on top of a bundled rule without `extends`. Rejected: the debugging story for "what does the effective pipeline look like after merging three layers" is awful, and the gain over `extends` is small.

**Bundled wins.** Protects against accidental project overrides but defeats the entire point of project-specific configuration. Trivially rejected.

**No layering at all.** Simpler, but loses the value of bundled defaults. Trivially rejected.
