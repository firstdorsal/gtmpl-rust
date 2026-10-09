//! Rendering a value the way Go prints it.
//!
//! Rust's `Display` for `f64` and Go's `strconv.FormatFloat` agree on the digits --
//! both produce the shortest decimal that round-trips -- but not on when to switch to
//! exponent notation, nor on how to write the exponent. Rust prints `1e20` as
//! `100000000000000000000` and `1.234568e3`; Go prints `1e+20` and `1.234568e+03`.
//!
//! This module implements Go's rules so that a `Value` renders the same whether it
//! goes through `Display` or through the engine's `printf`.

use std::fmt::Write;

/// Which of Go's float formats to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatFormat {
    /// `%f`: no exponent, a fixed number of digits after the point.
    Fixed,
    /// `%e`: one digit before the point and a decimal exponent.
    Exponent,
    /// `%E`, as `Exponent` with an upper-case `E`.
    ExponentUpper,
    /// `%g` and `%v`: `Fixed` or `Exponent`, whichever Go's rule picks.
    General,
    /// `%G`, as `General` with an upper-case `E`.
    GeneralUpper,
}

/// Formats a float the way Go's `strconv.FormatFloat` does.
///
/// `precision` counts digits after the point for `Fixed` and `Exponent`, and
/// significant digits for `General`. `None` asks for the shortest representation that
/// round-trips, which is what `%v` and a bare `%g` use.
///
/// # Examples
///
/// ```rust
/// use gtmpl_value_ng::{format_float, FloatFormat};
///
/// assert_eq!(format_float(1e20, FloatFormat::General, None), "1e+20");
/// assert_eq!(format_float(3.14, FloatFormat::General, None), "3.14");
/// assert_eq!(format_float(1234.5678, FloatFormat::Exponent, Some(6)), "1.234568e+03");
/// assert_eq!(format_float(f64::INFINITY, FloatFormat::General, None), "+Inf");
/// ```
pub fn format_float(value: f64, format: FloatFormat, precision: Option<usize>) -> String {
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value.is_infinite() {
        // Go always writes the sign of an infinity, even without the `+` flag.
        return if value.is_sign_negative() {
            "-Inf".to_owned()
        } else {
            "+Inf".to_owned()
        };
    }

    let magnitude = value.abs();
    let sign = if value.is_sign_negative() { "-" } else { "" };
    let body = match format {
        // Rust's fixed formatting already matches Go's: both round the binary value
        // correctly and then write every digit.
        FloatFormat::Fixed => format!("{:.*}", precision.unwrap_or(6), magnitude),
        FloatFormat::Exponent | FloatFormat::ExponentUpper => {
            let digits_after_point = precision.unwrap_or(6);
            let (digits, exponent) = decimal_digits(magnitude, Some(digits_after_point + 1), false);
            render_exponent(
                &digits,
                exponent,
                digits_after_point,
                format.exponent_char(),
            )
        }
        FloatFormat::General | FloatFormat::GeneralUpper => {
            let (digits, exponent) = decimal_digits(magnitude, precision.map(|p| p.max(1)), true);
            render_general(&digits, exponent, precision, format.exponent_char())
        }
    };
    format!("{}{}", sign, body)
}

impl FloatFormat {
    fn exponent_char(self) -> char {
        match self {
            FloatFormat::ExponentUpper | FloatFormat::GeneralUpper => 'E',
            _ => 'e',
        }
    }
}

/// Decomposes a non-negative, finite float into its decimal digits and the power of
/// ten of the first digit, so the value is `<d0>.<d1><d2>… * 10^exponent`.
///
/// `significant` is how many digits to round to; `None` asks for the shortest set that
/// round-trips. Go's `%g` drops trailing zeros left over from rounding, `%e` keeps
/// them, which is what `trim` selects.
fn decimal_digits(value: f64, significant: Option<usize>, trim: bool) -> (String, i32) {
    let formatted = match significant {
        Some(count) => format!("{:.*e}", count - 1, value),
        None => format!("{:e}", value),
    };
    let (mantissa, exponent) = formatted
        .split_once('e')
        .expect("Rust's LowerExp always writes an exponent");
    let mut digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let exponent: i32 = exponent.parse().expect("the exponent is an integer");

    if trim {
        let kept = digits.trim_end_matches('0').len().max(1);
        digits.truncate(kept);
    }
    (digits, exponent)
}

/// `d.ddde±dd`, with the exponent signed and at least two digits wide, as Go writes it.
fn render_exponent(
    digits: &str,
    exponent: i32,
    digits_after_point: usize,
    exponent_char: char,
) -> String {
    let mut out = String::with_capacity(digits.len() + 6);
    out.push_str(&digits[..1]);
    if digits_after_point > 0 {
        out.push('.');
        let fraction = &digits[1..];
        out.push_str(fraction);
        for _ in fraction.len()..digits_after_point {
            out.push('0');
        }
    }
    out.push(exponent_char);
    out.push(if exponent < 0 { '-' } else { '+' });
    let _ = write!(out, "{:02}", exponent.unsigned_abs());
    out
}

