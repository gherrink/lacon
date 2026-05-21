//! lacon run runtime: subprocess spawn, stdout+stderr merge, dual-buffer
//! pipeline run with on_error swap on non-zero exit, signal forwarding,
//! LACON_DISABLE bypass.
//!
//! Per ADR-0013: this is the PRODUCTION HOT PATH. Cold-start budget is
//! load-bearing; PLAN-07 measures cumulative startup of clap + loader
//! + this Runner.
//!
//! Per CONTEXT.md D-13: success_buffer and raw_buffer are both held in
//! memory until exit code is known. Per D-08 the byte-exact truncation
//! marker is emitted by `Stage::MaxBytes` in the pipeline — the runtime
//! does NOT impose a separate pre-cap that would silently drop lines.
//! Per-line cap (`MAX_LINE_BYTES`) defends against single pathological
//! lines but does NOT cap total output (that's `Stage::MaxBytes`'s job).
//!
//! W3 fix (revision 1): the previous "raw_buffer pre-cap with silent drop"
//! pattern is removed. NO hardcoded pre-cap on raw_buffer at the runtime
//! level — Stage::MaxBytes (injected by PLAN-03 loader for every rule that
//! lacks one explicitly) is the sole truncation enforcement point per D-08.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
use std::thread;
use std::time::Instant;

use crossbeam_channel::unbounded;
use os_pipe::pipe;

use crate::error::RuntimeError;
use crate::rules::loader::ResolvedRule;
use crate::starlark_host::ScriptCtx;

/// Maximum bytes for a single line read from the subprocess pipe.
///
/// This is a PER-LINE cap as a DoS defense against pathologically long
/// single lines (T-05-04). It is NOT a total-output cap — `Stage::MaxBytes`
/// in the success/on_error pipeline is the sole total-output truncation
/// enforcement point (D-08). When a line is truncated here, the suffix
/// `[lacon: line truncated]` is appended to signal the truncation.
const MAX_LINE_BYTES: usize = 1 << 20; // 1 MiB per-line cap

/// Options for `Runner::run`.
#[derive(Debug, Clone, Default)]
pub struct RunOptions {
    /// If set, the subprocess is spawned with this as its working directory.
    pub project_path: Option<PathBuf>,
    /// Additional environment variables injected into the subprocess environment.
    /// Used primarily for testing; PLAN-06 does not populate it in production.
    pub extra_env: HashMap<String, String>,
}

/// Byte counts for a single `Runner::run` invocation.
#[derive(Debug, Clone, Default)]
pub struct ByteCounts {
    /// Raw bytes read from the merged stdout+stderr pipe.
    /// (In v1 the two streams share a single pipe, so this count covers both.)
    pub raw_stdout_bytes: usize,
    /// Separate stderr byte count; always 0 in v1 (merged single stream).
    pub raw_stderr_bytes: usize,
    /// Filtered bytes written to the sink.
    pub filtered_bytes: usize,
}

/// Outcome of a single `Runner::run` call.
#[derive(Debug)]
pub struct RunOutcome {
    /// Exit code from the subprocess. On signal kill: `128 + signal_number` (D-12).
    pub exit_code: i32,
    /// Byte counts for this invocation.
    pub byte_counts: ByteCounts,
    /// Set to `Some(signal_number)` if the subprocess was killed by a signal.
    pub signaled: Option<i32>,
    /// True when `LACON_DISABLE=1` was set and filtering was skipped.
    pub bypassed: bool,
    /// True when the `[lacon: truncated, ` marker is present in the filtered output
    /// (emitted by `Stage::MaxBytes` when the output cap was exceeded — D-08).
    pub truncated: bool,
    /// Wall-clock duration of the entire `run()` call in milliseconds.
    pub duration_ms: u64,
}

