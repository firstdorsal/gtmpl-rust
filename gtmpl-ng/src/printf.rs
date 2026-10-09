use gtmpl_value_ng::{FromValue, Value};

use crate::print_verb::{print, type_name};

/// The flags, width and precision of one verb.
#[derive(Default, Clone, Copy)]
pub struct FormatParams {
    pub sharp: bool,
    pub zero: bool,
    pub plus: bool,
    pub minus: bool,
    pub space: bool,
    pub width: usize,
    pub precision: Option<usize>,
}

/// Formats `args` into `format` the way Go's `fmt.Sprintf` does.
///
/// Go's formatter never fails. A verb that does not apply to its argument, a missing
/// argument, a leftover argument and a malformed verb all become a `%!…` marker inside
/// the output, which is what a Go template produces and therefore what this produces.
pub fn sprintf(format: &str, args: &[Value]) -> String {
    let chars: Vec<char> = format.chars().collect();
    let mut out = String::with_capacity(format.len());
    let mut at = 0;
    // The next argument to consume, and whether an explicit `[n]` was ever used --
    // Go leaves leftover arguments unreported once a format has reordered them.
    let mut arg_index = 0;
    let mut reordered = false;

    while at < chars.len() {
        if chars[at] != '%' {
            out.push(chars[at]);
            at += 1;
            continue;
        }
        at += 1;

        let mut params = FormatParams::default();
        while at < chars.len() {
            match chars[at] {
                '#' => params.sharp = true,
                '0' => params.zero = !params.minus,
                '+' => params.plus = true,
                '-' => {
                    params.minus = true;
                    // Go does not pad with zeros to the right.
                    params.zero = false;
                }
                ' ' => params.space = true,
                _ => break,
            }
            at += 1;
        }

        // An explicit argument index may appear before the width and again before the
        // verb. `good_index` turns false for every shape Go reports as BADINDEX.
        let mut good_index = true;
        let mut after_index = read_index(
            &chars,
            &mut at,
            args.len(),
            &mut arg_index,
            &mut reordered,
            &mut good_index,
        );

        if at < chars.len() && chars[at] == '*' {
            at += 1;
            match take_int(args, &mut arg_index) {
                Some(width) => {
                    if width < 0 {
                        params.minus = true;
                        params.zero = false;
                    }
                    params.width = width.unsigned_abs() as usize;
                }
                None => out.push_str("%!(BADWIDTH)"),
            }
            after_index = false;
        } else if let Some(width) = read_number(&chars, &mut at) {
            // "%[3]2d": a width after an index is not something Go accepts.
            if after_index {
                good_index = false;
            }
            params.width = width;
        }

        if at + 1 < chars.len() && chars[at] == '.' {
            at += 1;
            // "%[3].2d", likewise.
            if after_index {
                good_index = false;
            }
            after_index = read_index(
                &chars,
                &mut at,
                args.len(),
                &mut arg_index,
                &mut reordered,
                &mut good_index,
            );
            if at < chars.len() && chars[at] == '*' {
                at += 1;
                match take_int(args, &mut arg_index) {
                    Some(precision) => params.precision = Some(precision.max(0) as usize),
                    None => out.push_str("%!(BADPREC)"),
                }
                after_index = false;
            } else {
                // A bare `.` means precision zero, not "no precision".
                params.precision = Some(read_number(&chars, &mut at).unwrap_or(0));
            }
        }

        if !after_index {
            read_index(
                &chars,
                &mut at,
                args.len(),
                &mut arg_index,
                &mut reordered,
                &mut good_index,
            );
        }

        if at >= chars.len() {
            out.push_str("%!(NOVERB)");
            break;
        }
        let verb = chars[at];
        at += 1;

        if verb == '%' {
            // `%%` consumes no argument and ignores the width and precision.
            out.push('%');
        } else if !good_index {
            out.push_str(&format!("%!{}(BADINDEX)", verb));
        } else if arg_index >= args.len() {
            out.push_str(&format!("%!{}(MISSING)", verb));
        } else {
            out.push_str(&print(&params, verb, &args[arg_index]));
            arg_index += 1;
        }
    }

    if !reordered && arg_index < args.len() {
        out.push_str("%!(EXTRA ");
        for (offset, arg) in args[arg_index..].iter().enumerate() {
            if offset > 0 {
                out.push_str(", ");
            }
            match type_name(arg) {
                Some(name) => out.push_str(&format!("{}={}", name, arg)),
                None => out.push_str("<nil>"),
            }
        }
        out.push(')');
    }
    out
}

