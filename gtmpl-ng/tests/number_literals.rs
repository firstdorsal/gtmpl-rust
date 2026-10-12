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
        "1_", "1__0", // separators that do not sit between digits
        "08", "09",  // not octal
        "0xg", // not hexadecimal
        // Hexadecimal float syntax the lexer now scans and the parser has to refuse:
        // Go requires the binary exponent, and an exponent no float can hold.
        "0x1.8", "0xp4", "0x1p", "0x1pz", "0x1p1024", "0x1_p4", "0x1p_4", "0x1p4_",
    ] {
        let error = render(literal).expect_err(&format!("{} should be rejected", literal));
        // The message names the literal, as Go's does -- an `is_err()` on its own
        // would also hold if it failed for some unrelated reason. `0xg` and `0x1pz`
        // are caught by the lexer and say "bad number syntax" where the parser says
        // "illegal", which is the same split Go has.
        assert!(
            error.contains("number syntax") && error.contains(literal),
            "{} should be reported as a bad number naming itself, said {:?}",
            literal,
            error
        );
    }
}

/// `_1` starts with an underscore, so it is an identifier rather than a number -- Go
/// reports it as an undefined function too, which is why it is not in the list above.
#[test]
fn a_leading_underscore_is_an_identifier_not_a_number() {
    let error = render("_1").expect_err("_1 is not a function");
    assert!(
        error.contains("function _1 not defined"),
        "said {:?}",
        error
    );
}

/// A separator has to sit between two digits in a float as well, so `1_0.5` is legal
/// and `1._5` is not. The float path used to strip every `_` without looking.
#[test]
fn malformed_float_separators_are_rejected() {
    for literal in ["1._5", "1.5_", "1_.5", "1e_3", "1e3_", ".5_", "1_e3"] {
        let error = render(literal).expect_err(&format!("{} should be rejected", literal));
        assert!(
            error.contains("illegal number syntax"),
            "{} should be reported as an illegal number, said {:?}",
            literal,
            error
        );
    }
}

/// A literal reaches the evaluator by a different path as a call argument than as a
/// bare action, so both are covered.
#[test]
fn literals_work_as_call_arguments() {
    for (literal, expected) in [
        ("0x10", "16"),
        ("017", "15"),
        ("0b101", "5"),
        ("1_000", "1000"),
        ("-0x10", "-16"),
    ] {
        let source = format!("{{{{ printf \"%d\" {} }}}}", literal);
        assert_eq!(
            common::render(&source, Value::Nil).as_deref(),
            Ok(expected),
            "{}",
            source
        );
    }
}

/// `nil` is usable as an argument, as in Go.
#[test]
fn nil_is_a_usable_argument() {
    assert_eq!(
        common::render("{{ eq nil nil }}", Value::Nil).as_deref(),
        Ok("true")
    );
    assert_eq!(
        common::render("{{ ne nil nil }}", Value::Nil).as_deref(),
        Ok("false")
    );
}

#[test]
fn character_constants() {
    assert_eq!(render("'a'").as_deref(), Ok("97"));
    assert_eq!(render(r"'\n'").as_deref(), Ok("10"));
}

/// Values beyond `i64` read correctly as unsigned.
///
/// This is a deliberate difference, not verified-matching behaviour: real Go fails at
/// execution time with "overflows int" for both of these. Rendering the value the
/// source asked for is not worse, but anyone comparing this suite against Go should
/// know it is a choice.
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
