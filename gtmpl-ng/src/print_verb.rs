//! Formatting one argument for one verb, the way Go's `fmt` does it.
//!
//! Go never fails on a verb. When one does not apply to its argument it writes a
//! `%!verb(type=value)` marker into the output and carries on, so nothing here returns
//! a `Result` either.
//!
//! The flag and width rules are Go's, which differ from Rust's in ways that are easy
//! to get wrong: zero padding goes between the sign and the digits, `%#o` writes a
//! bare `0` rather than Rust's `0o`, a string pads on the left by default where Rust
//! pads on the right, and an explicit precision makes Go ignore the zero flag.

use std::fmt::Write;

use crate::printf::FormatParams;

use gtmpl_value_ng::{format_float, FloatFormat, NumberKind, Value};

/// Formats one argument for one verb.
pub fn print(p: &FormatParams, verb: char, val: &Value) -> String {
    // Go's `%T` names the type whatever the value is, and pads it like a string.
    if verb == 'T' {
        return pad_string(p, type_name(val).unwrap_or("<nil>"));
    }
    match *val {
        Value::Number(ref n) => match n.kind() {
            NumberKind::Unsigned(u) => integer(p, verb, i128::from(u), val),
            NumberKind::Signed(i) => integer(p, verb, i128::from(i), val),
            NumberKind::Float(f) => match verb {
                'e' => float(p, f, FloatFormat::Exponent),
                'E' => float(p, f, FloatFormat::ExponentUpper),
                'f' | 'F' => float(p, f, FloatFormat::Fixed),
                'v' | 'g' => float(p, f, FloatFormat::General),
                'G' => float(p, f, FloatFormat::GeneralUpper),
                _ => bad_verb(p, verb, val),
            },
        },
        // An absent or a nil value has no representation of its own, so `%v` renders it
        // the way Go's formatter does and every other verb gets the marker.
        Value::Nil | Value::NoValue => match verb {
            'v' => pad_text(p, "<nil>"),
            _ => bad_verb(p, verb, val),
        },
        Value::Bool(b) => match verb {
            'v' | 't' => pad_text(p, if b { "true" } else { "false" }),
            _ => bad_verb(p, verb, val),
        },
        Value::String(ref s) => match verb {
            's' | 'v' => pad_string(p, s),
            // For `%x` Go's precision counts bytes to encode, not characters of the
            // result, and the quotes `%q` adds are never counted either -- both
            // truncate the string first and pad afterwards.
            'x' => pad_text(p, &hex_bytes(p, s, false)),
            'X' => pad_text(p, &hex_bytes(p, s, true)),
            'q' => pad_text(p, &quote_string(p, truncate(p, s))),
            _ => bad_verb(p, verb, val),
        },
        // Go applies the verb to each element rather than to the container, so a verb
        // that does not fit marks the offending element: `[1 %!d(string=a)]`.
        Value::Array(ref items) => {
            let rendered: Vec<String> = items.iter().map(|item| print(p, verb, item)).collect();
            format!("[{}]", rendered.join(" "))
        }
        Value::Map(ref entries) | Value::Object(ref entries) => {
            // Go has sorted map keys since 1.12, so the same data prints the same way
            // twice.
            let mut keys: Vec<&String> = entries.keys().collect();
            keys.sort();
            let rendered: Vec<String> = keys
                .into_iter()
                .map(|key| {
                    format!(
                        "{}:{}",
                        print(p, verb, &Value::String(key.clone())),
                        print(p, verb, &entries[key])
                    )
                })
                .collect();
            format!("map[{}]", rendered.join(" "))
        }
        Value::Function(_) => match verb {
            'v' => pad_text(p, &val.to_string()),
            _ => bad_verb(p, verb, val),
        },
    }
}

