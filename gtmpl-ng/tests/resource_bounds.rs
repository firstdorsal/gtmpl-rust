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
use gtmpl_ng::{Context, Limits, Template, Value};

/// Renders with every function registered, returning the error as a string.
fn render(source: &str) -> Result<String, String> {
    render_with(source, Limits::default())
}

fn render_with(source: &str, limits: Limits) -> Result<String, String> {
    let mut template = Template::with_limits(limits);
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

/// A count above `i64` is refused by the number itself, before any limit is consulted,
/// and Go reports the same thing.
#[test]
fn a_count_above_an_integer_is_refused_as_an_overflow() {
    let error = render("{{ range 18446744073709551615 }}x{{ end }}")
        .expect_err("above i64, which Go reports as an overflow");
    assert!(error.contains("overflows int"), "{error}");
}

/// The list-generating functions are ranges spelled as functions -- `{{ until 10000 }}`
/// and `{{ range 10000 }}` cost the same -- so they answer to the same limit, and
/// raising it for one raises it for both.
#[test]
fn the_list_functions_follow_the_range_limit() {
    for source in [
        "{{ until 20000 | len }}",
        "{{ seq 1 20000 | len }}",
        "{{ untilStep 0 20000 1 | len }}",
    ] {
        let error = render(source).expect_err(source);
        assert!(
            error.contains("more than the 10000 this allows"),
            "{source}: {error}"
        );
    }

    let roomier = Limits::default().with_iterations(50_000);
    assert_eq!(
        render_with("{{ until 20000 | len }}", roomier).unwrap(),
        "20000"
    );
    assert_eq!(
        render_with("{{ seq 1 20000 | len }}", roomier).unwrap(),
        "20000"
    );
}

/// The character bound is its own knob, so a template that legitimately builds a large
/// block of text can have it without also being allowed to iterate forever.
#[test]
fn the_character_limit_is_separate_from_the_range_limit() {
    // A million characters is the default, and the next one over is refused.
    assert_eq!(
        render(r#"{{ repeat 1000000 "x" | len }}"#).unwrap(),
        "1000000"
    );
    let error = render(r#"{{ repeat 1000001 "x" | len }}"#).expect_err("one past the limit");
    assert!(
        error.contains("more than the 1000000 this allows"),
        "{error}"
    );

    // Lowering it leaves the range limit alone: how far `until` may *count* is
    // `iterations`' business, and 30 is far below both. (A list weighs its elements
    // plus their contents, so thirty numbers weigh sixty against the budget of 100.)
    let tighter = Limits::default().with_function_output(100);
    let error = render_with(r#"{{ repeat 1000 "x" }}"#, tighter).expect_err("over the 100");
    assert!(error.contains("more than the 100 this allows"), "{error}");
    assert_eq!(render_with("{{ until 30 | len }}", tighter).unwrap(), "30");

    // What it does bound is the size of any value a function hands back -- that is
    // the general cap that stops `{{ $s = print $s $s }}` from doubling forever --
    // so a list of 5000 elements against a budget of 100 is refused as a value even
    // though the iteration count is allowed.
    let error = render_with("{{ until 5000 | len }}", tighter)
        .expect_err("5000 elements against a produced-value budget of 100");
    assert!(
        error.contains("a function produced a value larger than the 100 allowed"),
        "{error}"
    );

    // `mowsRandomString` cannot exhaust memory -- it parses its length as a `u16` --
    // but a caller who lowered the limit meant it for every generating function.
    let error = render_with(r#"{{ mowsRandomString "Aa" 500 }}"#, tighter)
        .expect_err("500 characters against a limit of 100");
    assert!(error.contains("more than the 100 this allows"), "{error}");
}

/// Opting out has to reach the functions too, which is the whole reason the limit is
/// readable from them at all.
#[test]
fn unlimited_reaches_the_generating_functions() {
    assert_eq!(
        render_with("{{ until 2000000 | len }}", Limits::unlimited()).unwrap(),
        "2000000"
    );
    assert_eq!(
        render_with(r#"{{ repeat 2000000 "x" | len }}"#, Limits::unlimited()).unwrap(),
        "2000000"
    );
}

/// `indent` and `replace` multiply: the spaces per line, and the growth per match.
/// Each input passes its own check, and the product was 10^11 bytes -- the allocator
/// abort the limits exist to prevent. The product is judged before the allocation.
#[test]
fn the_multiplying_functions_bound_their_product() {
    let cases = [
        "{{ indent 999999 (repeat 100000 \"x\\n\") }}",
        r#"{{ replace "x" (repeat 999999 "y") (repeat 1000000 "x") }}"#,
    ];
    for source in cases {
        let started = std::time::Instant::now();
        let error = render(source).expect_err(source);
        assert!(
            error.contains("this allows"),
            "{source} failed for another reason: {error}"
        );
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "{source}: the product must be refused before it is built"
        );
    }

    // The shapes a real chart uses stay untouched.
    assert_eq!(render("{{ indent 2 \"a\\nb\" }}").unwrap(), "  a\n  b");
    assert_eq!(render(r#"{{ replace "o" "0" "foo" }}"#).unwrap(), "f00");
}

/// The helm list constructor is another way to nest a value one level per call, and
/// `Clone`, `Drop` and `Display` recurse per level: measured, 10 001 levels aborted a
/// 2 MiB thread. The depth cap refuses it whatever the budgets say.
#[test]
fn a_list_cannot_nest_its_way_off_the_stack() {
    let error = render_with(
        r#"{{ $x := list 1 }}{{ range 10000 }}{{ $x = list $x }}{{ end }}{{ $x }}"#,
        Limits::unlimited(),
    )
    .expect_err("10000 levels against a cap of 128");
    assert!(
        error.contains("a function produced a value nested deeper than the 128 levels allowed"),
        "{error}"
    );
}

/// Outside a render there is no template to read limits from, so the default stands --
/// which is what keeps a function called directly from a unit test bounded.
#[test]
fn the_default_applies_outside_a_render() {
    assert_eq!(gtmpl_ng::limits::active(), Limits::default());
    let error = gtmpl_ng::helm_functions::lists::until(&[Value::from(9_999_999_999i64)])
        .expect_err("called straight, with no render around it");
    assert!(error.to_string().contains("this allows"), "{error}");
}
