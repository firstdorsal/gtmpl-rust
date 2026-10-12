# gtmpl_value_ng

[![Latest Version]][crates.io]

[Latest Version]: https://img.shields.io/crates/v/gtmpl_value_ng.svg
[crates.io]: https://crates.io/crates/gtmpl_value_ng

---

The value type used by [gtmpl-ng], the Golang templating language for Rust. A `Value`
is what a template parses its literals into and what a context hands it back.

```toml
[dependencies]
gtmpl_value_ng = "0.9"
```

Builds on Rust 1.85 and newer, with either feature below enabled or both, which CI
checks on that exact toolchain.

-   [gtmpl_value_ng at crates.io](https://crates.io/crates/gtmpl_value_ng)
-   [gtmpl_value_ng documentation](https://docs.rs/crate/gtmpl_value_ng)

```rust
use gtmpl_value_ng::Value;

let v: Value = "something".into();
assert_eq!(v.to_string(), "something");
```

[gtmpl_derive_ng] derives the `From` implementation for a struct.

## Relation to `gtmpl_value`

This is a fork of [gtmpl_value] by Florian Dieminger, taken from its `master` branch at
`a333139` — two years ahead of the last crates.io release and carrying the fix for
negative non-integer numbers that `0.5.1` truncates.

`Number::float`, `Number::signed` and `Number::unsigned` are new: they build a number
without the collapse that `From<f64>` applies, which turns a whole float into an
integer so `{{ 3.0 }}` renders `3` the way Go does. That collapse is lossy, and the
original crate had no way around it.

### Migrating from `gtmpl_value`

Enable the `gtmpl_value_compat` feature and `Value` gains a `TryFrom` conversion from
`gtmpl_value::Value`, so a tree that still uses the original `#[derive(Gtmpl)]` can be
migrated one call site at a time:

```toml
gtmpl_value_ng = { version = "0.9", features = ["gtmpl_value_compat"] }
```

```rust
use gtmpl_value_ng::Value;

#[derive(gtmpl_derive::Gtmpl)]
struct Config {
    name: String,
}

let legacy: gtmpl_value::Value = Config { name: "web".to_owned() }.into();
let value = Value::try_from(legacy).expect("a plain struct converts");
assert_eq!(value, Value::Object(
    [("name".to_owned(), Value::from("web"))].into_iter().collect(),
));
```

It is `TryFrom` rather than `From` because a `gtmpl_value::Func` names the other
crate's `Value` in its own signature and cannot be rewritten — register such a function
with the template instead of passing it through the context. And a negative
non-integer arrives already truncated: `gtmpl_value` destroyed the fraction when the
value was built, and the conversion carries that over rather than inventing one back.

## Features

| feature | effect |
|---|---|
| `serde` | `Serialize`/`Deserialize` for `Value` and `Number` |
| `gtmpl_value_compat` | the `TryFrom<gtmpl_value::Value>` conversion above |

Two things a document cannot carry, so `serde` does not pretend to: `Value::Object`
writes as a map and reads back as `Value::Map`, because provenance is not part of the
data; and `Value::NoValue` writes as `null` and reads back as `Value::Nil`, because
absence is not a value. A `Value::Function` is refused outright rather than written as
something it is not.

[gtmpl-ng]: https://github.com/firstdorsal/gtmpl-rust
[gtmpl_derive_ng]: https://crates.io/crates/gtmpl_derive_ng
[gtmpl_value]: https://github.com/fiji-flo/gtmpl_value