/// Reads an explicit `[n]` argument index, Go's one-based form. Returns whether one was
/// consumed, and clears `good_index` for an index that is malformed or out of range --
/// Go reports both as BADINDEX rather than failing.
fn read_index(
    chars: &[char],
    at: &mut usize,
    arg_count: usize,
    arg_index: &mut usize,
    reordered: &mut bool,
    good_index: &mut bool,
) -> bool {
    if *at >= chars.len() || chars[*at] != '[' {
        return false;
    }
    *reordered = true;
    let closing = chars[*at..].iter().position(|c| *c == ']');
    let Some(closing) = closing else {
        // No closing bracket: Go steps over the `[` alone and reports BADINDEX.
        *at += 1;
        *good_index = false;
        return false;
    };
    let digits: String = chars[*at + 1..*at + closing].iter().collect();
    let parsed = digits.parse::<usize>();
    *at += closing + 1;
    match parsed {
        // Go's indices are one-based, so `[0]` is out of range too.
        Ok(index) if index >= 1 && index - 1 < arg_count => {
            *arg_index = index - 1;
            true
        }
        _ => {
            *good_index = false;
            false
        }
    }
}

/// Reads a run of decimal digits, if any.
fn read_number(chars: &[char], at: &mut usize) -> Option<usize> {
    let start = *at;
    while *at < chars.len() && chars[*at].is_ascii_digit() {
        *at += 1;
    }
    if *at == start {
        return None;
    }
    let digits: String = chars[start..*at].iter().collect();
    // A run of digits longer than `usize` is not a width anyone meant.
    digits.parse().ok()
}

