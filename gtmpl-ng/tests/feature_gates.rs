//! Says out loud what a default `cargo test` run does not cover.
//!
//! A plain `cargo test` runs far fewer tests than `--all-features` does. The
//! difference is everything behind the optional function registries: the
//! `helm_functions` and `mows_functions` unit tests inside `src/`, and the
//! `panic_audit.rs`, `panic_regressions.rs` and `resource_bounds.rs` suites, which
//! compile to nothing without the feature and report "running 0 tests ... ok" — so a
//! contributor running the obvious command sees all green having exercised none of it.
//!
//! Each appears here as an ignored test instead, naming the feature it needs, so the
//! run says what it skipped. Run everything with
//! `cargo test --workspace --all-features`, or the `cargo test-all` alias, which is
//! the feature set CI uses.

/// The suite that holds the memory bounds on the generating functions. Every case in
/// it aborted the process before those bounds existed, so a run that skips it silently
/// is exactly what must not happen.
#[test]
#[cfg_attr(
    not(feature = "all-functions"),
    ignore = "needs --all-features: resource_bounds.rs needs every function registry"
)]
fn the_resource_bound_suite_ran() {}

/// The two suites whose whole purpose is catching panic regressions.
#[test]
#[cfg_attr(
    not(feature = "helm-functions"),
    ignore = "needs --all-features: panic_audit.rs and panic_regressions.rs run through the helm function registry"
)]
fn the_panic_suites_ran() {}

/// Roughly 130 unit tests inside `src/helm_functions/`, covering the 152 functions.
#[test]
#[cfg_attr(
    not(feature = "helm-functions"),
    ignore = "needs --all-features: ~130 helm_functions unit tests in src/ are gated out"
)]
fn the_helm_function_unit_tests_ran() {}

/// Twelve unit tests inside `src/mows_functions/`.
#[test]
#[cfg_attr(
    not(feature = "mows-functions"),
    ignore = "needs --all-features: the mows_functions unit tests in src/ are gated out"
)]
fn the_mows_function_unit_tests_ran() {}

/// The conversion from the published `gtmpl_value` types.
#[test]
#[cfg_attr(
    not(feature = "gtmpl_value_compat"),
    ignore = "needs --all-features: gtmpl_value_ng's compat tests are gated out"
)]
fn the_legacy_value_conversion_tests_ran() {}

/// `Serialize`/`Deserialize` for `Value`.
#[test]
#[cfg_attr(
    not(feature = "gtmpl_value_compat"),
    ignore = "needs --all-features: gtmpl_value_ng's serde round-trip tests are gated out"
)]
fn the_serde_round_trip_tests_ran() {}
