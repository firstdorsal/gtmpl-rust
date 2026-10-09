//! The parts of Go's numeric literal syntax that Rust's own parsers do not accept.

/// The outcome of reading a hexadecimal float literal.
pub enum HexadecimalFloat {
    /// Not hexadecimal float syntax at all, so the integer and decimal-float readings
    /// still apply.
    NotHexadecimal,
    /// A literal Go would accept, and its value.
    Value(f64),
    /// Hexadecimal float syntax Go would reject.
    Illegal,
}

/// Go's `underscoreOK`: a separator may only sit between two digits, or directly after
/// a base prefix, and never at the end.
///
/// `0x_1p4` is therefore legal and `0x1_p4`, `0x1p_4` and `0x1p4_` are not.
pub fn separators_are_allowed(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut at = 0;
    if matches!(bytes.first(), Some(b'-') | Some(b'+')) {
        at = 1;
    }

    // What the previous character was: a digit or base prefix, a separator, or
    // something else. Nothing yet, at the start.
    #[derive(PartialEq)]
    enum Previous {
        Nothing,
        Digit,
        Separator,
        Other,
    }
    let mut previous = Previous::Nothing;

    let mut hexadecimal = false;
    if bytes.len() >= at + 2 && bytes[at] == b'0' {
        match bytes[at + 1] | 0x20 {
            b'b' | b'o' | b'x' => {
                hexadecimal = (bytes[at + 1] | 0x20) == b'x';
                // A base prefix counts as a digit, so a separator may follow it.
                previous = Previous::Digit;
                at += 2;
            }
            _ => {}
        }
    }

    while at < bytes.len() {
        let byte = bytes[at];
        at += 1;
        if byte.is_ascii_digit() || (hexadecimal && matches!(byte | 0x20, b'a'..=b'f')) {
            previous = Previous::Digit;
            continue;
        }
        if byte == b'_' {
            if previous != Previous::Digit {
                return false;
            }
            previous = Previous::Separator;
            continue;
        }
        if previous == Previous::Separator {
            return false;
        }
        previous = Previous::Other;
    }
    previous != Previous::Separator
}

