//! REQ-cli-surface-cap pre-enforcement. Phase 4 owns the formal cap; this
//! test fails if anyone adds a 7th subcommand to `cli.rs`.

use assert_cmd::Command;

const ALLOWED_SUBCOMMANDS: &[&str] = &[
    "run", "validate", "init", "stats", "explain", "doctor",
];

#[test]
fn cli_surface_exposes_exactly_six_subcommands() {
    let output = Command::cargo_bin("lacon")
        .unwrap()
        .arg("--help")
        .output()
        .expect("run lacon --help");
    let stdout = String::from_utf8_lossy(&output.stdout);

    // clap's --help output lists subcommands at indented column. Count
    // occurrences of each allowed subcommand at line start (with leading
    // whitespace).
    let mut found = std::collections::HashSet::new();
    for line in stdout.lines() {
        let trimmed = line.trim_start();
        for name in ALLOWED_SUBCOMMANDS {
            if trimmed.starts_with(*name)
                && (trimmed.len() == name.len()
                    || trimmed.as_bytes()[name.len()].is_ascii_whitespace())
            {
                found.insert(*name);
            }
        }
    }
    assert_eq!(
        found.len(),
        6,
        "expected all 6 subcommands in --help output; found {:?}\nstdout:\n{}",
        found,
        stdout
    );
}

#[test]
fn unknown_subcommand_rejected_with_nonzero_exit() {
    Command::cargo_bin("lacon")
        .unwrap()
        .arg("flibbertigibbet")
        .assert()
        .failure();
}

#[test]
fn version_flag_works() {
    Command::cargo_bin("lacon")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicates::str::contains("0.1.0"));
}