/// Tracker metadata assembled by `lacon-cli::commands::run` after `Runner::run`
/// returns and INSERTed into `invocations` by `tracking::Tracker::record`.
/// Phase 2 D-03: this struct is EXTENDED additively from Phase 1 — never redefine.
#[derive(Debug, Clone)]
pub struct InvocationMeta {
    /// Unix millisecond timestamp of invocation start.
    pub ts_unix_ms: u64,
    /// Rule ID that matched (None if bypass or no rule).
    pub rule_id: Option<String>,
    /// Layer the rule was found in.
    pub rule_source: Option<crate::rules::RuleSource>,
    /// Full raw command string (for display).
    pub command_raw: String,
    /// Argv split (program + args).
    pub argv: Vec<String>,
    /// Subprocess exit code (or 128+sig).
    pub exit_code: i32,
    /// Wall-clock duration in ms.
    pub duration_ms: u64,
    /// Byte counts for this invocation.
    pub byte_counts: ByteCounts,
    /// Whether LACON_DISABLE bypass was active.
    pub bypassed: bool,
    /// Whether the adapter rewrote the command (set by adapter; runtime defaults to false).
    pub rewritten: bool,
    /// Whether any `Stage::MaxBytes` stage emitted a truncation marker.
    pub truncated_by_max_bytes: bool,
    // ─── Phase 2 additions (D-03) ──────────────────────────────────────
    /// Assistant identifier. Populated from env `LACON_ASSISTANT` (default `"claude-code"`)
    /// at the CLI assembly site. Maps to `invocations.assistant TEXT NOT NULL`.
    pub assistant: String,
    /// Optional session id from `LACON_SESSION_ID` (unset → None → SQL NULL).
    /// Maps to `invocations.session_id TEXT` (nullable).
    pub session_id: Option<String>,
    /// Project root at invocation time (typically `std::env::current_dir().ok()`).
    /// Maps to `invocations.project_path TEXT` (nullable).
    pub project_path: Option<std::path::PathBuf>,
    /// Stable command-grouping key produced by `tracking::normalize::normalize(&argv)`.
    /// Maps to `invocations.command_normalized TEXT NOT NULL`.
    pub command_normalized: String,
    /// FK into `raw_outputs(id)`; populated only when `cfg.store_raw_outputs == true`
    /// AND the raw output was successfully inserted. Maps to
    /// `invocations.raw_output_id INTEGER REFERENCES raw_outputs(id) ON DELETE SET NULL`.
    pub raw_output_id: Option<i64>,
}

/// `lacon run` runtime.
///
/// Spawns the subprocess, merges stdout+stderr via `os_pipe`, runs the filter
/// pipeline on the output, and propagates the subprocess exit code.
///
/// # Subprocess argument injection mitigation (T-05-01)
/// `argv` is passed as `Command::new(&argv[0]).args(&argv[1..])` — Rust's
/// `std::process::Command` never re-shell-interprets arguments. Do NOT
/// concatenate argv elements into a shell string.
pub struct Runner {
    resolved: ResolvedRule,
    options: RunOptions,
}

impl Runner {
    /// Create a new `Runner` from a resolved rule and options.
    pub fn new(resolved: ResolvedRule, options: RunOptions) -> Self {
        Self { resolved, options }
    }

