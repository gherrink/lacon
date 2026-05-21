//! TUI heuristic test gate — one `#[test]` per row of the spec's TUI table.
//!
//! Spec: `docs/specs/chained-commands.md:83-108` (22 pure-TUI basenames + the
//! 8-row conditional-pattern table). Negative tests lock against over-aggressive
//! matching (false positives cost filtering opportunity; false negatives hang
//! the terminal — see threat T-03-03-04 / T-03-03-05).

use lacon_adapter_claudecode::tui::is_tui;

/// Build a `Vec<String>` args slice from string literals.
fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

// ---------------------------------------------------------------------------
// 22 pure-TUI basenames (docs/specs/chained-commands.md:85-87)
// ---------------------------------------------------------------------------

#[test]
fn pure_tui_vim_is_tui() {
    assert!(is_tui("vim", &[]));
}

#[test]
fn pure_tui_vi_is_tui() {
    assert!(is_tui("vi", &[]));
}

#[test]
fn pure_tui_nvim_is_tui() {
    assert!(is_tui("nvim", &[]));
}

#[test]
fn pure_tui_nano_is_tui() {
    assert!(is_tui("nano", &[]));
}

#[test]
fn pure_tui_emacs_is_tui() {
    assert!(is_tui("emacs", &[]));
}

#[test]
fn pure_tui_less_is_tui() {
    assert!(is_tui("less", &[]));
}

#[test]
fn pure_tui_more_is_tui() {
    assert!(is_tui("more", &[]));
}

#[test]
fn pure_tui_most_is_tui() {
    assert!(is_tui("most", &[]));
}

#[test]
fn pure_tui_man_is_tui() {
    assert!(is_tui("man", &s(&["git"])));
}

#[test]
fn pure_tui_htop_is_tui() {
    assert!(is_tui("htop", &[]));
}

#[test]
fn pure_tui_top_is_tui() {
    assert!(is_tui("top", &[]));
}

#[test]
fn pure_tui_btop_is_tui() {
    assert!(is_tui("btop", &[]));
}

#[test]
fn pure_tui_screen_is_tui() {
    assert!(is_tui("screen", &[]));
}

#[test]
fn pure_tui_tmux_is_tui() {
    assert!(is_tui("tmux", &[]));
}

#[test]
fn pure_tui_ssh_is_tui() {
    assert!(is_tui("ssh", &s(&["host"])));
}

#[test]
fn pure_tui_mosh_is_tui() {
    assert!(is_tui("mosh", &s(&["host"])));
}

#[test]
fn pure_tui_ipython_is_tui() {
    assert!(is_tui("ipython", &[]));
}

#[test]
fn pure_tui_irb_is_tui() {
    assert!(is_tui("irb", &[]));
}

#[test]
fn pure_tui_pry_is_tui() {
    assert!(is_tui("pry", &[]));
}

#[test]
fn pure_tui_redis_cli_is_tui() {
    assert!(is_tui("redis-cli", &[]));
}

#[test]
fn pure_tui_crontab_is_tui() {
    assert!(is_tui("crontab", &s(&["-e"])));
}

#[test]
fn pure_tui_visudo_is_tui() {
    assert!(is_tui("visudo", &[]));
}

// ---------------------------------------------------------------------------
// 8 conditional-pattern rows (docs/specs/chained-commands.md:93-100)
// ---------------------------------------------------------------------------

// git rebase -i / --interactive
#[test]
fn git_rebase_interactive_flag_true() {
    assert!(is_tui("git", &s(&["rebase", "-i", "HEAD~5"])));
}

#[test]
fn git_rebase_no_interactive_flag_false() {
    assert!(!is_tui("git", &s(&["rebase", "HEAD~5"])));
}

// git commit without -m / -F
#[test]
fn git_commit_no_message_true() {
    assert!(is_tui("git", &s(&["commit"])));
}

#[test]
fn git_commit_with_dash_m_false() {
    assert!(!is_tui("git", &s(&["commit", "-m", "msg"])));
}

// git add -p
#[test]
fn git_add_patch_true() {
    assert!(is_tui("git", &s(&["add", "-p"])));
}

#[test]
fn git_add_filename_false() {
    assert!(!is_tui("git", &s(&["add", "file.txt"])));
}

// git checkout -p
#[test]
fn git_checkout_patch_true() {
    assert!(is_tui("git", &s(&["checkout", "-p"])));
}

// git stash -p
#[test]
fn git_stash_patch_true() {
    assert!(is_tui("git", &s(&["stash", "-p"])));
}

// npm / yarn / pnpm init without -y / --yes (one row per package manager)
#[test]
fn npm_init_no_yes_true() {
    assert!(is_tui("npm", &s(&["init"])));
}

#[test]
fn npm_init_with_yes_false() {
    assert!(!is_tui("npm", &s(&["init", "-y"])));
}

#[test]
fn yarn_init_no_yes_true() {
    assert!(is_tui("yarn", &s(&["init"])));
}

#[test]
fn pnpm_init_with_long_yes_false() {
    assert!(!is_tui("pnpm", &s(&["init", "--yes"])));
}

// node / python / python3 REPL (no positional argument)
#[test]
fn python3_no_args_true() {
    assert!(is_tui("python3", &[]));
}

#[test]
fn python3_with_script_false() {
    assert!(!is_tui("python3", &s(&["script.py"])));
}

#[test]
fn node_no_args_true() {
    assert!(is_tui("node", &[]));
}

// mysql / psql / sqlite3 interactive shell (no positional, no -c/-e/-f)
#[test]
fn psql_no_positional_true() {
    assert!(is_tui("psql", &[]));
}

#[test]
fn psql_with_dash_c_false() {
    assert!(!is_tui("psql", &s(&["-c", "SELECT 1"])));
}

#[test]
fn mysql_with_dash_e_false() {
    assert!(!is_tui("mysql", &s(&["-e", "SHOW DATABASES"])));
}

#[test]
fn sqlite3_with_dbfile_false() {
    assert!(!is_tui("sqlite3", &s(&["mydb.db"])));
}

// ---------------------------------------------------------------------------
// Negative tests — non-TUI lookalikes
// ---------------------------------------------------------------------------

#[test]
fn ls_not_tui() {
    assert!(!is_tui("ls", &s(&["-la"])));
}

#[test]
fn git_status_not_tui() {
    assert!(!is_tui("git", &s(&["status"])));
}

#[test]
fn pnpm_run_dev_not_tui() {
    assert!(!is_tui("pnpm", &s(&["run", "dev"])));
}

#[test]
fn cargo_test_not_tui() {
    assert!(!is_tui("cargo", &s(&["test"])));
}

#[test]
fn npm_install_not_tui() {
    assert!(!is_tui("npm", &s(&["install"])));
}

#[test]
fn git_rebase_continue_not_tui() {
    assert!(!is_tui("git", &s(&["rebase", "--continue"])));
}

// ---------------------------------------------------------------------------
// Path-stripping
// ---------------------------------------------------------------------------

#[test]
fn vim_with_absolute_path_still_tui() {
    assert!(is_tui("/usr/bin/vim", &[]));
}