/// Reads Go's hexadecimal float syntax, `0x1.8p1`, which `f64::from_str` does not
/// accept.
///
/// Go gets these from `strconv.ParseFloat`, which is correctly rounded, so this cannot
/// simply truncate. Instead the literal is turned into its *exact* decimal form and
/// handed to Rust's decimal parser, which is correctly rounded too: a hexadecimal
/// float is `m * 2^e`, and for a negative `e` that is `(m * 5^-e) * 10^e`, an integer
/// times a power of ten.
///
/// Only the leading hexadecimal digits are kept exactly. Beyond 53 bits of mantissa
/// the rounding decision depends on nothing but "is anything below still non-zero", so
/// the tail collapses into one sticky digit -- which also bounds the work a long
/// literal can ask for.
pub fn read_hexadecimal_float(text: &str) -> HexadecimalFloat {
    let (negative, body) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let Some(body) = body.strip_prefix("0x").or_else(|| body.strip_prefix("0X")) else {
        return HexadecimalFloat::NotHexadecimal;
    };
    // Go requires the binary exponent: `0x1.8` is not a number, and `0xff` is an
    // integer that is read elsewhere.
    let Some(exponent_at) = body.find(['p', 'P']) else {
        return HexadecimalFloat::NotHexadecimal;
    };
    if !separators_are_allowed(text) {
        return HexadecimalFloat::Illegal;
    }

    let mantissa_text = body[..exponent_at].replace('_', "");
    let exponent_text = body[exponent_at + 1..].replace('_', "");
    let (integer_part, fraction_part) = match mantissa_text.split_once('.') {
        Some((integer, fraction)) => (integer, fraction),
        None => (mantissa_text.as_str(), ""),
    };
    if integer_part.is_empty() && fraction_part.is_empty() {
        return HexadecimalFloat::Illegal;
    }
    let negative_exponent = exponent_text.starts_with('-');
    let binary_exponent = match exponent_text.parse::<i32>() {
        Ok(exponent) => exponent,
        Err(_) => {
            // Either the exponent is not a number at all, or it is one no float could
            // hold. Go only treats the overflow as a bad literal: an exponent that
            // small underflows to zero.
            let magnitude = exponent_text.trim_start_matches(['+', '-']);
            if magnitude.is_empty() || !magnitude.bytes().all(|b| b.is_ascii_digit()) {
                return HexadecimalFloat::Illegal;
            }
            return if negative_exponent {
                HexadecimalFloat::Value(if negative { -0.0 } else { 0.0 })
            } else {
                HexadecimalFloat::Illegal
            };
        }
    };

    let mut hex_digits: Vec<u32> = Vec::with_capacity(integer_part.len() + fraction_part.len());
    for c in integer_part.chars().chain(fraction_part.chars()) {
        match c.to_digit(16) {
            Some(digit) => hex_digits.push(digit),
            None => return HexadecimalFloat::Illegal,
        }
    }
    let zero = if negative { -0.0 } else { 0.0 };
    if hex_digits.iter().all(|digit| *digit == 0) {
        return HexadecimalFloat::Value(zero);
    }
    // Leading zeros carry no value -- the exponent comes from how many digits follow
    // the point, not from where the digits sit -- and keeping them would push the
    // significant ones past the window kept below, leaving the sticky digit to stand
    // for the whole number at the wrong magnitude.
    let leading_zeros = hex_digits.iter().take_while(|digit| **digit == 0).count();
    hex_digits.drain(..leading_zeros);

    // 20 hexadecimal digits are 80 bits, comfortably more than the 53 the mantissa
    // needs plus the two the rounding looks at, so a 21st digit carrying "something
    // below is non-zero" rounds exactly as the full literal would.
    const KEPT_DIGITS: usize = 20;
    let mut exponent = i64::from(binary_exponent) - 4 * fraction_part.len() as i64;
    if hex_digits.len() > KEPT_DIGITS {
        let sticky = hex_digits[KEPT_DIGITS..].iter().any(|digit| *digit != 0);
        exponent += 4 * (hex_digits.len() - KEPT_DIGITS - 1) as i64;
        hex_digits.truncate(KEPT_DIGITS);
        hex_digits.push(u32::from(sticky));
    }

    // The mantissa is at least one, so the value is at least 2^exponent, and it is
    // below 2^(4 * digits + exponent). Both bounds settle the extreme exponents before
    // any work is done on them.
    if exponent >= 1025 {
        return HexadecimalFloat::Illegal;
    }
    if 4 * hex_digits.len() as i64 + exponent <= -1075 {
        return HexadecimalFloat::Value(zero);
    }

    // Least significant decimal digit first.
    let mut decimal: Vec<u8> = vec![0];
    for digit in &hex_digits {
        multiply_and_add(&mut decimal, 16, *digit);
    }
    let decimal_exponent = if exponent >= 0 {
        for _ in 0..exponent {
            multiply_and_add(&mut decimal, 2, 0);
        }
        0
    } else {
        for _ in 0..-exponent {
            multiply_and_add(&mut decimal, 5, 0);
        }
        exponent
    };

    let mut exact = String::with_capacity(decimal.len() + 8);
    if negative {
        exact.push('-');
    }
    // `decimal` is non-zero here: an all-zero mantissa returned above, and multiplying
    // a non-zero value by 16, 2 or 5 cannot reach zero.
    for digit in decimal.iter().rev().skip_while(|digit| **digit == 0) {
        exact.push((b'0' + digit) as char);
    }
    exact.push('e');
    exact.push_str(&decimal_exponent.to_string());

    match exact.parse::<f64>() {
        // An overflow is a range error in Go, which reports it as a bad literal.
        Ok(value) if value.is_finite() => HexadecimalFloat::Value(value),
        _ => HexadecimalFloat::Illegal,
    }
}

