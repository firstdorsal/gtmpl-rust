//! Regression tests for panics that previously crashed the parser/executor
//! instead of returning a clean error.
//!
//! **Requires `--all-features`.** These cases run through the helm function registry,
//! so under the default feature set this file compiles to zero tests and `cargo test`
//! reports success having run none of it. CI uses `--all-features` for this reason.
//! Panic cases that do not need the registry belong in `lexer_panics.rs`, which is
//! deliberately ungated. `feature_gates.rs` reports this file as an ignored test when
//! the feature is off, so a default run says it was skipped rather than looking
//! complete -- keep the two together.
#![cfg(feature = "helm-functions")]

use gtmpl_ng::helm_functions::HELM_FUNCTIONS;
use gtmpl_ng::{Context, Template, Value};
use std::collections::HashMap;

fn render(tmpl_str: &str, ctx: Value) -> Result<String, String> {
    let mut tmpl = Template::default();
    tmpl.add_funcs(HELM_FUNCTIONS);
    tmpl.parse(tmpl_str).map_err(|e| format!("parse: {e}"))?;
    tmpl.render(&Context::from(ctx))
        .map_err(|e| format!("exec: {e}"))
}

/// Previously panicked in `helm_functions::logic::value_is_truthy`
/// (line 76) with `n.as_f64().unwrap()` on any integer Number, because
/// `gtmpl_value_ng::Number::as_f64()` returns `None` for non-float variants.
#[test]
fn default_with_integer_value_does_not_panic() {
    let mut ctx: HashMap<String, Value> = HashMap::new();
    ctx.insert("count".to_string(), Value::from(5i64));
    let out = render(r#"{{ .count | default 999 }}"#, Value::from(ctx)).unwrap();
    assert_eq!(out, "5");
}

#[test]
fn default_with_zero_integer_uses_default() {
    let mut ctx: HashMap<String, Value> = HashMap::new();
    ctx.insert("count".to_string(), Value::from(0i64));
    let out = render(r#"{{ .count | default 999 }}"#, Value::from(ctx)).unwrap();
    // 0 is "empty" in Helm's default semantics, so the default wins.
    assert_eq!(out, "999");
}

#[test]
fn default_with_unsigned_integer_does_not_panic() {
    let mut ctx: HashMap<String, Value> = HashMap::new();
    ctx.insert("count".to_string(), Value::from(42u64));
    let out = render(r#"{{ .count | default 999 }}"#, Value::from(ctx)).unwrap();
    assert_eq!(out, "42");
}

#[test]
fn default_with_float_value() {
    let mut ctx: HashMap<String, Value> = HashMap::new();
    ctx.insert("count".to_string(), Value::from(5.5f64));
    let out = render(r#"{{ .count | default 999.0 }}"#, Value::from(ctx)).unwrap();
    assert_eq!(out, "5.5");
}
/// The original user-facing report: a URL literal with `%5B`/`%5D`
/// passed as the default value in a pipe. This on its own should never
/// panic regardless of what `.sharingUrl` resolves to.
#[test]
fn url_literal_with_percent_encoded_brackets_as_default() {
    let tmpl = r#"{{ .sharingUrl | default "https://example.com/path?q=value%5B0%5D%5B1%5D" }}"#;

    // nil field
    let mut ctx: HashMap<String, Value> = HashMap::new();
    ctx.insert("sharingUrl".to_string(), Value::Nil);
    assert_eq!(
        render(tmpl, Value::from(ctx)).unwrap(),
        "https://example.com/path?q=value%5B0%5D%5B1%5D"
    );

    // empty string field
    let mut ctx: HashMap<String, Value> = HashMap::new();
    ctx.insert("sharingUrl".to_string(), Value::String(String::new()));
    assert_eq!(
        render(tmpl, Value::from(ctx)).unwrap(),
        "https://example.com/path?q=value%5B0%5D%5B1%5D"
    );

    // populated field
    let mut ctx: HashMap<String, Value> = HashMap::new();
    ctx.insert(
        "sharingUrl".to_string(),
        Value::String("https://actual.example/file".to_string()),
    );
    assert_eq!(
        render(tmpl, Value::from(ctx)).unwrap(),
        "https://actual.example/file"
    );
}

/// Previously panicked in `helm_functions::certs::validity_period`, which did
/// `OffsetDateTime::now_utc().checked_add(..).unwrap()`. A template names a
/// certificate's lifetime in days and can name any number; a few hundred million push
/// the date past the years `OffsetDateTime` holds, and a few quintillion overflow the
/// multiplication inside `Duration::days`, which panics by documented contract. Both
/// were a crash out of a template literal, so neither could be caught.
#[test]
fn a_certificate_lifetime_past_the_date_range_does_not_panic() {
    let cases = [
        r#"{{ genCA "ca" 999999999 }}"#,
        r#"{{ genCA "ca" 9223372036854775807 }}"#,
        r#"{{ genCA "ca" -9223372036854775808 }}"#,
        // Negative days land before year 0 long before any arithmetic overflows: the
        // ASN.1 writer asserts the year is 0..=9999, so -800000 days (year -164) hit
        // that assert and aborted. `genCA` parses its days as an i64 and reaches the
        // year bound; the other two refuse a negative at their u32 argument parse,
        // which the case below this list covers.
        r#"{{ genCA "ca" -800000 }}"#,
        r#"{{ genSelfSignedCert "host" nil nil 999999999 }}"#,
        r#"{{ $ca := genCA "ca" 365 }}{{ genSignedCert "host" nil nil 999999999 $ca }}"#,
    ];
    for source in cases {
        let error = render(source, Value::Nil).expect_err(source);
        assert!(
            error.contains("days is further off than a certificate date can reach"),
            "{source} failed for another reason: {error}"
        );
    }
}

/// The two functions that read their days as an unsigned number refuse a negative one
/// at the parse, before any date arithmetic -- also an error rather than the abort,
/// just a different one.
#[test]
fn a_negative_certificate_lifetime_is_refused_not_aborted() {
    for source in [
        r#"{{ genSelfSignedCert "host" nil nil -800000 }}"#,
        r#"{{ $ca := genCA "ca" 365 }}{{ genSignedCert "host" nil nil -800000 $ca }}"#,
    ] {
        let error = render(source, Value::Nil).expect_err(source);
        assert!(
            error.contains("Invalid number"),
            "{source} failed for another reason: {error}"
        );
    }
}

/// And the ordinary lifetime a chart asks for still works, so the bound did not simply
/// turn the functions off.
#[test]
fn an_ordinary_certificate_lifetime_still_works() {
    let rendered = render(r#"{{ (genCA "ca" 365).Cert }}"#, Value::Nil).expect("a year is fine");
    assert!(
        rendered.starts_with("-----BEGIN CERTIFICATE-----"),
        "{rendered}"
    );
}
