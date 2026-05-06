//! ValidationError enum — one thiserror variant per D-18 category.
//!
//! Display format (byte-exact per D-18):
//! `<path>:<line>: <Category>: <message>`
//!
//! Line 0 is used when line information is unavailable (e.g., file-level errors).

use std::path::PathBuf;

/// Validation error for rule and config files.
///
/// Every variant formats as `<path>:<line>: <Category>: <message>`.
/// Categories per D-18: InvalidRegex, UnknownPrimitive, CircularExtends,
/// MissingScriptFile, UserOnlyKeyInProject, UnknownKey, ParseError, IoError.
#[derive(thiserror::Error, Debug)]
pub enum ValidationError {
    /// A regex pattern in a rule file failed to compile.
    #[error("{path}:{line}: InvalidRegex: {message}")]
    InvalidRegex {
        path: PathBuf,
        line: usize,
        message: String,
    },

    /// A pipeline stage name was not recognised (deny_unknown_fields fires on StageSpec).
    /// Kept for semantic clarity; serde's deny_unknown_fields surfacing maps here.
    #[error("{path}:{line}: UnknownPrimitive: {message}")]
    UnknownPrimitive {
        path: PathBuf,
        line: usize,
        message: String,
    },

    /// An `extends` chain forms a cycle (T-03-03 mitigation, Pitfall 6).
    #[error("{path}:{line}: CircularExtends: {message}")]
    CircularExtends {
        path: PathBuf,
        line: usize,
        message: String,
    },

    /// A `script:` path is missing, absolute, or contains `..` (T-03-04 mitigation).
    #[error("{path}:{line}: MissingScriptFile: {message}")]
    MissingScriptFile {
        path: PathBuf,
        line: usize,
        message: String,
    },

    /// A project config file contains a user-only key (e.g., `retention`).
    /// T-03-06 mitigation.
    #[error("{path}:{line}: UserOnlyKeyInProject: {message}")]
    UserOnlyKeyInProject {
        path: PathBuf,
        line: usize,
        message: String,
    },

    /// An unknown top-level or nested key was found (deny_unknown_fields).
    #[error("{path}:{line}: UnknownKey: {message}")]
    UnknownKey {
        path: PathBuf,
        line: usize,
        message: String,
    },

    /// General YAML parse error (malformed YAML, type mismatch, etc.).
    #[error("{path}:{line}: ParseError: {message}")]
    ParseError {
        path: PathBuf,
        line: usize,
        message: String,
    },

    /// I/O error reading a file. Line is always 0 (file could not be opened).
    #[error("{path}: IoError: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// Runtime error for Starlark `post_process` execution and (in PLAN-05) subprocess I/O.
///
/// Variants follow the categories in the PLAN-04 interface spec.
/// `StarlarkResourceLimit` exists for forward-compat; not enforced in v1.
#[derive(thiserror::Error, Debug)]
pub enum RuntimeError {
    #[error("starlark parse error in {path}: {message}")]
    StarlarkParseError {
        path: std::path::PathBuf,
        message: String,
    },
    #[error("starlark evaluation error in {path}: {message}")]
    StarlarkEvalError {
        path: std::path::PathBuf,
        message: String,
    },
    #[error("starlark function `{function}` in {path} returned {got}, expected list[str]")]
    StarlarkResultTypeError {
        path: std::path::PathBuf,
        function: String,
        got: String,
    },
    #[error("starlark function `{function}` in {path} exceeded resource limit: {detail}")]
    StarlarkResourceLimit {
        path: std::path::PathBuf,
        function: String,
        detail: String,
    },
    // PLAN-05 will add subprocess/IO variants here.
}

impl ValidationError {
    /// Extract the path from any variant. Useful for tests.
    pub fn path(&self) -> &std::path::Path {
        match self {
            Self::InvalidRegex { path, .. } => path,
            Self::UnknownPrimitive { path, .. } => path,
            Self::CircularExtends { path, .. } => path,
            Self::MissingScriptFile { path, .. } => path,
            Self::UserOnlyKeyInProject { path, .. } => path,
            Self::UnknownKey { path, .. } => path,
            Self::ParseError { path, .. } => path,
            Self::Io { path, .. } => path,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_format_byte_exact() {
        let err = ValidationError::InvalidRegex {
            path: PathBuf::from(".lacon/rules/my-rule.yaml"),
            line: 7,
            message: "unclosed character class".to_owned(),
        };
        let s = format!("{err}");
        assert_eq!(
            s,
            ".lacon/rules/my-rule.yaml:7: InvalidRegex: unclosed character class"
        );
    }

    #[test]
    fn error_display_unknown_key() {
        let err = ValidationError::UnknownKey {
            path: PathBuf::from(".lacon/config.yaml"),
            line: 3,
            message: "field `banana` not found".to_owned(),
        };
        let s = format!("{err}");
        assert!(s.starts_with(".lacon/config.yaml:3: UnknownKey:"));
    }

    #[test]
    fn error_display_user_only_key_in_project() {
        let err = ValidationError::UserOnlyKeyInProject {
            path: PathBuf::from(".lacon/config.yaml"),
            line: 1,
            message: "key `retention` is user-only; move to ~/.config/lacon/config.yaml".to_owned(),
        };
        let s = format!("{err}");
        assert!(s.contains("UserOnlyKeyInProject"));
        assert!(s.contains("`retention` is user-only"));
    }

    #[test]
    fn error_display_io_format() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "No such file");
        let err = ValidationError::Io {
            path: PathBuf::from("missing.yaml"),
            source: io_err,
        };
        let s = format!("{err}");
        assert!(s.starts_with("missing.yaml: IoError:"));
    }

    #[test]
    fn error_path_accessor() {
        let err = ValidationError::CircularExtends {
            path: PathBuf::from("cycle.yaml"),
            line: 0,
            message: "cycle detected".to_owned(),
        };
        assert_eq!(err.path(), std::path::Path::new("cycle.yaml"));
    }
}