/// `decimal = decimal * factor + addend`, on decimal digits held least significant
/// first.
fn multiply_and_add(decimal: &mut Vec<u8>, factor: u32, addend: u32) {
    let mut carry = addend;
    for digit in decimal.iter_mut() {
        let product = u32::from(*digit) * factor + carry;
        *digit = (product % 10) as u8;
        carry = product / 10;
    }
    while carry > 0 {
        decimal.push((carry % 10) as u8);
        carry /= 10;
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn value(text: &str) -> Option<f64> {
        match read_hexadecimal_float(text) {
            HexadecimalFloat::Value(value) => Some(value),
            _ => None,
        }
    }

    fn is_illegal(text: &str) -> bool {
        matches!(read_hexadecimal_float(text), HexadecimalFloat::Illegal)
    }

    /// Every expectation here came from running the literal through Go 1.26.8.
    #[test]
    fn valid_literals_match_go() {
        assert_eq!(value("0x1p4"), Some(16.0));
        assert_eq!(value("0X1P-2"), Some(0.25));
        assert_eq!(value("0x1.8p1"), Some(3.0));
        assert_eq!(value("0x.8p1"), Some(1.0));
        assert_eq!(value("0x1p+8"), Some(256.0));
        assert_eq!(value("-0x1p4"), Some(-16.0));
        assert_eq!(value("0x1_0p4"), Some(256.0));
        assert_eq!(value("0x_1p4"), Some(16.0));
        assert_eq!(value("0x1p-1074"), Some(5e-324));
        assert_eq!(value("0x1.fffffffffffffp1023"), Some(f64::MAX));
    }

    /// The point of the exact-decimal detour: these need rounding, and Go rounds them.
    #[test]
    fn literals_that_need_rounding_match_go() {
        assert_eq!(value("0x1.00000000000001p0"), Some(1.0));
        assert_eq!(value("0x1.0000000000000fp0"), Some(1.0000000000000002));
        assert_eq!(value("0x1.fffffffffffff8p0"), Some(2.0));
        assert_eq!(value("0x1.23456789abcdefp0"), Some(1.1377777777777778));
        assert_eq!(value("0x10000000000000001p0"), Some(1.8446744073709552e19));
        assert_eq!(value("0x1.8p-1074"), Some(1e-323));
    }

    /// Past 20 hexadecimal digits the tail collapses into one sticky digit, which is
    /// what keeps a long literal bounded. These are the cases where that tail decides
    /// the rounding, so a wrong sticky bit or a wrong exponent correction shows up.
    #[test]
    fn a_mantissa_longer_than_the_kept_digits_still_rounds_like_go() {
        assert_eq!(value("0x1.0000000000000000000001p0"), Some(1.0));
        assert_eq!(
            value("0x1.00000000000008000000001p0"),
            Some(1.0000000000000002)
        );
        assert_eq!(
            value("0x1fffffffffffffffffffffffp0"),
            Some(9.903520314283042e27)
        );
        // The pair that turns on the sticky digit alone: both have the same first 20
        // digits, and only what follows decides whether they round up.
        assert_eq!(
            value("0x1.fffffffffffff7ffffffffffp0"),
            Some(1.9999999999999998)
        );
        assert_eq!(value("0x1.fffffffffffff8000000001p0"), Some(2.0));
        assert_eq!(
            value("0x123456789abcdef0123456789p0"),
            Some(9.014404268289631e28)
        );
        assert_eq!(value("0x1.000000000000000000000000000001p0"), Some(1.0));
    }

    #[test]
    fn underflow_and_overflow_match_go() {
        // Half of the smallest subnormal ties to even, which is zero.
        assert_eq!(value("0x1p-1075"), Some(0.0));
        // Far enough under that the bound settles it before any arithmetic.
        assert_eq!(value("0x1p-1200"), Some(0.0));
        assert_eq!(value("0x0p99999999"), Some(0.0));
        assert!(is_illegal("0x1p1024"));
        assert!(is_illegal("0x1p99999999"));
    }

    #[test]
    fn malformed_literals_are_refused() {
        assert!(is_illegal("0xp4"));
        assert!(is_illegal("0x1p"));
        assert!(is_illegal("0x1_p4"));
        assert!(is_illegal("0x1p_4"));
        assert!(is_illegal("0x1p4_"));
        assert!(is_illegal("0x1pz"));
    }

    /// Without a binary exponent it is not this syntax, so the integer reading gets it.
    #[test]
    fn other_literals_are_left_alone() {
        assert!(matches!(
            read_hexadecimal_float("0xff"),
            HexadecimalFloat::NotHexadecimal
        ));
        assert!(matches!(
            read_hexadecimal_float("0x1.8"),
            HexadecimalFloat::NotHexadecimal
        ));
        assert!(matches!(
            read_hexadecimal_float("1.5"),
            HexadecimalFloat::NotHexadecimal
        ));
        assert!(matches!(
            read_hexadecimal_float("1e3"),
            HexadecimalFloat::NotHexadecimal
        ));
    }

    #[test]
    fn separator_rule_matches_go() {
        for legal in [
            "0x_1p4", "0x1_0p4", "1_000", "1_000.5", "0b1_0", "0o1_7", "1e1_0",
        ] {
            assert!(separators_are_allowed(legal), "{} should be legal", legal);
        }
        for illegal in [
            "0x1_p4", "0x1p_4", "0x1p4_", "_1", "1__0", "1_", "1._5", "1.5_",
        ] {
            assert!(
                !separators_are_allowed(illegal),
                "{} should be illegal",
                illegal
            );
        }
    }
}
