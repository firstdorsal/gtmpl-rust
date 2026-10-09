//! `gtmpl_value_ng` holds the value type used by [`gtmpl-ng`][gtmpl-ng], the Golang
//! templating language for Rust. A [`Value`] is what a template parses its literals
//! into and what a context hands it back.
//!
//! The [`From`] trait is implemented for:
//!
//! * `String`, `&str`, `Cow<str>`
//! * most numeric types `u64, u32, …, i64, i32, …, f64, f32`
//! * `bool`
//! * `Vec<Value>`, `&[Value]`
//! * `HashMap<String, Value>`
//! * `Option<T>`, where `None` becomes [`Value::NoValue`]
//!
//! [`gtmpl_derive_ng`][gtmpl_derive_ng] derives it for a struct.
//!
//! # Examples
//!
//! ```rust
//! use gtmpl_value_ng::Value;
//!
//! let v: Value = "something".into();
//! assert_eq!(v.to_string(), "something");
//! ```
//!
//! # Relation to `gtmpl_value`
//!
//! This crate is a fork of [`gtmpl_value`][gtmpl_value] by Florian Dieminger, taken
//! from its `master` branch at `a333139` — which is two years ahead of the last
//! release on crates.io and carries the fix for negative non-integer numbers that
//! `0.5.1` truncates. It lives in the same workspace as the engine so that the value
//! type and the engine are released together.
//!
//! With the `gtmpl_value_compat` feature, [`Value`] gains a [`TryFrom`] conversion from
//! `gtmpl_value::Value`, so a tree that still derives against the published crate can
//! be migrated one call site at a time.
//!
//! [gtmpl-ng]: https://github.com/firstdorsal/gtmpl-rust
//! [gtmpl_derive_ng]: https://docs.rs/gtmpl_derive_ng
//! [gtmpl_value]: https://github.com/fiji-flo/gtmpl_value

#[cfg(feature = "gtmpl_value_compat")]
mod compat;
mod from;
mod go_fmt;
mod number;
mod value;

#[cfg(feature = "gtmpl_value_compat")]
pub use crate::compat::FromLegacyValueError;
pub use crate::from::*;
pub use crate::go_fmt::{format_float, write_float, FloatFormat};
pub use crate::value::*;

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_function_cmp() {
        fn f(a: &[Value]) -> Result<Value, FuncError> {
            Ok(a[0].clone())
        }
        let f1 = Function { f };
        let f2 = Function { f };
        assert_eq!(f1, f2);
    }
}
