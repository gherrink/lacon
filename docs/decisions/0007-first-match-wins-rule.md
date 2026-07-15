---
status: accepted
schema-version: 2
---

# 0007: First-match-wins rule resolution

## Context

When more than one rule could match a command, resolution needs a deterministic answer to "which rule applies?" The candidates are: first-match-wins (in priority order), most-specific-match wins, or merge all matching rules.

## Options

- **First-match-wins (chosen).** Walk layers in priority order (project → user → bundled), return the first rule whose `match` block matches; within a layer, check in lexicographic filename order. No merging, no specificity ranking.
- **Merge all matching rules.** Combine the pipelines of every matching rule — more flexible, but the debugging story for "the effective pipeline is the merge of these three rules" is awful, and `extends` covers the same use case more transparently. Rejected.
- **Most-specific-match wins.** Pick the "tightest" matching rule by a specificity metric — but defining specificity for arbitrary regex matchers is surprisingly hard, the behavior is unintuitive, and ties still need a deterministic tiebreaker. Rejected.

## Decision

First-match-wins. The rule resolver walks layers in priority order (project → user → bundled) and returns the first rule whose `match` block matches. Within a single layer, rules are checked in the lexicographic order of their filenames. No merging across rules; no specificity ranking.

## Consequences

- Predictable: users can reason about which rule fires for a command without running the system.
- Layering is explicit via `extends` rather than implicit via merge.
- A too-broad `match` in a high-priority layer shadows more specific lower-layer rules — surfaced via `lacon doctor` and the `v_filtered_offenders` view.
- Rule files in the same layer need stable, sortable names (any `<id>.yaml` works; alphabetical sorting handles ties).
