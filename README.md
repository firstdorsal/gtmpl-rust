# gtmpl-ng — Golang templates for Rust

[![Latest Version]][crates.io]

[Latest Version]: https://img.shields.io/crates/v/gtmpl-ng.svg
[crates.io]: https://crates.io/crates/gtmpl-ng

Go's [`text/template`](https://golang.org/pkg/text/template/) engine, in Rust. If your
tooling renders Helm charts, Docker format strings or anything else written in Go's
template syntax, this renders the same text without shelling out to Go.

It is a maintained fork of [gtmpl-rust], with the Go-conformance fixes and the Helm
function set that the original does not have.

## Install

```toml
[dependencies]
gtmpl-ng = "0.9"
```

For Helm charts you want the function set as well — `upper`, `toYaml`, `b64enc`,
`sha256sum` and 148 more:

```toml
[dependencies]
gtmpl-ng = { version = "0.9", features = ["helm-functions"] }
```

Add `gtmpl_derive_ng = "0.9"` and `gtmpl_value_ng = "0.9"` only if you want
`#[derive(Gtmpl)]` to turn your own structs into a template context.

## Use it

```rust
use gtmpl_ng as gtmpl;

let output = gtmpl::template("Finally! Some {{ . }} for Rust", "gtmpl");
assert_eq!(&output.unwrap(), "Finally! Some gtmpl for Rust");
```

With the Helm functions:

```rust
use gtmpl_ng::helm_functions::HELM_FUNCTIONS;
use gtmpl_ng::{Context, Template};

let mut template = Template::default();
template.add_funcs(&HELM_FUNCTIONS);
template.parse(r#"{{ "hello" | upper }}"#).unwrap();
assert_eq!(template.render(&Context::empty()).unwrap(), "HELLO");
```

**[Full documentation →](gtmpl-ng/README.md)** — a struct as context, custom functions,
methods on a context, the template semantics this follows, the feature flags, and the
list of Go features that are still missing.

## What you get

- **Go's template language**: actions, pipelines, variables and their scoping,
  `range`/`if`/`with`/`define`/`template`/`block`, trim markers, and Go's number
  syntax down to hexadecimal float literals.
- **155 functions** behind feature flags: 152 Helm-compatible ones
  (`helm-functions`), 3 mows-specific ones (`mows-functions`), or both
  (`all-functions`).
- **`printf` that matches Go's `fmt`**, verified rather than claimed: 237 templates are
  diffed against real Go 1.26.8 and recorded in
  [`go_conformance.txt`](gtmpl-ng/tests/go_conformance.txt), together with the
  handful of deliberate differences and the reason for each.
- **Deterministic output**: map keys are iterated in sorted order, so the same input
  renders the same bytes.

Needs Rust 1.85 or newer. `helm-functions` needs a newer compiler than that, because
`uuid` and `time` do; cargo names the version it wants.

## Which crate do I depend on?

Three crates, one version, released together — so a version of one always fits the same
version of the others.

| crate | depend on it when |
|---|---|
| [`gtmpl-ng`](gtmpl-ng) | you render templates. This is the one you want. |
| [`gtmpl_derive_ng`](gtmpl_derive_ng) | you want `#[derive(Gtmpl)]` on your own structs |
| [`gtmpl_value_ng`](gtmpl_value_ng) | you build or read a `Value` without the engine |

`gtmpl-ng` re-exports the whole value surface — `Value`, `Number`, `Func`, `FuncError`,
`from_value` and the rest — so rendering templates needs the first crate only.

Coming from the published `gtmpl_value`/`gtmpl_derive`? One import per crate changes;
[the migration note](gtmpl-ng/README.md#context) covers both routes.

## This repository

The engine is a fork of [gtmpl-rust] by Florian Dieminger, the value type and the derive
macro are forks of [gtmpl_value] and [gtmpl_derive]. They were three repositories, two
of them out of our hands: `gtmpl_value` 0.5.1 is from 2021 and silently truncates every
negative non-integer number, and the fix has been sitting unreleased on its `master`
branch since February 2025. Vendoring them here means the value type is released
whenever the engine needs it.

[CHANGES.md](CHANGES.md) covers all three crates. Contributions welcome — the test
suite is the specification, and `cargo test --workspace --all-features` is how you run
it.

[gtmpl-rust]: https://github.com/fiji-flo/gtmpl-rust
[gtmpl_value]: https://github.com/fiji-flo/gtmpl_value
[gtmpl_derive]: https://github.com/fiji-flo/gtmpl_derive
