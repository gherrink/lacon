// CLAP-COST-FINDING (PLAN-06): `lacon --version` measured at ~1ms median (5 runs, release binary).
// Well within the 10ms cold-start budget for `lacon run`.
// Plan-B trigger: if cumulative startup approaches the 10ms cold-start budget,
// replace clap derive with pico-args (per CONTEXT.md benchmark item 2).
// PLAN-07 owns the formal benchmark.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "lacon", version, about = "AI-assistant bash output filter")]
pub struct Cli {
    #[command(subcommand)]
    pub command: CliCommand,
}

#[derive(Subcommand, Debug)]
pub enum CliCommand {
    /// Spawn a subprocess, filter its output through a rule, propagate exit code.
    Run {
        /// Rule ID to apply (if omitted, eager-resolves against the inner command).
        #[arg(long, value_name = "ID")]
        rule: Option<String>,
        /// The inner command and args (everything after `--`).
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, num_args = 1..)]
        argv: Vec<String>,
    },
    /// Lint a rule file or config file without running it.
    Validate {
        /// Path to .yaml file (rule or config; dispatched by content).
        path: PathBuf,
    },
    /// Set up lacon at a chosen scope: rules skeleton, Claude Code hook, and a
    /// `LACON.md` + `@import` reference in CLAUDE.md.
    ///
    /// `--project` installs into the current directory; `--user` installs
    /// globally under `~/.claude` + `~/.config/lacon/rules`. Both may be passed
    /// to install both scopes. With neither flag, an interactive prompt picks the
    /// scope on a TTY; non-interactively, project scope is the default.
    Init {
        /// Install into the user (home-relative, global) scope.
        #[arg(long)]
        user: bool,
        /// Install into the project (cwd-relative) scope.
        #[arg(long)]
        project: bool,
    },
    /// Show top offenders, bypass rates, unmatched commands.
    Stats {
        /// Filter to one project. Normalized to an absolute path (`.`, relative
        /// paths, and trailing slashes are accepted) and matched verbatim
        /// against the stored project path; symlinks are NOT resolved.
        #[arg(long)]
        project: Option<PathBuf>,
        #[arg(long)]
        since: Option<String>,
        #[arg(long)]
        rule: Option<String>,
        /// Print exact integer byte counts instead of humanized values (scripting).
        #[arg(long)]
        bytes: bool,
        /// Print every row uncapped (suppresses the '… N more' lines).
        #[arg(long)]
        all: bool,
    },
    /// Re-run filtering against stored raw output for a tracked invocation.
    Explain {
        id: String,
    },
    /// Verify lacon setup at BOTH project and user scope (hook + LACON.md +
    /// `@import` reference per scope), plus configs/rules/DB health.
    Doctor,
}
