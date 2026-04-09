//! Helm/Sprig-compatible math functions.
//!
//! All functions return `Value::Number` (never `Value::String`) so they round-
//! trip through the template engine without losing type information. The
//! integer functions (`add`, `sub`, `mul`, `div`, `mod`, `pow`, `add1`, `max`,
//! `min`) operate on `i64` and return `i64`, matching Sprig's `toInt64`
//! semantics. The float-suffixed variants (`addf`, `subf`, `mulf`, `divf`,
//! `add1f`, `maxf`, `minf`) and `floor`/`ceil`/`round` operate on `f64` and
//! return `f64`. This means templated math behaves consistently regardless of
//! where a number originates (literal, JSON, YAML, or context).
//!
//! Arithmetic on the integer variants uses wrapping semantics to match Go's
//! `int64` arithmetic used by Sprig. Division and modulo by zero return an
//! error rather than panicking (divergence from Sprig/Go, but required so
//! that gtmpl-ng never panics on untrusted template input).

use crate::{FuncError, Value};
use std::convert::TryFrom;

// ---------------------------------------------------------------------------
// Conversion helpers — Sprig's toInt64 / toFloat64 semantics
// ---------------------------------------------------------------------------

fn to_i64(v: &Value) -> Result<i64, FuncError> {
    match v {
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(i)
            } else if let Some(u) = n.as_u64() {
                // Matches Go's `int64(uint64(_))` bit reinterpretation.
                Ok(u as i64)
            } else if let Some(f) = n.as_f64() {
                Ok(f as i64)
            } else {
                Err(FuncError::UnableToConvertFromValue)
            }
        }
        Value::Bool(b) => Ok(if *b { 1 } else { 0 }),
        Value::String(s) => {
            let s = s.trim();
            if let Ok(i) = s.parse::<i64>() {
                Ok(i)
            } else if let Ok(f) = s.parse::<f64>() {
                Ok(f as i64)
            } else {
                Err(FuncError::UnableToConvertFromValue)
            }
        }
        Value::Nil | Value::NoValue => Ok(0),
        _ => Err(FuncError::UnableToConvertFromValue),
    }
}

fn to_f64(v: &Value) -> Result<f64, FuncError> {
    match v {
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                Ok(f)
            } else if let Some(i) = n.as_i64() {
                Ok(i as f64)
            } else if let Some(u) = n.as_u64() {
                Ok(u as f64)
            } else {
                Err(FuncError::UnableToConvertFromValue)
            }
        }
        Value::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
        Value::String(s) => s
            .trim()
            .parse::<f64>()
            .map_err(|_| FuncError::UnableToConvertFromValue),
        Value::Nil | Value::NoValue => Ok(0.0),
        _ => Err(FuncError::UnableToConvertFromValue),
    }
}

// ---------------------------------------------------------------------------
// Integer math (Sprig: returns int64) — returns Value::Number wrapping i64
// ---------------------------------------------------------------------------

/// `add` — variadic sum. Matches Sprig `add`.
pub fn math_add(args: &[Value]) -> Result<Value, FuncError> {
    let mut sum: i64 = 0;
    for arg in args {
        sum = sum.wrapping_add(to_i64(arg)?);
    }
    Ok(Value::from(sum))
}

/// `sub` — takes exactly 2 arguments.
pub fn math_subtract(args: &[Value]) -> Result<Value, FuncError> {
    let a = args.first().ok_or(FuncError::ExactlyXArgs(
        "sub requires exactly 2 arguments".to_string(),
        2,
    ))?;
    let b = args.get(1).ok_or(FuncError::ExactlyXArgs(
        "sub requires exactly 2 arguments".to_string(),
        2,
    ))?;
    Ok(Value::from(to_i64(a)?.wrapping_sub(to_i64(b)?)))
}

/// `mul` — variadic product. Matches Sprig `mul`.
pub fn math_multiply(args: &[Value]) -> Result<Value, FuncError> {
    if args.is_empty() {
        return Err(FuncError::AtLeastXArgs(
            "mul requires at least 1 argument".to_string(),
            1,
        ));
    }
    let mut product: i64 = 1;
    for arg in args {
        product = product.wrapping_mul(to_i64(arg)?);
    }
    Ok(Value::from(product))
}

/// `div` — integer division. Errors on divide-by-zero instead of panicking.
pub fn math_divide(args: &[Value]) -> Result<Value, FuncError> {
    let a = args.first().ok_or(FuncError::ExactlyXArgs(
        "div requires exactly 2 arguments".to_string(),
        2,
    ))?;
    let b = args.get(1).ok_or(FuncError::ExactlyXArgs(
        "div requires exactly 2 arguments".to_string(),
        2,
    ))?;
    let a = to_i64(a)?;
    let b = to_i64(b)?;
    if b == 0 {
        return Err(FuncError::Generic("div: division by zero".to_string()));
    }
    // wrapping_div handles the i64::MIN / -1 overflow case.
    Ok(Value::from(a.wrapping_div(b)))
}

