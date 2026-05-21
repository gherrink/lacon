//! `rewrite` block application — `apply_rewrite(argv, &RewriteSpec) -> Vec<String>`.
//!
//! Per CONTEXT D-19. Apply order is `remove_flags` → `replace_flags` →
//! `add_flags`, so the `add_flags` idempotency check sees the post-remove /
//! post-replace argv. The function is pure and idempotent:
//! `apply_rewrite(&apply_rewrite(argv, rw), rw) == apply_rewrite(argv, rw)`
//! for every rewrite block and argv — locked by the T3 regression test below.
//! Idempotency matters because Plan 04 may re-invoke the hook on a
//! previously-rewritten command; without it, re-running would duplicate flags
//! or corrupt argv (threat T-03-03-02).
//!
//! `argv[0]` (the command) is NEVER touched, even when `replace_flags` maps it
//! (threat T-03-03-03 — argv[0] integrity is required for the command to run).

use crate::rules::schema::RewriteSpec;

/// Apply a [`RewriteSpec`] to `argv`, returning the rewritten argv.
///
/// Order: `remove_flags` first, then `replace_flags`, then `add_flags`. Empty
/// `argv` returns an empty `Vec`. `argv[0]` is preserved verbatim.
pub fn apply_rewrite(argv: &[String], rewrite: &RewriteSpec) -> Vec<String> {
    if argv.is_empty() {
        return Vec::new();
    }

    let mut out: Vec<String> = Vec::with_capacity(argv.len() + rewrite.add_flags.len());
    out.push(argv[0].clone()); // argv[0] preserved verbatim (D-19 invariant).

    // remove_flags: drop matching elements from argv[1..].
    let after_remove: Vec<&String> = argv[1..]
        .iter()
        .filter(|a| !rewrite.remove_flags.iter().any(|rf| rf == a.as_str()))
        .collect();

    // replace_flags: map old → new on each surviving element.
    let after_replace: Vec<String> = after_remove
        .iter()
        .map(|a| match rewrite.replace_flags.get(a.as_str()) {
            Some(new) => new.clone(),
            None => (*a).clone(),
        })
        .collect();

    out.extend(after_replace);

    // add_flags: append each only if not already present in argv[1..] (idempotent).
    for flag in &rewrite.add_flags {
        if !out[1..].iter().any(|existing| existing == flag) {
            out.push(flag.clone());
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    fn spec_add(flags: &[&str]) -> RewriteSpec {
        RewriteSpec {
            add_flags: s(flags),
            ..Default::default()
        }
    }

    fn spec_remove(flags: &[&str]) -> RewriteSpec {
        RewriteSpec {
            remove_flags: s(flags),
            ..Default::default()
        }
    }

    fn spec_replace(pairs: &[(&str, &str)]) -> RewriteSpec {
        RewriteSpec {
            replace_flags: BTreeMap::from_iter(
                pairs
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string())),
            ),
            ..Default::default()
        }
    }

    // T1 — basic add.
    #[test]
    fn add_to_clean_argv() {
        let spec = spec_add(&["--no-color"]);
        assert_eq!(
            apply_rewrite(&s(&["cargo", "test"]), &spec),
            s(&["cargo", "test", "--no-color"])
        );
    }

    // T2 — add of already-present flag is a no-op (no duplicate).
    #[test]
    fn add_already_present_is_noop() {
        let spec = spec_add(&["--no-color"]);
        assert_eq!(
            apply_rewrite(&s(&["cargo", "test", "--no-color"]), &spec),
            s(&["cargo", "test", "--no-color"])
        );
    }

    // T3 — `apply(apply(x)) == apply(x)` idempotency invariant (D-19).
    #[test]
    fn apply_twice_is_idempotent() {
        let spec = spec_add(&["--no-color"]);
        let argv = s(&["cargo", "test"]);
        let once = apply_rewrite(&argv, &spec);
        // D-19 invariant: re-applying the same spec to its own output is a no-op.
        assert_eq!(apply_rewrite(&once, &spec), once);
    }

    // T4 — basic remove.
    #[test]
    fn remove_present_flag() {
        let spec = spec_remove(&["--verbose"]);
        assert_eq!(
            apply_rewrite(&s(&["cargo", "test", "--verbose"]), &spec),
            s(&["cargo", "test"])
        );
    }

    // T5 — remove of absent flag is a no-op.
    #[test]
    fn remove_absent_flag_noop() {
        let spec = spec_remove(&["--verbose"]);
        assert_eq!(
            apply_rewrite(&s(&["cargo", "test"]), &spec),
            s(&["cargo", "test"])
        );
    }

    // T6 — remove drops ALL occurrences.
    #[test]
    fn remove_removes_all_occurrences() {
        let spec = spec_remove(&["--verbose"]);
        assert_eq!(
            apply_rewrite(&s(&["cargo", "test", "--verbose", "--verbose"]), &spec),
            s(&["cargo", "test"])
        );
    }

    // T7 — basic replace.
    #[test]
    fn replace_basic() {
        let spec = spec_replace(&[("--progress", "--no-progress")]);
        assert_eq!(
            apply_rewrite(&s(&["pnpm", "install", "--progress"]), &spec),
            s(&["pnpm", "install", "--no-progress"])
        );
    }

    // T8 — replace of already-replaced is a no-op (old form absent).
    #[test]
    fn replace_already_replaced_noop() {
        let spec = spec_replace(&[("--progress", "--no-progress")]);
        assert_eq!(
            apply_rewrite(&s(&["pnpm", "install", "--no-progress"]), &spec),
            s(&["pnpm", "install", "--no-progress"])
        );
    }

    // T9 — multi-arg add_flags use literal-element semantics: `--reporter` is
    // already present, so only `silent` is appended (per D-19 / RESEARCH:686).
    // For `--reporter silent` value-swap intent the rule author should use a
    // single `--reporter=silent` element or `replace_flags`.
    #[test]
    fn multi_arg_flag_literal_handling() {
        let spec = spec_add(&["--reporter", "silent"]);
        assert_eq!(
            apply_rewrite(&s(&["vitest", "--reporter", "verbose"]), &spec),
            s(&["vitest", "--reporter", "verbose", "silent"])
        );
    }

    // T10 — argv[0] is NEVER touched, even when replace_flags maps it (D-19).
    #[test]
    fn argv0_never_touched() {
        let spec = spec_replace(&[("cargo", "evil")]);
        let out = apply_rewrite(&s(&["cargo", "build"]), &spec);
        assert_eq!(out, s(&["cargo", "build"]));
        assert_eq!(out[0], "cargo");
    }

    // Edge case: empty argv returns empty Vec.
    #[test]
    fn empty_argv_returns_empty() {
        assert_eq!(apply_rewrite(&[], &spec_add(&["--no-color"])), Vec::<String>::new());
    }
}
