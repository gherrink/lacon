//! Pure command-normalization helper for `invocations.command_normalized`.
//!
//! Per CONTEXT D-18 + spec `docs/specs/tracking-data-model.md:68-72`:
//!   `<basename(argv[0])> [argv[1] if !starts_with('-')]`
//! else just `<basename(argv[0])>`.
//!
//! Normalization is implementation-defined — the spec says "may improve over time"
//! — so this fn is NOT a stable wire format. It exists to group cosmetically-
//! different invocations of the same command in `v_unmatched_offenders` /
//! `v_filtered_offenders`.

/// Derive a stable command-grouping key from `argv`.
///
/// # Examples
/// ```
/// use lacon_core::tracking::normalize;
/// assert_eq!(normalize(&["pnpm".into(), "install".into(), "--frozen-lockfile".into()]), "pnpm install");
/// assert_eq!(normalize(&["/usr/local/bin/pnpm".into(), "install".into()]), "pnpm install");
/// assert_eq!(normalize(&["cargo".into(), "-V".into()]), "cargo");
/// ```
pub fn normalize(argv: &[String]) -> String {
    let Some(prog) = argv.first() else {
        return String::new();
    };
    let basename = prog.rsplit('/').next().unwrap_or(prog);

    match argv.get(1) {
        Some(next) if !next.starts_with('-') => format!("{basename} {next}"),
        _ => basename.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn pnpm_install_with_flag_drops_flag() {
        assert_eq!(normalize(&s(&["pnpm", "install", "--frozen-lockfile"])), "pnpm install");
    }

    #[test]
    fn absolute_path_basename_stripped() {
        assert_eq!(normalize(&s(&["/usr/local/bin/pnpm", "install"])), "pnpm install");
    }

    #[test]
    fn cargo_v_flag_only_returns_basename() {
        assert_eq!(normalize(&s(&["cargo", "-V"])), "cargo");
    }

    #[test]
    fn single_arg_returns_basename() {
        assert_eq!(normalize(&s(&["cargo"])), "cargo");
    }

    #[test]
    fn cargo_test_release_keeps_first_subcommand() {
        assert_eq!(normalize(&s(&["cargo", "test", "--release"])), "cargo test");
    }

    #[test]
    fn relative_path_basename_stripped() {
        assert_eq!(normalize(&s(&["./scripts/build.sh", "release"])), "build.sh release");
    }

    #[test]
    fn empty_argv_returns_empty_string() {
        assert_eq!(normalize(&[]), "");
    }
}
