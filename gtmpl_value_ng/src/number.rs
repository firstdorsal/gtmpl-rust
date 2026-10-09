use std::cmp::{Ordering, PartialOrd};
use std::fmt;

/// Internal number format for `gtmpl_value_ng`.
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct Number {
    n: Num,
}

#[cfg(feature = "serde")]
impl serde::Serialize for Number {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.n.serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Number {
    fn deserialize<D>(deserializer: D) -> Result<Number, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Number {
            n: Num::deserialize(deserializer)?,
        })
    }
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged))]
#[derive(Copy, Clone, Debug)]
enum Num {
    U(u64),
    I(i64),
    F(f64),
}

/// `PartialOrd` for `Number`.
///
/// # Examples
///
/// ```rust
/// use gtmpl_value_ng::Number;
///
/// let i: Number = 23.into();
/// let f: Number = 23.42.into();
///
/// assert!(i < f);
/// ```
impl PartialOrd for Num {
    fn partial_cmp(&self, other: &Num) -> Option<Ordering> {
        match (*self, *other) {
            (Num::U(s), Num::U(o)) => s.partial_cmp(&o),
            (Num::I(s), Num::I(o)) => s.partial_cmp(&o),
            (Num::F(s), Num::F(o)) => s.partial_cmp(&o),
            (Num::I(_), Num::U(_)) => Some(Ordering::Less),
            (Num::U(_), Num::I(_)) => Some(Ordering::Greater),
            (Num::F(s), Num::I(o)) => s.partial_cmp(&(o as f64)),
            (Num::I(s), Num::F(o)) => (s as f64).partial_cmp(&o),
            (Num::F(s), Num::U(o)) => s.partial_cmp(&(o as f64)),
            (Num::U(s), Num::F(o)) => (s as f64).partial_cmp(&o),
        }
    }
}

/// `PartialEq` for `Number`.
///
/// # Examples
///
/// ```rust
/// use gtmpl_value_ng::Number;
///
/// let i: Number = 23.into();
/// let f: Number = 23.0.into();
///
/// assert!(i == f);
/// ```
impl PartialEq for Num {
    fn eq(&self, other: &Num) -> bool {
        match (*self, *other) {
            (Num::U(s), Num::U(o)) => s.eq(&o),
            (Num::I(s), Num::I(o)) => s.eq(&o),
            (Num::F(s), Num::F(o)) => s.eq(&o),
            _ => false,
        }
    }
}

/// Which of the three representations a [`Number`] holds.
///
/// Reading a number through `as_u64`/`as_i64`/`as_f64` means trying them in the right
/// order and handling a "none of them" case that cannot happen; this says it once.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NumberKind {
    Unsigned(u64),
    Signed(i64),
    Float(f64),
}

impl Number {
    /// What this number actually holds.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use gtmpl_value_ng::{Number, NumberKind};
    ///
    /// assert_eq!(Number::from(23u64).kind(), NumberKind::Unsigned(23));
    /// assert_eq!(Number::from(-23i64).kind(), NumberKind::Signed(-23));
    /// assert_eq!(Number::from(2.5).kind(), NumberKind::Float(2.5));
    /// ```
    pub fn kind(&self) -> NumberKind {
        match self.n {
            Num::U(n) => NumberKind::Unsigned(n),
            Num::I(n) => NumberKind::Signed(n),
            Num::F(n) => NumberKind::Float(n),
        }
    }

    /// Builds an unsigned number, keeping it unsigned.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use gtmpl_value_ng::Number;
    ///
    /// assert_eq!(Number::unsigned(23).as_u64(), Some(23));
    /// ```
    pub fn unsigned(n: u64) -> Self {
        Number { n: Num::U(n) }
    }

    /// Builds a signed number, keeping it signed even when it is not negative.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use gtmpl_value_ng::Number;
    ///
    /// assert_eq!(Number::signed(-23).as_i64(), Some(-23));
    /// ```
    pub fn signed(n: i64) -> Self {
        Number { n: Num::I(n) }
    }

    /// Builds a float, keeping it a float even when its value is whole.
    ///
    /// This is the difference to [`From<f64>`](#impl-From<f64>-for-Number), which
    /// stores a whole float as an integer so that `{{ 3.0 }}` renders `3` the way Go
    /// does. That collapse is lossy — `as_f64` then reports `None` — and this
    /// constructor is the way to avoid it.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use gtmpl_value_ng::Number;
    ///
    /// assert_eq!(Number::float(3.0).as_f64(), Some(3.0));
    /// assert_eq!(Number::from(3.0).as_f64(), None);
    /// ```
    pub fn float(n: f64) -> Self {
        Number { n: Num::F(n) }
    }

