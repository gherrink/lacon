//! Bundled rules — embedded via rust-embed at compile time.
//!
//! The `bundled-rules/` directory at the workspace root is embedded into the binary.
//! Phase 5 populates this directory with real YAML rule files. Phase 1 (PLAN-03) stands
//! up the embedding mechanism so the loader has a concrete API to call.
//!
//! An empty `bundled-rules/.gitkeep` was created by PLAN-01. rust-embed handles empty
//! directories by yielding zero files — no panic, no error.
//!
//! # Path resolution
//! rust-embed resolves relative paths from `$CARGO_MANIFEST_DIR` at compile time.
//! `crates/lacon-core/Cargo.toml` → `CARGO_MANIFEST_DIR` is `crates/lacon-core/`.
//! `../../bundled-rules/` → resolves to the workspace-root `bundled-rules/` directory.

use rust_embed::RustEmbed;

/// rust-embed handle for the bundled-rules/ directory.
///
/// The path `../../bundled-rules/` is relative to `crates/lacon-core/Cargo.toml`
/// (i.e., relative to `$CARGO_MANIFEST_DIR`), resolving to `<workspace>/bundled-rules/`.
#[derive(RustEmbed)]
#[folder = "../../bundled-rules/"]
pub struct BundledRules;

/// Iterate over the filenames of all bundled YAML rule files.
///
/// Returns only files ending in `.yaml`; skips `.gitkeep` and other non-YAML artifacts.
pub fn iter_bundled() -> impl Iterator<Item = String> {
    BundledRules::iter()
        .map(|s: std::borrow::Cow<'static, str>| s.into_owned())
        .filter(|s| s.ends_with(".yaml"))
}

/// Retrieve the raw bytes of a bundled rule file by filename.
///
/// Returns `None` if the filename does not exist in the embedded archive.
pub fn get_bundled(name: &str) -> Option<Vec<u8>> {
    BundledRules::get(name).map(|f| f.data.into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_iter_does_not_panic_on_empty_dir() {
        // bundled-rules/ only contains .gitkeep in Phase 1; iter should yield 0 items.
        let count = iter_bundled().count();
        // Phase 1: expect 0 bundled YAML files (.gitkeep is filtered out).
        // Phase 5 will add real rules; this test just ensures no panic.
        assert!(count == 0, "Phase 1 bundled dir contains no .yaml files; got {count}");
    }

    #[test]
    fn get_bundled_nonexistent_returns_none() {
        assert!(get_bundled("does-not-exist.yaml").is_none());
    }

    #[test]
    fn bundled_rules_struct_compiles() {
        // Smoke test: prove the RustEmbed derive compiled without error.
        let _iter = BundledRules::iter();
    }
}