/// Go's marker for a verb that does not apply: `%!d(string=x)`, or `%!d(<nil>)` for a
/// value with no type to name.
///
/// The value inside the marker is rendered with `%v` and the same flags, which is why
/// `%5d` of a boolean gives `%!d(bool= true)`.
pub fn bad_verb(p: &FormatParams, verb: char, val: &Value) -> String {
    match type_name(val) {
        Some(name) => format!("%!{}({}={})", verb, name, print(p, 'v', val)),
        None => format!("%!{}(<nil>)", verb),
    }
}

/// The Go type name that appears inside a `%!verb(…)` marker.
pub fn type_name(val: &Value) -> Option<&'static str> {
    match *val {
        Value::Nil | Value::NoValue => None,
        Value::Bool(_) => Some("bool"),
        Value::String(_) => Some("string"),
        Value::Function(_) => Some("func"),
        Value::Array(_) => Some("[]interface {}"),
        Value::Map(_) | Value::Object(_) => Some("map[string]interface {}"),
        Value::Number(ref n) => Some(match n.kind() {
            NumberKind::Float(_) => "float64",
            NumberKind::Signed(_) => "int",
            // A Go template literal cannot exceed an int, so this only arises from
            // context data.
            NumberKind::Unsigned(u) if u > i64::MAX as u64 => "uint64",
            NumberKind::Unsigned(_) => "int",
        }),
    }
}

/// The integer verbs. `i128` holds both a `u64` and an `i64` without loss, so the
/// signed and unsigned cases share one implementation.
fn integer(p: &FormatParams, verb: char, value: i128, val: &Value) -> String {
    let magnitude = value.unsigned_abs();
    let (digits, radix_prefix) = match verb {
        'd' | 'v' => (format!("{}", magnitude), ""),
        'b' => (format!("{:b}", magnitude), if p.sharp { "0b" } else { "" }),
        // Octal is the exception: Go's `#` is a bare leading zero rather than a
        // prefix, so it joins the digits below -- after the zero padding, which is why
        // `%#08o` is eight characters and `%#08x` is ten.
        'o' | 'O' => (format!("{:o}", magnitude), ""),
        'x' => (format!("{:x}", magnitude), if p.sharp { "0x" } else { "" }),
        'X' => (format!("{:X}", magnitude), if p.sharp { "0X" } else { "" }),
        'c' => return pad_text(p, &code_point(value).to_string()),
        'q' => return pad_text(p, &quote_rune(code_point(value), p.plus)),
        // Go reads the value as an unsigned 64-bit code point here, so a negative
        // number shows as its two's complement: `%U` of -5 is U+FFFFFFFFFFFFFFFB.
        'U' => return code_point_notation(p, value),
        'f' | 'F' => return float(p, value as f64, FloatFormat::Fixed),
        'e' => return float(p, value as f64, FloatFormat::Exponent),
        'E' => return float(p, value as f64, FloatFormat::ExponentUpper),
        'g' => return float(p, value as f64, FloatFormat::General),
        'G' => return float(p, value as f64, FloatFormat::GeneralUpper),
        _ => return bad_verb(p, verb, val),
    };

    // A precision of zero on a value of zero asks for no digits at all, so Go writes
    // the padding and nothing else -- not even a sign or a prefix.
    if p.precision == Some(0) && value == 0 {
        let without_zero_fill = FormatParams { zero: false, ..*p };
        return pad_with(&without_zero_fill, "", ' ');
    }

    let sign = if value < 0 {
        "-"
    } else if p.plus {
        "+"
    } else if p.space {
        " "
    } else {
        ""
    };

    // Go has two ways to ask for leading zeros, `%.3d` and `%03d`. An explicit
    // precision wins and the zero flag is then ignored, so the width is made up with
    // spaces instead. The zero count deliberately ignores the `0x` prefix, which is
    // why `%#08x` of 5 is ten characters wide.
    let minimum_digits = match p.precision {
        Some(precision) => precision,
        None if p.zero && !p.minus && p.width > 0 => p.width - if sign.is_empty() { 0 } else { 1 },
        None => 0,
    };
    let zeros = minimum_digits.saturating_sub(digits.len());

    let mut body = String::with_capacity(radix_prefix.len() + zeros + digits.len() + 3);
    for _ in 0..zeros {
        body.push('0');
    }
    body.push_str(&digits);
    // Go checks the leading digit after the padding, so `%#08o` of 8 needs no extra
    // zero while `%#o` of 8 does -- and `%#o` of 0 stays a single `0`.
    if matches!(verb, 'o' | 'O') && p.sharp && !body.starts_with('0') {
        body.insert(0, '0');
    }
    if verb == 'O' {
        body.insert_str(0, "0o");
    }
    body.insert_str(0, radix_prefix);
    body.insert_str(0, sign);
    // The zeros are already in place, so whatever width is left takes spaces.
    pad_with(p, &body, ' ')
}

