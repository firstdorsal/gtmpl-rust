//! Comparing numbers of different kinds through the builtin comparison functions.
//!
//! `lt`, `gt`, `ge` and friends are builtins, so none of this needs an optional
//! feature. These cases used to live in `panic_regressions.rs`, where the helm
//! function registry gated them out of a default `cargo test` run for no reason.

mod common;

use gtmpl_ng::Value;
use std::collections::HashMap;

/// Previously panicked in `value_is_truthy` with `as_f64().unwrap()` on an integer:
/// `Number::as_f64` answers only for the float variant.
#[test]
fn an_integer_compares_against_a_float_literal() {
    assert_eq!(
        common::render(
            r#"{{ lt 5 3.5 }} {{ lt 5 10.5 }} {{ gt 5 3.5 }} {{ ge 5 5.0 }}"#,
            Value::Nil
        ),
        Ok("false true true true".to_owned())
    );
}

#[test]
fn an_integer_from_the_context_compares_against_a_float_literal() {
    let data: HashMap<String, Value> = [("n".to_owned(), Value::from(5i64))].into_iter().collect();
    assert_eq!(
        common::render(r#"{{ lt .n 10.5 }}"#, Value::from(data)),
        Ok("true".to_owned())
    );
}
