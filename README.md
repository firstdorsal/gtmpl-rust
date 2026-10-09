# gtmpl-rust — the Golang templating language for Rust

This repository holds three crates, released together under one version:

| crate | what it is |
|---|---|
| [`gtmpl-ng`](gtmpl-ng) | the template engine — parser, lexer and evaluator for Go's `text/template` |
| [`gtmpl_value_ng`](gtmpl_value_ng) | the value type a template parses literals into and reads a context through |
| [`gtmpl_derive_ng`](gtmpl_derive_ng) | `#[derive(Gtmpl)]`, which turns a struct into a context |

Start with [`gtmpl-ng`'s README](gtmpl-ng/README.md) — it documents the template
syntax, the Helm function set and the differences from Go that are left.

## Why one repository

The engine is a fork of [gtmpl-rust] by Florian Dieminger; the value type and the
derive macro are forks of [gtmpl_value] and [gtmpl_derive]. They used to be three
repositories, two of which were out of our hands: `gtmpl_value` 0.5.1 is from 2021 and
silently truncates every negative non-integer number, and the fix for that has been
sitting unreleased on its `master` branch since February 2025.

Vendoring them here means the value type is released whenever the engine needs it, and
one tag releases a consistent set.

## Changes

[CHANGES.md](CHANGES.md) covers all three crates.

[gtmpl-rust]: https://github.com/fiji-flo/gtmpl-rust
[gtmpl_value]: https://github.com/fiji-flo/gtmpl_value
[gtmpl_derive]: https://github.com/fiji-flo/gtmpl_derive
