# gtmpl_derive_ng

[![Latest Version]][crates.io]

[Latest Version]: https://img.shields.io/crates/v/gtmpl_derive_ng.svg
[crates.io]: https://crates.io/crates/gtmpl_derive_ng

---

`#[derive(Gtmpl)]` for [gtmpl_value_ng]: it generates the `From<YourStruct> for Value`
implementation that turns a struct into a template context.

```toml
[dependencies]
gtmpl_derive_ng = "0.9"
gtmpl_value_ng = "0.9"
```

-   [gtmpl_derive_ng at crates.io](https://crates.io/crates/gtmpl_derive_ng)
-   [gtmpl_derive_ng documentation](https://docs.rs/crate/gtmpl_derive_ng)

```rust
use gtmpl_derive_ng::Gtmpl;
use gtmpl_value_ng::Value;

#[derive(Gtmpl)]
struct Foo {
    bar: u8,
}

let v: Value = Foo { bar: 23 }.into();
```

Every named field is converted with `Into<Value>`, so a field type only has to
implement that. The generated code names `::gtmpl_value_ng::Value` by absolute path,
which is why `gtmpl_value_ng` has to be a dependency of your crate as well.

Deriving on an enum or a tuple struct is rejected at compile time.

## Relation to `gtmpl_derive`

This is a fork of [gtmpl_derive] by Florian Dieminger. It emits
`::gtmpl_value_ng::Value` instead of `::gtmpl_value::Value`, which is the one thing
that cannot be smoothed over when migrating: `Into` is not transitive, so a value
produced by the original derive will not satisfy a bound asking for the new type.
`gtmpl_value_ng`'s `gtmpl_value_compat` feature bridges the two if you want to migrate
gradually.

[gtmpl_value_ng]: https://crates.io/crates/gtmpl_value_ng
[gtmpl_derive]: https://github.com/fiji-flo/gtmpl_derive
