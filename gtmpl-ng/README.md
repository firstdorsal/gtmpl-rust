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
gtmpl-ng = "1"
# Only for `#[derive(Gtmpl)]`, which generates the conversion for a struct.
gtmpl_derive_ng = "1"
gtmpl_value_ng = "1"
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
-   **A render is bounded, and the bounds are adjustable.** A template can be untrusted
    input, and four one-line templates used to end the process outright -- a recursive
    `{{ template }}` overflowed the stack, `{{ range 1000000000 }}` ran until it was
    killed. Go recovers a panicking template function into a template error; this engine
    returns `Result`, and an overflowed stack or a failed allocation aborts rather than
    returning, so a request that would end the process has to be refused rather than
    attempted. By default one `{{ range }}` (and one `until`, `untilStep` or `seq`) runs
    at most 10 000 iterations, a whole render 1 000 000, output stops at 64 MiB, and one
    `repeat`, `indent` or `rand*` produces at most 1 000 000 characters -- the same line
    `printf` holds on a width or precision. Pass your own through
    `Template::with_limits`, or `Limits::unlimited()` to switch them off; see
    [`limits`](https://docs.rs/gtmpl-ng/latest/gtmpl_ng/limits/) for what is bounded and
    what is not. Nesting depth is capped at 100 and is not adjustable, because no larger
    value is safe on a 2 MiB stack.
-   **A render can be given a deadline**, and it is the one limit that is off by
    default: the counters above all bound something the template asked for, so the same
    template answers the same way on any machine, where a deadline would make a render
    fail on a loaded host and pass on an idle one. It is what covers the gap the
    counters cannot -- a template inside every budget can still be written to be slow.
    Set `Limits::deadline`; the clock starts at each `render`, it is checked before
    every function call and on every 64th iteration, and it stops a render within a
    fraction of a millisecond for no measurable cost. It cannot interrupt a single call:
    one `genCA` runs to its end and the deadline stops the next one. A custom function
    that works in pieces can read `limits::remaining()`.
-   **`slice`** is Go's builtin: `slice x i j` is the half-open range, `slice x i` runs
    to the end, and `slice x` is `x`. It takes a string or a list. Go indexes a string
    by byte and will happily return invalid UTF-8 -- `slice "héllo" 1 2` is the single
    byte `\xc3` there -- which a Rust `String` cannot hold, so an index that falls
    inside a character is refused instead. **With `helm-functions` enabled, Helm's
    `slice` replaces it**, taking a list and refusing a string, exactly as Sprig's
    replaces Go's builtin in Helm itself.
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

## Where this engine deliberately differs from Go

These are decisions, not gaps: each one is pinned by cases in
`tests/go_conformance.txt`, whose header carries the full reasoning, and changing any
of them is a breaking change.

-   **Comparison is permissive, Sprig-style.** Values of different kinds compare
    instead of failing: `{{ eq 3.0 3 }}` and `{{ lt 5 3.5 }}` answer, `{{ eq "a" 1 }}`
    is `false`, and `{{ lt true false }}` orders booleans -- where Go refuses each with
    "incompatible types for comparison" or "invalid type for comparison".
-   **A whole float is stored as an integer**, which is what makes the above work and
    `{{ 3.0 }}` render `3` the way Go does. In exchange a float verb on an integer
    cannot be refused -- `{{ printf "%f" 3 }}` gives `3.000000` where Go gives
    `%!f(int=3)` -- a whole float never switches to the exponent notation Go uses from
    1e6 up, and `{{ range 3.0 }}` iterates where Go refuses to iterate a float.
-   **An integer literal above `i64` is read as a `u64`** and printed, where Go's
    parser refuses it with "overflows int".
-   **Rendered with no data at all, `{{ .field }}` is an error** rather than Go's
    `<no value>`: a field read on nothing is a mistake in the template, not missing
    data. A missing key on a real map still renders `<no value>`.
-   **A render is bounded by default** -- iterations, output size, function output,
    nesting -- where Go renders without limits. It is the one difference a caller can
    switch off, with `Limits::unlimited()`. See `src/limits.rs`.
-   **Printability follows this engine's Unicode tables, not Go's.** What `js` passes
    through raw and what `%q` leaves unescaped is decided by `strconv.IsPrint`'s
    definition applied over the Unicode version the `unicode-general-category`
    dependency ships, where Go applies it over its own (15.0.0 in Go 1.26). Runes
    assigned between the two versions render raw here and escaped there; every
    metacharacter either escaper exists for is escaped by both.
-   **Some `printf` verbs differ from Go's `fmt`**, all recorded in
    `tests/go_conformance.txt`: `%b` and `%x` on a float (Go writes a power-of-two
    mantissa and hexadecimal float notation), `%#v` (renders as plain `%v`), `%#g`
    (does not restore the trailing zeros Go restores), and `%p` and Go's struct-field
    `%+v`, which have no counterpart here because there are no addresses or struct
    types to show.
-   **Complex numbers do not exist here.** Go's parser accepts `2i` because Go the
    language has the type; a configuration template has no use for one, and `Value`
    has no variant to hold it.
-   **Declaring a variable from a large value (`{{ $top := . }}`) copies it**; field
    reads through the variable do not. A cost difference rather than a behavioural
    one.

## Enhancements

Even though it was never intended to extend the syntax of Golang text/template
there might be some convenient additions:

### Helm Template Functions

Enable `helm-functions` to get 150 Helm-compatible template functions:

```toml
[dependencies.gtmpl-ng]
version = "1"
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
version = "1"
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
version = "1"
features = ["all-functions"]
```

```rust
use gtmpl_ng::all_functions::all_functions;

fn main() {
    let funcs = all_functions(); // Vec of all 153 functions
}
```

### Dynamic Template

Enable `gtmpl_dynamic_template` in your `Cargo.toml`:

```toml
[dependencies.gtmpl-ng]
version = "1"
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
