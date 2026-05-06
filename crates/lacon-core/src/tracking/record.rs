//! Tracker write path (D-01, D-15, D-17).
//!
//! `Tracker::record(meta, raw_opt, ...)` performs the synchronous write at the
//! end of `lacon run`:
//!   1. If `cfg.store_raw_outputs == true` AND `raw_opt.is_some()`:
//!      a. Resolve the privacy marker path (project layer wins over user).
//!      b. Call `privacy::warn_once_if_needed` — first run prints the
//!         byte-stable warning + creates the marker; subsequent runs silent.
//!      c. INSERT into raw_outputs; capture the new id.
//!   2. INSERT into invocations with the captured raw_output_id (or NULL).
//!
//! The 17-column invocations INSERT is positional-?N for clarity (RESEARCH
//! line 198 — discretion choice, positional reads cleaner here).
//!
//! Best-effort posture (D-12): callers (`lacon-cli::commands::run`) wrap
//! this with `eprintln!`-on-error; this function returns Result without
//! ever altering the wrapper's exit code on its own.

use rusqlite::params;

use crate::error::TrackingError;
use crate::runtime::InvocationMeta;
use crate::tracking::{privacy, rule_source_str, RawOutput, Tracker};

impl Tracker {
    /// Insert one row into `invocations` (and conditionally `raw_outputs`)
    /// for this invocation. Returns the new `invocations.id`.
    ///
    /// # Behavior
    /// - `cfg_store_raw_outputs=false` → raw_outputs untouched; invocations.raw_output_id NULL.
    /// - `cfg_store_raw_outputs=true` + `raw=None` → raw_outputs untouched; invocations.raw_output_id NULL.
    /// - `cfg_store_raw_outputs=true` + `raw=Some(...)` →
    ///     1. Privacy warning checked exactly once via marker file (D-15).
    ///     2. INSERT raw_outputs → captured raw_id.
    ///     3. INSERT invocations with raw_output_id=raw_id.
    ///
    /// `project_root` and `user_config_dir` are passed in (rather than re-resolved
    /// inside) so callers can use a tempdir for testing without env-var stomping
    /// (RESEARCH Pitfall #4).
    ///
    /// # Errors
    /// `TrackingError::Sqlite` on SQLite failure; `TrackingError::Marker`
    /// on privacy marker creation failure (only when warning was attempted).
    pub fn record(
        &self,
        meta: &InvocationMeta,
        raw_opt: Option<&RawOutput>,
        project_root: Option<&std::path::Path>,
        user_config_dir: Option<&std::path::Path>,
        project_store_raw: bool,
        user_store_raw: bool,
    ) -> Result<i64, TrackingError> {
        let want_raw_insert = self.cfg_store_raw_outputs && raw_opt.is_some();
        let raw_output_id: Option<i64> = if want_raw_insert {
            // D-15: privacy warning trigger BEFORE the first raw_outputs INSERT.
            if let Some((cfg_path, marker_path)) = privacy::resolve_marker_path(
                project_root,
                user_config_dir,
                project_store_raw,
                user_store_raw,
            ) {
                privacy::warn_once_if_needed(&cfg_path, &marker_path)?;
            }

            let raw = raw_opt.expect("guarded by want_raw_insert");
            Some(self.insert_raw_output(raw, meta.ts_unix_ms)?)
        } else {
            None
        };

        let inv_id = self.insert_invocation(meta, raw_output_id)?;
        Ok(inv_id)
    }

    fn insert_raw_output(&self, raw: &RawOutput, ts_unix_ms: u64) -> Result<i64, TrackingError> {
        // invocation_id=0 is a placeholder. Per spec line 48 the column is
        // NOT NULL but FK is not declared on this side (FK direction is
        // invocations.raw_output_id → raw_outputs.id, not the reverse).
        // v1 keeps this loose; future migrations may introduce a bidirectional
        // FK if needed.
        let mut stmt = self.conn.prepare_cached(
            "INSERT INTO raw_outputs (invocation_id, stdout, stderr, created_ts)
             VALUES (0, ?1, ?2, ?3)",
        )?;
        let id = stmt.insert(params![
            &raw.stdout,
            &raw.stderr,
            ts_unix_ms as i64,
        ])?;
        Ok(id)
    }

    fn insert_invocation(
        &self,
        meta: &InvocationMeta,
        raw_output_id: Option<i64>,
    ) -> Result<i64, TrackingError> {
        let mut stmt = self.conn.prepare_cached(
            "INSERT INTO invocations (
                ts, assistant, session_id, project_path,
                command_raw, command_normalized, rule_id, rule_source,
                exit_code, duration_ms,
                raw_stdout_bytes, raw_stderr_bytes, filtered_bytes,
                bypassed, rewritten, truncated_by_max_bytes, raw_output_id
            ) VALUES (?1,?2,?3,?4, ?5,?6,?7,?8, ?9,?10, ?11,?12,?13, ?14,?15,?16, ?17)",
        )?;

        let project_path_str: Option<String> = meta
            .project_path
            .as_ref()
            .and_then(|p| p.to_str().map(|s| s.to_string()));
        let rule_source_value: Option<&'static str> =
            meta.rule_source.as_ref().map(rule_source_str);

        let id = stmt.insert(params![
            meta.ts_unix_ms as i64,
            &meta.assistant,
            // Pitfall #13: as_deref → Option<&str>; rusqlite serializes None
            // as SQL NULL via the ToSql blanket impl.
            meta.session_id.as_deref(),
            project_path_str,
            &meta.command_raw,
            &meta.command_normalized,
            meta.rule_id.as_deref(),
            rule_source_value,
            meta.exit_code as i64,
            meta.duration_ms as i64,
            meta.byte_counts.raw_stdout_bytes as i64,
            meta.byte_counts.raw_stderr_bytes as i64,
            meta.byte_counts.filtered_bytes as i64,
            meta.bypassed as i64,
            meta.rewritten as i64,
            meta.truncated_by_max_bytes as i64,
            raw_output_id,
        ])?;
        Ok(id)
    }
}