    /// ```rust
    /// use gtmpl_value_ng::Number;
    ///
    /// let big: Number = (i64::MAX as u64 + 10).into();
    ///
    /// assert!(big.as_u64().is_some());
    /// assert!(big.as_i64().is_none());
    /// assert!(big.as_f64().is_none());
    /// ```
    pub fn as_i64(&self) -> Option<i64> {
        match self.n {
            Num::U(n) => {
                if n <= (i64::MAX as u64) {
                    Some(n as i64)
                } else {
                    None
                }
            }
            Num::I(n) => Some(n),
            _ => None,
        }
    }
    /// ```rust
    /// use gtmpl_value_ng::Number;
    ///
    /// let neg: Number = (-10).into();
    ///
    /// assert!(neg.as_u64().is_none());
    /// assert!(neg.as_i64().is_some());
    /// assert!(neg.as_f64().is_none());
    /// ```
    pub fn as_u64(&self) -> Option<u64> {
        match self.n {
            Num::U(n) => Some(n),
            Num::I(n) => {
                if n >= 0 {
                    Some(n as u64)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// ```rust
    /// use gtmpl_value_ng::Number;
    ///
    /// let frac: Number = (10.1).into();
    ///
    /// assert!(frac.as_u64().is_none());
    /// assert!(frac.as_i64().is_none());
    /// assert!(frac.as_f64().is_some());
    /// ```
    pub fn as_f64(&self) -> Option<f64> {
        match self.n {
            Num::F(n) => Some(n),
            _ => None,
        }
    }
}

impl fmt::Display for Number {
    /// Renders the number the way Go prints it, which for a float means Go's `%v`:
    /// the shortest form that round-trips, in exponent notation outside a middling
    /// range. Rust's own `Display` would write `1e20` as `100000000000000000000`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.n {
            Num::U(n) => write!(f, "{}", n),
            Num::I(n) => write!(f, "{}", n),
            Num::F(n) => f.write_str(&crate::go_fmt::format_float(
                n,
                crate::go_fmt::FloatFormat::General,
                None,
            )),
        }
    }
}

macro_rules! from_i {
    ($($ty:ident)*) => {
        $(
            impl From<$ty> for Number {
                fn from(n: $ty) -> Self {
                    Number {
                        n: if n < 0 { Num::I(i64::from(n)) } else { Num::U(n as u64) }
                    }
                }
            }
        )*
    };
}

from_i!(
    i64 i32 i16 i8
);

impl From<isize> for Number {
    fn from(n: isize) -> Self {
        Number {
            n: if n < 0 {
                Num::I(n as i64)
            } else {
                Num::U(n as u64)
            },
        }
    }
}

macro_rules! from_u {
    ($($ty:ident)*) => {
        $(
            impl From<$ty> for Number {
                fn from(n: $ty) -> Self {
                    Number {
                        n: Num::U(u64::from(n)),
                    }
                }
            }
        )*
    };
}

from_u!(
    u64 u32 u16 u8
);

impl From<usize> for Number {
    fn from(n: usize) -> Self {
        Number {
            n: Num::U(n as u64),
        }
    }
}

/// A float whose value is whole is stored as an integer, so `{{ 3.0 }}` renders `3`
/// the way Go does. That only holds while the value actually fits an integer: an
/// `as` cast saturates, so `1e20 as u64` used to yield `u64::MAX` and quietly replace
/// the number with a different one. And "whole" has to mean exactly whole -- the test
/// used to be `fract() < EPSILON`, which is true of every magnitude below about
/// 2e-16, so `1e-20` collapsed to `0`.
macro_rules! from_f {
    ($($ty:ident)*) => {
        $(
            impl From<$ty> for Number {
                fn from(n: $ty) -> Self {
                    let value = f64::from(n);
                    // Negative zero is whole, but collapsing it to the integer `0`
                    // would drop its sign and render `0` where Go renders `-0`.
                    let num = if value.fract() != 0.0 || (value == 0.0 && value.is_sign_negative()) {
                        Num::F(value)
                    } else if value.is_sign_negative() {
                        if value >= (i64::MIN as f64) {
                            Num::I(value as i64)
                        } else {
                            Num::F(value)
                        }
                    } else if value < (u64::MAX as f64) {
                        Num::U(value as u64)
                    } else {
                        Num::F(value)
                    };
                    Number {
                        n: num,
                    }
                }
            }
        )*
    };
}

from_f!(
    f64 f32
);

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_i() {
        let num: Number = (-23i8).into();
        assert_eq!(num.as_i64(), Some(-23i64));
        assert_eq!(num.as_u64(), None);
        assert_eq!(num.as_f64(), None);
    }

    #[test]
    fn test_u() {
        let num: Number = 23u8.into();
        assert_eq!(num.as_i64(), Some(23i64));
        assert_eq!(num.as_u64(), Some(23u64));
        assert_eq!(num.as_f64(), None);
    }

    #[test]
    fn test_u_max() {
        let num: Number = u64::MAX.into();
        assert_eq!(num.as_i64(), None);
        assert_eq!(num.as_u64(), Some(u64::MAX));
        assert_eq!(num.as_f64(), None);
    }

    #[test]
    fn test_f() {
        let num: Number = 23.42f64.into();
        assert_eq!(num.as_i64(), None);
        assert_eq!(num.as_u64(), None);
        assert_eq!(num.as_f64(), Some(23.42f64));
        let num: Number = (-23.42f64).into();
        assert_eq!(num.as_i64(), None);
        assert_eq!(num.as_u64(), None);
        assert_eq!(num.as_f64(), Some(-23.42f64));
    }

    #[test]
    fn test_le() {
        let a: Number = 23.0f64.into();
        let b: Number = 24u64.into();
        assert!(a <= b);
    }

    #[test]
    fn test_ge() {
        let a: Number = 1u64.into();
        let b: Number = (-1i64).into();
        assert!(a > b);
    }
}
