//! TUI (terminal-user-interface) heuristic — `is_tui(command, args) -> bool`.
//!
//! Authoritative spec: `docs/specs/chained-commands.md:83-108` (the v1 TUI list
//! and the conditional-pattern table). Per CONTEXT D-15 this lives in adapter
//! code, NOT `lacon-core` — the spec is explicit ("The list lives in adapter
//! code") and there is no cross-adapter reuse to justify promoting it (YAGNI).
//!
//! The heuristic runs per-segment AFTER chain splitting and BEFORE rule
//! resolution (CON-chained-tui-bypass-whole-chain): any segment matching
//! `is_tui` causes the WHOLE chain to bypass filtering (v1 conservative). Plan
//! 04 enforces the whole-chain decision; this module owns the predicate.
//!
//! It is a pure predicate — no side effects, no I/O.

use std::path::Path;

/// Pure-TUI tools matched by `basename(command)`. The 22 entries are verbatim
/// from `docs/specs/chained-commands.md:85-87`. A linear scan is faster than a
/// `HashSet` for n=22 and adds nothing to cold start.
pub const PURE_TUI: &[&str] = &[
    // Editors
    "vim", "vi", "nvim", "nano", "emacs",
    // Pagers
    "less", "more", "most", "man",
    // System monitors
    "htop", "top", "btop",
    // Multiplexers and remote shells
    "screen", "tmux", "ssh", "mosh",
    // REPLs (always interactive)
    "ipython", "irb", "pry",
    // Tools that take over the terminal
    "redis-cli", "crontab", "visudo",
];

/// True if `command` is a pure-TUI tool, or if `(command, args)` matches one of
/// the conditional patterns from `docs/specs/chained-commands.md:91-101`.
///
/// Lookup is by `basename(command)` so path-prefixed forms (`/usr/bin/vim`)
/// resolve the same as bare names (D-16).
pub fn is_tui(command: &str, args: &[String]) -> bool {
    // Step 1: extract basename via the Path API (not rsplit('/')) so trailing
    // slashes and platform separators are handled uniformly.
    let basename = Path::new(command)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(command);

    // Step 2: pure-TUI table lookup.
    if PURE_TUI.contains(&basename) {
        return true;
    }

    // Step 3: conditional-pattern dispatch (D-17).
    match basename {
        "git" => is_git_interactive(args),
        "npm" | "yarn" | "pnpm" => is_pkg_init_interactive(args),
        "node" | "python" | "python3" => is_repl(args),
        "mysql" | "psql" | "sqlite3" => is_db_interactive(args, basename),
        _ => false,
    }
}

/// `git` subcommands that take over the terminal interactively, per the spec
/// table (`docs/specs/chained-commands.md:93-97`). Dispatch is on `args[0]`.
fn is_git_interactive(args: &[String]) -> bool {
    let Some(subcmd) = args.first() else {
        return false;
    };
    let rest = &args[1..];
    match subcmd.as_str() {
        // `git rebase -i` / `--interactive`.
        "rebase" => has_any_flag(rest, &["-i", "--interactive"]),
        // `git commit` is interactive UNLESS a message is supplied inline.
        "commit" => !has_commit_message(rest),
        // `git add -p` / `--patch` / `-i` / `--interactive`.
        "add" => has_any_flag(rest, &["-p", "--patch", "-i", "--interactive"]),
        // `git checkout -p` / `--patch`.
        "checkout" => has_any_flag(rest, &["-p", "--patch"]),
        // `git stash -p` / `--patch`.
        "stash" => has_any_flag(rest, &["-p", "--patch"]),
        _ => false,
    }
}

/// True if a commit message is supplied inline (so `git commit` is NOT
/// interactive): `-m` / `--message` / `--message=…` / `-F` / `--file`, OR a
/// bundled short-flag form whose letters include `m` (e.g. `-am`, `-vm`) —
/// `git commit -am "msg"` is an extremely common non-interactive invocation
/// that the exact-match form misclassified as TUI (WR-01).
fn has_commit_message(args: &[String]) -> bool {
    args.iter().any(|a| {
        a == "-m"
            || a == "--message"
            || a.starts_with("--message=")
            || a == "-F"
            || a == "--file"
            || is_bundled_short_flag_with_m(a)
    })
}

/// True if `arg` is a single-dash bundled short-flag cluster containing `m`
/// (matches `^-[a-zA-Z]*m`): a short cluster like `-am` / `-vm` / `-amend`?
/// No `--` long flags, no `=` (a `-m`-with-glued-value is `-m` exact-matched
/// above). The presence of `m` anywhere in a single-dash ASCII-letter cluster
/// means a `git commit` message is supplied inline.
fn is_bundled_short_flag_with_m(arg: &str) -> bool {
    let Some(rest) = arg.strip_prefix('-') else {
        return false;
    };
    // Exclude long flags (`--…`) and any cluster that is not purely ASCII
    // letters (so `-m=x` / `-1` / numeric fd forms do not match).
    if rest.starts_with('-') || rest.is_empty() {
        return false;
    }
    rest.bytes().all(|c| c.is_ascii_alphabetic()) && rest.contains('m')
}

/// `npm`/`yarn`/`pnpm init` is interactive UNLESS `-y` / `--yes` is present.
fn is_pkg_init_interactive(args: &[String]) -> bool {
    matches!(args.first(), Some(s) if s == "init") && !has_any_flag(&args[1..], &["-y", "--yes"])
}

/// REPL launchers (`node`, `python`, `python3`) drop into an interactive prompt
/// when given no positional argument.
///
/// Conservative form (CONTEXT/RESEARCH:607): `python --version` is treated as
/// TUI because it has no positional argument. A false positive only costs one
/// whole-chain bypass; a false negative would hang the user's terminal. The
/// `--version`/`--help` exemption is deferred to v1.5 polish.
fn is_repl(args: &[String]) -> bool {
    args.iter().all(|a| a.starts_with('-'))
}

/// Interactive DB shells (`mysql`, `psql`, `sqlite3`) launch a prompt when given
/// no positional argument AND no inline-command flag
/// (`-c`/`-e`/`-f`/`--command`/`--execute`/`--file`).
fn is_db_interactive(args: &[String], _basename: &str) -> bool {
    // A positional argument (anything not starting with `-`) means a script /
    // database file was supplied — not interactive.
    if args.iter().any(|a| !a.starts_with('-')) {
        return false;
    }
    // An inline-command flag means a one-shot query — not interactive.
    !has_any_flag(
        args,
        &["-c", "-e", "-f", "--command", "--execute", "--file"],
    )
}

/// True if any element of `args` exactly equals one of `flags`.
fn has_any_flag(args: &[String], flags: &[&str]) -> bool {
    args.iter().any(|a| flags.contains(&a.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn basename_extraction_strips_path() {
        assert!(is_tui("/usr/bin/vim", &[]));
    }

    #[test]
    fn pure_tui_happy_path() {
        assert!(is_tui("htop", &[]));
    }

    #[test]
    fn non_tui_is_false() {
        assert!(!is_tui("ls", &s(&["-la"])));
    }

    #[test]
    fn git_commit_no_message_is_interactive() {
        assert!(is_tui("git", &s(&["commit"])));
    }
}