    /// Spawn `argv` as a subprocess, merge stdout+stderr, run the filter
    /// pipeline (or bypass if `LACON_DISABLE=1`), write filtered bytes to
    /// `sink`, and return the `RunOutcome`.
    ///
    /// # Errors
    /// Returns `RuntimeError::EmptyArgv` if `argv` is empty.
    /// Returns `RuntimeError::SpawnFailed` if the subprocess could not be spawned.
    /// Returns `RuntimeError::IoError` for I/O errors on the pipe or sink.
    /// Returns `RuntimeError` variants from `Pipeline::run_with_post_process`.
    pub fn run<W: Write>(
        &mut self,
        argv: &[String],
        sink: &mut W,
    ) -> Result<RunOutcome, RuntimeError> {
        if argv.is_empty() {
            return Err(RuntimeError::EmptyArgv);
        }

        let started = Instant::now();

        // LACON_DISABLE=1 bypass (REQ-engine-bypass).
        // Checked at entry; subprocess writes directly to inherited stdout/stderr.
        if std::env::var("LACON_DISABLE").as_deref() == Ok("1") {
            return self.run_bypassed(argv, sink, started);
        }

        // ─── os_pipe merge (D-09, T-05-02 Pitfall 1) ───────────────────────
        // Both stdout and stderr are connected to the same write-end of a pipe.
        // The CRITICAL pitfall: Command holds internal writer copies. We must
        // drop the Command value BEFORE reading from the pipe reader, or the
        // read-end never sees EOF and deadlocks forever.
        let (reader, writer) =
            pipe().map_err(|e| RuntimeError::IoError { source: e })?;
        let writer_clone =
            writer.try_clone().map_err(|e| RuntimeError::IoError { source: e })?;

        let mut cmd = std::process::Command::new(&argv[0]);
        cmd.args(&argv[1..])
            .stdout(writer)   // os_pipe::PipeWriter implements Into<Stdio>
            .stderr(writer_clone);
        if let Some(p) = &self.options.project_path {
            cmd.current_dir(p);
        }
        for (k, v) in &self.options.extra_env {
            cmd.env(k, v);
        }

        let mut child = cmd.spawn().map_err(|e| RuntimeError::SpawnFailed {
            program: argv[0].clone(),
            source: e,
        })?;

        // CRITICAL: drop the Command (and its internal writer copies) BEFORE
        // reading from the pipe reader — otherwise the read-end never sees EOF.
        // Pitfall 1 from RESEARCH.md; enforced by acceptance criterion grep.
        drop(cmd);

        // ─── Signal forwarder (D-12) ──────────────────────────────────────
        // Install a watcher thread that forwards SIGTERM/SIGINT to the child PID.
        // The stop_flag is set after child.wait() to allow the watcher to exit
        // cleanly without hanging on Signals::forever().
        //
        // WR-03: child.id() returns u32. The cast to i32 is safe on all supported
        // platforms (macOS and Linux): Linux's default PID_MAX_LIMIT is 4,194,304
        // (2^22), well within i32::MAX (2,147,483,647 = 2^31 - 1). Systems with
        // custom kernel.pid_max > 4194304 are not officially supported by v1.
        // If the cast would overflow (u32 value > i32::MAX), we log a warning and
        // skip signal forwarding rather than sending a signal to the wrong PID.
        let child_pid_u32 = child.id();
        let child_pid = match i32::try_from(child_pid_u32) {
            Ok(pid) => pid,
            Err(_) => {
                eprintln!(
                    "lacon: warning: subprocess PID {} exceeds i32::MAX; \
                     signal forwarding disabled for this process",
                    child_pid_u32
                );
                // Use -1 as a sentinel — install_signal_forwarder will not forward.
                -1
            }
        };
        let (signal_handle, signal_stop) = install_signal_forwarder(child_pid);

        // ─── Reader thread (D-10, Pitfall 2) ─────────────────────────────
        // Reads lines from the merged pipe via read_until(b'\n') — NOT the
        // `lines()` iterator method which panics on non-UTF8 input (Pitfall 2).
        // Per-line cap: MAX_LINE_BYTES (T-05-04 DoS defense; not a total-output cap).
        let (tx, rx) = unbounded::<String>();
        let raw_byte_counter = Arc::new(AtomicUsize::new(0));
        let raw_byte_counter_thread = raw_byte_counter.clone();

        let reader_handle = thread::spawn(move || -> std::io::Result<()> {
            let mut br = BufReader::new(reader);
            let mut buf = Vec::with_capacity(8192);
            loop {
                buf.clear();
                let n = br.read_until(b'\n', &mut buf)?;
                if n == 0 {
                    break; // EOF — all writers dropped (including child)
                }
                raw_byte_counter_thread.fetch_add(n, Ordering::Relaxed);

                // Per-line DoS defense: cap a SINGLE line at MAX_LINE_BYTES.
                // This is NOT a total-output cap (Stage::MaxBytes owns that).
                // T-05-04: defends against pathologically long single lines.
                let line_truncated = if buf.len() > MAX_LINE_BYTES {
                    buf.truncate(MAX_LINE_BYTES);
                    true
                } else {
                    false
                };

                // Convert bytes to String via from_utf8_lossy (Pitfall 2 mitigation).
                // Invalid UTF-8 bytes are replaced with U+FFFD rather than panicking.
                let mut s = String::from_utf8_lossy(&buf).to_string();
                if s.ends_with('\n') {
                    s.pop();
                }
                if line_truncated {
                    s.push_str(" [lacon: line truncated]");
                }

                if tx.send(s).is_err() {
                    break; // main dropped rx — abort reader
                }
            }
            Ok(())
        });

        // ─── Main: collect raw lines (D-13 dual-buffer model) ────────────
        //
        // Per D-13: accumulate ALL raw lines in raw_buffer until exit code is
        // known, then choose the success or on_error pipeline based on exit code.
        //
        // NO hardcoded pre-cap here (W3 fix, revision 1):
        //   - A hardcoded pre-cap would either (a) silently drop lines — D-08 violation:
        //     no truncation marker emitted — or (b) duplicate Stage::MaxBytes' contract.
        //   - Stage::MaxBytes in the pipeline is the SOLE total-output truncation point.
        //   - PLAN-03 ensures every rule has a MaxBytes stage (implicit injection at load).
        //   - Therefore raw_buffer is naturally bounded per CON-nfr-streaming-memory.
        let mut raw_buffer: Vec<String> = Vec::new();
        for line in rx.iter() {
            raw_buffer.push(line);
        }

        // Wait for reader to finish (ensures all bytes are counted).
        let _ = reader_handle.join();

        // Reap the subprocess.
        let status = child
            .wait()
            .map_err(|e| RuntimeError::IoError { source: e })?;

        // Signal forwarder no longer needed — tell it to exit.
        signal_stop.store(true, Ordering::Relaxed);
        let _ = signal_handle.join();

        // Compute exit code (D-12). On signal kill: 128 + signal_number.
        let (exit_code, signaled) = if let Some(code) = status.code() {
            (code, None)
        } else {
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                let sig = status.signal().unwrap_or(0);
                (128 + sig, Some(sig))
            }
            #[cfg(not(unix))]
            {
                (1, None)
            }
        };

