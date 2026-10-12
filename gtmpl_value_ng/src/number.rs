use std::cmp::{Ordering, PartialOrd};
use std::fmt;

/// Internal number format for `gtmpl_value_ng`.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
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
/// Exact across all three representations: a `u64` past the `f64` mantissa is not
/// widened to compare it, and a float is split into its integral and fractional parts
/// rather than the integer being pushed through `as f64`.
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
///
/// // `u64::MAX` is 18446744073709551615 and the nearest float above it is 2^64,
/// // 18446744073709551616. Widening the integer to compare them makes the two look
/// // equal; splitting the float does not.
/// assert!(Number::unsigned(u64::MAX) < Number::float(1.8446744073709552e19));
/// ```
impl PartialOrd for Num {
    fn partial_cmp(&self, other: &Num) -> Option<Ordering> {
        match (*self, *other) {
            (Num::U(s), Num::U(o)) => Some(s.cmp(&o)),
            (Num::I(s), Num::I(o)) => Some(s.cmp(&o)),
            (Num::F(s), Num::F(o)) => s.partial_cmp(&o),
            // A signed value is below an unsigned one only once it is negative.
            // `Number::signed` can hold a non-negative value, so this has to look
            // rather than assume.
            (Num::I(s), Num::U(o)) => Some(if s < 0 {
                Ordering::Less
            } else {
                (s as u64).cmp(&o)
            }),
            (Num::U(s), Num::I(o)) => Some(if o < 0 {
                Ordering::Greater
            } else {
                s.cmp(&(o as u64))
            }),
            (Num::F(s), Num::I(o)) => compare_float_to_signed(s, o),
            (Num::I(s), Num::F(o)) => compare_float_to_signed(o, s).map(Ordering::reverse),
            (Num::F(s), Num::U(o)) => compare_float_to_unsigned(s, o),
            (Num::U(s), Num::F(o)) => compare_float_to_unsigned(o, s).map(Ordering::reverse),
        }
    }
}

/// `PartialEq` for `Number`, defined through the ordering so the two can never
/// disagree -- `ge` answering `true` where `eq` answered `false` for the same pair was
/// exactly the bug that defining them separately produced.
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
/// // Across representations too, which a whole float only reaches through
/// // `Number::float` or a deserializer.
/// assert_eq!(Number::float(3.0), Number::unsigned(3));
/// assert_eq!(Number::signed(3), Number::unsigned(3));
/// ```
impl PartialEq for Num {
    fn eq(&self, other: &Num) -> bool {
        self.partial_cmp(other) == Some(Ordering::Equal)
    }
}

/// Compares a float against a signed integer without widening either.
///
/// `trunc` is exact, and so is the cast once the value is known to be in range, so the
/// integral parts compare as integers and the fraction only breaks a tie.
fn compare_float_to_signed(value: f64, integer: i64) -> Option<Ordering> {
    if value.is_nan() {
        return None;
    }
    // 2^63 and -2^63 are the first values outside an i64, both exactly representable.
    if value >= 9_223_372_036_854_775_808.0 {
        return Some(Ordering::Greater);
    }
    if value < -9_223_372_036_854_775_808.0 {
        return Some(Ordering::Less);
    }
    let truncated = value.trunc();
    Some((truncated as i64).cmp(&integer).then_with(|| {
        // `trunc` rounds toward zero, so the remainder carries the value's sign.
        let fraction = value - truncated;
        if fraction > 0.0 {
            Ordering::Greater
        } else if fraction < 0.0 {
            Ordering::Less
        } else {
            Ordering::Equal
        }
    }))
}

/// Compares a float against an unsigned integer without widening either.
fn compare_float_to_unsigned(value: f64, integer: u64) -> Option<Ordering> {
    if value.is_nan() {
        return None;
    }
    if value < 0.0 {
        return Some(Ordering::Less);
    }
    // 2^64 is the first value outside a u64, exactly representable.
    if value >= 18_446_744_073_709_551_616.0 {
        return Some(Ordering::Greater);
    }
    let truncated = value.trunc();
    Some((truncated as u64).cmp(&integer).then_with(|| {
        let fraction = value - truncated;
        if fraction > 0.0 {
            Ordering::Greater
        } else {
            Ordering::Equal
        }
    }))
}

