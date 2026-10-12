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

Rendering a template needs nothing else — `Value`, `Func`, `FuncError` and the rest are
re-exported from `gtmpl-ng` itself. Two optional crates:

```toml
[dependencies]
gtmpl-ng = { version = "0.9", features = ["helm-functions"] }
# only for #[derive(Gtmpl)], to use your own structs as a context
gtmpl_derive_ng = "0.9"
gtmpl_value_ng = "0.9"
```

## Hello world

```rust
let output = gtmpl_ng::template("Finally! Some {{ . }} for Rust", "gtmpl");
assert_eq!(output.unwrap(), "Finally! Some gtmpl for Rust");
```

## Everything, in one file

This is a complete program. It compiles and every assertion in it holds — CI extracts
the code from this README and runs it, so it cannot drift from the engine.

```toml
[package]
name = "example"
version = "0.1.0"
edition = "2021"

[dependencies]
gtmpl-ng = { version = "0.9", features = ["all-functions", "gtmpl_dynamic_template"] }
gtmpl_derive_ng = "0.9"
gtmpl_value_ng = "0.9"
```

```rust
use gtmpl_derive_ng::Gtmpl;
use gtmpl_ng::all_functions::all_functions;
use gtmpl_ng::{gtmpl_fn, template, Context, Func, FuncError, Function, NumberKind, Template, Value};
use std::collections::HashMap;

// Any struct can be a template context. Every field is converted with `Into<Value>`,
// so a field type only has to implement that.
#[derive(Gtmpl)]
struct Service {
    name: String,
    replicas: u8,
    ports: Vec<u16>,
    tags: Vec<String>,
    labels: HashMap<String, String>,
    // A function field is callable from the template as `{{ .url }}`. It receives the
    // struct it lives on as its only argument and cannot take arguments of its own.
    url: Func,
}

fn url(args: &[Value]) -> Result<Value, FuncError> {
    let Some(Value::Object(fields)) = args.first() else {
        return Err(FuncError::Generic("url wants its own object".to_owned()));
    };
    let name = fields.get("name").map(ToString::to_string).unwrap_or_default();
    Ok(Value::from(format!("https://{name}.example.com")))
}

// `gtmpl_fn!` writes the argument conversion for a normal Rust function, so a template
// can call `{{ add 1 2 }}`.
gtmpl_fn!(
    fn add(a: u64, b: u64) -> Result<u64, FuncError> {
        Ok(a + b)
    }
);

fn main() {
    // Render one template in one call.
    assert_eq!(template("Some {{ . }}", "gtmpl").unwrap(), "Some gtmpl");

    // The derive takes the struct by value, so build the `Value` once and clone it for
    // each render. Cloning a `Value` is cheap compared to re-deriving it.
    let service = Value::from(Service {
        name: "web".to_owned(),
        replicas: 3,
        ports: vec![80, 443],
        tags: Vec::new(),
        labels: [("tier".to_owned(), "frontend".to_owned())]
            .into_iter()
            .collect(),
        url,
    });

    // The usual shape: compile once, register the functions, render with a context.
    let render = |source: &str| {
        let mut tmpl = Template::default();
        tmpl.add_funcs(&all_functions());
        tmpl.add_func("add", add);
        tmpl.parse(source).expect("the template parses");
        tmpl.render(&Context::from(service.clone())).expect("it renders")
    };

    // Fields and nested fields.
    assert_eq!(render("{{ .name }} x{{ .replicas }}"), "web x3");
    assert_eq!(render("{{ .labels.tier }}"), "frontend");

    // A key a map does not have ends the chain as `<no value>`, however deep, so a
    // lookup into optional data is simply falsy rather than fatal. A field a *struct*
    // does not have is an error instead -- see the last section.
    assert_eq!(render("{{ .labels.absent.deeper }}"), "<no value>");
    assert_eq!(render("{{ if .labels.absent }}set{{ else }}unset{{ end }}"), "unset");

    // range, if/else, with
    assert_eq!(render("{{ range .ports }}:{{ . }}{{ end }}"), ":80:443");
    assert_eq!(render("{{ if gt .replicas 1 }}ha{{ else }}single{{ end }}"), "ha");
    assert_eq!(render("{{ with .labels.tier }}{{ . }}{{ end }}"), "frontend");
    // `else` on a range runs when the collection is empty.
    assert_eq!(render("{{ range .tags }}x{{ else }}none{{ end }}"), "none");

    // `:=` declares a variable, `=` assigns the nearest existing one -- which is how a
    // loop accumulates into something declared outside it.
    assert_eq!(
        render("{{ $sum := 0 }}{{ range .ports }}{{ $sum = add $sum . }}{{ end }}{{ $sum }}"),
        "523"
    );

    // A function field, called as a method.
    assert_eq!(render("{{ .url }}"), "https://web.example.com");

    // `printf` follows Go's `fmt`, down to the flag and padding rules.
    assert_eq!(
        render(r#"{{ printf "%-6s|%03d|%.2f|%q" .name .replicas 1.5 .name }}"#),
        "web   |003|1.50|\"web\""
    );

    // The Helm function set: 152 functions, including the pipeline staples.
    assert_eq!(render("{{ .name | upper | quote }}"), "\"WEB\"");
    assert_eq!(render("{{ .ports | first }}"), "80");
    assert_eq!(render("{{ toYaml .ports }}"), "- 80\n- 443\n");
    assert_eq!(
        render(r#"{{ "hello" | sha256sum | trunc 8 }}"#),
        "2cf24dba"
    );

    // The mows function set: 3 more.
    assert_eq!(render(r#"{{ mowsJoinDomain .name "example.com" }}"#), "web.example.com");

    // Named templates, defined and called.
    assert_eq!(
        render(r#"{{ define "row" }}[{{ . }}]{{ end }}{{ range .ports }}{{ template "row" . }}{{ end }}"#),
        "[80][443]"
    );

    // A trim marker eats the whitespace on its side, including newlines.
    assert_eq!(render("{{ .name }}\n   {{- .replicas }}"), "web3");

    // A function registered by name, without the Helm set.
    let mut tmpl = Template::default();
    tmpl.add_func("add", add);
    tmpl.parse("{{ add 1 2 }}").unwrap();
    assert_eq!(tmpl.render(&Context::empty()).unwrap(), "3");

    // A function passed in as the context and invoked with `call`.
    let called = template("{{ call . 1 2 }}", Value::Function(Function { f: add }));
    assert_eq!(called.unwrap(), "3");

    // With the `gtmpl_dynamic_template` feature the template name can come from a
    // pipeline, so which block renders is decided at runtime.
    let mut tmpl = Template::default();
    tmpl.parse(r#"{{- define "a"}}A{{ end -}}{{- define "b"}}B{{ end -}}{{- template (.) -}}"#)
        .unwrap();
    assert_eq!(tmpl.render(&Context::from(Value::from("b"))).unwrap(), "B");

    // Reading a `Value` back out. `kind()` answers once instead of trying `as_u64`,
    // `as_i64` and `as_f64` in the right order.
    let Value::Object(fields) = &service else { panic!("a struct derives into an object") };
    let Some(Value::Number(replicas)) = fields.get("replicas") else { panic!("no replicas") };
    assert_eq!(replicas.kind(), NumberKind::Unsigned(3));

    // Errors carry the position of what went wrong: line, column and length.
    let error = Template::default().parse("{{ if }}").unwrap_err();
    assert!(error.to_string().contains(":1:"), "got: {error}");

    // And a field the struct does not have is one of them. This is the one place where
    // a struct and a map differ: the map answers `<no value>`, the struct refuses,
    // because a field name that does not exist is a mistake rather than missing data.
    let mut tmpl = Template::default();
    tmpl.parse("{{ .typo }}").unwrap();
    let error = tmpl.render(&Context::from(service.clone())).unwrap_err();
    assert!(error.to_string().contains("no field `typo`"), "got: {error}");

    println!("every assertion held");
}
```

