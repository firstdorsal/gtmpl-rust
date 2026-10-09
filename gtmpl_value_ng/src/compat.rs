//! Conversions from the published `gtmpl_value` 0.5 types.
//!
//! Renaming the crate means a tree that uses the original `#[derive(Gtmpl)]` produces
//! a `gtmpl_value::Value`, and `Into` is not transitive: that will not satisfy a
//! `Into<gtmpl_value_ng::Value>` bound. This module bridges the two so the derive can
//! stay where it is and the conversion happens at the call site.

use std::collections::HashMap;

use thiserror::Error;

use crate::number::Number;
use crate::value::Value;

/// Why a `gtmpl_value::Value` could not be converted.
#[derive(Debug, Error)]
pub enum FromLegacyValueError {
    /// `gtmpl_value::Func` is `fn(&[gtmpl_value::Value]) -> …`, a different function
    /// type, and a function pointer cannot be wrapped without allocating — so there is
    /// nothing to convert it into. Register the function with the template instead of
    /// passing it through the context.
    #[error(
        "a gtmpl_value function cannot be converted, its signature names the other crate's Value"
    )]
    Function,
    /// `gtmpl_value::Number` keeps its representation private and is read through
    /// `as_u64`/`as_i64`/`as_f64`. All three returning `None` is not reachable through
    /// 0.5.1, but the type is foreign, so it is reported rather than assumed away.
    #[error("a gtmpl_value number reported neither an integer nor a float")]
    UnreadableNumber,
}

impl TryFrom<gtmpl_value::Value> for Value {
    type Error = FromLegacyValueError;

    /// # Examples
    ///
    /// ```rust
    /// use gtmpl_value_ng::Value;
    ///
    /// let legacy = gtmpl_value::Value::from("something");
    /// assert_eq!(Value::try_from(legacy).unwrap(), Value::from("something"));
    /// ```
    fn try_from(value: gtmpl_value::Value) -> Result<Self, Self::Error> {
        use gtmpl_value::Value as Legacy;
        Ok(match value {
            Legacy::NoValue => Value::NoValue,
            Legacy::Nil => Value::Nil,
            Legacy::Bool(b) => Value::Bool(b),
            Legacy::String(s) => Value::String(s),
            Legacy::Object(o) => Value::Object(convert_map(o)?),
            Legacy::Map(m) => Value::Map(convert_map(m)?),
            Legacy::Array(a) => Value::Array(
                a.into_iter()
                    .map(Value::try_from)
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            Legacy::Function(_) => return Err(FromLegacyValueError::Function),
            Legacy::Number(n) => Value::Number(convert_number(&n)?),
        })
    }
}

fn convert_map(
    map: HashMap<String, gtmpl_value::Value>,
) -> Result<HashMap<String, Value>, FromLegacyValueError> {
    map.into_iter()
        .map(|(key, value)| Value::try_from(value).map(|value| (key, value)))
        .collect()
}

/// Reads a legacy number through its accessors and rebuilds it without going through
/// `From`, which would collapse a whole float to an integer.
///
/// A float is checked first because `as_f64` is the only accessor that answers for one.
/// An unsigned number and a non-negative signed number are indistinguishable from the
/// outside and both arrive as unsigned; 0.5.1 only ever stores a non-negative number
/// as unsigned, so that costs nothing in practice.
fn convert_number(n: &gtmpl_value::Number) -> Result<Number, FromLegacyValueError> {
    if let Some(f) = n.as_f64() {
        Ok(Number::float(f))
    } else if let Some(u) = n.as_u64() {
        Ok(Number::unsigned(u))
    } else if let Some(i) = n.as_i64() {
        Ok(Number::signed(i))
    } else {
        Err(FromLegacyValueError::UnreadableNumber)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn scalars_round_trip() {
        let cases: Vec<(gtmpl_value::Value, Value)> = vec![
            (gtmpl_value::Value::NoValue, Value::NoValue),
            (gtmpl_value::Value::Nil, Value::Nil),
            (gtmpl_value::Value::from(true), Value::from(true)),
            (gtmpl_value::Value::from("x"), Value::from("x")),
            (gtmpl_value::Value::from(23u64), Value::from(23u64)),
            (gtmpl_value::Value::from(-23i64), Value::from(-23i64)),
            (gtmpl_value::Value::from(3.25f64), Value::from(3.25f64)),
            (gtmpl_value::Value::from(u64::MAX), Value::from(u64::MAX)),
        ];
        for (legacy, expected) in cases {
            assert_eq!(Value::try_from(legacy).unwrap(), expected);
        }
    }

    #[test]
    fn a_negative_float_arrives_already_truncated() {
        // `gtmpl_value` 0.5.1 truncates every negative non-integer on construction --
        // the published crate stores `-3.25` as the integer `-3`, and `as_f64` reports
        // `None`. The fraction is gone before this conversion is reached, so there is
        // nothing to recover: converting carries the damage over rather than inventing
        // a value the source never held. Build the value with this crate to avoid it.
        let converted = Value::try_from(gtmpl_value::Value::from(-3.25f64)).unwrap();
        assert_eq!(converted, Value::Number(Number::signed(-3)));
        assert_eq!(Value::from(-3.25f64), Value::Number(Number::float(-3.25)));
    }

    #[test]
    fn a_whole_float_stays_whatever_the_legacy_crate_made_of_it() {
        // 0.5.1 turns `3.0` into an unsigned integer, so that is what comes back --
        // the conversion does not invent a float the source never held.
        let converted = Value::try_from(gtmpl_value::Value::from(3.0f64)).unwrap();
        assert_eq!(converted, Value::Number(Number::unsigned(3)));
    }

    #[test]
    fn nested_containers_convert() {
        let inner: HashMap<String, gtmpl_value::Value> =
            [("a".to_owned(), gtmpl_value::Value::from(1u64))]
                .into_iter()
                .collect();
        let legacy = gtmpl_value::Value::Array(vec![
            gtmpl_value::Value::Object(inner),
            gtmpl_value::Value::from("x"),
        ]);

        let converted = Value::try_from(legacy).unwrap();
        match converted {
            Value::Array(ref items) => {
                assert_eq!(items.len(), 2);
                match items[0] {
                    Value::Object(ref o) => assert_eq!(o.get("a"), Some(&Value::from(1u64))),
                    ref other => panic!("expected an object, got {:?}", other),
                }
                assert_eq!(items[1], Value::from("x"));
            }
            other => panic!("expected an array, got {:?}", other),
        }
    }

    #[test]
    fn a_function_is_refused_rather_than_dropped() {
        fn f(a: &[gtmpl_value::Value]) -> Result<gtmpl_value::Value, gtmpl_value::FuncError> {
            Ok(a[0].clone())
        }
        let legacy = gtmpl_value::Value::from(f as gtmpl_value::Func);
        assert!(matches!(
            Value::try_from(legacy),
            Err(FromLegacyValueError::Function)
        ));
    }

    #[test]
    fn a_function_nested_in_a_container_is_refused_too() {
        fn f(a: &[gtmpl_value::Value]) -> Result<gtmpl_value::Value, gtmpl_value::FuncError> {
            Ok(a[0].clone())
        }
        let inner: HashMap<String, gtmpl_value::Value> = [(
            "f".to_owned(),
            gtmpl_value::Value::from(f as gtmpl_value::Func),
        )]
        .into_iter()
        .collect();
        assert!(matches!(
            Value::try_from(gtmpl_value::Value::Object(inner)),
            Err(FromLegacyValueError::Function)
        ));
    }
}