/// `mod` — integer modulo. Errors on modulo-by-zero instead of panicking.
pub fn math_modulo(args: &[Value]) -> Result<Value, FuncError> {
    let a = args.first().ok_or(FuncError::ExactlyXArgs(
        "mod requires exactly 2 arguments".to_string(),
        2,
    ))?;
    let b = args.get(1).ok_or(FuncError::ExactlyXArgs(
        "mod requires exactly 2 arguments".to_string(),
        2,
    ))?;
    let a = to_i64(a)?;
    let b = to_i64(b)?;
    if b == 0 {
        return Err(FuncError::Generic("mod: division by zero".to_string()));
    }
    Ok(Value::from(a.wrapping_rem(b)))
}

/// `pow` — integer exponentiation. Not in Sprig; kept for backwards
/// compatibility. Negative exponents error (use float math instead).
pub fn math_power(args: &[Value]) -> Result<Value, FuncError> {
    let a = args.first().ok_or(FuncError::ExactlyXArgs(
        "pow requires exactly 2 arguments".to_string(),
        2,
    ))?;
    let b = args.get(1).ok_or(FuncError::ExactlyXArgs(
        "pow requires exactly 2 arguments".to_string(),
        2,
    ))?;
    let base = to_i64(a)?;
    let exp = to_i64(b)?;
    if exp < 0 {
        return Err(FuncError::Generic(
            "pow: negative exponent is not supported; use float math instead".to_string(),
        ));
    }
    let exp_u32 = u32::try_from(exp)
        .map_err(|_| FuncError::Generic("pow: exponent out of range".to_string()))?;
    Ok(Value::from(base.wrapping_pow(exp_u32)))
}

/// `add1` — increment by 1 (integer).
pub fn add1(args: &[Value]) -> Result<Value, FuncError> {
    let a = args.first().ok_or(FuncError::ExactlyXArgs(
        "add1 requires exactly 1 argument".to_string(),
        1,
    ))?;
    Ok(Value::from(to_i64(a)?.wrapping_add(1)))
}

/// `max` — variadic maximum (integer). Matches Sprig `max`.
pub fn max(args: &[Value]) -> Result<Value, FuncError> {
    if args.is_empty() {
        return Err(FuncError::AtLeastXArgs(
            "max requires at least 1 argument".to_string(),
            1,
        ));
    }
    let mut m = i64::MIN;
    for arg in args {
        let v = to_i64(arg)?;
        if v > m {
            m = v;
        }
    }
    Ok(Value::from(m))
}

/// `min` — variadic minimum (integer). Matches Sprig `min`.
pub fn min(args: &[Value]) -> Result<Value, FuncError> {
    if args.is_empty() {
        return Err(FuncError::AtLeastXArgs(
            "min requires at least 1 argument".to_string(),
            1,
        ));
    }
    let mut m = i64::MAX;
    for arg in args {
        let v = to_i64(arg)?;
        if v < m {
            m = v;
        }
    }
    Ok(Value::from(m))
}

// ---------------------------------------------------------------------------
// Float math (Sprig: returns float64) — returns Value::Number wrapping f64
// ---------------------------------------------------------------------------

/// `addf` — variadic float sum. Matches Sprig `addf`.
pub fn addf(args: &[Value]) -> Result<Value, FuncError> {
    let mut sum: f64 = 0.0;
    for arg in args {
        sum += to_f64(arg)?;
    }
    Ok(Value::from(sum))
}

/// `add1f` — increment by 1 (float).
pub fn add1f(args: &[Value]) -> Result<Value, FuncError> {
    let a = args.first().ok_or(FuncError::ExactlyXArgs(
        "add1f requires exactly 1 argument".to_string(),
        1,
    ))?;
    Ok(Value::from(to_f64(a)? + 1.0))
}

/// `subf` — float subtraction (2 args).
pub fn subf(args: &[Value]) -> Result<Value, FuncError> {
    let a = args.first().ok_or(FuncError::ExactlyXArgs(
        "subf requires exactly 2 arguments".to_string(),
        2,
    ))?;
    let b = args.get(1).ok_or(FuncError::ExactlyXArgs(
        "subf requires exactly 2 arguments".to_string(),
        2,
    ))?;
    Ok(Value::from(to_f64(a)? - to_f64(b)?))
}

