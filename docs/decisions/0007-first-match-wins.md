# 0007: First-match-wins rule resolution

**Status:** Accepted

## Context

When more than one rule could match a given command, we need a deterministic answer to "which rule applies?" The candidates are: first match wins (in priority order), most-specific match wins, or merge all matching rules.

## Decision

First-match-wins. The rule resolver walks layers in priority order (project → user → bundled), returns the first rule whose `match` block matches. Within a single layer, rules are checked in the lexicographic order of their filenames.

No merging across rules. No specificity ranking.

## Consequences

- Predictable: users can reason about which rule will fire for a given command without running the system
- Layering is explicit via `extends` rather than implicit via merge
- A too-broad `match` in a high-priority layer will shadow more specific rules in lower layers — surfaced via `lacon doctor` and via the `v_filtered_offenders` view, which makes it visible if a rule is matching commands it shouldn't
- Rule files in the same layer need stable, sortable names (any `<id>.yaml` filename works; alphabetical sorting handles ties)

## Alternatives considered

**Merge all matching rules.** Combine the pipelines of every rule that matches. More flexible — a project rule could add an extra stage to a bundled rule without `extends`. Rejected: the debugging story for "the effective pipeline is the merge of these three rules" is awful, and `extends` covers the same use case more transparently.

**Most-specific-match wins.** Use a specificity metric (e.g. number of match constraints, regex anchor strength) to pick the "tightest" matching rule. Rejected: defining a specificity metric for arbitrary regex matchers is surprisingly hard, the resulting behavior is not intuitive, and ties still need a deterministic tiebreaker.
