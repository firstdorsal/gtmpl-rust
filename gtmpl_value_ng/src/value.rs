use std::cmp::PartialEq;
use std::collections::HashMap;
use std::fmt;
use thiserror::Error;

#[doc(inline)]
pub use crate::number::{Number, NumberKind};

#[derive(Debug, Error)]
pub enum FuncError {
    #[error("unable to convert argument from value")]
    UnableToConvertFromValue,
    #[error("{0} requires at least {1} argument(s)")]
    AtLeastXArgs(String, usize),
    #[error("{0} requires exactly {1} argument(s)")]
    ExactlyXArgs(String, usize),
    #[error("{0}")]
    Generic(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Function type supported by `gtmpl_value_ng`.
pub type Func = fn(&[Value]) -> Result<Value, FuncError>;

/// Wrapper struct for `Func`.
#[derive(Clone)]
pub struct Function {
    pub f: Func,
}

impl PartialEq for Function {
    /// Two wrappers are equal when they hold the same function.
    ///
    /// Function addresses are not guaranteed to be unique -- the compiler may merge
    /// identical functions or emit one in several codegen units -- so this answers
    /// about the pointer, not about behaviour. It is what the template engine needs to
    /// compare two `Value::Function`s at all.
    fn eq(&self, other: &Function) -> bool {
        std::ptr::fn_addr_eq(self.f, other.f)
    }
}

impl fmt::Debug for Function {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Function")
    }
}

impl fmt::Display for Function {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Function")
    }
}

/// Represents a gtmpl value.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged))]
pub enum Value {
    /// A value that is not there at all, which is how Go renders a missing field.
    /// It writes as `null`, but absence is not something a document can state, so
    /// reading `null` back gives [`Value::Nil`].
    #[cfg_attr(feature = "serde", serde(skip_deserializing))]
    NoValue,
    Nil,
    Bool(bool),
    String(String),
    /// A struct's fields, as `#[derive(Gtmpl)]` produces. It has the same shape as
    /// [`Value::Map`] and writes as one; only its provenance differs, and a document
    /// cannot carry that, so reading a map back gives `Map`.
    #[cfg_attr(feature = "serde", serde(skip_deserializing))]
    Object(HashMap<String, Value>),
    Map(HashMap<String, Value>),
    Array(Vec<Value>),
    /// A function cannot be written to a document or read from one.
    #[cfg_attr(feature = "serde", serde(skip))]
    Function(Function),
    Number(Number),
}

impl Value {
    pub fn from<T>(t: T) -> Self
    where
        T: Into<Value>,
    {
        t.into()
    }
}

