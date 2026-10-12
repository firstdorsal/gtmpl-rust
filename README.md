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
gtmpl-ng = "1"
```

Rendering a template needs nothing else — `Value`, `Func`, `FuncError` and the rest are
re-exported from `gtmpl-ng` itself. Two optional crates:

```toml
[dependencies]
gtmpl-ng = { version = "1", features = ["helm-functions"] }
# only for #[derive(Gtmpl)], to use your own structs as a context
gtmpl_derive_ng = "1"
gtmpl_value_ng = "1"
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
gtmpl-ng = { version = "1", features = ["all-functions", "gtmpl_dynamic_template"] }
gtmpl_derive_ng = "1"
gtmpl_value_ng = "1"
```

```rust
use gtmpl_derive_ng::Gtmpl;
use gtmpl_ng::all_functions::all_functions;
use gtmpl_ng::{gtmpl_fn, template, Context, Func, FuncError, Function, NumberKind, Template, Value};
use std::collections::HashMap;

// Any struct can be a template context. Every field is converted with `Into<Value>`,
// so a field type only has to implement that -- including another derived struct, or a
// `Vec` or `HashMap` of one. None of them has to be `Clone`.
#[derive(Gtmpl)]
struct Container {
    name: String,
    image: String,
}

#[derive(Gtmpl)]
struct Service {
    name: String,
    replicas: u8,
    ports: Vec<u16>,
    tags: Vec<String>,
    containers: Vec<Container>,
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
        containers: vec![
            Container { name: "nginx".to_owned(), image: "nginx:1.27".to_owned() },
            Container { name: "sidecar".to_owned(), image: "envoy:1.31".to_owned() },
        ],
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
    // Ranging over a list of structs: dot is each struct in turn, so its fields read
    // as they do at the top level.
    assert_eq!(
        render("{{ range .containers }}{{ .name }}={{ .image }} {{ end }}"),
        "nginx=nginx:1.27 sidecar=envoy:1.31 "
    );
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

