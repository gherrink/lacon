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
use lacon_core::tracking;
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

    let options = RunOptions {
        project_path,
        extra_env: Default::default(),
        // Capture is wired to the resolved store_raw_outputs decision in Task 2;
        // default-off here keeps the field present so the workspace compiles.
        capture_raw: false,
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
    outcome: RunOutcome,
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

    // ─── Issue #9 fix: load layered config so SC2 is reachable via CLI ───
    // Resolve config paths under XDG. choose_base_strategy honours XDG_CONFIG_HOME
    // (set by Plan 06's e2e tests via assert_cmd's .env(...)) — so test isolation
    // doesn't pollute the real user config. user_config_dir is also reused below
    // for the privacy marker path resolution (D-14).
    let user_config_dir: Option<PathBuf> = {
        use etcetera::BaseStrategy;
        etcetera::choose_base_strategy()
            .ok()
            .map(|s| s.config_dir().join("lacon"))
    };
    let project_config_path: Option<PathBuf> = project_path
        .as_ref()
        .map(|p| p.join(".lacon").join("config.yaml"))
        .filter(|p| p.exists());
    let user_config_path: Option<PathBuf> = user_config_dir
        .as_ref()
        .map(|d| d.join("config.yaml"))
        .filter(|p| p.exists());
    // WR-05: `lacon run` is on the hook hot path (spawned for every matched
    // segment, thousands of times per session) and ADR-0013 budgets ≤10ms cold
    // start. When neither a project nor a user config file exists — the common
    // case — skip the YAML parse entirely and use defaults; `load_layered(None,
    // None)` would otherwise do avoidable startup work. The `Path::exists()`
    // probes above already gate this cheaply.
    //
    // load_layered returns Vec<ValidationError> on failure; treat any error as
    // "config invalid; fall back to defaults" — best-effort posture (D-12).
    // Validation errors are emitted earlier by `lacon validate`; here we don't
    // re-surface them to keep stderr quiet on the run path.
    let cfg: Config = if project_config_path.is_none() && user_config_path.is_none() {
        Config::default()
    } else {
        config::load_layered(project_config_path.as_deref(), user_config_path.as_deref())
            .unwrap_or_else(|_| Config::default())
    };
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

    if let Err(e) = tracker.record(
        &meta,
        None, // v1 default: no raw output bytes captured (raw_outputs is opt-in)
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
