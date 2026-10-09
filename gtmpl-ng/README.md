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
```

and look at the docs:

-   [gtmpl-ng at crates.io](https://crates.io/crates/gtmpl-ng)
-   [gtmpl-ng documentation](https://docs.rs/crate/gtmpl-ng)
-   [golang documentation](https://golang.org/pkg/text/template/)

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
use gtmpl_value_ng::Function;
use gtmpl_ng::{FuncError, gtmpl_fn, template, Value};

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

use gtmpl_derive_ng::Gtmpl;
use gtmpl_ng::{Func, FuncError, Value};

fn plus_one(args: &[Value]) -> Result<Value, FuncError> {
    if let Value::Object(ref o) = &args[0] {
        if let Some(Value::Number(ref n)) = o.get("num") {
            if let Some(i) = n.as_i64() {
                return Ok((i +1).into())
            }
        }
    }
    Err(anyhow!("integer required, got: {:?}", args))
}

#[derive(Gtmpl)]
struct AddMe {
    num: u8,
    plus_one: Func
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
-   **A missing map key ends a field chain** and renders as `<no value>`, however deep
    the chain is, so `{{ if .a.b.c }}` is simply false when any level is absent. A
    receiver of the wrong type still errors.
-   **Number literals** follow Go's integer syntax: `0x1f`, `0b1010`, `0o17`, a bare
    leading zero for octal (`017`), and `_` separators (`1_000`).
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
-   the following functions have not been implemented:
    -   `html`, `js`, `slice`
-   a float verb on an integer is accepted where Go refuses it: `{{ printf "%f" 3 }}`
    gives `3.000000` rather than Go's `%!f(int=3)`. A whole float is stored as an
    integer so `{{ 3.0 }}` renders `3` the way Go does, which leaves the two
    indistinguishable; refusing the float case would be the worse trade
-   an integer literal above `i64` is accepted and printed where Go's parser refuses it
-   rendered with no data at all, `{{ .field }}` is an error rather than Go's
    `<no value>`
-   declaring a variable from a large value (`{{ $top := . }}`) copies it; field reads
    through the variable do not

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
            r#"
            {{- define "tmpl1"}} some {{ end -}}
            {{- define "tmpl2"}} some other {{ end -}}
            there is {{- template (.) -}} template
            "#,
        )
        .unwrap();

    let context = Context::from("tmpl2");

    let output = template.render(&context);
    assert_eq!(output.unwrap(), "there is some other template".to_string());
}
```

The following syntax is used:

```
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
