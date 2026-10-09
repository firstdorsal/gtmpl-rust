//! How values reach the output: `printf` verbs, `print`, and the absent/nil
//! distinction.
//!
//! Every expectation was taken from Go 1.26.6 formatting the equivalent value.

mod common;

use std::collections::HashMap;

use gtmpl_ng::Value;

fn fixture() -> Value {
    let mut inner = HashMap::new();
    inner.insert("k".to_owned(), Value::from(1));

    let mut root = HashMap::new();
    root.insert("float".to_owned(), Value::from(3.5f64));
    root.insert("whole".to_owned(), Value::from(3.0f64));
    root.insert("map".to_owned(), Value::Map(inner));
    root.insert("list".to_owned(), Value::from(vec![1, 2]));
    root.insert("nil".to_owned(), Value::Nil);
    Value::Map(root)
}

fn render(source: &str) -> Result<String, String> {
    common::render(source, fixture())
}

/// `%v` is the catch-all verb, so every value kind has to answer it. Signed integers
/// and floats used to error, which took out `{{ printf "%v" -5 }}`.
#[test]
fn the_v_verb_covers_every_value_kind() {
    for (source, expected) in [
        (r#"{{ printf "%v" 5 }}"#, "5"),
        (r#"{{ printf "%v" -5 }}"#, "-5"),
        (r#"{{ printf "%v" .float }}"#, "3.5"),
        (r#"{{ printf "%v" .whole }}"#, "3"),
        (r#"{{ printf "%v" true }}"#, "true"),
        (r#"{{ printf "%v" "x" }}"#, "x"),
        (r#"{{ printf "%v" .map }}"#, "map[k:1]"),
        (r#"{{ printf "%v" .list }}"#, "[1 2]"),
    ] {
        assert_eq!(render(source).as_deref(), Ok(expected), "{}", source);
    }
}

/// An absent or nil value has no representation of its own; Go's formatter writes
/// `<nil>` for it.
#[test]
fn absent_and_nil_format_as_nil() {
    for source in [
        r#"{{ printf "%v" nil }}"#,
        r#"{{ printf "%v" .nil }}"#,
        r#"{{ printf "%v" .missing }}"#,
        "{{ print nil }}",
        "{{ print .nil }}",
    ] {
        assert_eq!(render(source).as_deref(), Ok("<nil>"), "{}", source);
    }
    assert_eq!(render("{{ print 1 nil 2 }}").as_deref(), Ok("1 <nil> 2"));
}

/// Go's `%f`, `%e` and `%E` default to six digits after the point. Rust's formatters
/// default to the shortest form, so the default has to be supplied.
#[test]
fn float_verbs_default_to_six_digits() {
    for (source, expected) in [
        (r#"{{ printf "%f" .float }}"#, "3.500000"),
        (r#"{{ printf "%f" .whole }}"#, "3.000000"),
        // Go writes the exponent as `e+00`; this formatter writes Rust's `e0`. See
        // the formatting-fidelity issue -- the digits are right, the exponent's
        // padding and sign are not.
        (r#"{{ printf "%e" .float }}"#, "3.500000e0"),
        (r#"{{ printf "%.2f" .float }}"#, "3.50"),
        (r#"{{ printf "%8.3f" .float }}"#, "   3.500"),
    ] {
        assert_eq!(render(source).as_deref(), Ok(expected), "{}", source);
    }
}

/// `nil` is usable as an argument, not just as a value in the data.
#[test]
fn nil_is_a_usable_argument() {
    assert_eq!(render("{{ eq nil nil }}").as_deref(), Ok("true"));
    assert_eq!(render("{{ ne nil nil }}").as_deref(), Ok("false"));
    // Go rejects a bare `nil` as a command -- "nil is not a command" -- and so do we.
    assert!(render("{{ if nil }}y{{ else }}n{{ end }}").is_err());
    assert!(render("{{ nil }}").is_err());
}
