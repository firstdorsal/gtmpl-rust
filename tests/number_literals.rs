//! Number literals follow Go's integer literal syntax: base prefixes, a bare leading
//! zero for octal, and `_` separators that have to sit between digits.
//!
//! Every expectation was taken from Go 1.26.6 rendering `{{ <literal> }}`.

mod common;

use gtmpl_ng::Value;

fn render(literal: &str) -> Result<String, String> {
    common::render(&format!("{{{{ {} }}}}", literal), Value::Nil)
}

#[test]
fn decimal_literals() {
    for (literal, expected) in [
        ("42", "42"),
        ("-5", "-5"),
        ("+7", "7"),
        ("0", "0"),
        ("-0", "0"),
        ("1_000", "1000"),
        ("1_000_000", "1000000"),
    ] {
        assert_eq!(render(literal).as_deref(), Ok(expected), "{}", literal);
    }
}

#[test]
fn hexadecimal_literals() {
    for (literal, expected) in [
        ("0x10", "16"),
        ("0X1f", "31"),
        ("0xff", "255"),
        ("0x0", "0"),
        ("-0x10", "-16"),
        ("0xDEADBEEF", "3735928559"),
        ("0x_ff", "255"),
    ] {
        assert_eq!(render(literal).as_deref(), Ok(expected), "{}", literal);
    }
}

#[test]
fn binary_literals() {
    for (literal, expected) in [("0b101", "5"), ("0B101", "5"), ("0b1010_1010", "170")] {
        assert_eq!(render(literal).as_deref(), Ok(expected), "{}", literal);
    }
}

/// Octal is written either with an `0o` prefix or, as in Go, a bare leading zero.
#[test]
fn octal_literals() {
    for (literal, expected) in [
        ("0o17", "15"),
        ("0O17", "15"),
        ("017", "15"),
        ("0755", "493"),
        ("-0o17", "-15"),
        ("-017", "-15"),
    ] {
        assert_eq!(render(literal).as_deref(), Ok(expected), "{}", literal);
    }
}

#[test]
fn float_literals() {
    for (literal, expected) in [
        ("3.14", "3.14"),
        (".5", "0.5"),
        ("5.", "5"),
        ("1e3", "1000"),
        ("1E3", "1000"),
        ("1e-3", "0.001"),
        ("2.5e2", "250"),
        ("1_0.5", "10.5"),
    ] {
        assert_eq!(render(literal).as_deref(), Ok(expected), "{}", literal);
    }
}

/// A literal that is shaped like a number but is not a legal one has to be rejected,
/// not quietly read as zero.
#[test]
fn malformed_literals_are_rejected() {
    for literal in [
        "-", "+", // a bare sign used to evaluate to 0
        "0x", "0b", "0o", "0x_", // a prefix with no digits
        "1_", "_1", "1__0", // separators that do not sit between digits
        "08", "09",  // not octal
        "0xg", // not hexadecimal
    ] {
        assert!(
            render(literal).is_err(),
            "{} should be rejected, got {:?}",
            literal,
            render(literal)
        );
    }
}

#[test]
fn character_constants() {
    assert_eq!(render("'a'").as_deref(), Ok("97"));
    assert_eq!(render(r"'\n'").as_deref(), Ok("10"));
}

/// Values beyond `i64` still read correctly as unsigned. Go reports an error for these
/// when printing; rendering the actual value is not worse.
#[test]
fn values_above_the_signed_range() {
    assert_eq!(
        render("9223372036854775807").as_deref(),
        Ok("9223372036854775807")
    );
    assert_eq!(
        render("9223372036854775808").as_deref(),
        Ok("9223372036854775808")
    );
    assert_eq!(
        render("18446744073709551615").as_deref(),
        Ok("18446744073709551615")
    );
}