        // Build ScriptCtx for Starlark post_process (PLAN-04 interface).
        let ctx = ScriptCtx {
            exit_code,
            duration_ms: started.elapsed().as_millis() as u64,
            command: argv[0].clone(),
            args: argv[1..].to_vec(),
            project_path: self.options.project_path.as_ref().map(|p| p.display().to_string()),
        };

        // ─── Exit code branch (D-13, ADR-0010) ───────────────────────────
        //
        // exit_code == 0: run raw lines through success_pipeline.
        // exit_code != 0: DISCARD success path; run raw lines through on_error
        //   pipeline if present, otherwise emit raw output unchanged.
        //
        // ADR-0010: on_error REPLACES the success pipeline — never merges.
        let filtered = if exit_code == 0 {
            self.resolved.success_pipeline.run_with_post_process(
                raw_buffer.into_iter(),
                self.resolved.post_process.as_ref(),
                &ctx,
            )?
        } else if let Some(ref mut on_err) = self.resolved.on_error_pipeline {
            on_err.run_with_post_process(
                raw_buffer.into_iter(),
                self.resolved.on_error_post_process.as_ref(),
                &ctx,
            )?
        } else {
            // No on_error block: per ADR-0010 the spec is silent on this case.
            // Conservative choice: emit raw output unchanged. If a rule author
            // wants filtering on errors, they declare on_error.
            raw_buffer
        };

        // Write filtered output to sink.
        let joined = filtered.join("\n");

        // Detect truncation by scanning for the byte-exact marker (D-08).
        let truncated = joined.contains("[lacon: truncated, ");
        let bytes_written = joined.len();

        sink.write_all(joined.as_bytes())
            .map_err(|e| RuntimeError::IoError { source: e })?;
        if !joined.is_empty() {
            // Add trailing newline after the last line.
            sink.write_all(b"\n")
                .map_err(|e| RuntimeError::IoError { source: e })?;
        }

