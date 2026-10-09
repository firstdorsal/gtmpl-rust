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
    NoValue,
    Nil,
    Bool(bool),
    String(String),
    #[cfg_attr(feature = "serde", serde(skip))]
    Object(HashMap<String, Value>),
    Map(HashMap<String, Value>),
    Array(Vec<Value>),
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