/// Formats a float through Go's rules, then applies the flags and width. Unlike an
/// integer, a float keeps zero padding even when a precision is given; an infinity or
/// a NaN is never zero padded because it does not look like a number.
fn float(p: &FormatParams, value: f64, format: FloatFormat) -> String {
    let text = format_float(value, format, p.precision);
    let (written_sign, digits) = match text.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => match text.strip_prefix('+') {
            Some(rest) => ("+", rest),
            None => ("", text.as_str()),
        },
    };
    let sign = if !written_sign.is_empty() {
        written_sign
    } else if p.plus {
        "+"
    } else if p.space {
        " "
    } else {
        ""
    };

    let content = sign.chars().count() + digits.chars().count();
    if p.zero && !p.minus && value.is_finite() && p.width > content {
        let mut out = String::from(sign);
        for _ in content..p.width {
            out.push('0');
        }
        out.push_str(digits);
        return out;
    }
    pad_with(p, &format!("{}{}", sign, digits), ' ')
}

/// Pads a run of text to the width. Go pads text with the zero flag's character, so
/// `%05v` of `true` is `0true`.
///
/// A precision is *not* applied: Go truncates strings only, and reaches a boolean, a
/// character and a nil without passing through `truncateString` -- `%.2v` of `true` is
/// `true` there, not `tr`.
fn pad_text(p: &FormatParams, text: &str) -> String {
    pad_with(p, text, if p.zero { '0' } else { ' ' })
}

/// `pad_text` for a string, where Go's precision is a maximum length in characters.
fn pad_string(p: &FormatParams, text: &str) -> String {
    pad_text(p, truncate(p, text))
}

/// A string cut to the precision, if one was given.
fn truncate<'a>(p: &FormatParams, text: &'a str) -> &'a str {
    match p.precision {
        Some(precision) => match text.char_indices().nth(precision) {
            Some((at, _)) => &text[..at],
            None => text,
        },
        None => text,
    }
}

/// Go's `fmt.pad`: left unless the `-` flag asked for right.
fn pad_with(p: &FormatParams, text: &str, fill: char) -> String {
    let length = text.chars().count();
    if p.width <= length {
        return text.to_owned();
    }
    let padding: String = std::iter::repeat_n(fill, p.width - length).collect();
    if p.minus {
        format!("{}{}", text, padding)
    } else {
        format!("{}{}", padding, text)
    }
}

/// `%U`: `U+` and at least four upper-case hex digits, with the character itself
/// appended when the `#` flag is given.
///
/// The precision raises that minimum rather than truncating, and the zero flag does
/// not apply -- Go pads this one with spaces whatever the flags say.
fn code_point_notation(p: &FormatParams, value: i128) -> String {
    let scalar = value as i64 as u64;
    let digits = p.precision.unwrap_or(4).max(4);
    let mut out = format!("U+{:01$X}", scalar, digits);
    if p.sharp {
        if let Some(c) = u32::try_from(scalar).ok().and_then(char::from_u32) {
            if is_printable(c) {
                let _ = write!(out, " {}", quote_rune(c, false));
            }
        }
    }
    let without_zero_fill = FormatParams {
        zero: false,
        precision: None,
        ..*p
    };
    pad_text(&without_zero_fill, &out)
}

