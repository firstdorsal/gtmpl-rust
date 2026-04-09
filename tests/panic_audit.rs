//! Audit tests for panic paths discovered by code inspection.
//!
//! Each `#[test]` here exercises a single suspected panic. A passing test
//! means the code returns a clean error (or a valid result); a failing test
//! means the code still panics.
#![cfg(feature = "helm-functions")]

use gtmpl_ng::helm_functions::{
    conversion::serde_yaml_value_to_gtmpl_value, string, HELM_FUNCTIONS,
};
use gtmpl_ng::{Context, Template, Value};
use std::collections::HashMap;

fn render(tmpl_str: &str, ctx: Value) -> Result<String, String> {
    let mut tmpl = Template::default();
    tmpl.add_funcs(&HELM_FUNCTIONS);
    tmpl.parse(tmpl_str).map_err(|e| format!("parse: {e}"))?;
    tmpl.render(&Context::from(ctx))
        .map_err(|e| format!("exec: {e}"))
}

// ---------------------------------------------------------------------------
// helm_functions::string::substr — `&value[start..end]` byte slice
// ---------------------------------------------------------------------------

#[test]
fn substr_out_of_bounds_end_returns_error_not_panic() {
    let args = [
        Value::from(0u64),
        Value::from(999u64),
        Value::String("hi".to_string()),
    ];
    let result = string::substr(&args);
    assert!(result.is_err(), "expected error, got {:?}", result);
}

#[test]
fn substr_start_greater_than_end_returns_error_not_panic() {
    let args = [
        Value::from(5u64),
        Value::from(2u64),
        Value::String("hello world".to_string()),
    ];
    let result = string::substr(&args);
    assert!(result.is_err(), "expected error, got {:?}", result);
}

#[test]
fn substr_non_char_boundary_returns_error_not_panic() {
    // "héllo" has an é that's 2 bytes; slicing byte range 1..2 lands inside it.
    let args = [
        Value::from(1u64),
        Value::from(2u64),
        Value::String("héllo".to_string()),
    ];
    let result = string::substr(&args);
    assert!(result.is_err(), "expected error, got {:?}", result);
}

// ---------------------------------------------------------------------------
// helm_functions::string::trunc — slice panics
// ---------------------------------------------------------------------------

#[test]
fn trunc_larger_than_string_returns_original_not_panic() {
    let args = [Value::from(100u64), Value::String("short".to_string())];
    let result = string::trunc(&args).expect("must not panic");
    assert_eq!(result.to_string(), "short");
}

#[test]
fn trunc_on_non_char_boundary_returns_error_not_panic() {
    // "héllo": bytes h=[0], é=[1,2], l=[3]… — byte index 2 lands inside é.
    let args = [Value::from(2u64), Value::String("héllo".to_string())];
    let result = string::trunc(&args);
    assert!(result.is_err(), "expected error, got {:?}", result);
}

// ---------------------------------------------------------------------------
// helm_functions::string::abbrev — usize underflow and slice panics
// ---------------------------------------------------------------------------

#[test]
fn abbrev_max_length_less_than_three_returns_original_not_panic() {
    // Previously `max_length - 3` underflowed usize and panicked.
    // Sprig semantics: too small a max_length returns the original.
    let args = [Value::from(2u64), Value::String("hello world".to_string())];
    let result = string::abbrev(&args).expect("must not panic");
    assert_eq!(result.to_string(), "hello world");
}

#[test]
fn abbrev_max_length_larger_than_value_returns_original_string() {
    // Classic abbrev semantics: if max_length >= len, return the original.
    let args = [Value::from(100u64), Value::String("short".to_string())];
    let result = string::abbrev(&args).expect("must not panic");
    assert_eq!(result.to_string(), "short");
}

// ---------------------------------------------------------------------------
// helm_functions::string::abbrevboth — underflow + slice range
// ---------------------------------------------------------------------------

#[test]
fn abbrevboth_short_max_length_returns_original_not_panic() {
    // Previously panicked from `max_length - 3` usize underflow.
    let args = [
        Value::from(0u64),
        Value::from(2u64),
        Value::String("hello world".to_string()),
    ];
    let result = string::abbrevboth(&args).expect("must not panic");
    assert_eq!(result.to_string(), "hello world");
}

// ---------------------------------------------------------------------------
// helm_functions::conversion — unreachable!() on tagged YAML values
// ---------------------------------------------------------------------------

#[test]
fn tagged_yaml_value_does_not_panic() {
    // A tagged YAML scalar — e.g. `!mytag hello`.
    let yaml = "!mytag hello";
    let parsed: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
    // Should not panic; accept any Value variant.
    let _ = serde_yaml_value_to_gtmpl_value(parsed);
}

#[test]
fn from_yaml_with_tagged_value_does_not_panic() {
    // Exercising the path through the `fromYaml` template function.
    let tmpl = r#"{{ fromYaml "!mytag hello" }}"#;
    let _ = render(tmpl, Value::Nil);
}

// ---------------------------------------------------------------------------
// default/empty/coalesce/ternary on a list of integer arguments
// ---------------------------------------------------------------------------

