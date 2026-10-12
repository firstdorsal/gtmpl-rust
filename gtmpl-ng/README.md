# gtmpl-ng – Golang Templates for Rust

[![Latest Version]][crates.io]

[Latest Version]: https://img.shields.io/crates/v/gtmpl-ng.svg
[crates.io]: https://crates.io/crates/gtmpl-ng

---

This is a fork of [gtmpl-rust] with some additional features and fixes.

[gtmpl-ng] provides the [Golang text/template] engine for Rust. This enables
seamless integration of Rust application into the world of devops tools around
[kubernetes], [docker] and whatnot.

## Getting Started

Add the following dependency to your Cargo manifest…

```toml
[dependencies]
gtmpl-ng = "0.9"
# Only for `#[derive(Gtmpl)]`, which generates the conversion for a struct.
gtmpl_derive_ng = "0.9"
gtmpl_value_ng = "0.9"
```

`gtmpl_value_ng`'s whole surface is re-exported here -- `Value`, `Number`,
`NumberKind`, `Function`, `Func`, `FuncError`, `FromValue`, `from_value`,
`format_float` -- so only the derive needs naming separately.

and look at the docs:

-   [gtmpl-ng at crates.io](https://crates.io/crates/gtmpl-ng)
-   [gtmpl-ng documentation](https://docs.rs/crate/gtmpl-ng)
-   [golang documentation](https://golang.org/pkg/text/template/)

All three crates build on Rust 1.85 and newer, which CI checks on that exact
toolchain. The `helm-functions` feature is the one exception; see below.

It's not perfect, yet. Help and feedback is more than welcome.

## Some Examples

Basic template:

```rust
use gtmpl_ng as gtmpl;

fn main() {
    let output = gtmpl::template("Finally! Some {{ . }} for Rust", "gtmpl");
    assert_eq!(&output.unwrap(), "Finally! Some gtmpl for Rust");
}
```

Adding custom functions:

```rust
use gtmpl_ng::{gtmpl_fn, template, FuncError, Function, Value};