/// Consumes the next argument as an integer, for a `*` width or precision.
fn take_int(args: &[Value], arg_index: &mut usize) -> Option<i64> {
    let value = args.get(*arg_index)?;
    *arg_index += 1;
    i64::from_value(value)
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use super::*;

    /// Every expectation in this module was taken from Go 1.26.8 rather than reasoned
    /// out, by running the same template through `text/template`.
    fn check(format: &str, args: &[Value], expected: &str) {
        assert_eq!(sprintf(format, args), expected, "printf {:?}", format);
    }

    #[test]
    fn plain_verbs() {
        check("foo%v2000", &["bar".into()], "foobar2000");
        check("%+0v", &[1.into()], "+1");
        check("foobar%d", &[2000.into()], "foobar2000");
        check("%+0d", &[1.into()], "+1");
        check("%+0b", &[5.into()], "+101");
        check("%x", &[255.into()], "ff");
        check("%#x", &[255.into()], "0xff");
        check("%#o", &[8.into()], "010");
        check("%U", &[65.into()], "U+0041");
        check("%v", &[true.into()], "true");
        check("%v", &["".into()], "");
    }

    #[test]
    fn strings() {
        check("%x", &["foobar2000".into()], "666f6f62617232303030");
        check("%X", &["foobar2000".into()], "666F6F62617232303030");
        check("%.6s", &["foobar2000".into()], "foobar");
        check("%6.3s|", &["abcdef".into()], "   abc|");
        check("%5v|", &["ab".into()], "   ab|");
        check("%q", &["a\"b".into()], "\"a\\\"b\"");
        check("%q", &["tab\there".into()], "\"tab\\there\"");
    }

    #[test]
    fn fancy_verbs() {
        check("%+-#10c", &[10000.into()], "✐         ");
        check("%+-#10q", &[10000.into()], "'\\u2710'  ");
    }

    #[test]
    fn explicit_indices() {
        check(
            "%[1]v %v",
            &["foo".into(), "bar".into(), 2000.into()],
            "foo bar",
        );
        check(
            "%[2]v %v%[1]v %v%[1]v",
            &["!".into(), "wtf".into(), "golang".into()],
            "wtf golang! wtf!",
        );
        check("%[2]d", &[1.into(), 2.into()], "2");
    }

    #[test]
    fn floats_follow_go() {
        check("%v", &[Value::from(1e20)], "1e+20");
        check("%v", &[Value::from(1e-7)], "1e-07");
        check("%e", &[Value::from(1234.5678)], "1.234568e+03");
        check("%E", &[Value::from(1234.5678)], "1.234568E+03");
        check("%g", &[Value::from(1e20)], "1e+20");
        check("%.3g", &[Value::from(1234.5678)], "1.23e+03");
        check("%.5g", &[Value::from(3.0)], "3");
        check("%10.2e", &[Value::from(1234.5)], "  1.23e+03");
        check("%-12.2e|", &[Value::from(1234.5)], "1.23e+03    |");
        check("%012.2e", &[Value::from(1234.5)], "00001.23e+03");
        check("%+.2e", &[Value::from(1234.5)], "+1.23e+03");
        check("% .2e", &[Value::from(1234.5)], " 1.23e+03");
        check("%08.2f", &[Value::from(-3.5)], "-0003.50");
        check("%+08d", &[42.into()], "+0000042");
        check("%v", &[Value::from(f64::INFINITY)], "+Inf");
        check("%v", &[Value::from(f64::NEG_INFINITY)], "-Inf");
        check("%v", &[Value::from(f64::NAN)], "NaN");
        check("%f", &[Value::from(f64::INFINITY)], "+Inf");
        check("%08f", &[Value::from(f64::INFINITY)], "    +Inf");
    }

    #[test]
    fn a_verb_that_does_not_apply_is_marked_not_an_error() {
        check("%s", &[Value::Nil], "%!s(<nil>)");
        check("%d", &[Value::Nil], "%!d(<nil>)");
        check("%d", &[Value::NoValue], "%!d(<nil>)");
        check("%v", &[Value::Nil], "<nil>");
        check("%d", &["x".into()], "%!d(string=x)");
        check("%s", &[5.into()], "%!s(int=5)");
        check("%d", &[true.into()], "%!d(bool=true)");
        check("%t", &[5.into()], "%!t(int=5)");
        check("%s", &[true.into()], "%!s(bool=true)");
        check("%q", &[true.into()], "%!q(bool=true)");
        check("%c", &["x".into()], "%!c(string=x)");
        check("%d", &[Value::from(1.5)], "%!d(float64=1.5)");
        check("%f", &["x".into()], "%!f(string=x)");
        check("%z", &[1.into()], "%!z(int=1)");
        check("%d", &[Value::from(f64::INFINITY)], "%!d(float64=+Inf)");
    }

    #[test]
    fn a_container_is_marked_element_by_element() {
        let array = Value::Array(vec![1.into(), "a".into(), true.into()]);
        check("%d", &[array.clone()], "[1 %!d(string=a) %!d(bool=true)]");
        check("%v", &[array], "[1 a true]");

        let mut map: HashMap<String, Value> = HashMap::new();
        map.insert("k".to_owned(), "v".into());
        check("%s", &[Value::Map(map.clone())], "map[k:v]");
        check("%d", &[Value::Map(map)], "map[%!d(string=k):%!d(string=v)]");
    }

    #[test]
    fn a_map_prints_its_keys_in_order() {
        let map: HashMap<String, Value> = [
            ("z".to_owned(), 3.into()),
            ("a".to_owned(), 1.into()),
            ("m".to_owned(), 2.into()),
        ]
        .into_iter()
        .collect();
        // Sorted, as Go has sorted map keys since 1.12 -- the old implementation
        // walked the HashMap and printed a different order from run to run.
        check("%v", &[Value::Map(map)], "map[a:1 m:2 z:3]");
    }

    #[test]
    fn arrays_and_maps_render_like_go() {
        let values: Vec<Value> = vec!["hello".into(), "world".into()];
        check("foo %v", &[Value::Array(values)], "foo [hello world]");
        let values: Vec<Value> = vec![42.into(), 100.into()];
        check("foo %v", &[Value::Array(values)], "foo [42 100]");

        let mut map: HashMap<String, Value> = HashMap::new();
        map.insert("hello".into(), "world".into());
        map.insert("number".into(), 42.into());
        check(
            "foo %v",
            &[Value::Map(map)],
            "foo map[hello:world number:42]",
        );

        let mut map: HashMap<String, Value> = HashMap::new();
        map.insert("float".into(), Value::from(4.2));
        check("%v", &[Value::Map(map)], "map[float:4.2]");
    }

    #[test]
    fn missing_and_leftover_arguments_are_marked() {
        check("%d", &[], "%!d(MISSING)");
        check("%v %v", &[1.into()], "1 %!v(MISSING)");
        check("%s %s", &["a".into()], "a %!s(MISSING)");
        check("%d", &[1.into(), 2.into()], "1%!(EXTRA int=2)");
        check("x", &[1.into()], "x%!(EXTRA int=1)");
        check("", &[1.into(), 2.into()], "%!(EXTRA int=1, int=2)");
        check(
            "%v",
            &[1.into(), "a".into(), true.into()],
            "1%!(EXTRA string=a, bool=true)",
        );
        check(
            "%d %d",
            &[1.into(), 2.into(), 3.into()],
            "1 2%!(EXTRA int=3)",
        );
        check("%%d", &[5.into()], "%d%!(EXTRA int=5)");
        // An explicit index means Go stops reporting leftovers.
        check("%[1]d", &[1.into(), 2.into()], "1");
    }

    #[test]
    fn malformed_verbs_are_marked() {
        check("100%", &[], "100%!(NOVERB)");
        check("%", &[], "%!(NOVERB)");
        check("%%", &[], "%");
        check("a%!b", &[1.into()], "a%!!(int=1)b");
        check("%[3]d", &[1.into(), 2.into()], "%!d(BADINDEX)");
        check("%[0]d", &[5.into()], "%!d(BADINDEX)");
        check("%[d", &[5.into()], "%!d(BADINDEX)");
        check("%[1]2d", &[5.into()], "%!d(BADINDEX)");
        check("%[1].2f", &[Value::from(5.5)], "%!f(BADINDEX)");
        check("%*d", &["x".into(), 5.into()], "%!(BADWIDTH)5");
        // Go writes `%!(BADPREC)%!f(int=5)` here: it rejects `%f` on an integer, and
        // we cannot -- see `a_float_verb_on_an_integer_is_the_one_deviation`.
        check("%.*f", &["x".into(), 5.into()], "%!(BADPREC)5.000000");
    }

    /// The one place this formatter knowingly differs from Go.
    ///
    /// `Value` stores a whole float as an integer so that `{{ 3.0 }}` renders `3` the
    /// way Go does, which means a float verb cannot tell `3` from `3.0`. Go rejects a
    /// float verb on a true integer and writes `%!f(int=3)`; refusing the float case
    /// instead would be the worse trade, so the verb is accepted here.
    #[test]
    fn a_float_verb_on_an_integer_is_the_one_deviation() {
        check("%f", &[3.into()], "3.000000");
        check("%e", &[3.into()], "3.000000e+00");
        check("%.2f", &[3.into()], "3.00");
        // A float that is not whole is stored as one, so it behaves exactly like Go.
        check("%d", &[Value::from(3.5)], "%!d(float64=3.5)");
    }

    // `3.14159` is a Go reference value, not pi.
    #[allow(clippy::approx_constant)]
    #[test]
    fn star_width_and_precision() {
        check("%*d", &[5.into(), 42.into()], "   42");
        check("%-*d|", &[5.into(), 42.into()], "42   |");
        check("%.*f", &[2.into(), Value::from(3.14159)], "3.14");
        check("%-5v|", &[1.into()], "1    |");
        check("%05v|", &[7.into()], "00007|");
    }
}