/// An invalid code point becomes the replacement character, as it does in Go.
fn code_point(value: i128) -> char {
    u32::try_from(value)
        .ok()
        .and_then(char::from_u32)
        .unwrap_or(char::REPLACEMENT_CHARACTER)
}

/// `%x`/`%X` on a string: the hex of its bytes, with the `#` flag's prefix.
///
/// Go's precision limits how many *bytes* are encoded, so `%.2x` of `"abcdef"` is the
/// hex of `ab`; the space flag separates each byte, and with `#` prefixes each one.
/// Nothing to encode writes nothing, prefix included.
fn hex_bytes(p: &FormatParams, s: &str, upper: bool) -> String {
    let bytes = match p.precision {
        Some(precision) => &s.as_bytes()[..precision.min(s.len())],
        None => s.as_bytes(),
    };
    if bytes.is_empty() {
        return String::new();
    }
    let prefix = if p.sharp {
        if upper {
            "0X"
        } else {
            "0x"
        }
    } else {
        ""
    };
    let mut out = String::with_capacity(bytes.len() * 5 + 2);
    for (index, byte) in bytes.iter().enumerate() {
        if p.space && index > 0 {
            out.push(' ');
        }
        if index == 0 || p.space {
            out.push_str(prefix);
        }
        let _ = if upper {
            write!(out, "{:02X}", byte)
        } else {
            write!(out, "{:02x}", byte)
        };
    }
    out
}

/// Go's `%q` on a string: a backquoted raw string with the `#` flag where that is
/// possible, `strconv.QuoteToASCII` with `+`, and `strconv.Quote` otherwise.
fn quote_string(p: &FormatParams, s: &str) -> String {
    if p.sharp && can_backquote(s) {
        return format!("`{}`", s);
    }
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        escape_into(&mut out, c, '"', p.plus);
    }
    out.push('"');
    out
}

/// Go's `strconv.CanBackquote`: one line, no backquote, and no control character
/// other than a tab.
fn can_backquote(s: &str) -> bool {
    s.chars().all(|c| {
        if c == '\u{feff}' {
            // A byte-order mark is invisible, so Go refuses to backquote it.
            return false;
        }
        if !c.is_ascii() {
            return true;
        }
        !((c < ' ' && c != '\t') || c == '`' || c == '\u{7f}')
    })
}

/// Go's `strconv.QuoteRune`, or `QuoteRuneToASCII` with the `+` flag.
fn quote_rune(c: char, ascii_only: bool) -> String {
    let mut out = String::with_capacity(8);
    out.push('\'');
    escape_into(&mut out, c, '\'', ascii_only);
    out.push('\'');
    out
}

/// Writes one character the way Go's `strconv` escapes it.
fn escape_into(out: &mut String, c: char, quote: char, ascii_only: bool) {
    if c == quote || c == '\\' {
        out.push('\\');
        out.push(c);
        return;
    }
    match c {
        '\u{7}' => out.push_str("\\a"),
        '\u{8}' => out.push_str("\\b"),
        '\u{c}' => out.push_str("\\f"),
        '\n' => out.push_str("\\n"),
        '\r' => out.push_str("\\r"),
        '\t' => out.push_str("\\t"),
        '\u{b}' => out.push_str("\\v"),
        c if (c as u32) < 0x20 || c as u32 == 0x7f => {
            let _ = write!(out, "\\x{:02x}", c as u32);
        }
        c if !is_printable(c) => escape_unicode(out, c),
        c if ascii_only && !c.is_ascii() => escape_unicode(out, c),
        c => out.push(c),
    }
}