fn main() {
    gtmpl_fn!(
    fn add(a: u64, b: u64) -> Result<u64, FuncError> {
        Ok(a + b)
    });
    let equal = template(r#"{{ call . 1 2 }}"#, Value::Function(Function { f: add }));
    assert_eq!(&equal.unwrap(), "3");
}
```

Passing a struct as context:

```rust
use gtmpl_derive_ng::Gtmpl;

#[derive(Gtmpl)]
struct Foo {
    bar: u8
}

fn main() {
    let foo = Foo { bar: 42 };
    let output = gtmpl_ng::template("The answer is: {{ .bar }}", foo);
    assert_eq!(&output.unwrap(), "The answer is: 42");
}
```

Invoking a _method_ on a context:

```rust
use anyhow::anyhow;
use gtmpl_derive_ng::Gtmpl;
use gtmpl_ng::{Func, FuncError, Value};

fn plus_one(args: &[Value]) -> Result<Value, FuncError> {
    if let Value::Object(ref o) = &args[0] {
        if let Some(Value::Number(ref n)) = o.get("num") {
            if let Some(i) = n.as_i64() {
                return Ok((i + 1).into());
            }
        }
    }
    Err(anyhow!("integer required, got: {:?}", args).into())
}

#[derive(Gtmpl)]
struct AddMe {
    num: u8,
    plus_one: Func,
}

fn main() {
    let add_me = AddMe { num: 42, plus_one };
    let output = gtmpl_ng::template("The answer is: {{ .plus_one }}", add_me);
    assert_eq!(&output.unwrap(), "The answer is: 43");
}
```

## Template Semantics

Behaviour follows Go's `text/template`:

-   **Variables** are declared with `:=` and reassigned with `=`. A declaration is
    scoped to its block, an assignment updates the nearest existing binding, so a
    loop can accumulate into a variable declared outside it:

    ```text
    {{ $acc := "seed" }}{{ range . }}{{ $acc = printf "%s-%d" $acc . }}{{ end }}{{ $acc }}
    ```

-   **Map keys are iterated in sorted order**, which makes rendering deterministic.
-   **`{{ else }}` on a `range`** runs only when the collection is empty, and ranging
    over nil takes that branch rather than failing.
-   **`{{ break }}` and `{{ continue }}`** work inside a `{{ range }}`, as Go has since
    1.18. They are lexically scoped the way Go scopes them: one that no `{{ range }}`
    encloses is a parse error, and so is one in a `{{ define }}` or `{{ block }}` body
    called from a range, or in the range's own `{{ else }}` branch. An `{{ if }}` or
    `{{ with }}` in between is not a barrier.
-   **`{{ range }}` over an integer** counts from zero, as Go has since 1.22:
    `{{ range 3 }}{{ . }}{{ end }}` is `012`, and `{{ range $i := 3 }}` binds `$i` to
    each index. A negative count is empty, so it takes the `{{ else }}` branch, and the
    two-variable form is an error -- there is no value to pair an index with.
-   **A missing map key ends a field chain** and renders as `<no value>`, however deep
    the chain is, so `{{ if .a.b.c }}` is simply false when any level is absent. A
    receiver of the wrong type still errors.
-   **Number literals** follow Go's integer syntax: `0x1f`, `0b1010`, `0o17`, a bare
    leading zero for octal (`017`), and `_` separators (`1_000`) -- and Go's
    hexadecimal float syntax, which carries a binary exponent, so `0x1.8p1` is `3`.
-   **An action may span lines**, and whitespace around a trim marker may be a tab or
    a newline as well as a space.

## Running the Tests

Use `--all-features`:

```sh
cargo test --all-features
```

`panic_audit.rs` and `panic_regressions.rs` run through the optional helm function
registry, so a plain `cargo test` leaves them out. They are reported as ignored rather
than silently skipped (see `tests/feature_gates.rs`); run the whole suite with
`cargo test --workspace --all-features`, or the `cargo test-all` alias.

## Current Limitations

This is work in progress. Currently the following features are not supported:

-   complex numbers
-   `html` and `js` have not been implemented
-   Go's builtin `slice` is not in the default function set. `helm-functions` provides
    a `slice`, with Helm's argument semantics rather than Go's
-   numbers of different kinds compare permissively, Sprig-style rather than Go-style:
    `{{ eq 3.0 3 }}` and `{{ lt 5 3.5 }}` answer, where Go refuses with "incompatible
    types for comparison". A whole float is stored as an integer, which is what makes
    that work -- and it also means a float verb on an integer cannot be refused, so
    `{{ printf "%f" 3 }}` gives `3.000000` where Go gives `%!f(int=3)`, and
    `{{ range 3.0 }}` iterates where Go refuses to iterate a float
-   an integer literal above `i64` is accepted and printed where Go's parser refuses it
-   rendered with no data at all, `{{ .field }}` is an error rather than Go's
    `<no value>`
-   declaring a variable from a large value (`{{ $top := . }}`) copies it; field reads
    through the variable do not
-   some `printf` verbs differ from Go's `fmt`, all recorded in
    `tests/go_conformance.txt`:
    -   `%b` and `%x` on a float are not implemented; Go writes a power-of-two mantissa
        and hexadecimal float notation
    -   `%#v` renders as plain `%v` rather than Go's syntax representation
    -   `%#g` does not restore the trailing zeros Go restores
    -   `%p` and Go's struct-field `%+v` have no counterpart here

## Enhancements

Even though it was never intended to extend the syntax of Golang text/template
there might be some convenient additions:

### Helm Template Functions

Enable `helm-functions` to get 152 Helm-compatible template functions:

```toml
[dependencies.gtmpl-ng]
version = "0.9"
features = ["helm-functions"]
```

This is the one feature that needs a newer compiler than the crate's declared
`rust-version`: it pulls `uuid` and `time`, which set their own, higher minimum. Cargo
refuses the build with the version it wants and the crate that wants it, so there is
nothing to guess -- and a feature cannot carry its own `rust-version`, which is why the
manifest cannot say this for you.

```rust
use gtmpl_ng::{Template, Context};
use gtmpl_ng::helm_functions::HELM_FUNCTIONS;

fn main() {
    let mut tmpl = Template::default();
    tmpl.add_funcs(&HELM_FUNCTIONS);
    tmpl.parse(r#"{{ "hello" | upper }}"#).unwrap();
    let output = tmpl.render(&Context::empty()).unwrap();
    assert_eq!(output, "HELLO");
}
```

### Mows Template Functions

Enable `mows-functions` to get 3 additional mows-specific functions:

```toml
[dependencies.gtmpl-ng]
version = "0.9"
features = ["mows-functions"]
```

```rust
use gtmpl_ng::mows_functions::MOWS_FUNCTIONS;
// mowsRandomString, mowsDigest, mowsJoinDomain
```

### All Functions

Enable `all-functions` to get both Helm and mows functions:

```toml
[dependencies.gtmpl-ng]
version = "0.9"
features = ["all-functions"]
```

```rust
use gtmpl_ng::all_functions::all_functions;

fn main() {
    let funcs = all_functions(); // Vec of all 155 functions
}
```

### Dynamic Template

Enable `gtmpl_dynamic_template` in your `Cargo.toml`:

```toml
[dependencies.gtmpl-ng]
version = "0.9"
features = ["gtmpl_dynamic_template"]
```

Now you can have dynamic template names for the `template` action.

#### Example

```rust
use gtmpl_ng::{Context, Template};

fn main() {
    let mut template = Template::default();
    template
        .parse(
            r#"{{- define "tmpl1"}} some {{ end -}}
{{- define "tmpl2"}} some other {{ end -}}
there is {{- template (.) -}} template"#,
        )
        .unwrap();

    let context = Context::from("tmpl2");

    let output = template.render(&context);
    assert_eq!(output.unwrap(), "there is some other template");
}
```

The following syntax is used:

```text
{{template (pipeline)}}
	The template with the name evaluated from the pipeline (parenthesized) is
    executed with nil data.

{{template (pipeline) pipeline}}
	The template with the name evaluated from the first pipeline (parenthesized)
    is executed with dot set to the value of the second pipeline.
```

## Context

[gtmpl_value_ng]'s `Value` is the internal data type, re-exported here as
`gtmpl_ng::Value`. [gtmpl_derive_ng] provides the `derive` macro that generates the
`From` implementation for it. Both live in this repository and are released with the
engine.

See:

-   [gtmpl_value_ng at crates.io](https://crates.io/crates/gtmpl_value_ng)
-   [gtmpl_value_ng documentation](https://docs.rs/crate/gtmpl_value_ng)
-   [gtmpl_derive_ng at crates.io](https://crates.io/crates/gtmpl_derive_ng)
-   [gtmpl_derive_ng documentation](https://docs.rs/crate/gtmpl_derive_ng)

Coming from the published `gtmpl_value`/`gtmpl_derive`? `Into` is not transitive, so a
value from the original derive will not satisfy a bound asking for the new type.
Either switch the derive import, or enable the `gtmpl_value_compat` feature and
convert with `Value::try_from`.

## Why do we need this?

Why? Dear god, why? I can already imagine the question coming up why anyone would
ever do this. I wasn't a big fan of Golang templates when i first had to write
some custom formatting strings for **docker**. Learning a new template language
usually isn't something one is looking forward to. Most people avoid it
completely. However, it's really useful for automation if you're looking for
something more lightweight than a full blown DSL.

The main motivation for this is to make it easier to write devops tools in Rust
that feel native. [docker] and [helm] ([kubernetes]) use golang templates and
it feels more native if tooling around them uses the same.

[gtmpl-rust]: https://github.com/fiji-flo/gtmpl-rust
[gtmpl-ng]: https://github.com/firstdorsal/gtmpl-rust
[Golang text/template]: https://golang.org/pkg/text/template/
[kubernetes]: https://kubernetes.io
[helm]: https://github.com/kubernetes/helm/blob/master/docs/chart_best_practices/templates.md
[docker]: https://docker.com
[gtmpl_value_ng]: https://crates.io/crates/gtmpl_value_ng
[gtmpl_derive_ng]: https://crates.io/crates/gtmpl_derive_ng
