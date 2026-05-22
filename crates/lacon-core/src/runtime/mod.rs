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
    /// When `true`, `Runner::run` serializes the buffered raw (pre-filter) lines
    /// into `RunOutcome.raw_captured` so the caller can persist them to the
    /// `raw_outputs` table (Phase 7, D-02). Defaults to `false` via
    /// `#[derive(Default)]`, so every existing call site stays capture-OFF and
    /// the cold-start hot path pays ZERO extra cost (D-03): the join-to-bytes
    /// serialization runs ONLY when this flag is set.
    pub capture_raw: bool,
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
    /// Captured raw (pre-filter) bytes, present ONLY when `RunOptions.capture_raw`
    /// was `true` for this run (Phase 7, D-01). The canonical capture form is
    /// `raw_buffer.join("\n")` with NO re-added trailing newline (D-05) — the
    /// exact inverse of the per-line build (lossy decode + strip one trailing
    /// `\n`), so `Runner::filter_bytes`' re-split regenerates the identical
    /// `Vec<String>` the live pipeline consumed (byte-exact `lacon explain`
    /// reproduction). `None` on the default-off hot path, on bypass, and for
    /// unmatched runs.
    pub raw_captured: Option<Vec<u8>>,
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
        let (reader, writer) = pipe().map_err(|e| RuntimeError::IoError { source: e })?;
        let writer_clone = writer
            .try_clone()
            .map_err(|e| RuntimeError::IoError { source: e })?;

        let mut cmd = std::process::Command::new(&argv[0]);
        cmd.args(&argv[1..])
            .stdout(writer) // os_pipe::PipeWriter implements Into<Stdio>
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
            project_path: self
                .options
                .project_path
                .as_ref()
                .map(|p| p.display().to_string()),
        };

        // ─── Gated raw capture (Phase 7, D-01/D-03/D-05) ──────────────────
        //
        // Serialize the buffered raw lines into bytes ONLY when capture was
        // requested. This MUST happen BEFORE `raw_buffer` is moved into the
        // pipeline at the exit-code branch below (`.into_iter()` / move-out).
        //
        // The default-off hot path (capture_raw == false) pays ZERO extra cost
        // (D-03): the join-to-bytes is computed strictly inside the `true` arm,
        // and `raw_buffer` is consumed by the pipeline exactly as before.
        //
        // The capture form is `raw_buffer.join("\n")` with NO trailing newline
        // re-added (D-05) — the exact inverse of the per-line build at the
        // reader (lossy decode + strip one trailing `\n`), so
        // `Runner::filter_bytes`' re-split regenerates the identical lines for
        // byte-exact `lacon explain` reproduction.
        let raw_captured: Option<Vec<u8>> = if self.options.capture_raw {
            Some(raw_buffer.join("\n").into_bytes())
        } else {
            None
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
            raw_captured,
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
        // v1 is a single merged stream per ByteCounts at :60-66). `split_merged_bytes`
        // is the exact inverse of the capture form `raw_buffer.join("\n")` (D-05),
        // including the empty-input case (WR-01): empty bytes → ZERO lines, matching
        // a live run that produced zero raw lines.
        let lines: Vec<String> = split_merged_bytes(merged_bytes);

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
            raw_captured: None, // bypass captures nothing (D-01)
        })
    }
}