## Feature flags

| flag | what it adds |
|---|---|
| *(none)* | the engine and the built-in functions Go has: `printf`, `len`, `index`, `eq`, `and`, `call`, … |
| `helm-functions` | 152 Helm-compatible functions — `upper`, `toYaml`, `b64enc`, `sha256sum`, `trunc`, `default`, … |
| `mows-functions` | 3 more: `mowsRandomString`, `mowsDigest`, `mowsJoinDomain` |
| `all-functions` | both of the above, and `all_functions()` to register them in one call |
| `gtmpl_dynamic_template` | `{{ template (pipeline) }}`, so the template name can be computed |
| `gtmpl_value_compat` | `Value::try_from` for the published `gtmpl_value` types, for migrating |

`helm-functions` needs a newer compiler than the crate's own minimum, because `uuid`
and `time` set their own; cargo names the version it wants. Everything else builds on
Rust 1.85, which CI checks on that exact toolchain.

## Things worth knowing before you start

- **A missing map key is not an error, a missing struct field is.** `{{ .labels.nope }}`
  on a map renders `<no value>` and is falsy however deep the chain goes, so reading
  optional data needs no guard. `{{ .nope }}` on a `#[derive(Gtmpl)]` struct fails with
  `no field \`nope\``, because a field name that does not exist is a typo rather than
  missing data.
