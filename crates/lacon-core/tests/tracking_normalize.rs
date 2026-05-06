//! Integration tests for tracking::normalize — verifies the public API surface
//! (lacon_core::tracking::normalize) outside the crate boundary.

use lacon_core::tracking::normalize;

#[test]
fn public_api_pnpm_install() {
    let argv: Vec<String> = vec!["pnpm".into(), "install".into()];
    assert_eq!(normalize(&argv), "pnpm install");
}

#[test]
fn public_api_basename_strip() {
    let argv: Vec<String> = vec!["/opt/homebrew/bin/cargo".into(), "build".into()];
    assert_eq!(normalize(&argv), "cargo build");
}

#[test]
fn public_api_flag_arg_dropped() {
    let argv: Vec<String> = vec!["jest".into(), "--watch".into()];
    assert_eq!(normalize(&argv), "jest");
}