/// Which of the three representations a [`Number`] holds.
///
/// Reading a number through `as_u64`/`as_i64`/`as_f64` means trying them in the right
/// order and handling a "none of them" case that cannot happen; this says it once.
///
/// Deliberately exhaustive: `#[non_exhaustive]` would force back the fallback arm
/// this type exists to remove, so a fourth representation would be a breaking change
/// and is meant to be.
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
    /// This is the difference to the `From<f64>` conversion, which
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
            Num::F(n) => {
                crate::go_fmt::write_float(f, n, crate::go_fmt::FloatFormat::General, None)
            }
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

    /// The three constructors can build states the `From` impls never produced --
    /// a non-negative `Signed`, and a whole `Float` -- and the comparison arms used to
    /// take a shortcut that assumed they could not exist. `ge` then answered `true`
    /// where `eq` answered `false` for the same pair.
    #[test]
    fn comparison_is_exact_across_representations() {
        assert_eq!(Number::signed(5), Number::unsigned(5));
        assert_eq!(Number::float(3.0), Number::unsigned(3));
        assert_eq!(Number::float(-3.0), Number::signed(-3));
        assert!(Number::signed(5) > Number::unsigned(3));
        assert!(Number::signed(-5) < Number::unsigned(0));
        assert!(Number::float(3.5) > Number::unsigned(3));
        assert!(Number::float(3.5) < Number::unsigned(4));
        assert!(Number::float(-3.5) < Number::signed(-3));
        assert!(Number::float(-3.5) > Number::signed(-4));
    }

    /// Widening either side to `f64` to compare them loses the distinction once the
    /// integer is past the mantissa, so neither side is widened.
    #[test]
    fn comparison_does_not_widen_past_the_mantissa() {
        // 2^64, the first float above u64::MAX.
        let just_above = Number::float(18_446_744_073_709_551_616.0);
        assert!(Number::unsigned(u64::MAX) < just_above);
        assert_ne!(Number::unsigned(u64::MAX), just_above);
        // Two integers one apart, both of which widen to the same f64.
        assert!(Number::unsigned(u64::MAX - 1) < Number::unsigned(u64::MAX));
        // -9223372036854775807.0 is not representable and rounds to exactly i64::MIN,
        // so the comparison needs the next float that really is above it.
        assert_eq!(
            Number::signed(i64::MIN),
            Number::float(-9_223_372_036_854_775_808.0)
        );
        assert!(Number::signed(i64::MIN) < Number::float(-9_223_372_036_854_774_784.0));
    }

    #[test]
    fn a_nan_compares_to_nothing() {
        let nan = Number::float(f64::NAN);
        assert_eq!(nan.partial_cmp(&Number::unsigned(1)), None);
        assert_eq!(Number::unsigned(1).partial_cmp(&nan), None);
        assert_ne!(nan, Number::float(f64::NAN));
    }

    /// A whole float is stored as an integer so `{{ 3.0 }}` renders `3` as it does in
    /// Go. That only holds while the value fits an integer: an `as` cast saturates,
    /// so this used to replace the number with a different one.
    #[test]
    fn a_float_past_the_integers_stays_a_float() {
        assert_eq!(Number::from(1e20).kind(), NumberKind::Float(1e20));
        assert_eq!(Number::from(1e20).as_u64(), None);
        assert_eq!(Number::from(-1e20).kind(), NumberKind::Float(-1e20));
        assert_eq!(Number::from(-1e20).as_i64(), None);
        assert_eq!(Number::from(1e300).kind(), NumberKind::Float(1e300));

        // The boundaries the collapse branches on: 2^64 is the first float outside a
        // u64, and -2^63 is the last one inside an i64.
        assert_eq!(
            Number::from(18_446_744_073_709_551_616.0).kind(),
            NumberKind::Float(18_446_744_073_709_551_616.0)
        );
        assert_eq!(
            Number::from(-9_223_372_036_854_775_808.0).kind(),
            NumberKind::Signed(i64::MIN)
        );
    }

    /// "Whole" used to be tested as `fract() < EPSILON`, which every magnitude below
    /// about 2e-16 satisfies, so a tiny float collapsed to zero.
    #[test]
    fn a_tiny_float_is_not_collapsed_to_zero() {
        assert_eq!(Number::from(1e-20).kind(), NumberKind::Float(1e-20));
        assert_eq!(Number::from(1e-20).as_f64(), Some(1e-20));
        assert_eq!(
            Number::from(f64::MIN_POSITIVE).as_f64(),
            Some(f64::MIN_POSITIVE)
        );
        assert_eq!(Number::from(5e-324f64).as_f64(), Some(5e-324));
    }

    /// Negative zero is whole, but as an integer it loses its sign and renders `0`
    /// where Go renders `-0`.
    #[test]
    fn negative_zero_stays_a_float() {
        match Number::from(-0.0).kind() {
            NumberKind::Float(f) => assert!(f.is_sign_negative() && f == 0.0),
            other => panic!("expected a float, got {:?}", other),
        }
        assert_eq!(Number::from(-0.0).to_string(), "-0");
        assert_eq!(Number::from(0.0).kind(), NumberKind::Unsigned(0));
    }

    /// A whole float still reads as an integer, which is what makes `{{ 3.0 }}`
    /// render `3`.
    #[test]
    fn a_whole_float_in_range_still_collapses() {
        assert_eq!(Number::from(3.0).kind(), NumberKind::Unsigned(3));
        assert_eq!(Number::from(-3.0).kind(), NumberKind::Signed(-3));
        assert_eq!(Number::from(1e3).kind(), NumberKind::Unsigned(1000));
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
