//! lacon run subcommand: spawn a subprocess via the rule's pipeline,
//! propagate the subprocess's exit code.
//!
//! Production usage (per ADR-0013): `lacon run --rule <id> -- <cmd> [args]`,
//! emitted by the Claude Code adapter's PreToolUse rewrite (Phase 3).
//! Manual-test usage: `lacon run -- <cmd>` (no --rule) runs the eager
//! resolver against the inner argv. Per D-14.

use std::io::Write;
use std::path::PathBuf;

use lacon_core::error::{RuntimeError, ValidationError};
use lacon_core::rules::loader::{RuleLoader, ResolvedRule};
use lacon_core::runtime::{Runner, RunOptions};

pub fn execute(rule: Option<String>, argv: Vec<String>) -> anyhow::Result<i32> {
    if argv.is_empty() {
        eprintln!("lacon run: no command provided after `--`");
        return Ok(2);
    }

    let project_path = std::env::current_dir().ok();
    let mut loader = RuleLoader::new(project_path.clone());

    let resolved: Option<ResolvedRule> = match rule {
        Some(rule_id) => match loader.resolve(&rule_id) {
            Ok(r) => Some(r),
            Err(e) => {
                eprintln!("{}", e);
                return Ok(1);
            }
        },
        None => match try_match_via_load_all(&mut loader, &argv) {
            Ok(opt) => opt,
            Err(errors) => {
                for e in errors {
                    eprintln!("{}", e);
                }
                return Ok(1);
            }
        },
    };

    let stdout = std::io::stdout();
    let mut sink = stdout.lock();

    match resolved {
        Some(r) => run_with_rule(r, argv, project_path, &mut sink),
        None => run_unmatched(argv, &mut sink),
    }
}

fn try_match_via_load_all(
    loader: &mut RuleLoader,
    argv: &[String],
) -> Result<Option<ResolvedRule>, Vec<ValidationError>> {
    let candidates = loader.load_all()?;
    let prog_basename = argv[0].rsplit('/').next().unwrap_or(&argv[0]).to_owned();
    for r in candidates {
        match rule_matches_argv(&r, &prog_basename, &argv[1..]) {
            Ok(true) => return Ok(Some(r)),
            Ok(false) => continue,
            Err(e) => return Err(vec![e]),
        }
    }
    Ok(None)
}

/// Returns `Ok(true)` if the rule matches `(prog_basename, args)`.
///
/// WR-02 fix: `command_regex` is now compiled here with an explicit error path.
/// In practice, `load_all()` already validates regexes via `compile_resolved`, so
/// a compile failure here indicates a bug rather than a user error. The error is
/// propagated rather than silently treated as a non-match, which would hide it.
fn rule_matches_argv(
    r: &ResolvedRule,
    prog_basename: &str,
    args: &[String],
) -> Result<bool, ValidationError> {
    use lacon_core::rules::schema::MatchSpec;
    use std::path::PathBuf;

    fn spec_matches(
        spec: &MatchSpec,
        prog: &str,
        args: &[String],
        rule_id: &str,
    ) -> Result<bool, ValidationError> {
        if let Some(any) = &spec.any {
            for s in any {
                if spec_matches(s, prog, args, rule_id)? {
                    return Ok(true);
                }
            }
            return Ok(false);
        }
        if let Some(all) = &spec.all {
            for s in all {
                if !spec_matches(s, prog, args, rule_id)? {
                    return Ok(false);
                }
            }
            return Ok(true);
        }
        if let Some(cmd) = &spec.command {
            if cmd != prog {
                return Ok(false);
            }
        }
        if let Some(prefix) = &spec.args_prefix {
            if args.len() < prefix.len() {
                return Ok(false);
            }
            for (i, p) in prefix.iter().enumerate() {
                if &args[i] != p {
                    return Ok(false);
                }
            }
        }
        if let Some(contain) = &spec.args_contain {
            if !contain.iter().all(|c| args.iter().any(|a| a == c)) {
                return Ok(false);
            }
        }
        if let Some(re_str) = &spec.command_regex {
            let mut joined = prog.to_owned();
            for a in args {
                joined.push(' ');
                joined.push_str(a);
            }
            // WR-02: propagate compile errors instead of silently treating them
            // as a non-match. If load_all() validated this regex at load time,
            // this Err branch should be unreachable in practice.
            let re = regex::Regex::new(re_str).map_err(|e| ValidationError::InvalidRegex {
                path: PathBuf::from(format!("<rule:{rule_id}>")),
                line: 0,
                message: format!("command_regex compile error: {e}"),
            })?;
            if !re.is_match(&joined) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    match &r.rule.match_spec {
        Some(spec) => spec_matches(spec, prog_basename, args, &r.id),
        None => Ok(false),
    }
}

fn run_with_rule<W: Write>(
    resolved: ResolvedRule,
    argv: Vec<String>,
    project_path: Option<PathBuf>,
    sink: &mut W,
) -> anyhow::Result<i32> {
    let options = RunOptions {
        project_path,
        extra_env: Default::default(),
    };
    let mut runner = Runner::new(resolved, options);
    match runner.run(&argv, sink) {
        Ok(outcome) => Ok(outcome.exit_code),
        Err(RuntimeError::SpawnFailed { program, source }) => {
            eprintln!("lacon run: failed to spawn `{}`: {}", program, source);
            Ok(127) // POSIX "command not found" convention
        }
        Err(e) => {
            eprintln!("lacon run: {}", e);
            Ok(1)
        }
    }
}

fn run_unmatched<W: Write>(argv: Vec<String>, _sink: &mut W) -> anyhow::Result<i32> {
    use std::process::{Command, Stdio};
    let status = Command::new(&argv[0])
        .args(&argv[1..])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status();
    match status {
        Ok(s) => Ok(s.code().unwrap_or_else(|| {
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                128 + s.signal().unwrap_or(0)
            }
            #[cfg(not(unix))]
            {
                1
            }
        })),
        Err(e) => {
            eprintln!("lacon run: failed to spawn `{}`: {}", argv[0], e);
            Ok(127)
        }
    }
}