        let byte_counts = ByteCounts {
            raw_stdout_bytes: raw_byte_counter.load(Ordering::Relaxed),
            raw_stderr_bytes: 0, // merged single stream in v1
            filtered_bytes: bytes_written,
        };

        Ok(RunOutcome {
            exit_code,
            byte_counts,
            signaled,
            bypassed: false,
            truncated,
            duration_ms: started.elapsed().as_millis() as u64,
        })
    }

    /// Re-derive filtered output from STORED stdout/stderr bytes WITHOUT
    /// spawning a subprocess (D-04). Unlike [`Runner::run`] — which always
    /// spawns the original command — `filter_bytes` feeds previously-captured
    /// bytes through the rule's pipeline. The `explain` command (Wave 2) uses
    /// this to reproduce what the live runner emitted from stored `raw_outputs`.
    ///
    /// # Exit-code branch source of truth
    /// The success / on_error / raw-passthrough branch MUST mirror the live
    /// runner at `runtime/mod.rs:342-359` (ADR-0010) exactly:
    ///   - `exit_code == 0`            -> `success_pipeline` (+ `post_process`)
    ///   - `exit_code != 0` + on_error -> `on_error_pipeline` (+ `on_error_post_process`)
    ///   - `exit_code != 0` + none     -> raw lines unchanged (ADR-0010 passthrough)
    ///
    /// Phase 6 reproducibility (SC3) depends on this branch staying byte-for-byte
    /// identical to the live runner. The branch-fidelity tests in
    /// `tests/runtime_filter_bytes.rs` lock all three cases so a future edit that
    /// desyncs one path from the other fails CI (T-04-04).
    ///
    /// # ScriptCtx provenance
    /// `command`/`args` are reconstructed from the STORED `command_raw` (a
    /// whitespace split), NOT a live process. `exit_code`, `duration_ms`, and
    /// `project_path` are likewise the STORED values supplied by the caller.
    /// An empty `command_raw` yields an empty command + empty args.
    ///
    /// # Stream model
    /// v1 stores a single merged stdout+stderr stream (see [`ByteCounts`]).
    /// `merged_bytes` is split on `b'\n'` and lossily decoded as UTF-8, mirroring
    /// the live reader's `String::from_utf8_lossy` approach (runtime/mod.rs:265-270).
    ///
    /// # Errors
    /// Returns `RuntimeError` variants from `Pipeline::run_with_post_process`.
    pub fn filter_bytes(
        &mut self,
        merged_bytes: &[u8],
        exit_code: i32,
        duration_ms: u64,
        command_raw: &str,
        project_path: Option<String>,
    ) -> Result<Vec<String>, RuntimeError> {
        // ScriptCtx command/args from the STORED command_raw (not a live process).
        // Whitespace split is acceptable for ctx population per D-04: the ctx is
        // a best-effort reconstruction for Starlark, not a re-execution argv.
        let mut parts = command_raw.split_whitespace();
        let command = parts.next().unwrap_or("").to_string();
        let args: Vec<String> = parts.map(|s| s.to_string()).collect();

        // Split merged bytes into lines (mirror runtime/mod.rs:265-270 lossy UTF-8;
        // v1 is a single merged stream per ByteCounts at :60-66).
        let lines: Vec<String> = merged_bytes
            .split(|&b| b == b'\n')
            .map(|l| String::from_utf8_lossy(l).into_owned())
            .collect();

        // ScriptCtx from STORED values (mirror :327-333, sourced from args).
        let ctx = ScriptCtx {
            exit_code,
            duration_ms,
            command,
            args,
            project_path,
        };

        // ─── Exit code branch (mirrors runtime/mod.rs:342-359, ADR-0010) ──────
        // exit_code == 0: success_pipeline. exit_code != 0: on_error_pipeline if
        // present, else raw passthrough. NEVER calls Runner::run (no spawn).
        let filtered = if exit_code == 0 {
            self.resolved.success_pipeline.run_with_post_process(
                lines.into_iter(),
                self.resolved.post_process.as_ref(),
                &ctx,
            )?
        } else if let Some(ref mut on_err) = self.resolved.on_error_pipeline {
            on_err.run_with_post_process(
                lines.into_iter(),
                self.resolved.on_error_post_process.as_ref(),
                &ctx,
            )?
        } else {
            // No on_error block: ADR-0010 raw passthrough — replay MUST preserve
            // this so a non-zero exit with no on_error returns lines unchanged.
            lines
        };

        Ok(filtered)
    }

    /// Run bypassed: subprocess inherits stdout/stderr; no filtering.
    ///
    /// Called when `LACON_DISABLE=1` is set in the environment.
    /// Per CON-chained-bypass-whole-command: this is whole-command bypass.
    /// T-05-07: documented intentional user-controlled escape hatch.
    fn run_bypassed<W: Write>(
        &self,
        argv: &[String],
        _sink: &mut W,
        started: Instant,
    ) -> Result<RunOutcome, RuntimeError> {
        let mut child = std::process::Command::new(&argv[0])
            .args(&argv[1..])
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .spawn()
            .map_err(|e| RuntimeError::SpawnFailed {
                program: argv[0].clone(),
                source: e,
            })?;
        let status = child
            .wait()
            .map_err(|e| RuntimeError::IoError { source: e })?;
        let (exit_code, signaled) = match status.code() {
            Some(c) => (c, None),
            None => {
                #[cfg(unix)]
                {
                    use std::os::unix::process::ExitStatusExt;
                    let sig = status.signal().unwrap_or(0);
                    (128 + sig, Some(sig))
                }
                #[cfg(not(unix))]
                {
                    (1, None)
                }
            }
        };
        Ok(RunOutcome {
            exit_code,
            byte_counts: ByteCounts::default(),
            signaled,
            bypassed: true,
            truncated: false,
            duration_ms: started.elapsed().as_millis() as u64,
        })
    }
}