/// `divf` — float division (2 args). IEEE-754: `/0.0` yields `inf`/`NaN`.
pub fn divf(args: &[Value]) -> Result<Value, FuncError> {
    let a = args.first().ok_or(FuncError::ExactlyXArgs(
        "divf requires exactly 2 arguments".to_string(),
        2,
    ))?;
    let b = args.get(1).ok_or(FuncError::ExactlyXArgs(
        "divf requires exactly 2 arguments".to_string(),
        2,
    ))?;
    Ok(Value::from(to_f64(a)? / to_f64(b)?))
}

/// `mulf` — variadic float product. Matches Sprig `mulf`.
pub fn mulf(args: &[Value]) -> Result<Value, FuncError> {
    if args.is_empty() {
        return Err(FuncError::AtLeastXArgs(
            "mulf requires at least 1 argument".to_string(),
            1,
        ));
    }
    let mut product: f64 = 1.0;
    for arg in args {
        product *= to_f64(arg)?;
    }
    Ok(Value::from(product))
}

/// `maxf` — variadic float maximum.
pub fn maxf(args: &[Value]) -> Result<Value, FuncError> {
    if args.is_empty() {
        return Err(FuncError::AtLeastXArgs(
            "maxf requires at least 1 argument".to_string(),
            1,
        ));
    }
    let mut m = f64::NEG_INFINITY;
    for arg in args {
        let v = to_f64(arg)?;
        if v > m {
            m = v;
        }
    }
    Ok(Value::from(m))
}

/// `minf` — variadic float minimum.
pub fn minf(args: &[Value]) -> Result<Value, FuncError> {
    if args.is_empty() {
        return Err(FuncError::AtLeastXArgs(
            "minf requires at least 1 argument".to_string(),
            1,
        ));
    }
    let mut m = f64::INFINITY;
    for arg in args {
        let v = to_f64(arg)?;
        if v < m {
            m = v;
        }
    }
    Ok(Value::from(m))
}

/// `floor` — round down to nearest integer (returns float).
pub fn floor(args: &[Value]) -> Result<Value, FuncError> {
    let a = args.first().ok_or(FuncError::ExactlyXArgs(
        "floor requires exactly 1 argument".to_string(),
        1,
    ))?;
    Ok(Value::from(to_f64(a)?.floor()))
}

/// `ceil` — round up to nearest integer (returns float).
pub fn ceil(args: &[Value]) -> Result<Value, FuncError> {
    let a = args.first().ok_or(FuncError::ExactlyXArgs(
        "ceil requires exactly 1 argument".to_string(),
        1,
    ))?;
    Ok(Value::from(to_f64(a)?.ceil()))
}

