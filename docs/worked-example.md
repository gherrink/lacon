# Worked example: writing a project-specific filter rule

This walkthrough shows how to write a filter rule for one of your own projects, building
on a bundled rule rather than starting from scratch. It uses `extends`, the only explicit
layering mechanism in `lacon` ([ADR 0012](decisions/0012-append-only-inheritance-extends.md)).

The full rule format is documented in
[`docs/specs/filter-rule-schema.md`](specs/filter-rule-schema.md); this page is a guided
tour of the most common task: trimming a few extra noisy lines that a bundled rule
doesn't know about, without rebuilding the whole rule.

## The scenario

Your monorepo runs `pnpm install` a lot. The bundled `pkg-install` rule already strips
deprecation warnings, progress bars, and funding notices — but in *this* repo, `pnpm`
also emits dozens of `Lockfile is up to date` / `Already up to date` lines you never need
to see. You want to keep everything `pkg-install` does and additionally drop those two
lines.

## Where the rule lives

Project rules live in `<repo>/.lacon/rules/*.yaml`. They are the highest-priority layer:

| Priority | Path |
|----------|------|
| 1 (highest) | `<cwd>/.lacon/rules/*.yaml` |
| 2 | `~/.config/lacon/rules/*.yaml` |
| 3 (lowest) | bundled (embedded in the binary) |

Create `.lacon/rules/our-monorepo-pnpm.yaml` in your repo root.

## The rule

```yaml
# .lacon/rules/our-monorepo-pnpm.yaml
id: our-monorepo-pnpm
description: pnpm install in our monorepo (verbose lockfile output we want to strip)
extends: bundled/pkg-install

# pnpm install in this repo emits 50+ "Lockfile is up to date" lines we don't need
pipeline:
  - drop_regex: '^Lockfile is up to date'
  - drop_regex: '^Already up to date'
```

That's the whole rule. It defines an `id`, a `description`, an `extends` pointer to the
bundled parent, and a `pipeline` with just the two extra stages it adds.

## Why it behaves the way it does

This rule does three things, all driven by the `extends` contract:

- **It inherits `match`, `rewrite`, and `on_error` from `bundled/pkg-install`.** Because
  this rule doesn't define those fields, they come from the parent — so it matches the
  same `pnpm install` / `npm install` / `yarn install` invocations and reuses the parent's
  error-path filtering. You only had to write the part that's different. Note this means
  your two extra `drop_regex` stages run on the **success path only**: the inherited
  `on_error` pipeline is unchanged, so on a *failed* install those two lines are still
  governed by the parent's error-path filtering, not your additions. To suppress them on
  errors too, redefine `on_error` in this rule (it replaces, not extends, the parent's).
- **It runs the bundled pipeline first, then the two extra `drop_regex` stages.** The
  parent's `pipeline` stages are *prepended* to yours. So output flows through all of
  `pkg-install`'s stages (strip ANSI, drop deprecation warnings, drop progress, …) and
  *then* through your two `drop_regex` stages. Inheritance is append-only — the parent's
  stages run before yours, in order.
- **It wins resolution against `bundled/pkg-install`.** Rule resolution is
  first-match-wins with project > user > bundled precedence
  ([ADR 0007](decisions/0007-first-match-wins-rule-resolution.md)). Because this rule
  lives in the project layer, it is selected ahead of the bundled rule it extends for
  any command both would match.

A note on what `extends` does **not** do: there is no way to remove, reorder, or insert
into the parent's pipeline stages. The model is deliberately simple — the parent's stages
are prepended, scalar fields you omit are inherited, and that's it. If you need finer
control, copy the bundled rule into your project layer and edit the copy directly. (These
remove/reorder/insert operations are out of scope for v1.)

## Validate and inspect

Once the file is in place, check it loads cleanly:

```sh
lacon validate .lacon/rules/our-monorepo-pnpm.yaml
```

`lacon validate` parses and type-checks the rule, flattens its `extends` chain, and
fails loudly on an invalid regex, an unknown primitive, a circular `extends`, or a
missing referenced Starlark file.

After running a matched command, you can inspect what a recorded invocation looked like
before and after filtering:

```sh
lacon explain <id>
```

`lacon explain` re-derives the filtered output from the stored raw bytes and shows it
side-by-side with the raw output, so you can confirm your rule trimmed exactly what you
intended and nothing you wanted to keep.
