---
status: accepted
schema-version: 2
---

# 0004: Project > User > Bundled config precedence

## Context

`lacon` has three configuration layers: bundled (shipped with the binary), user (`~/.config/lacon/`), and project (`<cwd>/.lacon/`). When more than one layer defines a rule for the same command, resolution needs a deterministic precedence rule.

## Options

- **Project > User > Bundled, first-match-wins (chosen).** Closer to the work has higher priority; layers do not merge.
- **Merge stages from all matching layers.** More flexible — a project could add stages to a bundled rule without `extends` — but the debugging story for "the effective pipeline after merging three layers" is awful, and the gain over `extends` is small. Rejected.
- **Bundled wins.** Guards against accidental project overrides but defeats the point of project-specific configuration. Rejected.
- **No layering at all.** Simpler, but loses the value of bundled defaults. Rejected.

## Decision

Project rules win over user rules, which win over bundled rules. Closer to the work has higher priority. Resolution is first-match-wins; rules from different layers do not merge.

## Consequences

- Matches the convention every other dev tool uses (git, npm, eslint, prettier), so there are no surprises for users.
- Simple mental model: "the rule closest to my code wins."
- A project can't strengthen a user rule, only override it; users who want to layer use `extends` explicitly.
- A too-broad project rule can shadow more specific user/bundled rules, but this is debuggable via `lacon doctor` and the `v_filtered_offenders` view.