/// `round` — round to a given number of decimals (default 0).
pub fn round(args: &[Value]) -> Result<Value, FuncError> {
    let a = args.first().ok_or(FuncError::ExactlyXArgs(
        "round requires at least 1 argument".to_string(),
        1,
    ))?;
    let decimals: i32 = if let Some(d) = args.get(1) {
        i32::try_from(to_i64(d)?)
            .map_err(|_| FuncError::Generic("round: decimals out of range".to_string()))?
    } else {
        0
    };
    let a = to_f64(a)?;
    let multiplier = 10_f64.powi(decimals);
    Ok(Value::from((a * multiplier).round() / multiplier))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- basic integer operations (Sprig parity) ---

    #[test]
    fn test_math_add() {
        let result = math_add(&[Value::Number(3.into()), Value::Number(5.into())]).unwrap();
        assert_eq!(result.to_string(), "8");
    }

    #[test]
    fn test_math_add_variadic() {
        let result = math_add(&[
            Value::Number(1.into()),
            Value::Number(2.into()),
            Value::Number(3.into()),
            Value::Number(4.into()),
        ])
        .unwrap();
        assert_eq!(result.to_string(), "10");
    }

    #[test]
    fn test_math_add_returns_number_not_string() {
        let result = math_add(&[Value::Number(3.into()), Value::Number(5.into())]).unwrap();
        assert!(matches!(result, Value::Number(_)), "expected Number, got {:?}", result);
    }

    #[test]
    fn test_math_subtract() {
        let result = math_subtract(&[Value::Number(10.into()), Value::Number(4.into())]).unwrap();
        assert_eq!(result.to_string(), "6");
    }

    #[test]
    fn test_math_multiply() {
        let result = math_multiply(&[Value::Number(3.into()), Value::Number(4.into())]).unwrap();
        assert_eq!(result.to_string(), "12");
    }

    #[test]
    fn test_math_multiply_variadic() {
        let result = math_multiply(&[
            Value::Number(2.into()),
            Value::Number(3.into()),
            Value::Number(4.into()),
        ])
        .unwrap();
        assert_eq!(result.to_string(), "24");
    }

    #[test]
    fn test_math_divide() {
        let result = math_divide(&[Value::Number(10.into()), Value::Number(2.into())]).unwrap();
        assert_eq!(result.to_string(), "5");
    }

    #[test]
    fn test_math_divide_truncates() {
        // Integer division truncates toward zero: 10/3 = 3.
        let result = math_divide(&[Value::Number(10.into()), Value::Number(3.into())]).unwrap();
        assert_eq!(result.to_string(), "3");
    }

    #[test]
    fn test_math_divide_by_zero_returns_error() {
        let result = math_divide(&[Value::Number(10.into()), Value::Number(0.into())]);
        assert!(result.is_err(), "expected error, got {:?}", result);
    }

    #[test]
    fn test_math_modulo_by_zero_returns_error() {
        let result = math_modulo(&[Value::Number(10.into()), Value::Number(0.into())]);
        assert!(result.is_err(), "expected error, got {:?}", result);
    }

    #[test]
    fn test_math_power() {
        let result = math_power(&[Value::Number(2.into()), Value::Number(3.into())]).unwrap();
        assert_eq!(result.to_string(), "8");
    }

    #[test]
    fn test_math_modulo() {
        let result = math_modulo(&[Value::Number(10.into()), Value::Number(3.into())]).unwrap();
        assert_eq!(result.to_string(), "1");
    }

    #[test]
    fn test_max() {
        let result = max(&[
            Value::Number(1.into()),
            Value::Number(5.into()),
            Value::Number(3.into()),
        ])
        .unwrap();
        assert_eq!(result.to_string(), "5");
    }

    #[test]
    fn test_min() {
        let result = min(&[
            Value::Number(1.into()),
            Value::Number(5.into()),
            Value::Number(3.into()),
        ])
        .unwrap();
        assert_eq!(result.to_string(), "1");
    }

    // --- integer functions should not lose type information ---

    #[test]
    fn test_add_preserves_integer_type() {
        // `add` must return a Number whose integer accessors are populated,
        // never a Value::String (which is what the previous implementation
        // produced and which broke downstream pipelines that expected a
        // numeric value).
        let result = math_add(&[Value::Number(3.into()), Value::Number(5.into())]).unwrap();
        match result {
            Value::Number(n) => {
                assert_eq!(to_i64(&Value::Number(n.clone())).ok(), Some(8));
            }
            _ => panic!("expected Number, got {:?}", result),
        }
    }

    #[test]
    fn test_addf_with_fractional_result_stays_float() {
        // 1.5 + 3.7 = 5.2 — a genuine non-integer float, so the library
        // cannot canonicalize it to an integer variant.
        let result = addf(&[Value::Number(1.5.into()), Value::Number(3.7.into())]).unwrap();
        match result {
            Value::Number(n) => {
                let f = n.as_f64().expect("addf with fractional result must expose f64");
                assert!((f - 5.2).abs() < 1e-9, "got {}", f);
            }
            _ => panic!("expected Number, got {:?}", result),
        }
    }

    // --- inputs from strings (parser/JSON/YAML path) must still work ---

    #[test]
    fn test_add_accepts_string_inputs() {
        let result = math_add(&[
            Value::String("3".to_string()),
            Value::String("5".to_string()),
        ])
        .unwrap();
        assert_eq!(result.to_string(), "8");
    }

    #[test]
    fn test_add_accepts_mixed_number_and_string() {
        let result = math_add(&[Value::Number(3.into()), Value::String("5".to_string())]).unwrap();
        assert_eq!(result.to_string(), "8");
    }

    // --- float operations ---

    #[test]
    fn test_floor() {
        let result = floor(&[Value::Number(3.7.into())]).unwrap();
        assert_eq!(result.to_string(), "3");
    }

    #[test]
    fn test_ceil() {
        let result = ceil(&[Value::Number(3.2.into())]).unwrap();
        assert_eq!(result.to_string(), "4");
    }

    #[test]
    fn test_round() {
        let result = round(&[Value::Number(3.567.into()), Value::Number(2.into())]).unwrap();
        assert_eq!(result.to_string(), "3.57");
    }

    #[test]
    fn test_round_no_decimals() {
        let result = round(&[Value::Number(3.567.into())]).unwrap();
        assert_eq!(result.to_string(), "4");
    }

    #[test]
    fn test_add1() {
        let result = add1(&[Value::Number(5.into())]).unwrap();
        assert_eq!(result.to_string(), "6");
    }

    #[test]
    fn test_add1f() {
        let result = add1f(&[Value::Number(5.5.into())]).unwrap();
        assert_eq!(result.to_string(), "6.5");
    }

    #[test]
    fn test_addf() {
        let result = addf(&[Value::Number(1.5.into()), Value::Number(2.5.into())]).unwrap();
        assert_eq!(result.to_string(), "4");
    }

    #[test]
    fn test_divf() {
        let result = divf(&[Value::Number(10.into()), Value::Number(4.into())]).unwrap();
        assert_eq!(result.to_string(), "2.5");
    }
}
