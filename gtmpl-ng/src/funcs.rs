//! Builtin functions.
use std::borrow::Cow;
use std::cmp::Ordering;

use gtmpl_value_ng::{Func, FuncError, Value};

use crate::printf::sprintf;
use crate::utils::is_true;

pub static BUILTINS: &[(&str, Func)] = &[
    ("eq", eq as Func),
    ("ne", ne as Func),
    ("lt", lt as Func),
    ("le", le as Func),
    ("gt", gt as Func),
    ("ge", ge as Func),
    ("len", len as Func),
    ("and", and as Func),
    ("or", or as Func),
    ("not", not as Func),
    ("urlquery", urlquery as Func),
    ("html", html as Func),
    ("js", js as Func),
    ("print", print as Func),
    ("println", println as Func),
    ("printf", printf as Func),
    ("index", index as Func),
    ("slice", slice as Func),
    ("call", call as Func),
];

macro_rules! val {
    ($x:expr) => {
        Value::from($x)
    };
}

/// Help to write new functions for gtmpl.
#[macro_export]
macro_rules! gtmpl_fn {
 (
  $(#[$outer:meta])*
  fn $name:ident() -> Result<$otyp:ty, FuncError>
  { $($body:tt)* }
 ) => {
  $(#[$outer])*
  pub fn $name(args: &[$crate::Value]) -> Result<$crate::Value, FuncError> {
   fn inner() -> Result<$otyp, FuncError> {
    $($body)*
   }
   Ok($crate::Value::from(inner()?))
  }
 };
 (
  $(#[$outer:meta])*
  fn $name:ident($arg0:ident : $typ0:ty) -> Result<$otyp:ty, FuncError>
  { $($body:tt)* }
 ) => {
  $(#[$outer])*
  pub fn $name(
   args: &[$crate::Value]
  ) -> Result<$crate::Value, FuncError> {
   if args.is_empty() {
    return Err(FuncError::AtLeastXArgs(stringify!($name).into(), 1));
   }
   let x = &args[0];
   let $arg0: $typ0 = $crate::from_value(x)
    .ok_or(FuncError::UnableToConvertFromValue)?;
   fn inner($arg0 : $typ0) -> Result<$otyp, FuncError> {
    $($body)*
   }
   let ret: $crate::Value = inner($arg0)?.into();
   Ok(ret)
  }
 };
 (
  $(#[$outer:meta])*
  fn $name:ident($arg0:ident : $typ0:ty$(, $arg:ident : $typ:ty)*) -> Result<$otyp:ty, FuncError>
  { $($body:tt)* }
 ) => {
  $(#[$outer])*
  pub fn $name(
   args: &[$crate::Value]
  ) -> Result<$crate::Value, FuncError> {
   #[allow(unused_mut)]
   let mut args = args;
   if args.is_empty() {
    return Err(FuncError::AtLeastXArgs(stringify!($name).into(), 1));
   }
   let x = &args[0];
   let $arg0: $typ0 = $crate::from_value(x)
    .ok_or(FuncError::UnableToConvertFromValue)?;
   $(args = &args[1..];
     let x = &args[0];
     let $arg: $typ = $crate::from_value(x)
    .ok_or(FuncError::UnableToConvertFromValue)?;)*
   fn inner($arg0 : $typ0, $($arg : $typ,)*) -> Result<$otyp, FuncError> {
    $($body)*
   }
   let ret: $crate::Value = inner($arg0, $($arg),*)?.into();
   Ok(ret)
  }
 }
}

macro_rules! gn {
 (
  $(#[$outer:meta])*
  $name:ident($arg1:ident : ref Value, $arg2:ident : ref Value) ->
   Result<Value, FuncError>
  { $($body:tt)* }
 ) => {
  $(#[$outer])*
  pub fn $name(args: &[Value]) -> Result<Value, FuncError> {
   if args.len() != 2 {
    // Go's wording is "want 2": only `eq` is variadic among the comparisons.
    return Err(FuncError::ExactlyXArgs(stringify!($name).into(), 2));
   }
   let $arg1 = &args[0];
   let $arg2 = &args[1];
   fn inner($arg1: &Value, $arg2: &Value) -> Result<Value, FuncError> {
    $($body)*
   }
   inner($arg1, $arg2)
  }
 }
}

/// Returns the boolean OR of its arguments by returning the
/// first non-empty argument or the last argument, that is,
/// "or x y" behaves as "if x then x else y".
///
/// In a template, `or` evaluates lazily, as Go's has since 1.18: the arguments after
/// the first truthy one are never evaluated. Calling this function directly is eager
/// by construction, because its arguments are values that already exist.
///
/// # Example
/// ```
/// use gtmpl_ng::template;
/// let equal = template("{{ or 1 2.0 false . }}", "foo");
/// assert_eq!(&equal.unwrap(), "1");
/// ```
pub fn or(args: &[Value]) -> Result<Value, FuncError> {
    for arg in args {
        if is_true(arg) {
            return Ok(arg.clone());
        }
    }
    args.last()
        .cloned()
        .ok_or_else(|| FuncError::AtLeastXArgs("or".into(), 1))
}

/// Returns the boolean AND of its arguments by returning the
/// first empty argument or the last argument, that is,
/// "and x y" behaves as "if x then y else x".
///
/// In a template, `and` evaluates lazily, as Go's has since 1.18: the arguments after
/// the first falsy one are never evaluated. Calling this function directly is eager
/// by construction, because its arguments are values that already exist.
///
/// # Example
/// ```
/// use gtmpl_ng::template;
/// let equal = template("{{ and 1 2.0 true . }}", "foo");
/// assert_eq!(&equal.unwrap(), "foo");
/// ```
pub fn and(args: &[Value]) -> Result<Value, FuncError> {
    for arg in args {
        if !is_true(arg) {
            return Ok(arg.clone());
        }
    }
    args.last()
        .cloned()
        .ok_or_else(|| FuncError::AtLeastXArgs("and".into(), 1))
}

/// Returns the boolean negation of its single argument.
///
/// # Example
/// ```
/// use gtmpl_ng::template;
/// let equal = template("{{ not 0 }}", "");
/// assert_eq!(&equal.unwrap(), "true");
/// ```
pub fn not(args: &[Value]) -> Result<Value, FuncError> {
    if args.len() != 1 {
        Err(FuncError::ExactlyXArgs("not".into(), 1))
    } else {
        Ok(val!(!is_true(&args[0])))
    }
}

/// Returns the integer length of its argument.
///
/// # Example
/// ```
/// use gtmpl_ng::template;
/// let equal = template("{{ len . }}", "foo");
/// assert_eq!(&equal.unwrap(), "3");
/// ```
pub fn len(args: &[Value]) -> Result<Value, FuncError> {
    if args.len() != 1 {
        return Err(FuncError::ExactlyXArgs("len".into(), 1));
    }
    let arg = &args[0];
    let len = match *arg {
        Value::String(ref s) => s.len(),
        Value::Array(ref a) => a.len(),
        Value::Map(ref o) | Value::Object(ref o) => o.len(),
        _ => {
            return Err(FuncError::Generic(format!("unable to call len on {}", arg)));
        }
    };

    Ok(val!(len))
}

/// Returns the result of calling the first argument, which
/// must be a function, with the remaining arguments as parameters.
///
/// # Example
/// ```
/// use gtmpl_ng::{gtmpl_fn, template, Value};
/// use gtmpl_ng::{FuncError, Function};
///
/// gtmpl_fn!(
/// fn add(a: u64, b: u64) -> Result<u64, FuncError> {
///     Ok(a + b)
/// });
/// let equal = template(r#"{{ call . 1 2 }}"#, Value::Function(Function { f: add }));
/// assert_eq!(&equal.unwrap(), "3");
/// ```
pub fn call(args: &[Value]) -> Result<Value, FuncError> {
    if args.is_empty() {
        Err(FuncError::AtLeastXArgs("call".into(), 1))
    } else if let Value::Function(ref f) = args[0] {
        (f.f)(&args[1..])
    } else {
        Err(FuncError::Generic(
            "call requires the first argument to be a function".into(),
        ))
    }
}

/// How `print` and `println` render a value, which is Go's `fmt` and not the template
/// printer: `fmt` writes `<nil>` where the template printer writes `<no value>`.
fn printed(val: &Value) -> String {
    if crate::utils::is_absent_or_nil(val) {
        "<nil>".to_owned()
    } else {
        val.to_string()
    }
}

/// An implementation of golang's `fmt.Sprint`.
///
/// Formats its operands with their default format and returns the resulting string.
/// A space goes between two operands when neither is a string.
///
/// # Example
/// ```
/// use gtmpl_ng::template;
/// let equal = template(r#"{{ print "Hello " . "!" }}"#, "world");
/// assert_eq!(&equal.unwrap(), "Hello world!");
/// ```
pub fn print(args: &[Value]) -> Result<Value, FuncError> {
    Ok(val!(sprint(args)))
}

/// The joining `print` shares with the escapers: operands in their default format,
/// with a space between two of them when neither reads as a string.
fn sprint(args: &[Value]) -> String {
    sprint_shown(args, |value| match value {
        Value::String(text) => Some(Cow::Borrowed(text.as_str())),
        _ => None,
    })
}

/// Go's `evalArgs` view of the same joining, which [`urlquery`], [`html`] and [`js`]
/// go through: identical to [`sprint`] except that an untyped nil reads as the
/// *string* `<no value>` -- so it shows the way a missing value does, and takes no
/// joining space, where `fmt.Sprint` would write `<nil>` with one. Measured against
/// Go 1.26.8: `{{ html 1 nil }}` is `1&lt;no value&gt;`, no space.
fn sprint_printable(args: &[Value]) -> String {
    sprint_shown(args, |value| match value {
        Value::String(text) => Some(Cow::Borrowed(text.as_str())),
        Value::Nil => Some(Cow::Borrowed("<no value>")),
        _ => None,
    })
}

/// `fmt.Sprint`'s joining, with `as_string` deciding which operands count as strings.
fn sprint_shown<'v>(
    args: &'v [Value],
    as_string: impl Fn(&'v Value) -> Option<Cow<'v, str>>,
) -> String {
    let mut no_space = true;
    let mut s = String::new();
    for val in args {
        if let Some(text) = as_string(val) {
            no_space = true;
            s.push_str(&text);
        } else {
            if no_space {
                s += &printed(val);
            } else {
                s += &format!(" {}", printed(val))
            }
            no_space = false;
        }
    }
    s
}

/// An implementation of golang's fmt.Sprintln
///
/// Sprintln formats using the default formats for its operands and returns the
/// resulting string. Spaces are always added between operands and a newline is appended.
///
/// # Example
/// ```
/// use gtmpl_ng::template;
/// let equal = template(r#"{{ println "Hello" . "!" }}"#, "world");
/// assert_eq!(&equal.unwrap(), "Hello world !\n");
/// ```
pub fn println(args: &[Value]) -> Result<Value, FuncError> {
    let mut iter = args.iter();
    let s = match iter.next() {
        None => String::from("\n"),
        Some(first_elt) => {
            let (lower, _) = iter.size_hint();
            let mut result = String::with_capacity(lower + 1);
            // `printed` rather than `Display`, so an absent value reads `<nil>` the
            // way `fmt` writes it -- `print` has done that since 0.8.0 and this was
            // left behind, which is the inconsistency `printed` exists to remove.
            result.push_str(&printed(first_elt));
            for elt in iter {
                result.push(' ');
                result.push_str(&printed(elt));
            }
            result.push('\n');
            result
        }
    };
    Ok(val!(s))
}

/// An implementation of golang's `fmt.Sprintf`.
///
/// Like `fmt`, this never fails on a verb: one that does not apply to its argument
/// becomes a `%!d(string=x)` marker in the output, and a missing or leftover argument
/// a `%!d(MISSING)` or `%!(EXTRA int=2)`.
///
/// Where it still differs from Go, all verified against Go 1.26.8 and recorded in
/// `tests/go_conformance.txt`:
///
/// - A float verb on an integer is accepted. A whole float is stored as an integer so
///   `{{ 3.0 }}` renders `3` as it does in Go, which leaves the two indistinguishable;
///   Go writes `%!f(int=3)`.
/// - `%b` and `%x` on a float are not implemented -- Go writes a power-of-two mantissa
///   and hexadecimal float notation -- and neither is `%p`.
/// - `%#v` renders as plain `%v` rather than Go's syntax representation, and `%#g`
///   does not restore the trailing zeros Go restores.
///
/// # Example
/// ```
/// use gtmpl_ng::template;
/// let equal = template(r#"{{ printf "%v %s %v" "Hello" . "!" }}"#, "world");
/// assert_eq!(&equal.unwrap(), "Hello world !");
/// ```
pub fn printf(args: &[Value]) -> Result<Value, FuncError> {
    if args.is_empty() {
        return Err(FuncError::AtLeastXArgs("printf".into(), 1));
    }
    if let Value::String(ref s) = args[0] {
        Ok(val!(sprintf(s, &args[1..])))
    } else {
        Err(FuncError::Generic("printf requires a format string".into()))
    }
}

/// Returns the result of indexing its first argument by the
/// following arguments. Thus "index x 1 2 3" is, in Go syntax,
/// `x[1][2][3]`. Each indexed item must be a map, slice or array.
///
/// # Example
/// ```
/// use gtmpl_ng::template;
/// let ctx = vec![23, 42, 7];
/// let index = template("{{ index . 1 }}", ctx);
/// assert_eq!(&index.unwrap(), "42");
/// ```
/// `slice x [i [j [k]]]`, Go's builtin.
///
/// `slice x` is `x` itself, `slice x i` runs from `i` to the end, and `slice x i j` is
/// the half-open range `[i, j)`. A fourth argument is Go's capacity index, which only a
/// list accepts and which cannot be observed from a template.
///
/// Go indexes a string by byte and is content to produce invalid UTF-8 --
/// `slice "héllo" 1 2` is the single byte `\xc3` there. A Rust `String` cannot hold
/// that, so an index that falls inside a character is refused rather than answered with
/// something else. Every index that lands on a boundary agrees with Go.
pub fn slice(args: &[Value]) -> Result<Value, FuncError> {
    let (item, indices) = args
        .split_first()
        .ok_or_else(|| FuncError::AtLeastXArgs("slice".into(), 1))?;

    let length = match item {
        Value::String(text) => text.len(),
        Value::Array(values) => values.len(),
        Value::Nil => return Err(FuncError::Generic("slice of untyped nil".to_owned())),
        other => {
            return Err(FuncError::Generic(format!(
                "can't slice item of type {}",
                crate::print_verb::type_name(other).unwrap_or("nil")
            )))
        }
    };

    if indices.len() > 3 || (indices.len() > 2 && matches!(item, Value::String(_))) {
        return Err(FuncError::Generic(format!(
            "cannot {}-index slice a {}",
            indices.len(),
            crate::print_verb::type_name(item).unwrap_or("nil")
        )));
    }

    // Go checks each index against the length as it reads it, then checks that they do
    // not cross, so `slice "hello" 9 1` reports the 9 rather than the ordering.
    let mut bounds = [0usize, length, length];
    for (position, index) in indices.iter().enumerate() {
        let index = match index {
            Value::Number(number) => number.as_i64().filter(|index| *index >= 0),
            _ => None,
        };
        let index = index.and_then(|index| usize::try_from(index).ok());
        match index.filter(|index| *index <= length) {
            Some(index) => bounds[position] = index,
            None => {
                return Err(FuncError::Generic(format!(
                    "index out of range: {}",
                    indices[position]
                )))
            }
        }
    }
    if indices.len() == 1 {
        bounds[1] = length;
    }
    for pair in bounds.windows(2) {
        if pair[0] > pair[1] {
            return Err(FuncError::Generic(format!(
                "invalid slice index: {} > {}",
                pair[0], pair[1]
            )));
        }
    }
    let (start, end) = (bounds[0], bounds[1]);

    match item {
        Value::String(text) => match text.get(start..end) {
            Some(cut) => Ok(Value::from(cut.to_owned())),
            // Go would answer with invalid UTF-8 here; see this function's note.
            None => Err(FuncError::Generic(format!(
                "slice index {} is inside a character of {:?}",
                if text.is_char_boundary(start) {
                    end
                } else {
                    start
                },
                text
            ))),
        },
        Value::Array(values) => Ok(Value::Array(values[start..end].to_vec())),
        _ => unreachable!("the length match above rejected every other kind"),
    }
}

pub fn index(args: &[Value]) -> Result<Value, FuncError> {
    // Go wants at least one argument, and with exactly one -- no keys -- it answers
    // the argument itself.
    if args.is_empty() {
        return Err(FuncError::AtLeastXArgs("index".into(), 1));
    }
    let mut col = &args[0];
    for (position, val) in args[1..].iter().enumerate() {
        // Go indexes a string by byte and answers the byte as a number --
        // `{{ index "abc" 1 }}` is `98` -- which makes a value rather than borrowing
        // one, so it cannot go through `get_item`.
        if let (Value::String(text), Value::Number(n)) = (col, val) {
            // Go also refuses to index into the byte, so a follow-on key is answered
            // before the byte is even looked up.
            if let Some(next) = args.get(position + 2) {
                return Err(FuncError::Generic(format!(
                    "unable to index a byte of a string with {next}"
                )));
            }
            let byte = n
                .as_u64()
                .and_then(|i| usize::try_from(i).ok())
                .and_then(|i| text.as_bytes().get(i).copied());
            let Some(byte) = byte else {
                return Err(FuncError::IndexOutOfRange {
                    index: val.to_string(),
                });
            };
            return Ok(Value::from(u64::from(byte)));
        }
        col = get_item(col, val)?;
    }

    Ok(col.clone())
}

fn get_item<'a>(col: &'a Value, key: &Value) -> Result<&'a Value, FuncError> {
    let ret = match (col, key) {
        (Value::Array(a), Value::Number(n)) => {
            if let Some(i) = n.as_u64() {
                a.get(i as usize)
            } else {
                None
            }
        }
        (&Value::Object(ref o), &Value::Number(ref n))
        | (&Value::Map(ref o), &Value::Number(ref n)) => o.get(&n.to_string()),
        (&Value::Object(ref o), &Value::String(ref s))
        | (&Value::Map(ref o), &Value::String(ref s)) => o.get(s),
        _ => None,
    };
    match *col {
        Value::Map(_) => Ok(ret.unwrap_or(&Value::NoValue)),
        _ => ret.ok_or_else(|| FuncError::Generic(format!("unable to get {} in {}", key, col))),
    }
}

/// Go's `urlquery`: the arguments joined the way [`print()`] joins them, escaped for
/// embedding in a URL query.
///
/// The escape is `net/url`'s `QueryEscape`, byte for byte: the unreserved characters
/// `A-Z a-z 0-9 - _ . ~` pass, a space becomes `+`, and every other byte becomes
/// uppercase `%XX`. Before 1.0 this was a percent-encoding *denylist* that let `&`,
/// `=` and `+` through -- the characters that structure a query string -- so the one
/// splice the function exists to prevent, `{{ urlquery .value }}` injecting
/// parameters into a URL, was exactly what it allowed.
///
/// Spelled out rather than taken from a crate: `percent-encoding` can express the
/// allowlist (its `AsciiSet` has `remove`), but writes a space as `%20` where Go
/// writes `+`, so a second pass would be needed anyway -- and `form_urlencoded`
/// keeps `*` and escapes `~`, the reverse of Go on both. One byte loop is simpler
/// than either plus a fix-up.
///
/// # Example
/// ```
/// use gtmpl_ng::template;
/// let url = template(r#"{{ urlquery "foo bar?" }}"#, 0);
/// assert_eq!(&url.unwrap(), "foo+bar%3F");
/// ```
pub fn urlquery(args: &[Value]) -> Result<Value, FuncError> {
    use std::fmt::Write as _;
    let joined = sprint_printable(args);
    let mut escaped = String::with_capacity(joined.len());
    for &byte in joined.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                escaped.push(char::from(byte));
            }
            b' ' => escaped.push('+'),
            // Writing into a `String` cannot fail.
            _ => {
                let _ = write!(escaped, "%{byte:02X}");
            }
        }
    }
    Ok(val!(escaped))
}

/// Go's `html`: the arguments joined the way [`print()`] joins them, escaped for
/// embedding in HTML text.
///
/// Safe for HTML text content and quoted attribute values, exactly like Go's: an
/// *unquoted* attribute is not a context it can make safe, because a space or `=`
/// passes through.
///
/// Go's `template.HTMLEscapeString`, rune for rune: `<`, `>`, `&`, `"` and `'` become
/// entities -- the numeric `&#34;` and `&#39;`, which is what Go writes, rather than
/// `&quot;` and `&apos;` -- a NUL becomes U+FFFD, and everything else passes through,
/// control characters included.
///
/// # Example
/// ```
/// use gtmpl_ng::template;
/// let text = template(r#"{{ html "<b>1 & 2</b>" }}"#, 0);
/// assert_eq!(&text.unwrap(), "&lt;b&gt;1 &amp; 2&lt;/b&gt;");
/// ```
pub fn html(args: &[Value]) -> Result<Value, FuncError> {
    let joined = sprint_printable(args);
    let mut escaped = String::with_capacity(joined.len());
    for c in joined.chars() {
        match c {
            '\0' => escaped.push('\u{FFFD}'),
            '"' => escaped.push_str("&#34;"),
            '\'' => escaped.push_str("&#39;"),
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            c => escaped.push(c),
        }
    }
    Ok(val!(escaped))
}

/// Go's `js`: the arguments joined the way [`print()`] joins them, escaped for
/// embedding in a JavaScript string literal.
///
/// Safe for single- or double-quoted JS strings, exactly like Go's: a template
/// literal is not a context it can make safe, because a backtick and `${` pass
/// through.
///
/// Go's `template.JSEscapeString`: `\`, `'` and `"` get a backslash; `<`, `>`, `&`
/// and `=` become `\u00XX` so the output cannot close a surrounding `<script>` or
/// start a comment; a control character becomes `\u00XX`; and a rune that Go's
/// `strconv.IsPrint` rejects is written `\uXXXX`, with more than four digits for an
/// astral rune, exactly as Go writes it --
/// `strconv.IsPrint` and the `unicode.IsPrint` Go consults here answer the same for
/// every rune, verified exhaustively against Go 1.26.8.
///
/// # Example
/// ```
/// use gtmpl_ng::template;
/// let text = template(r#"{{ js "a='x'&\"y\"" }}"#, 0);
/// assert_eq!(&text.unwrap(), r#"a\u003D\'x\'\u0026\"y\""#);
/// ```
pub fn js(args: &[Value]) -> Result<Value, FuncError> {
    use std::fmt::Write as _;
    // Go writes `\u%04X`: at least four uppercase digits, more when the rune needs
    // them. Writing into a `String` cannot fail.
    let push_u_escape = |escaped: &mut String, scalar: u32| {
        let _ = write!(escaped, "\\u{scalar:04X}");
    };
    let joined = sprint_printable(args);
    let mut escaped = String::with_capacity(joined.len());
    for c in joined.chars() {
        match c {
            '\\' => escaped.push_str("\\\\"),
            '\'' => escaped.push_str("\\'"),
            '"' => escaped.push_str("\\\""),
            '<' | '>' | '&' | '=' => push_u_escape(&mut escaped, c as u32),
            c if (c as u32) < 0x20 => push_u_escape(&mut escaped, c as u32),
            c if c.is_ascii() || crate::print_verb::is_printable(c) => escaped.push(c),
            c => push_u_escape(&mut escaped, c as u32),
        }
    }
    Ok(val!(escaped))
}

/// Returns the boolean truth of arg1 == arg2 [== arg3 ...]
///
/// # Example
/// ```
/// use gtmpl_ng::template;
/// let equal = template("{{ eq 1 1 . }}", 1);
/// assert_eq!(&equal.unwrap(), "true");
/// ```
pub fn eq(args: &[Value]) -> Result<Value, FuncError> {
    if args.len() < 2 {
        return Err(FuncError::AtLeastXArgs("eq".into(), 2));
    }
    let first = &args[0];
    // Go's `eq`: true when the first argument equals *any* of the rest --
    // `{{ eq 1 2 1 }}` is true -- not when all of them are equal.
    Ok(Value::from(args.iter().skip(1).any(|x| *x == *first)))
}

gn!(
#[doc="
Returns the boolean truth of arg1 != arg2

# Example
```
use gtmpl_ng::template;
let not_equal = template(\"{{ ne 2 . }}\", 1);
assert_eq!(&not_equal.unwrap(), \"true\");
```
"]
ne(a: ref Value, b: ref Value) -> Result<Value, FuncError> {
 Ok(Value::from(a != b))
});

gn!(
#[doc="
Returns the boolean truth of arg1 < arg2

# Example
```
use gtmpl_ng::template;
let less_than = template(\"{{ lt 0 . }}\", 1);
assert_eq!(&less_than.unwrap(), \"true\");
```
"]
lt(a: ref Value, b: ref Value) -> Result<Value, FuncError> {
 let ret = match cmp(a, b) {
  None => return Err(FuncError::Generic(format!("unable to compare {} and {}", a, b))),
  Some(Ordering::Less) => true,
  _ => false,
 };
 Ok(Value::from(ret))
});

gn!(
#[doc="
Returns the boolean truth of arg1 <= arg2

# Example
```
use gtmpl_ng::template;
let less_or_equal = template(\"{{ le 1.4 . }}\", 1.4);
assert_eq!(less_or_equal.unwrap(), \"true\");

let less_or_equal = template(\"{{ le 0.2 . }}\", 1.4);
assert_eq!(&less_or_equal.unwrap(), \"true\");
```
"]
le(a: ref Value, b: ref Value) -> Result<Value, FuncError> {
 let ret = match cmp(a, b) {
  None => return Err(FuncError::Generic(format!("unable to compare {} and {}", a, b))),
  Some(Ordering::Less) | Some(Ordering::Equal) => true,
  _ => false,
 };
 Ok(Value::from(ret))
});

gn!(
#[doc="
Returns the boolean truth of arg1 > arg2

# Example
```
use gtmpl_ng::template;
let greater_than = template(\"{{ gt 1.4 . }}\", 1.2);
assert_eq!(&greater_than.unwrap(), \"true\");
```
"]
gt(a: ref Value, b: ref Value) -> Result<Value, FuncError> {
 let ret = match cmp(a, b) {
  None => return Err(FuncError::Generic(format!("unable to compare {} and {}", a, b))),
  Some(Ordering::Greater) => true,
  _ => false,
 };
 Ok(Value::from(ret))
});

gn!(
#[doc="
Returns the boolean truth of arg1 >= arg2

# Example
```
use gtmpl_ng::template;
let greater_or_equal = template(\"{{ ge 1.4 1.3 }}\", 1.2);
assert_eq!(greater_or_equal.unwrap(), \"true\");

let greater_or_equal = template(\"{{ ge 1.4 . }}\", 0.2);
assert_eq!(&greater_or_equal.unwrap(), \"true\");
```
"]
ge(a: ref Value, b: ref Value) -> Result<Value, FuncError> {
 let ret = match cmp(a, b) {
  None => return Err(FuncError::Generic(format!("unable to compare {} and {}", a, b))),
  Some(Ordering::Greater) | Some(Ordering::Equal) => true,
  _ => false,
 };
 Ok(Value::from(ret))
});

fn cmp(left: &Value, right: &Value) -> Option<Ordering> {
    match (left, right) {
        // `Number`'s own ordering is exact across all three representations, so
        // there is nothing left to do here. It used to widen a mixed pair to `f64`,
        // which is lossy past the mantissa, and before that it answered `None` for
        // any pair stored as integers -- `lt 5 3.5` reported "unable to compare".
        (Value::Number(l), Value::Number(r)) => l.partial_cmp(r),
        (Value::Bool(l), Value::Bool(r)) => l.partial_cmp(r),
        (Value::String(l), Value::String(r)) => l.partial_cmp(r),
        (Value::Array(l), Value::Array(r)) => l.len().partial_cmp(&r.len()),
        _ => None,
    }
}

#[cfg(test)]
mod tests_mocked {
    use super::*;
    use std::collections::HashMap;

    /// Go's builtin on a list. The conformance fixture renders without the helm
    /// registry, so it has no `list` to build one with -- these go through the
    /// function directly.
    #[test]
    fn slicing_a_list_follows_go() {
        let list = Value::from(vec![1u64, 2, 3, 4, 5]);
        let cut = |args: &[Value]| slice(args).map(|v| v.to_string());

        assert_eq!(cut(std::slice::from_ref(&list)).unwrap(), "[1 2 3 4 5]");
        assert_eq!(cut(&[list.clone(), val!(1u64)]).unwrap(), "[2 3 4 5]");
        assert_eq!(
            cut(&[list.clone(), val!(1u64), val!(3u64)]).unwrap(),
            "[2 3]"
        );
        assert_eq!(cut(&[list.clone(), val!(5u64)]).unwrap(), "[]");
        assert_eq!(cut(&[list.clone(), val!(2u64), val!(2u64)]).unwrap(), "[]");
        // A third index is Go's capacity, which a template cannot observe; the point is
        // that a list accepts it where a string does not.
        assert_eq!(
            cut(&[list.clone(), val!(1u64), val!(3u64), val!(4u64)]).unwrap(),
            "[2 3]"
        );

        let message = |args: &[Value]| slice(args).unwrap_err().to_string();
        assert_eq!(
            message(&[list.clone(), val!(0u64), val!(9u64)]),
            "index out of range: 9"
        );
        assert_eq!(
            message(&[list.clone(), val!(3u64), val!(1u64)]),
            "invalid slice index: 3 > 1"
        );
        assert_eq!(
            message(&[list, val!(1u64), val!(2u64), val!(3u64), val!(4u64)]),
            "cannot 4-index slice a []interface {}"
        );
        assert_eq!(message(&[]), "slice requires at least 1 argument(s)");
    }

    #[test]
    fn test_macro() {
        gtmpl_fn!(
            fn f1(i: i64) -> Result<i64, FuncError> {
                Ok(i + 1)
            }
        );
        let vals: Vec<Value> = vec![val!(1i64)];
        let ret = f1(&vals);
        assert_eq!(ret.unwrap(), Value::from(2i64));

        gtmpl_fn!(
            fn f3(i: i64, j: i64, k: i64) -> Result<i64, FuncError> {
                Ok(i + j + k)
            }
        );
        let vals: Vec<Value> = vec![val!(1i64), val!(2i64), val!(3i64)];
        let ret = f3(&vals);
        assert_eq!(ret.unwrap(), Value::from(6i64));
    }

    #[test]
    fn test_eq() {
        let vals: Vec<Value> = vec![val!("foo".to_owned()), val!("foo".to_owned())];
        let ret = eq(&vals);
        assert_eq!(ret.unwrap(), Value::Bool(true));
        let vals: Vec<Value> = vec![val!(1u32), val!(1u32), val!(1i8)];
        let ret = eq(&vals);
        assert_eq!(ret.unwrap(), Value::Bool(true));
        let vals: Vec<Value> = vec![val!(false), val!(false), val!(false)];
        let ret = eq(&vals);
        assert_eq!(ret.unwrap(), Value::Bool(true));
    }

    #[test]
    fn test_and() {
        let vals: Vec<Value> = vec![val!(0i32), val!(1u8)];
        let ret = and(&vals);
        assert_eq!(ret.unwrap(), Value::from(0i32));

        let vals: Vec<Value> = vec![val!(1i32), val!(2u8)];
        let ret = and(&vals);
        assert_eq!(ret.unwrap(), Value::from(2u8));
    }

    #[test]
    fn test_or() {
        let vals: Vec<Value> = vec![val!(0i32), val!(1u8)];
        let ret = or(&vals);
        assert_eq!(ret.unwrap(), Value::from(1u8));

        let vals: Vec<Value> = vec![val!(0i32), val!(0u8)];
        let ret = or(&vals);
        assert_eq!(ret.unwrap(), Value::from(0u8));
    }

    #[test]
    fn test_ne() {
        let vals: Vec<Value> = vec![val!(0i32), val!(1u8)];
        let ret = ne(&vals);
        assert_eq!(ret.unwrap(), Value::from(true));

        let vals: Vec<Value> = vec![val!(0i32), val!(0u8)];
        let ret = ne(&vals);
        assert_eq!(ret.unwrap(), Value::from(false));

        let vals: Vec<Value> = vec![val!("foo"), val!("bar")];
        let ret = ne(&vals);
        assert_eq!(ret.unwrap(), Value::from(true));

        let vals: Vec<Value> = vec![val!("foo"), val!("foo")];
        let ret = ne(&vals);
        assert_eq!(ret.unwrap(), Value::from(false));
    }

    #[test]
    fn test_lt() {
        let vals: Vec<Value> = vec![val!(-1i32), val!(1u8)];
        let ret = lt(&vals);
        assert_eq!(ret.unwrap(), Value::from(true));

        let vals: Vec<Value> = vec![val!(0i32), val!(0u8)];
        let ret = lt(&vals);
        assert_eq!(ret.unwrap(), Value::from(false));

        let vals: Vec<Value> = vec![val!(1i32), val!(0u8)];
        let ret = lt(&vals);
        assert_eq!(ret.unwrap(), Value::from(false));
    }

    #[test]
    fn test_le() {
        let vals: Vec<Value> = vec![val!(-1i32), val!(1u8)];
        let ret = le(&vals);
        assert_eq!(ret.unwrap(), Value::from(true));

        let vals: Vec<Value> = vec![val!(0i32), val!(0u8)];
        let ret = le(&vals);
        assert_eq!(ret.unwrap(), Value::from(true));

        let vals: Vec<Value> = vec![val!(1i32), val!(0u8)];
        let ret = le(&vals);
        assert_eq!(ret.unwrap(), Value::from(false));
    }

    #[test]
    fn test_gt() {
        let vals: Vec<Value> = vec![val!(-1i32), val!(1u8)];
        let ret = gt(&vals);
        assert_eq!(ret.unwrap(), Value::from(false));

        let vals: Vec<Value> = vec![val!(0i32), val!(0u8)];
        let ret = gt(&vals);
        assert_eq!(ret.unwrap(), Value::from(false));

        let vals: Vec<Value> = vec![val!(1i32), val!(0u8)];
        let ret = gt(&vals);
        assert_eq!(ret.unwrap(), Value::from(true));
    }

    #[test]
    fn test_ge() {
        let vals: Vec<Value> = vec![val!(-1i32), val!(1u8)];
        let ret = ge(&vals);
        assert_eq!(ret.unwrap(), Value::from(false));

        let vals: Vec<Value> = vec![val!(0i32), val!(0u8)];
        let ret = ge(&vals);
        assert_eq!(ret.unwrap(), Value::from(true));

        let vals: Vec<Value> = vec![val!(1i32), val!(0u8)];
        let ret = ge(&vals);
        assert_eq!(ret.unwrap(), Value::from(true));
    }

    #[test]
    fn test_print() {
        let vals: Vec<Value> = vec![val!("foo"), val!(1u8)];
        let ret = print(&vals);
        assert_eq!(ret.unwrap(), Value::from("foo1"));

        let vals: Vec<Value> = vec![val!("foo"), val!(1u8), val!(2)];
        let ret = print(&vals);
        assert_eq!(ret.unwrap(), Value::from("foo1 2"));

        let vals: Vec<Value> = vec![val!(true), val!(1), val!("foo"), val!(2)];
        let ret = print(&vals);
        assert_eq!(ret.unwrap(), Value::from("true 1foo2"));
    }

    #[test]
    fn test_println() {
        let vals: Vec<Value> = vec![val!("foo"), val!(1u8)];
        let ret = println(&vals);
        assert_eq!(ret.unwrap(), Value::from("foo 1\n"));

        let vals: Vec<Value> = vec![];
        let ret = println(&vals);
        assert_eq!(ret.unwrap(), Value::from("\n"));
    }

    #[test]
    fn test_index() {
        let vals: Vec<Value> = vec![val!(vec![vec![1, 2], vec![3, 4]]), val!(1), val!(0)];
        let ret = index(&vals);
        assert_eq!(ret.unwrap(), Value::from(3));

        let mut o = HashMap::new();
        o.insert(String::from("foo"), vec![String::from("bar")]);
        let col = Value::from(o);
        let vals: Vec<Value> = vec![col, val!("foo"), val!(0)];
        let ret = index(&vals);
        assert_eq!(ret.unwrap(), Value::from("bar"));

        let mut o = HashMap::new();
        o.insert(String::from("foo"), String::from("bar"));
        let col = Value::from(o);
        let vals: Vec<Value> = vec![col, val!("foo2")];
        let ret = index(&vals);
        assert_eq!(ret.unwrap(), Value::NoValue);
    }

    #[test]
    fn test_builtins() {
        let vals: Vec<Value> = vec![val!("foo".to_owned()), val!("foo".to_owned())];
        let builtin_eq = BUILTINS
            .iter()
            .find(|&&(n, _)| n == "eq")
            .map(|&(_, f)| f)
            .unwrap();
        let ret = builtin_eq(&vals);
        assert_eq!(ret.unwrap(), Value::Bool(true));
    }

    #[test]
    fn test_gtmpl_fn() {
        gtmpl_fn!(
            fn add(a: u64, b: u64) -> Result<u64, FuncError> {
                Ok(a + b)
            }
        );
        let vals: Vec<Value> = vec![val!(1u32), val!(2u32)];
        let ret = add(&vals);
        assert_eq!(ret.unwrap(), Value::from(3u32));

        gtmpl_fn!(
            fn has_prefix(s: String, prefix: String) -> Result<bool, FuncError> {
                Ok(s.starts_with(&prefix))
            }
        );
        let vals: Vec<Value> = vec![val!("foobar"), val!("foo")];
        let ret = has_prefix(&vals);
        assert_eq!(ret.unwrap(), Value::from(true));
    }

    /// Every pair was run through Go 1.26.8's `text/template`; the same cases render
    /// end to end in `tests/go_conformance.txt`. The second case is the reason the
    /// function exists: `&`, `=` and `+` used to pass through unescaped, which let a
    /// value splice parameters into the query string it was quoted for.
    #[test]
    fn urlquery_matches_gos_query_escape() {
        for (input, expected) in [
            ("a b", "a+b"),
            ("a+b/c?d=e&f", "a%2Bb%2Fc%3Fd%3De%26f"),
            ("\u{e4}\u{f6}\u{fc}", "%C3%A4%C3%B6%C3%BC"),
            ("~-_.!*'()", "~-_.%21%2A%27%28%29"),
            ("#[]|\\^%", "%23%5B%5D%7C%5C%5E%25"),
            ("", ""),
        ] {
            let escaped = urlquery(&[val!(input)]).unwrap();
            assert_eq!(escaped, val!(expected), "{input:?}");
        }
    }

    /// The rows Go 1.26.8 printed for `template.HTMLEscapeString` over every ASCII
    /// byte, plus the NUL replacement, which is the one rewrite that is not an entity.
    #[test]
    fn html_matches_gos_escaper() {
        for (input, expected) in [
            ("<b>1 & 2</b>", "&lt;b&gt;1 &amp; 2&lt;/b&gt;"),
            ("a'b\"c", "a&#39;b&#34;c"),
            ("a\u{0}b", "a\u{FFFD}b"),
            ("\n\t\u{1b}", "\n\t\u{1b}"),
            (
                "\u{e4}\u{f6}\u{fc} \u{1F600}",
                "\u{e4}\u{f6}\u{fc} \u{1F600}",
            ),
        ] {
            assert_eq!(html(&[val!(input)]).unwrap(), val!(expected), "{input:?}");
        }
    }

    /// The rows Go 1.26.8 printed for `template.JSEscapeString`: the backslash
    /// escapes, the `\u00XX` metacharacters, the control characters, and the
    /// printability boundary -- U+2028 and U+2029 escaped (they end a JS line),
    /// the no-break space and C1 controls escaped, an astral non-printable rune
    /// written with five digits the way Go writes it.
    #[test]
    fn js_matches_gos_escaper() {
        for (input, expected) in [
            ("a='x'&\"y\"", "a\\u003D\\'x\\'\\u0026\\\"y\\\""),
            ("back\\slash", "back\\\\slash"),
            ("<script>", "\\u003Cscript\\u003E"),
            ("\u{0}\u{1f}", "\\u0000\\u001F"),
            ("\u{2028}\u{2029}", "\\u2028\\u2029"),
            ("\u{a0}\u{80}\u{85}", "\\u00A0\\u0080\\u0085"),
            ("\u{E0001}", "\\uE0001"),
            ("\u{e4} \u{4e2d} \u{1F600}", "\u{e4} \u{4e2d} \u{1F600}"),
        ] {
            assert_eq!(js(&[val!(input)]).unwrap(), val!(expected), "{input:?}");
        }
    }

    /// `html` and `js` join the same way, and an untyped nil shows as `<no value>`
    /// the way Go's `evalArgs` shows it -- where `print` itself writes `<nil>`.
    #[test]
    fn html_and_js_take_what_print_takes() {
        assert_eq!(html(&[]).unwrap(), val!(""));
        assert_eq!(js(&[]).unwrap(), val!(""));
        assert_eq!(
            html(&[Value::from(1), val!("<")]).unwrap(),
            val!("1&lt;"),
            "a number is stringified, two non-strings would get a space"
        );
        assert_eq!(js(&[val!("a"), val!("<")]).unwrap(), val!("a\\u003C"));
        assert_eq!(html(&[Value::Nil]).unwrap(), val!("&lt;no value&gt;"));
        assert_eq!(
            js(&[Value::Nil]).unwrap(),
            val!("\\u003Cno value\\u003E"),
            "the brackets of <no value> are metacharacters like any other"
        );
        assert_eq!(
            urlquery(&[Value::Nil, Value::Nil]).unwrap(),
            val!("%3Cno+value%3E%3Cno+value%3E"),
            "two nils join with a space, and the space escapes to +"
        );
        assert_eq!(
            print(&[Value::Nil]).unwrap(),
            val!("<nil>"),
            "print keeps fmt.Sprint's spelling"
        );
    }

    /// Go's `URLQueryEscaper` is variadic over any values, joined like `print` -- so
    /// a number is stringified rather than refused, and the space `print` puts
    /// between two non-strings is escaped like any other.
    #[test]
    fn urlquery_takes_what_print_takes() {
        assert_eq!(urlquery(&[Value::from(42)]).unwrap(), val!("42"));
        assert_eq!(urlquery(&[val!("a"), val!("b")]).unwrap(), val!("ab"));
        assert_eq!(
            urlquery(&[Value::from(1), Value::from(2)]).unwrap(),
            val!("1+2")
        );
        assert_eq!(urlquery(&[]).unwrap(), val!(""));
    }
}