/// Go's `%g`: exponent notation when the exponent is below -4 or at least as large as
/// the precision, fixed notation otherwise. For the shortest representation Go uses 6
/// as that threshold, which is why `1e+06` prints with an exponent but `999999` does
/// not.
fn render_general(
    digits: &str,
    exponent: i32,
    precision: Option<usize>,
    exponent_char: char,
) -> String {
    let digit_count = digits.len() as i32;
    let point_position = exponent + 1;
    let threshold = match precision {
        None => 6,
        Some(precision) => {
            let precision = precision.max(1) as i32;
            if precision > digit_count && digit_count >= point_position {
                digit_count
            } else {
                precision
            }
        }
    };

    if exponent < -4 || exponent >= threshold {
        render_exponent(
            digits,
            exponent,
            (digit_count - 1).max(0) as usize,
            exponent_char,
        )
    } else {
        render_fixed_from_digits(digits, point_position)
    }
}

/// Writes already-rounded digits in positional notation, so no second rounding can
/// disagree with the first.
fn render_fixed_from_digits(digits: &str, point_position: i32) -> String {
    let digit_count = digits.len() as i32;
    let mut out = String::with_capacity(digits.len() + 8);

    if point_position <= 0 {
        out.push_str("0.");
        for _ in 0..-point_position {
            out.push('0');
        }
        out.push_str(digits);
        return out;
    }

    if point_position >= digit_count {
        out.push_str(digits);
        for _ in digit_count..point_position {
            out.push('0');
        }
        return out;
    }

    let split = point_position as usize;
    out.push_str(&digits[..split]);
    out.push('.');
    out.push_str(&digits[split..]);
    out
}

#[cfg(test)]
mod test {
    use super::*;

    /// Every expectation here was taken from Go 1.26.8 rather than reasoned out.
    // The literals are Go's reference values; `3.14` is not meant as pi.
    #[allow(clippy::approx_constant)]
    #[test]
    fn shortest_matches_go() {
        let cases: &[(f64, &str)] = &[
            (3.0, "3"),
            (3.14, "3.14"),
            (0.5, "0.5"),
            (100000.0, "100000"),
            (999999.0, "999999"),
            (1e6, "1e+06"),
            (1234567.0, "1.234567e+06"),
            (123456789.0, "1.23456789e+08"),
            (1e20, "1e+20"),
            (1e21, "1e+21"),
            (1e-4, "0.0001"),
            (1e-5, "1e-05"),
            (1e-7, "1e-07"),
            (1e-20, "1e-20"),
            (1e300, "1e+300"),
            (1234.5678, "1234.5678"),
            (-1234.5678, "-1234.5678"),
            (2.5, "2.5"),
            (-2.5, "-2.5"),
            (1.0 / 3.0, "0.3333333333333333"),
            (999999.5, "999999.5"),
            (0.0, "0"),
            (-0.0, "-0"),
            (f64::INFINITY, "+Inf"),
            (f64::NEG_INFINITY, "-Inf"),
        ];
        for (value, expected) in cases {
            assert_eq!(
                format_float(*value, FloatFormat::General, None),
                *expected,
                "shortest form of {:?}",
                value
            );
        }
        assert_eq!(format_float(f64::NAN, FloatFormat::General, None), "NaN");
    }

    #[test]
    fn exponent_form_matches_go() {
        assert_eq!(
            format_float(1234.5678, FloatFormat::Exponent, Some(6)),
            "1.234568e+03"
        );
        assert_eq!(
            format_float(1234.5678, FloatFormat::ExponentUpper, Some(6)),
            "1.234568E+03"
        );
        assert_eq!(
            format_float(1234.5, FloatFormat::Exponent, Some(0)),
            "1e+03"
        );
        assert_eq!(
            format_float(1e300, FloatFormat::Exponent, Some(6)),
            "1.000000e+300"
        );
        assert_eq!(
            format_float(1e-7, FloatFormat::Exponent, Some(6)),
            "1.000000e-07"
        );
        assert_eq!(
            format_float(3.0, FloatFormat::Exponent, Some(6)),
            "3.000000e+00"
        );
    }

    #[test]
    fn general_with_precision_matches_go() {
        assert_eq!(
            format_float(1234.5678, FloatFormat::General, Some(3)),
            "1.23e+03"
        );
        assert_eq!(format_float(3.0, FloatFormat::General, Some(5)), "3");
        assert_eq!(format_float(1e20, FloatFormat::General, Some(6)), "1e+20");
        assert_eq!(format_float(0.996, FloatFormat::General, Some(2)), "1");
    }

    // `3.14159` is a Go reference value, not pi.
    #[allow(clippy::approx_constant)]
    #[test]
    fn fixed_form_matches_rust_and_go() {
        assert_eq!(format_float(3.14159, FloatFormat::Fixed, Some(2)), "3.14");
        assert_eq!(format_float(1.0, FloatFormat::Fixed, None), "1.000000");
        assert_eq!(format_float(-3.5, FloatFormat::Fixed, Some(2)), "-3.50");
    }
}
