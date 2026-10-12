# Changes

This crate is released with the engine under one version. See
[the workspace changelog](../CHANGES.md).

## Before the fork

`gtmpl_value_ng` continues `gtmpl_value` from its `master` branch at `a333139`, two
years after its last crates.io release.

### [0.5.0]
- Move from `String` to `thiserror` for errors

### Unreleased upstream, and inherited here
- Negative non-integer numbers are no longer truncated (`eff95ed`)
- Optional `serde` support (`f032fb2`, `1e541d6`, `a333139`)