/// Go's `strconv.IsPrint`, which `%q` has to apply for its output to read back as the
/// same string.
///
/// Go takes a shortcut below U+0100 -- the ASCII graphic characters and the space,
/// then Latin-1 from U+00A1 up, minus the soft hyphen -- which leaves the no-break
/// space at U+00A0 unprintable. Above that it is the letters, marks, numbers,
/// punctuation and symbols, so the invisible format characters (a zero-width space, a
/// byte-order mark, a bidi override), the private-use areas and the unassigned code
/// points are all escaped.
pub(crate) fn is_printable(c: char) -> bool {
    let scalar = c as u32;
    if scalar <= 0xFF {
        return (0x20..=0x7E).contains(&scalar)
            || ((0xA1..=0xFF).contains(&scalar) && scalar != 0xAD);
    }
    use unicode_general_category::GeneralCategory::*;
    matches!(
        unicode_general_category::get_general_category(c),
        UppercaseLetter
            | LowercaseLetter
            | TitlecaseLetter
            | ModifierLetter
            | OtherLetter
            | NonspacingMark
            | SpacingMark
            | EnclosingMark
            | DecimalNumber
            | LetterNumber
            | OtherNumber
            | ConnectorPunctuation
            | DashPunctuation
            | OpenPunctuation
            | ClosePunctuation
            | InitialPunctuation
            | FinalPunctuation
            | OtherPunctuation
            | MathSymbol
            | CurrencySymbol
            | ModifierSymbol
            | OtherSymbol
    )
}