#[test]
fn coalesce_with_integers_does_not_panic() {
    let mut ctx: HashMap<String, Value> = HashMap::new();
    ctx.insert("a".to_string(), Value::from(0i64));
    ctx.insert("b".to_string(), Value::from(42i64));
    let out = render(r#"{{ coalesce .a .b }}"#, Value::from(ctx)).unwrap();
    assert_eq!(out, "42");
}

#[test]
fn ternary_with_integer_condition_does_not_panic() {
    let mut ctx: HashMap<String, Value> = HashMap::new();
    ctx.insert("cond".to_string(), Value::from(1i64));
    let out = render(
        r#"{{ ternary "yes" "no" .cond }}"#,
        Value::from(ctx),
    )
    .unwrap();
    // Helm ternary returns the first arg when condition is truthy.
    assert_eq!(out, "yes");
}

#[test]
fn empty_with_integer_does_not_panic() {
    let mut ctx: HashMap<String, Value> = HashMap::new();
    ctx.insert("n".to_string(), Value::from(0i64));
    let out = render(r#"{{ empty .n }}"#, Value::from(ctx)).unwrap();
    assert_eq!(out, "true");
}

// ---------------------------------------------------------------------------
// Math parity: integer math must stay integer, float math must stay float,
// and identical inputs must produce identical results whether they come from
// literals, the context, JSON, or YAML.
// ---------------------------------------------------------------------------

#[test]
fn add_returns_number_not_string() {
    // Regression: before this fix, `add` returned `Value::String("8")`,
    // which broke downstream numeric pipelines (e.g. `add 3 5 | add 1`
    // would re-parse a string through the i64 path).
    let out = render(r#"{{ add 3 5 | typeOf }}"#, Value::Nil).unwrap();
    assert_eq!(out, "number", "add should produce Value::Number");
}

#[test]
fn add_literal_integers_produces_integer() {
    let out = render(r#"{{ add 3 5 }}"#, Value::Nil).unwrap();
    assert_eq!(out, "8");
}

#[test]
fn add_integer_from_context_produces_integer() {
    let mut ctx: HashMap<String, Value> = HashMap::new();
    ctx.insert("a".to_string(), Value::from(3i64));
    ctx.insert("b".to_string(), Value::from(5i64));
    let out = render(r#"{{ add .a .b }}"#, Value::from(ctx)).unwrap();
    assert_eq!(out, "8");
}

#[test]
fn add_integer_from_json_produces_integer() {
    // Previously JSON integers were converted to f64 before math, so
    // `{"n": 5}` → `add .n 3` produced "8" only by coincidence; large
    // integers would lose precision. This asserts integer identity is
    // preserved through the fromJson path.
    let out = render(
        r#"{{ $d := fromJson "{\"n\": 5}" }}{{ add $d.n 3 }}"#,
        Value::Nil,
    )
    .unwrap();
    assert_eq!(out, "8");
}

#[test]
fn add_integer_from_yaml_produces_integer() {
    let out = render(
        r#"{{ $d := fromYaml "n: 5" }}{{ add $d.n 3 }}"#,
        Value::Nil,
    )
    .unwrap();
    assert_eq!(out, "8");
}

#[test]
fn mul_variadic_matches_sprig() {
    // Sprig's `mul` is variadic.
    let out = render(r#"{{ mul 2 3 4 }}"#, Value::Nil).unwrap();
    assert_eq!(out, "24");
}

#[test]
fn add_variadic_matches_sprig() {
    let out = render(r#"{{ add 1 2 3 4 }}"#, Value::Nil).unwrap();
    assert_eq!(out, "10");
}

#[test]
fn div_by_zero_is_error_not_panic() {
    let result = render(r#"{{ div 10 0 }}"#, Value::Nil);
    assert!(result.is_err(), "expected error, got {:?}", result);
}

#[test]
fn mod_by_zero_is_error_not_panic() {
    let result = render(r#"{{ mod 10 0 }}"#, Value::Nil);
    assert!(result.is_err(), "expected error, got {:?}", result);
}

#[test]
fn math_pipeline_with_default_does_not_panic_on_integer_context() {
    // This is the exact shape that previously crashed `value_is_truthy`.
    let mut ctx: HashMap<String, Value> = HashMap::new();
    ctx.insert("count".to_string(), Value::from(3i64));
    let out = render(
        r#"{{ .count | default 1 | add 10 }}"#,
        Value::from(ctx),
    )
    .unwrap();
    assert_eq!(out, "13");
}

// ---------------------------------------------------------------------------
// mows_functions::crypto::random_string — parse panic and empty-range panic
// ---------------------------------------------------------------------------

#[cfg(feature = "mows-functions")]
mod mows {
    use super::*;
    use gtmpl_ng::mows_functions::crypto::random_string;

    #[test]
    fn random_string_invalid_length_returns_error_not_panic() {
        let result = random_string(&[
            Value::String("Aa".to_string()),
            Value::String("not-a-number".to_string()),
        ]);
        assert!(result.is_err(), "expected error, got {:?}", result);
    }

    #[test]
    fn random_string_negative_length_returns_error_not_panic() {
        let result = random_string(&[
            Value::String("Aa".to_string()),
            Value::String("-5".to_string()),
        ]);
        assert!(result.is_err(), "expected error, got {:?}", result);
    }

    #[test]
    fn random_string_empty_method_returns_error_not_panic() {
        // Previously panicked in `rng.random_range(0..0)` because the
        // charset stayed empty.
        let result = random_string(&[
            Value::String("".to_string()),
            Value::Number(10.into()),
        ]);
        assert!(result.is_err(), "expected error, got {:?}", result);
    }
}
