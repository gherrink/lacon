//! POSIX-portable shell-quote — `quote_for_shell(arg) -> Cow<str>`.
//!
//! Algorithm (CONTEXT D-20): if `arg` has no shell metacharacters and no
//! whitespace, return `Cow::Borrowed` (zero allocation). Otherwise wrap in
//! single quotes and replace each embedded `'` with the `'\''` close-escape-
//! reopen idiom. Portable across sh, bash, dash, zsh in argv position.
//!
//! Trust scope (CONTEXT D-22): this quoter only has to survive ONE shell parse
//! — Claude Code's `bash -c` invocation of the rewritten `lacon run` command.
//! The downstream Runner uses `Command::new(&argv[0]).args(&argv[1..])` (see
//! `crates/lacon-core/src/runtime/mod.rs:138-141`), so there is NO second
//! shell-interpret hop. A quoting bug here is a command-injection vulnerability
//! (threat T-03-03-01 / T-quote-injection) — the inline round-trip tests through
//! `/bin/sh` are the regression guard, with the `$(...)` case the critical one.

use std::borrow::Cow;

/// Shell-quote `arg` for safe insertion into a single bash command line.
///
/// Returns `Cow::Borrowed(arg)` when no quoting is needed; otherwise a
/// single-quote-wrapped `Cow::Owned`.
pub fn quote_for_shell(arg: &str) -> Cow<'_, str> {
    // Verbatim D-20 metachar set: pipe, ampersand, semicolon, redirections,
    // parens, dollar, backtick, backslash, double-quote, single-quote, newline,
    // tab, space, glob chars, comment, tilde, assignment, job-control, history.
    const METACHARS: &[u8] = b"|&;<>()$`\\\"'\n\t *?[#~=%!";

    let needs_quote = arg.is_empty() || arg.bytes().any(|b| METACHARS.contains(&b));
    if !needs_quote {
        return Cow::Borrowed(arg);
    }

    let mut out = String::with_capacity(arg.len() + 2);
    out.push('\'');
    for c in arg.chars() {
        if c == '\'' {
            // Close the single-quote, emit an escaped literal quote, reopen.
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trip helper: build `printf '%s\n' <quoted args...>`, run it through
    /// `/bin/sh -c`, and split stdout back into the recovered argv. This is the
    /// first test in the module that shells out — justified by the D-22 security
    /// property: the only honest proof that quoting survives a real shell parse
    /// is to feed it through one. `/bin/sh` is present on all v1 targets
    /// (macOS + Linux), so CI hermeticity is not violated.
    fn roundtrip_via_sh(argv: &[&str]) -> Vec<String> {
        let parts: Vec<String> = std::iter::once("printf '%s\\n'".to_string())
            .chain(argv.iter().map(|a| quote_for_shell(a).into_owned()))
            .collect();
        let cmd = parts.join(" ");
        let output = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(&cmd)
            .output()
            .unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        stdout.lines().map(String::from).collect()
    }

    #[test]
    fn quote_plain_no_quote() {
        // Borrow path — exact equality, no shell needed.
        assert_eq!(quote_for_shell("hello"), "hello");
    }

    #[test]
    fn quote_with_space() {
        assert_eq!(roundtrip_via_sh(&["a b"]), vec!["a b"]);
    }

    #[test]
    fn quote_with_dollar() {
        // CRITICAL T-quote-injection guard: the `rm` must NOT execute; the
        // literal `$(rm -rf /)` text must survive intact.
        assert_eq!(roundtrip_via_sh(&["$(rm -rf /)"]), vec!["$(rm -rf /)"]);
    }

    #[test]
    fn quote_with_backtick() {
        assert_eq!(roundtrip_via_sh(&["`whoami`"]), vec!["`whoami`"]);
    }

    #[test]
    fn quote_with_single_q() {
        assert_eq!(roundtrip_via_sh(&["it's"]), vec!["it's"]);
    }

    #[test]
    fn quote_with_newline() {
        assert_eq!(roundtrip_via_sh(&["a\nb"]), vec!["a", "b"]);
    }

    #[test]
    fn quote_with_tab() {
        assert_eq!(roundtrip_via_sh(&["a\tb"]), vec!["a\tb"]);
    }

    #[test]
    fn quote_empty() {
        assert_eq!(quote_for_shell(""), "''");
    }

    #[test]
    fn quote_eq_value() {
        assert_eq!(roundtrip_via_sh(&["--reporter=val"]), vec!["--reporter=val"]);
    }

    #[test]
    fn quote_eq_with_space() {
        assert_eq!(
            roundtrip_via_sh(&["--reporter=custom reporter"]),
            vec!["--reporter=custom reporter"]
        );
    }

    #[test]
    fn quote_paren() {
        assert_eq!(roundtrip_via_sh(&["(group)"]), vec!["(group)"]);
    }
}