fn escape_unicode(out: &mut String, c: char) {
    let scalar = c as u32;
    let _ = if scalar > 0xFFFF {
        write!(out, "\\U{:08x}", scalar)
    } else {
        write!(out, "\\u{:04x}", scalar)
    };
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use super::*;
    use gtmpl_value_ng::{Func, FuncError};

    /// Every expectation here was taken from Go 1.26.8. The conformance fixture covers
    /// what can be rendered with no data; these are the cases that need a value the
    /// fixture cannot carry -- a container, a function -- plus the corners of the
    /// quoting and code-point verbs.
    fn check(params: &FormatParams, verb: char, value: &Value, expected: &str) {
        assert_eq!(print(params, verb, value), expected, "verb {:?}", verb);
    }

    fn plain() -> FormatParams {
        FormatParams::default()
    }

    fn array() -> Value {
        Value::Array(vec![Value::from(1u64), Value::from("a"), Value::from(true)])
    }

    fn map() -> Value {
        Value::Map(
            [
                ("b".to_owned(), Value::from(2u64)),
                ("a".to_owned(), Value::from(1u64)),
            ]
            .into_iter()
            .collect(),
        )
    }

    /// Go keeps the flags while it recurses and pads no brackets, so a width applies to
    /// each element rather than to the container.
    #[test]
    fn a_containers_elements_carry_the_flags() {
        let width = FormatParams {
            width: 5,
            ..plain()
        };
        check(
            &width,
            'd',
            &array(),
            "[    1 %!d(string=    a) %!d(bool= true)]",
        );
        check(&width, 'v', &map(), "map[    a:    1     b:    2]");

        let zero = FormatParams {
            width: 3,
            zero: true,
            ..plain()
        };
        check(&zero, 'd', &Value::Array(vec![Value::from(1u64)]), "[001]");

        let precision = FormatParams {
            precision: Some(3),
            ..plain()
        };
        check(
            &precision,
            'd',
            &Value::Array(vec![Value::from(1u64)]),
            "[001]",
        );
    }

    #[test]
    fn a_nested_container_recurses() {
        let nested = Value::Array(vec![array(), Value::Map(HashMap::new())]);
        check(&plain(), 'v', &nested, "[[1 a true] map[]]");
    }

    #[test]
    fn a_function_renders_only_for_v() {
        fn f(a: &[Value]) -> Result<Value, FuncError> {
            Ok(a[0].clone())
        }
        let function = Value::from(f as Func);
        check(&plain(), 'v', &function, "Function");
        check(&plain(), 'd', &function, "%!d(func=Function)");
    }

    #[test]
    fn the_octal_verbs_follow_go() {
        let sharp = FormatParams {
            sharp: true,
            ..plain()
        };
        check(&plain(), 'O', &Value::from(8u64), "0o10");
        check(&sharp, 'O', &Value::from(8u64), "0o010");
        check(&sharp, 'o', &Value::from(8u64), "010");
        // `#` looks at the leading digit, which is already a zero here.
        check(&sharp, 'o', &Value::from(0u64), "0");
    }

    /// `%q` on a string: `#` asks for a backquoted raw string where that is possible,
    /// `+` for an ASCII-only escape, and neither for Go's plain quoting.
    #[test]
    fn string_quoting_follows_go() {
        let sharp = FormatParams {
            sharp: true,
            ..plain()
        };
        let plus = FormatParams {
            plus: true,
            ..plain()
        };
        check(&sharp, 'q', &Value::from("hello"), "`hello`");
        check(&sharp, 'q', &Value::from("has\ttab"), "`has\ttab`");
        // A newline cannot live in a backquoted string, so Go falls back to quoting.
        check(&sharp, 'q', &Value::from("a\nb"), "\"a\\nb\"");
        // Nor can a byte-order mark, which is invisible.
        check(&sharp, 'q', &Value::from("\u{feff}x"), "\"\\ufeffx\"");
        check(&plain(), 'q', &Value::from("zöe"), "\"zöe\"");
        check(&plus, 'q', &Value::from("zöe"), "\"z\\u00f6e\"");
        check(&plus, 'q', &Value::from("🎉"), "\"\\U0001f389\"");
        check(&plain(), 'q', &Value::from("a\"b"), "\"a\\\"b\"");
    }

    /// Go escapes a space that is not `U+0020`, which `%q` has to do for its output to
    /// read back as the same string.
    #[test]
    fn unusual_spaces_are_escaped() {
        check(&plain(), 'q', &Value::from("x\u{a0}y"), "\"x\\u00a0y\"");
        check(&plain(), 'q', &Value::from("x\u{2028}y"), "\"x\\u2028y\"");
        check(&plain(), 'q', &Value::from("x\u{3000}y"), "\"x\\u3000y\"");
    }

    #[test]
    fn code_point_notation_follows_go() {
        let sharp = FormatParams {
            sharp: true,
            ..plain()
        };
        check(&plain(), 'U', &Value::from(65u64), "U+0041");
        check(&sharp, 'U', &Value::from(65u64), "U+0041 'A'");
        // Not printable, so Go appends nothing.
        check(&sharp, 'U', &Value::from(5u64), "U+0005");
        // Go reads the value as an unsigned 64-bit code point.
        check(&plain(), 'U', &Value::from(-5i64), "U+FFFFFFFFFFFFFFFB");
        let wide = FormatParams {
            precision: Some(8),
            ..plain()
        };
        check(&wide, 'U', &Value::from(65u64), "U+00000041");
    }

    #[test]
    fn hex_of_a_string_follows_go() {
        let space = FormatParams {
            space: true,
            ..plain()
        };
        let sharp_space = FormatParams {
            sharp: true,
            space: true,
            ..plain()
        };
        check(&plain(), 'x', &Value::from("foobar"), "666f6f626172");
        check(&space, 'x', &Value::from("foobar"), "66 6f 6f 62 61 72");
        check(
            &sharp_space,
            'x',
            &Value::from("foobar"),
            "0x66 0x6f 0x6f 0x62 0x61 0x72",
        );
        // Nothing to encode writes nothing, prefix included.
        let sharp = FormatParams {
            sharp: true,
            ..plain()
        };
        check(&sharp, 'x', &Value::from(""), "");
    }

    #[test]
    fn type_names_follow_go() {
        check(&plain(), 'T', &Value::from(5u64), "int");
        check(&plain(), 'T', &Value::from(-5i64), "int");
        check(&plain(), 'T', &Value::from(u64::MAX), "uint64");
        check(&plain(), 'T', &Value::from(1.5f64), "float64");
        check(&plain(), 'T', &Value::from("s"), "string");
        check(&plain(), 'T', &Value::from(true), "bool");
        check(&plain(), 'T', &Value::Nil, "<nil>");
        check(&plain(), 'T', &array(), "[]interface {}");
        check(&plain(), 'T', &map(), "map[string]interface {}");
    }
}