    // The Helm function set: 150 functions, including the pipeline staples.
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

## Registering functions

Each registry is a constant you hand to `add_funcs`, and your own functions go in one at
a time with `add_func`. Each feature works on its own — `helm-functions` without
`mows-functions`, or neither.

```rust
use gtmpl_ng::helm_functions::HELM_FUNCTIONS; // &[(&str, Func)], needs `helm-functions`
use gtmpl_ng::mows_functions::MOWS_FUNCTIONS; // [(&str, Func); 3], needs `mows-functions`
use gtmpl_ng::{Context, FuncError, Template, Value};

fn shout(args: &[Value]) -> Result<Value, FuncError> {
    let text = args.first().map(ToString::to_string).unwrap_or_default();
    Ok(Value::from(format!("{}!", text.to_uppercase())))
}

let mut tmpl = Template::default();
tmpl.add_funcs(&HELM_FUNCTIONS); // one registry
tmpl.add_funcs(&MOWS_FUNCTIONS); // or several
tmpl.add_func("shout", shout); // and your own, by name
tmpl.parse(r#"{{ shout (.name | lower) }}"#).unwrap();

let mut data = std::collections::HashMap::new();
data.insert("name".to_owned(), Value::from("Web"));
assert_eq!(tmpl.render(&Context::from(Value::Map(data))).unwrap(), "WEB!");
```

`all-functions` adds `gtmpl_ng::all_functions::all_functions()` on top, which returns
both sets as one `Vec<(&str, Func)>` for a single `add_funcs` call — that is the one the
example above uses.

Without any feature you still have every builtin Go itself provides: `and`, `call`,
`eq`, `ge`, `gt`, `html`, `index`, `js`, `le`, `len`, `lt`, `ne`, `not`, `or`,
`print`, `printf`, `println`, `slice` and `urlquery`.

## Rendering templates you did not write

If the templates come from your own repository, skip this. If a template can arrive from
somewhere else — a chart a user uploads, a notification body someone edits in a web form,
a dashboard panel — then **the template is untrusted input**, and that is worth a moment.

The engine has no I/O to abuse: no filesystem, no network, no environment, no
subprocesses, and the function set deliberately omits Sprig's `env`, `expandenv` and
`getHostByName`. What a hostile template can do instead is make your process spend what
it does not have. Four one-line templates used to be enough to end it:

| Template | Before |
|---|---|
| `{{define "r"}}{{template "r" .}}{{end}}{{template "r" .}}` | stack overflow, `SIGABRT` |
| `{{range 1000000000}}{{end}}` | ran until killed |
| `{{range 100000}}{{repeat 1000000 "x"}}{{end}}` | allocator failed, `SIGABRT` |
| `{{range 10000}}{{range 10000}}{{range 10000}}{{end}}{{end}}{{end}}` | ran until killed |

None of them could be caught, because an overflowed stack and a failed allocation abort
the process rather than return — the `Result` was never reached. Every one of them is now
an ordinary error, and the defaults that make it so are these:

| `Limits` field | Default | Bounds |
|---|---|---|
| `iterations` | 10 000 | one `{{range N}}`, and one `until` / `untilStep` / `seq` |
| `total_iterations` | 1 000 000 | every loop in the render together, so nesting cannot multiply |
| `output_bytes` | 64 MiB | what the render writes |
| `function_output` | 1 000 000 | characters from one `repeat`, `indent` or `rand*` |
| `deadline` | *none* | how long one render may take — opt-in, see below |

They are sized for templates a human wrote: measured on the author's deployment corpus
of 157 templates, the 149 deterministic ones render byte-identical to the unlimited
0.10.1, and the deepest does not pass 8 levels of nesting. Raise
the one you need, and the others stay where they are:

```rust
use gtmpl_ng::{Context, Limits, Template};

// A report that legitimately loops further than the default allows.
let mut tmpl = Template::with_limits(Limits::default().with_iterations(50_000));
tmpl.parse("{{ range 20000 }}.{{ end }}").unwrap();
assert_eq!(tmpl.render(&Context::empty()).unwrap().len(), 20_000);

// Or opt out entirely, for templates that are all your own.
let mut tmpl = Template::with_limits(Limits::unlimited());
tmpl.parse("{{ range 2000000 }}{{ end }}").unwrap();
assert_eq!(tmpl.render(&Context::empty()).unwrap(), "");

// The default refuses that same loop rather than running it.
let mut tmpl = Template::default();
tmpl.parse("{{ range 2000000 }}{{ end }}").unwrap();
assert!(tmpl.render(&Context::empty()).is_err());
```

### Time is the fifth limit, and it is opt-in

The four counters above bound memory and stack, because those are what end a process. They
do not bound time, and the gap is measurable. This template is inside every default:

```text
{{ range 100 }}{{ range 10000 }}{{ len (repeat 1000000 "x") }}{{ end }}{{ end }}
```

It spends the whole million-iteration budget on bodies that each build a megabyte and throw
it away, and runs for **13.8 seconds** before `total_iterations` stops it. Worse is flat:

```text
{{ range 10000 }}{{ htpasswd "u" "p" }}{{ end }}
```

is 10 000 bcrypt hashes at ~165 ms each — measured on the author's machine, a
**27-minute render that succeeds**,
with 20 000 bytes of output and every counter far from its limit, because bcrypt's cost is
time by design. Every count in both templates is allowed; no counter has anything to say
about how long they take.

`deadline` covers that, and it is **off by default on purpose**. Every other limit counts
something the template asked for, so the same template gives the same answer on any
machine; a deadline makes a render fail on a loaded host and succeed on an idle one. That
is the right trade when the templates are not yours, and the wrong one as a default:

```rust
use gtmpl_ng::{Context, Limits, Template};
use std::time::Duration;

// `unlimited` here so that the deadline is plainly what stops this, and not one of
// the counters -- in a real program you would keep the counters and add the deadline.
let mut tmpl = Template::with_limits(Limits::unlimited().with_deadline(Duration::from_millis(50)));
tmpl.parse("{{ range 100000000 }}{{ end }}").unwrap();
let error = tmpl.render(&Context::empty()).unwrap_err();
assert!(error.to_string().contains("did not finish within"));
// The template above stops at 2.00 s under a two-second deadline, against 13.8 s unbounded.

// Every limit is independent, so the usual shape is the defaults plus a deadline.
let guarded = Limits::default().with_deadline(Duration::from_secs(5));
assert_eq!(guarded.iterations, Some(10_000));
```

The clock starts at each `render`, so a cached template does not arrive with its deadline
spent. It is checked before every function call and on every 64th iteration, which costs
nothing measurable — 27.0 ns per iteration against 26.9 without a deadline — and stops a
render within a fraction of a millisecond of its deadline. What it cannot do is interrupt
a single call: one `genCA` runs to its end, and the deadline stops the next one. A custom
function that works in pieces can read `gtmpl_ng::limits::remaining()`.

To take control back mid-call, render on a thread and abandon it — `Template`, `Context`
and `Value` are `Send`, so this works today with no help from the engine:

```rust,no_run
use gtmpl_ng::{Context, Limits, Template, Value};
use std::sync::mpsc;
use std::time::Duration;

let budget = Duration::from_millis(500);
let (sender, receiver) = mpsc::channel();
std::thread::spawn(move || {
    let mut tmpl = Template::with_limits(Limits::default().with_deadline(budget));
    tmpl.parse("{{ range 100 }}x{{ end }}").unwrap();
    // The receiver may be gone by now; that is the abandoned case.
    let _ = sender.send(tmpl.render(&Context::empty()));
});
let rendered = receiver.recv_timeout(budget);
// Timed out? Control is back here immediately. The thread finishes its one
// in-flight call and exits on the deadline — measured against the worst case in
// the registry (`htpasswd`, one ~165 ms bcrypt hash), it outlived the caller's
// timeout by 159 ms, burning its core only that long.
```

### Recursion is not a knob

`{{template}}` nesting and nested actions are capped together at 100 levels, measured
against the smallest stack a caller realistically has. `Limits::unlimited()` does not lift
it: there is no value above it that is safe, so offering the choice would only offer a way
to abort the process.

A custom function can read the limits in force with `gtmpl_ng::limits::active()`. Do that
if your function turns a number from the template into an allocation, or
`{{ myFunc 9999999999 }}` becomes a way around all of the above.

## Feature flags

| flag | what it adds |
|---|---|
| *(none)* | the engine and the built-in functions Go has: `printf`, `len`, `index`, `eq`, `and`, `call`, … |
| `helm-functions` | 150 Helm-compatible functions — `upper`, `toYaml`, `b64enc`, `sha256sum`, `trunc`, `default`, … |
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
  render, as the example does. A field may hold any type that is `Into<Value>`,
  including a `Vec` or `HashMap` of another derived struct, and none of them have to be
  `Clone`.
- **A `Func` field is a method with no arguments.** It receives the struct it lives on
  as its only argument; a function that needs arguments is registered with `add_func`
  instead.
- **Map keys are iterated in sorted order**, so the same input always renders the same
  bytes.
- **A render is bounded by default**, so a template cannot exhaust memory or run
  forever. The numbers are in [Rendering templates you did not
  write](#rendering-templates-you-did-not-write), and they are adjustable.
- **Comparison is permissive.** `{{ eq 3.0 3 }}` answers `true` and `{{ eq "a" 1 }}`
  answers `false` where Go refuses with "incompatible types for comparison". The
  engine README lists every deliberate difference from Go.

## Where to read more

- **[The engine's README](gtmpl-ng/README.md)** — the template semantics in full, the
  dynamic-template syntax, and the full list of deliberate differences from Go.
- **[`go_conformance.txt`](gtmpl-ng/tests/go_conformance.txt)** — 442 templates diffed
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
