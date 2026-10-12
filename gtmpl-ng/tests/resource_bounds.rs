//! A template must not be able to exhaust memory through a function that turns a
//! number into an allocation size.
//!
//! Go recovers a panic inside a template function and reports it as a template error,
//! so in Helm these are ugly but contained. This engine returns `Result`, so a function
//! that panics or runs the allocator out escapes the type the API promises: every case
//! below aborted the process with a core dump before it was bounded, from a template
//! literal alone and with no context data involved.
//!
//! The companion to `printf.rs`'s width and precision cap, which is the same bug in
//! the formatter.

#![cfg(feature = "all-functions")]

use gtmpl_ng::all_functions::all_functions;
use gtmpl_ng::{Context, Template, Value};

/// Renders with every function registered, returning the error as a string.
fn render(source: &str) -> Result<String, String> {
    let mut template = Template::default();
    template.add_funcs(&all_functions());
    template.parse(source).map_err(|e| format!("parse: {e}"))?;
    template
        .render(&Context::from(Value::Nil))
        .map_err(|e| e.to_string())
}

/// Each of these took the process down. `9999999999` is past `u32` but inside `i64`,
/// which is what a template literal can carry -- the parser refuses anything larger,
/// so this is the worst a template can ask for.
#[test]
fn a_huge_count_is_refused_rather_than_allocated() {
    let cases = [
        r#"{{ repeat 9999999999 "x" }}"#,
        "{{ until 9999999999 }}",
        "{{ seq 9999999999 }}",
        "{{ seq 1 9999999999 }}",
        "{{ untilStep 0 9999999999 1 }}",
        "{{ randAscii 9999999999 }}",
        "{{ randAlpha 9999999999 }}",
        "{{ randAlphaNum 9999999999 }}",
        "{{ randNumeric 9999999999 }}",
        r#"{{ indent 9999999999 "x" }}"#,
    ];
    for source in cases {
        let error = render(source).expect_err(source);
        assert!(
            error.contains("this allows"),
            "{source} failed for another reason: {error}"
        );
    }
}

/// `repeat` multiplies, so the count alone is not the bound: a hundred thousand
/// repeats of a hundred thousand characters is ten gigabytes from two small numbers.
#[test]
fn repeat_bounds_the_product_and_not_the_count() {
    let big = render(r#"{{ repeat 100000 (repeat 100000 "x") }}"#)
        .expect_err("the inner repeat alone is already at the limit");
    assert!(big.contains("this allows"), "{big}");

    // The inner call is exactly at the limit, so it is the outer one that refuses.
    let inner = render(r#"{{ len (repeat 1000000 "x") }}"#).expect("a million is allowed");
    assert_eq!(inner, "1000000");
}

/// The limit has to leave ordinary charts alone -- these are the shapes a real
/// template uses.
#[test]
fn ordinary_counts_still_work() {
    assert_eq!(render(r#"{{ repeat 3 "ab" }}"#).unwrap(), "ababab");
    assert_eq!(render("{{ until 4 }}").unwrap(), "[0 1 2 3]");
    assert_eq!(render("{{ seq 1 4 }}").unwrap(), "[1 2 3 4]");
    assert_eq!(render("{{ untilStep 3 6 2 }}").unwrap(), "[3 5]");
    assert_eq!(render("{{ len (randAlpha 24) }}").unwrap(), "24");
    assert_eq!(render(r#"{{ indent 4 "x" }}"#).unwrap(), "    x");
    assert_eq!(
        render(r#"{{ mowsRandomString "Aa" 64 }}"#).unwrap().len(),
        64
    );
}

/// The engine itself has no counterpart to bound: `{{range}}` over an integer is Go's
/// own construct and Go does not cap it either, but the parser refuses a literal above
/// `i64`, and the loop body is what costs -- an empty one simply counts.
#[test]
fn ranging_over_a_large_integer_is_not_a_function_bound() {
    let error = render("{{ range 18446744073709551615 }}x{{ end }}")
        .expect_err("above i64, which Go reports as an overflow");
    assert!(error.contains("overflows int"), "{error}");
}