impl fmt::Display for Value {
    /// Renders the value the way Go's `fmt` prints it with `%v`.
    ///
    /// A container used to fall back on Rust's `Debug`, so an array rendered as
    /// `[String("a"), Bool(true)]` where Go writes `[a true]`. A map's keys are
    /// sorted, as Go has sorted them since 1.12, so the same data renders the same way
    /// twice.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Value::NoValue => write!(f, "<no value>"),
            Value::Nil => write!(f, "<nil>"),
            Value::Bool(ref b) => write!(f, "{}", b),
            Value::String(ref s) => write!(f, "{}", s),
            Value::Function(ref func) => write!(f, "{}", func),
            Value::Number(ref n) => write!(f, "{}", n),
            Value::Array(ref a) => {
                f.write_str("[")?;
                for (index, item) in a.iter().enumerate() {
                    if index > 0 {
                        f.write_str(" ")?;
                    }
                    write!(f, "{}", item)?;
                }
                f.write_str("]")
            }
            Value::Object(ref entries) | Value::Map(ref entries) => {
                f.write_str("map[")?;
                let mut keys: Vec<&String> = entries.keys().collect();
                keys.sort();
                for (index, key) in keys.into_iter().enumerate() {
                    if index > 0 {
                        f.write_str(" ")?;
                    }
                    write!(f, "{}:{}", key, entries[key])?;
                }
                f.write_str("]")
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::number::Number;

    /// `Display` is the path `{{ .list }}` and `{{ print .map }}` take, and it used to
    /// fall back on Rust's `Debug` for a container. Expectations from Go's `fmt`.
    #[test]
    fn a_container_renders_the_way_go_renders_it() {
        let array = Value::Array(vec![
            Value::from("a"),
            Value::from(true),
            Value::from(1u64),
            Value::Nil,
        ]);
        assert_eq!(array.to_string(), "[a true 1 <nil>]");
        assert_eq!(Value::Array(Vec::new()).to_string(), "[]");

        let nested = Value::Array(vec![Value::Array(vec![Value::from("x")])]);
        assert_eq!(nested.to_string(), "[[x]]");
    }

    /// Built in an order that is not the sorted one, so a missing sort shows up.
    #[test]
    fn a_maps_keys_are_sorted() {
        let entries: HashMap<String, Value> = [
            ("z".to_owned(), Value::from(3u64)),
            ("a".to_owned(), Value::from(1u64)),
            ("m".to_owned(), Value::from(2u64)),
        ]
        .into_iter()
        .collect();
        assert_eq!(
            Value::Map(entries.clone()).to_string(),
            "map[a:1 m:2 z:3]",
            "Go has sorted map keys since 1.12"
        );
        assert_eq!(Value::Object(entries).to_string(), "map[a:1 m:2 z:3]");
        assert_eq!(Value::Map(HashMap::new()).to_string(), "map[]");
    }

    #[test]
    fn the_scalars_render_the_way_go_renders_them() {
        assert_eq!(Value::Nil.to_string(), "<nil>");
        assert_eq!(Value::NoValue.to_string(), "<no value>");
        assert_eq!(Value::from(true).to_string(), "true");
        assert_eq!(Value::from("text").to_string(), "text");
        assert_eq!(Value::from(-5i64).to_string(), "-5");
        assert_eq!(Value::Number(Number::float(1e20)).to_string(), "1e+20");
        assert_eq!(Value::Number(Number::float(f64::NAN)).to_string(), "NaN");
    }

    #[test]
    fn a_function_renders_as_its_placeholder() {
        fn f(a: &[Value]) -> Result<Value, FuncError> {
            Ok(a[0].clone())
        }
        assert_eq!(Value::from(f as Func).to_string(), "Function");
    }
}

#[cfg(all(test, feature = "serde"))]
mod serde_test {
    use super::*;
    use crate::number::Number;

    fn round_trip(value: Value) -> Value {
        let written = serde_json::to_string(&value)
            .unwrap_or_else(|e| panic!("{:?} should serialize: {}", value, e));
        serde_json::from_str(&written)
            .unwrap_or_else(|e| panic!("{} should read back: {}", written, e))
    }

    #[test]
    fn every_representable_variant_round_trips() {
        for value in [
            Value::Nil,
            Value::from(true),
            Value::from(false),
            Value::from("text"),
            Value::from(23u64),
            Value::from(-23i64),
            Value::Number(Number::float(1.5)),
            Value::Array(vec![Value::from(1u64), Value::from("a")]),
            Value::Map(
                [("k".to_owned(), Value::from(1u64))]
                    .into_iter()
                    .collect::<HashMap<_, _>>(),
            ),
        ] {
            assert_eq!(round_trip(value.clone()), value);
        }
    }

    /// `Object` is what `#[derive(Gtmpl)]` produces and used to be `serde(skip)`, so
    /// serializing one failed at runtime with "the enum variant Value::Object cannot
    /// be serialized" -- for the commonest shape there is.
    #[test]
    fn an_object_writes_as_a_map_and_reads_back_as_one() {
        let fields: HashMap<String, Value> =
            [("k".to_owned(), Value::from(1u64))].into_iter().collect();
        let written = serde_json::to_string(&Value::Object(fields.clone())).unwrap();
        assert_eq!(written, r#"{"k":1}"#);
        // A document cannot say "these came from a struct", so they come back as a map.
        assert_eq!(
            round_trip(Value::Object(fields.clone())),
            Value::Map(fields)
        );
    }

    /// Absence is not something a document can state, so it writes as `null` and
    /// reads back as an explicit nil.
    #[test]
    fn an_absent_value_writes_as_null_and_reads_back_as_nil() {
        assert_eq!(serde_json::to_string(&Value::NoValue).unwrap(), "null");
        assert_eq!(round_trip(Value::NoValue), Value::Nil);
    }

    #[test]
    fn a_function_is_refused_rather_than_written_as_something_else() {
        fn f(a: &[Value]) -> Result<Value, FuncError> {
            Ok(a[0].clone())
        }
        assert!(serde_json::to_string(&Value::from(f as Func)).is_err());
    }
}