/// Split captured/stored merged bytes into lines, lossily decoding UTF-8.
///
/// This is the EXACT inverse of the capture form `raw_buffer.join("\n")` (D-05),
/// so feeding `split_merged_bytes(raw_buffer.join("\n").into_bytes())` back in
/// regenerates the original `raw_buffer` line-for-line.
///
/// WR-01 (empty-output round-trip): an EMPTY input maps to ZERO lines, NOT a
/// single empty string. The live runner consumes `[].into_iter()` (zero lines)
/// when the subprocess emits nothing, and `[].join("\n") == ""` captures as an
/// empty BLOB. A naive `b"".split(b'\n')` would yield one empty element `[""]`,
/// so the replay would see ONE (blank) line where the live run saw none — an
/// extra phantom row in `lacon explain`. Special-casing the empty input keeps
/// the split a clean inverse of the join for the zero-output case, preserving
/// the byte-exact reproduction contract documented on `RunOutcome.raw_captured`.
fn split_merged_bytes(merged_bytes: &[u8]) -> Vec<String> {
    if merged_bytes.is_empty() {
        Vec::new()
    } else {
        merged_bytes
            .split(|&b| b == b'\n')
            .map(|l| String::from_utf8_lossy(l).into_owned())
            .collect()
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

    let mut signals = Signals::new([SIGTERM, SIGINT]).expect("signal-hook Signals::new failed");

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

#[cfg(test)]
mod tests {
    //! Phase 7 D-10: guard the Some/None shape of the gated raw-capture field.
    //!
    //! A `Runner` built with `RunOptions { capture_raw: true, .. }` MUST yield
    //! `RunOutcome.raw_captured == Some(..)`; with `capture_raw: false` it MUST
    //! yield `None`. Asserting only the SHAPE here (not byte content — that is the
    //! E2E test's job in `tracking_e2e.rs`) means a future edit that silently
    //! drops the capture wiring fails this fast, hermetic unit test.
    //!
    //! The Runner is built over a hand-assembled `ResolvedRule` mirroring the
    //! pattern in `crates/lacon-core/tests/runtime_filter_bytes.rs::make_rule`,
    //! driven against `printf` (POSIX-available on macOS + Linux, the only v1
    //! platforms). `env!("CARGO_BIN_EXE_test_emitter")` is NOT available inside
    //! lacon-core (test_emitter is a separate workspace member), hence a PATH
    //! command.

    use super::{split_merged_bytes, RunOptions, Runner};
    use crate::pipeline::Pipeline;
    use crate::rules::loader::{ResolvedRule, RuleSource};
    use crate::rules::schema::RuleFile;

    fn make_passthrough_rule() -> ResolvedRule {
        ResolvedRule {
            id: "capture-test".into(),
            source: RuleSource::Project,
            rule: RuleFile {
                id: "capture-test".into(),
                description: None,
                extends: None,
                match_spec: None,
                bypass_when: None,
                rewrite: None,
                pipeline: None,
                on_error: None,
                post_process: None,
            },
            // An empty success pipeline is a no-op passthrough — sufficient for a
            // shape-only assertion on the capture field.
            success_pipeline: Pipeline::new(vec![]),
            on_error_pipeline: None,
            post_process: None,
            on_error_post_process: None,
        }
    }

    fn printf_argv() -> Vec<String> {
        // Deterministic two-line output: "a\nb\n".
        vec!["printf".into(), "a\\nb\\n".into()]
    }

    #[test]
    fn capture_raw_true_yields_some() {
        let options = RunOptions {
            capture_raw: true,
            ..Default::default()
        };
        let mut runner = Runner::new(make_passthrough_rule(), options);
        let mut sink: Vec<u8> = Vec::new();
        let outcome = runner
            .run(&printf_argv(), &mut sink)
            .expect("runner.run with capture_raw=true");
        assert!(
            outcome.raw_captured.is_some(),
            "capture_raw=true must populate raw_captured: {:?}",
            outcome.raw_captured
        );
    }

    #[test]
    fn capture_raw_false_yields_none() {
        let options = RunOptions {
            capture_raw: false,
            ..Default::default()
        };
        let mut runner = Runner::new(make_passthrough_rule(), options);
        let mut sink: Vec<u8> = Vec::new();
        let outcome = runner
            .run(&printf_argv(), &mut sink)
            .expect("runner.run with capture_raw=false");
        assert!(
            outcome.raw_captured.is_none(),
            "capture_raw=false must leave raw_captured None: {:?}",
            outcome.raw_captured
        );
    }

    /// IN-03 / WR-01: lock the round-trip invariant the `RunOutcome.raw_captured`
    /// doc claims — `raw_buffer.join("\n")` is the exact inverse of the
    /// `filter_bytes` re-split (`split_merged_bytes`), so capture → store →
    /// replay regenerates the identical `Vec<String>` the live pipeline consumed.
    ///
    /// Parameterized over the cases the prose contract only spot-checks:
    /// (a) empty buffer (WR-01: zero lines, NOT one blank line),
    /// (b) a single line,
    /// (c) a trailing-empty line,
    /// (d) an invalid-UTF-8 line (lossy-decoded both at capture build and replay),
    /// (e) a CRLF line.
    #[test]
    fn raw_buffer_join_split_round_trips() {
        // The `\u{FFFD}` is the lossy replacement char: the runtime's live reader
        // already lossy-decodes (`String::from_utf8_lossy`) before pushing into
        // `raw_buffer`, so an invalid-UTF-8 *byte sequence* never reaches the
        // buffer as raw bytes — it arrives already replaced. The round-trip we
        // assert is therefore over the post-decode `Vec<String>`.
        let cases: Vec<(&str, Vec<String>)> = vec![
            ("empty buffer", Vec::new()),
            ("single line", vec!["only line".to_string()]),
            (
                "trailing-empty line",
                vec!["first".to_string(), String::new()],
            ),
            (
                "lossy-decoded (was invalid utf-8) line",
                vec!["bad\u{FFFD}byte".to_string(), "next".to_string()],
            ),
            (
                "CRLF line (the \\r is data, not a separator)",
                vec!["windows\r".to_string(), "unix".to_string()],
            ),
        ];

        for (label, raw_buffer) in cases {
            // Capture form (D-05): join on "\n", into_bytes(), NO trailing newline.
            let captured = raw_buffer.join("\n").into_bytes();
            // Replay split (the exact inverse, with the WR-01 empty guard).
            let replayed = split_merged_bytes(&captured);
            assert_eq!(
                replayed, raw_buffer,
                "round-trip must regenerate the original raw_buffer for case: {label}"
            );
        }

        // WR-01 explicitly: an empty capture is ZERO lines, never `[""]`.
        assert_eq!(
            split_merged_bytes(b""),
            Vec::<String>::new(),
            "empty merged bytes must yield zero lines (WR-01), not one blank line"
        );
    }
}