// ─── Signal forwarding (D-12) ─────────────────────────────────────────────────
//
// On unix targets: a watcher thread polls SIGTERM and SIGINT via signal-hook's
// `Signals::pending()` and forwards each signal to the subprocess PID via
// `nix::sys::signal::kill`. The thread polls in a ~50ms loop and exits when
// `stop_flag` is set (after the subprocess has been reaped).
//
// T-05-09 race: if a signal arrives between child.wait() returning and
// stop_flag.store(true), kill() returns ESRCH (process no longer exists).
// This is benign — handled by `let _ = kill(...)`.

#[cfg(unix)]
fn install_signal_forwarder(child_pid: i32) -> (thread::JoinHandle<()>, Arc<AtomicBool>) {
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;
    use signal_hook::consts::{SIGINT, SIGTERM};
    use signal_hook::iterator::Signals;

    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag_thread = stop_flag.clone();

    let mut signals =
        Signals::new([SIGTERM, SIGINT]).expect("signal-hook Signals::new failed");

    let handle = thread::spawn(move || {
        loop {
            if stop_flag_thread.load(Ordering::Relaxed) {
                break;
            }
            // Poll without blocking — pending() returns immediately.
            // WR-03: child_pid == -1 is the sentinel set when the PID overflowed
            // i32::MAX at spawn time. In that case, skip signal forwarding entirely
            // to avoid kill(-1, sig) which would broadcast to all processes.
            if child_pid > 0 {
                for sig in signals.pending() {
                    let s = match sig {
                        SIGTERM => Signal::SIGTERM,
                        SIGINT => Signal::SIGINT,
                        _ => continue,
                    };
                    // T-05-09: kill() may return ESRCH if child already exited — benign.
                    let _ = kill(Pid::from_raw(child_pid), s);
                }
            }
            thread::sleep(std::time::Duration::from_millis(50));
        }
    });

    (handle, stop_flag)
}

/// No-op stub on non-unix targets (Windows out of v1 scope per CON-nfr-platform-support).
#[cfg(not(unix))]
fn install_signal_forwarder(_child_pid: i32) -> (thread::JoinHandle<()>, Arc<AtomicBool>) {
    let stop = Arc::new(AtomicBool::new(false));
    let h = thread::spawn(|| {});
    (h, stop)
}
