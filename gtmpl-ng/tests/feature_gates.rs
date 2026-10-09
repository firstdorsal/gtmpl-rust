//! Says out loud which suites a default `cargo test` run does not cover.
//!
//! `panic_audit.rs` and `panic_regressions.rs` exercise the optional helm function
//! registry, so without `helm-functions` they compile to nothing and report "running
//! 0 tests ... ok" -- a contributor running the obvious command sees all green having
//! exercised neither. Each appears here as an ignored test instead, so the run says
//! what it skipped.
//!
//! Run everything with `cargo test --workspace --all-features`, which is what CI does.

#[test]
#[cfg_attr(
    not(feature = "helm-functions"),
    ignore = "skipped without --all-features: panic_audit.rs runs through the helm function registry"
)]
fn the_panic_audit_suite_ran() {}

#[test]
#[cfg_attr(
    not(feature = "helm-functions"),
    ignore = "skipped without --all-features: panic_regressions.rs runs through the helm function registry"
)]
fn the_panic_regressions_suite_ran() {}
