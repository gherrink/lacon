//! lacon run subcommand: spawn a subprocess via the rule's pipeline,
//! propagate the subprocess's exit code.
//!
//! Production usage (per ADR-0013): `lacon run --rule <id> -- <cmd> [args]`,
//! emitted by the Claude Code adapter's PreToolUse rewrite (Phase 3).
//! Manual-test usage: `lacon run -- <cmd>` (no --rule) runs the eager
//! resolver against the inner argv. Per D-14.

use std::io::Write;
use std::path::PathBuf;

use lacon_core::config::{self, Config};
use lacon_core::error::{RuntimeError, TrackingError};
use lacon_core::rules::loader::{ResolvedRule, RuleLoader, RuleSource};
use lacon_core::runtime::{ByteCounts, InvocationMeta, RunOptions, RunOutcome, Runner};
use lacon_core::tracking::{self, RawOutput};
use std::time::{SystemTime, UNIX_EPOCH};

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
        None => match lacon_core::rules::match_argv_via_load_all(&mut loader, &argv) {
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

fn run_with_rule<W: Write>(
    resolved: ResolvedRule,
    argv: Vec<String>,
    project_path: Option<PathBuf>,
    sink: &mut W,
) -> anyhow::Result<i32> {
    // ─── Issue #2 fix: capture rule fields BEFORE Runner::new moves `resolved` ───
    // RuleSource is Clone but NOT Copy (verified at crates/lacon-core/src/rules/loader.rs:50).
    // Cloning here is cheap (RuleSource is a 3-variant enum with no payload).
    let rule_id = resolved.id.clone();
    let rule_source = Some(resolved.source.clone());
    let project_path_for_tracker = project_path.clone();
    // ────────────────────────────────────────────────────────────────────────────

    // ─── Phase 7 (D-02/D-03/D-06): gate raw capture on the resolved opt-in ───
    // Resolve `store_raw_outputs` for this project BEFORE the run so the runner
    // serializes raw bytes ONLY when the user opted in. `record_invocation`
    // re-resolves the SAME value via `resolve_store_raw_outputs` for the
    // double-gate, so the capture flag and the persist gate never diverge.
    // Config awareness stays in run.rs — the core runner remains config-unaware
    // (D-07).
    let capture_raw = resolve_store_raw_outputs(project_path.as_deref());

    let options = RunOptions {
        project_path,
        extra_env: Default::default(),
        capture_raw,
    };
    let mut runner = Runner::new(resolved, options);
    match runner.run(&argv, sink) {
        Ok(outcome) => {
            // Phase 2 best-effort tracker write (D-02 + D-12). Filtered bytes
            // already on stdout; any tracker error is logged and swallowed,
            // never altering the wrapper's exit code.
            let exit_code = outcome.exit_code;
            record_invocation(
                Some(rule_id),
                rule_source,
                argv,
                project_path_for_tracker,
                outcome,
            );
            Ok(exit_code)
        }
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
    let started = SystemTime::now();
    let status = Command::new(&argv[0])
        .args(&argv[1..])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status();
    let duration_ms = started.elapsed().map(|d| d.as_millis() as u64).unwrap_or(0);
    match status {
        Ok(s) => {
            let exit_code = s.code().unwrap_or_else(|| {
                #[cfg(unix)]
                {
                    use std::os::unix::process::ExitStatusExt;
                    128 + s.signal().unwrap_or(0)
                }
                #[cfg(not(unix))]
                {
                    1
                }
            });
            let outcome = RunOutcome {
                exit_code,
                byte_counts: ByteCounts::default(),
                signaled: None,
                bypassed: false,
                truncated: false,
                duration_ms,
                raw_captured: None, // unmatched runs capture nothing (D-01)
            };
            let project_path = std::env::current_dir().ok();
            record_invocation(None, None, argv, project_path, outcome);
            Ok(exit_code)
        }
        Err(e) => {
            eprintln!("lacon run: failed to spawn `{}`: {}", argv[0], e);
            Ok(127)
        }
    }
}

/// Resolve the user's config directory under XDG (honours `XDG_CONFIG_HOME`,
/// set by the e2e tests via assert_cmd's `.env(...)`). Returned as
/// `<config_dir>/lacon`. `None` only when no base strategy resolves (no
/// platform support — v1 covers Linux + macOS).
fn user_config_dir() -> Option<PathBuf> {
    use etcetera::BaseStrategy;
    etcetera::choose_base_strategy()
        .ok()
        .map(|s| s.config_dir().join("lacon"))
}

/// Resolve the existing project/user `config.yaml` paths for `project_path`.
/// Each is `Some` only when the file actually exists on disk (the `Path::exists`
/// probes also gate the WR-05 fast-path below).
fn config_paths(project_path: Option<&std::path::Path>) -> (Option<PathBuf>, Option<PathBuf>) {
    let project_config_path: Option<PathBuf> = project_path
        .map(|p| p.join(".lacon").join("config.yaml"))
        .filter(|p| p.exists());
    let user_config_path: Option<PathBuf> = user_config_dir()
        .map(|d| d.join("config.yaml"))
        .filter(|p| p.exists());
    (project_config_path, user_config_path)
}

/// Load the layered config for `project_path`, applying the WR-05 cold-start
/// fast-path: when neither a project nor a user `config.yaml` exists — the
/// common case on the hook hot path — skip the YAML parse entirely and use
/// `Config::default()`. Any load/validation error degrades to defaults
/// (best-effort posture, D-12). The same resolution is used both to set the
/// capture flag (`run_with_rule`) and to gate the persist (`record_invocation`)
/// so the two never diverge.
fn load_cfg(project_path: Option<&std::path::Path>) -> Config {
    let (project_config_path, user_config_path) = config_paths(project_path);
    if project_config_path.is_none() && user_config_path.is_none() {
        Config::default()
    } else {
        config::load_layered(project_config_path.as_deref(), user_config_path.as_deref())
            .unwrap_or_else(|_| Config::default())
    }
}

/// Resolve the project's effective `store_raw_outputs` opt-in decision (D-06).
/// `run_with_rule` calls this BEFORE the run to set `RunOptions.capture_raw`;
/// `record_invocation` re-derives the SAME value (via `load_cfg`) for the
/// double-gate, so capture and persist always agree.
fn resolve_store_raw_outputs(project_path: Option<&std::path::Path>) -> bool {
    load_cfg(project_path).store_raw_outputs
}

/// Best-effort tracker write (D-12). Errors are logged with the literal
/// `lacon: tracker` prefix and swallowed; exit code is never altered here.
///
/// Per CONTEXT D-02: filtered bytes are already on stdout by the time this
/// function is called.
/// Per CONTEXT D-17: env-var contract for assistant + session_id.
/// Per revision iteration 1, Issue #9: loads `EngineConfig::load_layered` so
/// SC2 (privacy warning trigger via flipping project config) is reachable
/// end-to-end via the CLI.
fn record_invocation(
    rule_id: Option<String>,
    rule_source: Option<RuleSource>,
    argv: Vec<String>,
    project_path: Option<PathBuf>,
    mut outcome: RunOutcome,
) {
    // Assemble InvocationMeta. Failures here can only come from system-time
    // anomalies; map to TrackingError::Clock and short-circuit silently.
    let now_ms = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_millis() as u64,
        Err(_) => {
            eprintln!("lacon: tracker skipped: system time before unix epoch");
            return;
        }
    };

    let assistant = std::env::var("LACON_ASSISTANT").unwrap_or_else(|_| "claude-code".to_owned());
    let session_id = std::env::var("LACON_SESSION_ID").ok();
    let command_raw = argv.join(" ");
    let command_normalized = tracking::normalize(&argv);

    // Phase 7 (D-06): move the captured raw bytes out of `outcome` before its
    // other fields are consumed by `meta` below. `Some` only when the runner
    // captured this run (capture_raw was true ⇒ store_raw_outputs opt-in);
    // `None` for default-off, bypass, and unmatched runs (set in Task 1).
    let raw_captured = outcome.raw_captured.take();

    // ─── Issue #9 fix: load layered config so SC2 is reachable via CLI ───
    // The same resolution feeds `run_with_rule`'s capture flag — both go through
    // `load_cfg` / `config_paths` so the capture decision and the persist gate
    // read identical values (D-06/D-07). `user_config_dir` is reused below for
    // the privacy marker path resolution (D-14); `project_config_path` is reused
    // for the project-vs-user layer split. WR-05 cold-start fast-path lives in
    // `load_cfg`.
    let user_config_dir: Option<PathBuf> = user_config_dir();
    let (project_config_path, _user_config_path) = config_paths(project_path.as_deref());
    let cfg: Config = load_cfg(project_path.as_deref());
    // ──────────────────────────────────────────────────────────────────────

    let meta = InvocationMeta {
        ts_unix_ms: now_ms,
        rule_id,
        rule_source,
        command_raw,
        argv,
        exit_code: outcome.exit_code,
        duration_ms: outcome.duration_ms,
        byte_counts: outcome.byte_counts,
        bypassed: outcome.bypassed,
        rewritten: false, // Adapter rewrites land in Phase 3; runtime defaults to false.
        truncated_by_max_bytes: outcome.truncated,
        assistant,
        session_id,
        project_path: project_path.clone(),
        command_normalized,
        raw_output_id: None, // Set by Tracker::record on the raw-insert branch.
    };

    // Resolve DB path — fall back silently if etcetera fails (no platform
    // support is the only realistic case; v1 covers Linux + macOS).
    let db_path = match tracking::Tracker::xdg_db_path() {
        Some(p) => p,
        None => {
            eprintln!("lacon: tracker skipped: could not resolve XDG data dir");
            return;
        }
    };

    let tracker =
        match tracking::Tracker::open(&db_path, &cfg.retention, cfg.store_raw_outputs, now_ms) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("lacon: tracker open failed: {e}");
                return;
            }
        };

    // Per-layer split for resolve_marker_path: Phase 1's load_layered collapses
    // project + user into a single bool, so we can't perfectly distinguish.
    // For v1, when cfg.store_raw_outputs is true AND a project config file
    // exists, treat it as project-layer ON; otherwise user-layer ON.
    // privacy::resolve_marker_path then routes to the right layer's marker path.
    let project_store_raw = cfg.store_raw_outputs && project_config_path.is_some();
    let user_store_raw = cfg.store_raw_outputs && !project_store_raw;

    let project_root = project_path.as_deref();
    let user_dir_ref = user_config_dir.as_deref();

    // ─── Phase 7 (D-04/D-06/D-07): construct RawOutput and pass Some(&raw) ───
    // When the runner captured raw bytes for this run, build a `RawOutput` with
    // the entire merged stream in the `stdout` column and an EMPTY `stderr`
    // column (D-04: v1 has a single interleaved stream by the time raw bytes
    // exist — there is no separable stderr). Passing `Some` is ALWAYS safe: the
    // defensive double-gate in `Tracker::record` only fires the raw_outputs
    // INSERT when `(cfg_store_raw_outputs, Some) == (true, Some)` (D-07), and
    // `capture_raw` was itself set from the resolved `store_raw_outputs`. When
    // capture was off (default), or this was a bypass/unmatched run,
    // `raw_captured` is `None` and we pass `None` exactly as before.
    let raw_output = raw_captured.map(|stdout| RawOutput {
        stdout,
        stderr: Vec::new(),
    });

    if let Err(e) = tracker.record(
        &meta,
        raw_output.as_ref(),
        project_root,
        user_dir_ref,
        project_store_raw,
        user_store_raw,
    ) {
        // Distinguish "could not record" from "open failed" for stderr clarity.
        match e {
            TrackingError::Marker { .. } => {
                eprintln!("lacon: tracker privacy marker write failed: {e}");
            }
            _ => {
                eprintln!("lacon: tracker write failed: {e}");
            }
        }
    }
}