- **`#[derive(Gtmpl)]` converts by value.** Build the `Value` once and clone it per
  render, as the example does.
- **A `Func` field is a method with no arguments.** It receives the struct it lives on
  as its only argument; a function that needs arguments is registered with `add_func`
  instead.
- **Map keys are iterated in sorted order**, so the same input always renders the same
  bytes.
- **Numbers compare permissively.** `{{ eq 3.0 3 }}` answers `true` where Go refuses
  with "incompatible types for comparison".

## Where to read more

- **[The engine's README](gtmpl-ng/README.md)** — the template semantics in full, the
  dynamic-template syntax, and the list of Go features that are still missing.
- **[`go_conformance.txt`](gtmpl-ng/tests/go_conformance.txt)** — 237 templates diffed
  against real Go 1.26.8, with each deliberate difference and its reason. This is the
  specification `printf` is held to.
- **[CHANGES.md](CHANGES.md)** — all three crates.
- [docs.rs/gtmpl-ng](https://docs.rs/gtmpl-ng) · [docs.rs/gtmpl_value_ng](https://docs.rs/gtmpl_value_ng) · [docs.rs/gtmpl_derive_ng](https://docs.rs/gtmpl_derive_ng)
- [Go's own `text/template` documentation](https://golang.org/pkg/text/template/) — the
  language this implements.

## Which crate do I depend on?

Three crates, one version, released together — so a version of one always fits the same
version of the others.

| crate | depend on it when |
|---|---|
| [`gtmpl-ng`](gtmpl-ng) | you render templates. This is the one you want. |
| [`gtmpl_derive_ng`](gtmpl_derive_ng) | you want `#[derive(Gtmpl)]` on your own structs |
| [`gtmpl_value_ng`](gtmpl_value_ng) | you build or read a `Value` without the engine |

Coming from the published `gtmpl_value`/`gtmpl_derive`? `Into` is not transitive, so a
value from the old derive will not satisfy a bound asking for the new type. Change the
derive import, or enable `gtmpl_value_compat` and convert with `Value::try_from`.

## This repository

The engine is a fork of [gtmpl-rust] by Florian Dieminger, the value type and the derive
macro are forks of [gtmpl_value] and [gtmpl_derive]. They were three repositories, two
of them out of our hands: `gtmpl_value` 0.5.1 is from 2021 and silently truncates every
negative non-integer number, and the fix has been sitting unreleased on its `master`
branch since February 2025. Vendoring them here means the value type is released
whenever the engine needs it.

Contributions welcome — the test suite is the specification, and
`cargo test --workspace --all-features` is how you run it.

[gtmpl-rust]: https://github.com/fiji-flo/gtmpl-rust
[gtmpl_value]: https://github.com/fiji-flo/gtmpl_value
[gtmpl_derive]: https://github.com/fiji-flo/gtmpl_derive
