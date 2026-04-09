use crate::{FuncError, Value};
use rand::seq::SliceRandom;
use rand::Rng;

pub fn trim(args: &[Value]) -> Result<Value, FuncError> {
    let value = &args.first().ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 1 argument.".to_string(),
        1,
    ))?;

    let value = value.to_string();
    let value = value.trim();
    Ok(Value::from(value))
}

pub fn trim_all(args: &[Value]) -> Result<Value, FuncError> {
    let character = args
        .first()
        .ok_or(FuncError::ExactlyXArgs(
            "This function requires exactly 2 arguments.".to_string(),
            2,
        ))?
        .to_string();
    let value = &args.get(1).ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 2 arguments.".to_string(),
        2,
    ))?;
    let character = character
        .chars()
        .next()
        .ok_or(FuncError::Generic("Invalid character".to_string()))?;
    let value = value.to_string();
    let value = value.trim_matches(character);
    Ok(Value::from(value))
}

pub fn trim_prefix(args: &[Value]) -> Result<Value, FuncError> {
    let arg0 = args.first().ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 2 arguments.".to_string(),
        2,
    ))?;
    let prefix = arg0.to_string();
    let value = &args.get(1).ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 2 arguments.".to_string(),
        2,
    ))?;
    let value = value.to_string();
    let value = value.trim_start_matches(&prefix);
    Ok(Value::from(value))
}

pub fn trim_suffix(args: &[Value]) -> Result<Value, FuncError> {
    let arg0 = &args.first().ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 2 arguments.".to_string(),
        2,
    ))?;
    let suffix = arg0.to_string();
    let value = &args.get(1).ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 2 arguments.".to_string(),
        2,
    ))?;
    let value = value.to_string();
    let value = value.trim_end_matches(&suffix);
    Ok(Value::from(value))
}

pub fn lower(args: &[Value]) -> Result<Value, FuncError> {
    let value = &args.first().ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 1 argument.".to_string(),
        1,
    ))?;
    let value = value.to_string();
    let value = value.to_lowercase();
    Ok(Value::from(value))
}

pub fn upper(args: &[Value]) -> Result<Value, FuncError> {
    let value = &args.first().ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 1 argument.".to_string(),
        1,
    ))?;
    let value = value.to_string();
    let value = value.to_uppercase();
    Ok(Value::from(value))
}

pub fn title(args: &[Value]) -> Result<Value, FuncError> {
    let value = &args.first().ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 1 argument.".to_string(),
        1,
    ))?;
    let value = value.to_string();

    let words = value.split_whitespace();
    let mut result = String::new();
    for word in words {
        let mut chars = word.chars();
        let mut first_char = chars.next().ok_or(FuncError::Generic(
            "Invalid word. Word must have at least one character".to_string(),
        ))?;
        first_char = first_char.to_uppercase().next().ok_or(FuncError::Generic(
            "Invalid word. Word must have at least one character".to_string(),
        ))?;
        result.push(first_char);
        for kar in chars {
            result.push(kar);
        }
        result.push(' ');
    }

    Ok(Value::from(value))
}

pub fn untitle(args: &[Value]) -> Result<Value, FuncError> {
    let value = &args.first().ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 1 argument.".to_string(),
        1,
    ))?;
    let value = value.to_string();

    let words = value.split_whitespace();
    let mut result = String::new();
    for word in words {
        let mut chars = word.chars();
        let mut first_char = chars.next().ok_or(FuncError::Generic(
            "Invalid word. Word must have at least one character".to_string(),
        ))?;
        first_char = first_char.to_lowercase().next().ok_or(FuncError::Generic(
            "Invalid word. Word must have at least one character".to_string(),
        ))?;
        result.push(first_char);
        for kar in chars {
            result.push(kar);
        }
        result.push(' ');
    }

    Ok(Value::from(value))
}

pub fn repeat(args: &[Value]) -> Result<Value, FuncError> {
    let times = &args.first().ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 2 arguments.".to_string(),
        2,
    ))?;
    let value = &args.get(1).ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 2 arguments.".to_string(),
        2,
    ))?;
    let value = value.to_string();
    let times = times.to_string();
    let times = times
        .parse::<usize>()
        .map_err(|_| FuncError::Generic("Invalid number".to_string()))?;
    let value = value.repeat(times);
    Ok(Value::from(value))
}

pub fn substr(args: &[Value]) -> Result<Value, FuncError> {
    let start = &args.first().ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 3 arguments.".to_string(),
        3,
    ))?;
    let start = start.to_string();
    let start = start
        .parse::<usize>()
        .map_err(|_| FuncError::Generic("Invalid number".to_string()))?;
    let end = &args.get(1).ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 3 arguments.".to_string(),
        3,
    ))?;
    let end = end.to_string();
    let end = end
        .parse::<usize>()
        .map_err(|_| FuncError::Generic("Invalid number".to_string()))?;
    let value = &args.get(2).ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 3 arguments.".to_string(),
        3,
    ))?;
    let value = value.to_string();
    // Validate indices before slicing. Without these checks, `&value[start..end]`
    // would panic on out-of-bounds, inverted ranges, or non-char-boundary slices
    // (e.g., slicing in the middle of a multibyte character).
    if start > end {
        return Err(FuncError::Generic(format!(
            "substr: start ({start}) must be <= end ({end})"
        )));
    }
    if end > value.len() {
        return Err(FuncError::Generic(format!(
            "substr: end index {end} out of bounds for string of length {}",
            value.len()
        )));
    }
    if !value.is_char_boundary(start) || !value.is_char_boundary(end) {
        return Err(FuncError::Generic(
            "substr: index is not on a UTF-8 character boundary".to_string(),
        ));
    }
    let value = &value[start..end];
    Ok(Value::from(value))
}

pub fn nospace(args: &[Value]) -> Result<Value, FuncError> {
    let value = &args.first().ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 1 argument.".to_string(),
        1,
    ))?;
    let value = value.to_string();
    let value = value.replace(' ', "");
    Ok(Value::from(value))
}

pub fn trunc(args: &[Value]) -> Result<Value, FuncError> {
    let trunc_index = &args.first().ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 2 arguments.".to_string(),
        2,
    ))?;
    let trunc_index = trunc_index.to_string();
    // Parse as i64 so that negative indices (Sprig/Helm semantics: trunc the
    // *last* N bytes when N is negative) are supported; the previous code
    // parsed as usize which made the `negative` branch dead.
    let trunc_index: i64 = trunc_index.parse().map_err(|_| {
        FuncError::Generic("Invalid number. Number must be an integer".to_string())
    })?;
    let value = &args.get(1).ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 2 arguments.".to_string(),
        2,
    ))?;
    let value = value.to_string();

    // If the request exceeds the string length, fall back to the full string
    // (matching Sprig's `trunc` behaviour and avoiding byte-index panics).
    let (lo, hi) = if trunc_index < 0 {
        let n = (-trunc_index) as usize;
        if n >= value.len() {
            return Ok(Value::from(value));
        }
        (value.len() - n, value.len())
    } else {
        let n = trunc_index as usize;
        if n >= value.len() {
            return Ok(Value::from(value));
        }
        (0, n)
    };

    if !value.is_char_boundary(lo) || !value.is_char_boundary(hi) {
        return Err(FuncError::Generic(
            "trunc: index is not on a UTF-8 character boundary".to_string(),
        ));
    }
    Ok(Value::from(&value[lo..hi]))
}

pub fn abbrev(args: &[Value]) -> Result<Value, FuncError> {
    let max_length = &args.first().ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 2 arguments.".to_string(),
        2,
    ))?;
    let max_length = max_length.to_string();
    let max_length = max_length.parse::<usize>().map_err(|_| {
        FuncError::Generic("Invalid number. Number must be a positive integer".to_string())
    })?;

    let value = &args.get(1).ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 2 arguments.".to_string(),
        2,
    ))?;
    let value = value.to_string();

    // Sprig semantics: an abbreviation needs at least 4 chars ("x..."), and
    // if the input already fits in max_length we return it unchanged. Without
    // these guards, the old `max_length - 3` subtraction underflowed usize,
    // and `&value[..max_length - 3]` panicked for long `max_length`.
    if max_length < 4 || value.len() <= max_length {
        return Ok(Value::from(value));
    }
    let cut = max_length - 3;
    if !value.is_char_boundary(cut) {
        return Err(FuncError::Generic(
            "abbrev: cut point is not on a UTF-8 character boundary".to_string(),
        ));
    }
    let mut out = String::from(&value[..cut]);
    out.push_str("...");
    Ok(Value::from(out))
}

pub fn abbrevboth(args: &[Value]) -> Result<Value, FuncError> {
    let left_offset = &args.first().ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 3 arguments.".to_string(),
        3,
    ))?;
    let left_offset = left_offset.to_string();
    let left_offset = left_offset.parse::<usize>().map_err(|_| {
        FuncError::Generic("Invalid number. Number must be a positive integer".to_string())
    })?;

    let max_length = &args.get(1).ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 3 arguments.".to_string(),
        3,
    ))?;
    let max_length = max_length.to_string();
    let max_length = max_length.parse::<usize>().map_err(|_| {
        FuncError::Generic("Invalid number. Number must be a positive integer".to_string())
    })?;

    let value = &args.get(2).ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 3 arguments.".to_string(),
        3,
    ))?;
    let value = value.to_string();

    // Needs room for "..." on each side, so max_length must be at least 7 for
    // any non-trivial abbreviation. Short inputs or undersized max_length
    // return the original value. These guards prevent the previous
    // `max_length - 3` underflow and out-of-bounds slice.
    if max_length < 7 || value.len() <= max_length {
        return Ok(Value::from(value));
    }
    let end = max_length - 3;
    if left_offset >= end || end > value.len() {
        return Err(FuncError::Generic(format!(
            "abbrevboth: invalid range {left_offset}..{end} for string of length {}",
            value.len()
        )));
    }
    if !value.is_char_boundary(left_offset) || !value.is_char_boundary(end) {
        return Err(FuncError::Generic(
            "abbrevboth: index is not on a UTF-8 character boundary".to_string(),
        ));
    }
    let mut out = String::from("...");
    out.push_str(&value[left_offset..end]);
    out.push_str("...");
    Ok(Value::from(out))
}

pub fn initials(args: &[Value]) -> Result<Value, FuncError> {
    let value = &args.first().ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 1 argument.".to_string(),
        1,
    ))?;
    let value = value.to_string();
    let words = value.split_whitespace();
    let mut result = String::new();
    for word in words {
        let mut chars = word.chars();
        let mut first_char = chars.next().ok_or(FuncError::Generic(
            "Invalid word. Word must have at least one character".to_string(),
        ))?;
        first_char = first_char.to_uppercase().next().ok_or(FuncError::Generic(
            "Invalid word. Word must have at least one character".to_string(),
        ))?;
        result.push(first_char);
    }
    Ok(Value::from(result))
}

pub fn rand_alpha(args: &[Value]) -> Result<Value, FuncError> {
    let length = &args.first().ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 1 argument.".to_string(),
        1,
    ))?;
    let length = length.to_string();
    let length = length
        .parse::<usize>()
        .map_err(|_| FuncError::Generic("Invalid number".to_string()))?;
    let mut rng = rand::rng();

    let charset: Vec<u8> = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz".to_vec();

    let generated: String = (0..length)
        .map(|_| {
            let idx = rng.random_range(0..charset.len());
            *charset.get(idx).unwrap() as char
        })
        .collect();
    Ok(Value::from(generated))
}

pub fn rand_numeric(args: &[Value]) -> Result<Value, FuncError> {
    let length = &args.first().ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 1 argument.".to_string(),
        1,
    ))?;
    let length = length.to_string();
    let length = length
        .parse::<usize>()
        .map_err(|_| FuncError::Generic("Invalid number".to_string()))?;
    let mut rng = rand::rng();

    let charset: Vec<u8> = b"0123456789".to_vec();

    let generated: String = (0..length)
        .map(|_| {
            let idx = rng.random_range(0..charset.len());
            *charset.get(idx).unwrap() as char
        })
        .collect();
    Ok(Value::from(generated))
}

pub fn rand_alpha_num(args: &[Value]) -> Result<Value, FuncError> {
    let length = &args.first().ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 1 argument.".to_string(),
        1,
    ))?;
    let length = length.to_string();
    let length = length
        .parse::<usize>()
        .map_err(|_| FuncError::Generic("Invalid number".to_string()))?;
    let mut rng = rand::rng();

    let charset: Vec<u8> =
        b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz".to_vec();

    let generated: String = (0..length)
        .map(|_| {
            let idx = rng.random_range(0..charset.len());
            *charset.get(idx).unwrap() as char
        })
        .collect();
    Ok(Value::from(generated))
}

pub fn rand_ascii(args: &[Value]) -> Result<Value, FuncError> {
    let length = &args.first().ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 1 argument.".to_string(),
        1,
    ))?;
    let length = length.to_string();
    let length = length
        .parse::<usize>()
        .map_err(|_| FuncError::Generic("Invalid number".to_string()))?;
    let mut rng = rand::rng();

    let charset: Vec<u8> = b" !#$%&\'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~".to_vec();

    let generated: String = (0..length)
        .map(|_| {
            let idx = rng.random_range(0..charset.len());
            *charset.get(idx).unwrap() as char
        })
        .collect();
    Ok(Value::from(generated))
}

pub fn contains(args: &[Value]) -> Result<Value, FuncError> {
    let needle = &args.first().ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 2 arguments.".to_string(),
        2,
    ))?;
    let needle = needle.to_string();
    let haystack = &args.get(1).ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 2 arguments.".to_string(),
        2,
    ))?;
    let haystack = haystack.to_string();
    let result = haystack.contains(&needle);
    Ok(Value::from(result))
}

pub fn has_prefix(args: &[Value]) -> Result<Value, FuncError> {
    let prefix = &args.first().ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 2 arguments.".to_string(),
        2,
    ))?;
    let prefix = prefix.to_string();
    let value = &args.get(1).ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 2 arguments.".to_string(),
        2,
    ))?;
    let value = value.to_string();
    let result = value.starts_with(&prefix);
    Ok(Value::from(result))
}

pub fn has_suffix(args: &[Value]) -> Result<Value, FuncError> {
    let suffix = &args.first().ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 2 arguments.".to_string(),
        2,
    ))?;
    let suffix = suffix.to_string();
    let value = &args.get(1).ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 2 arguments.".to_string(),
        2,
    ))?;
    let value = value.to_string();
    let result = value.ends_with(&suffix);
    Ok(Value::from(result))
}

pub fn quote(args: &[Value]) -> Result<Value, FuncError> {
    let value = &args.first().ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 1 argument.".to_string(),
        1,
    ))?;
    let value = value.to_string();
    let mut result = String::new();
    result.push('"');
    result.push_str(&value);
    result.push('"');
    Ok(Value::from(result))
}

pub fn squote(args: &[Value]) -> Result<Value, FuncError> {
    let value = &args.first().ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 1 argument.".to_string(),
        1,
    ))?;
    let value = value.to_string();
    let mut result = String::new();
    result.push('\'');
    result.push_str(&value);
    result.push('\'');
    Ok(Value::from(result))
}

pub fn cat(args: &[Value]) -> Result<Value, FuncError> {
    let mut result = String::new();
    for arg in args {
        let arg = arg.to_string();
        result.push_str(&arg);

        result.push(' ');
    }
    result.pop();
    Ok(Value::from(result))
}

pub fn indent(args: &[Value]) -> Result<Value, FuncError> {
    // first argument is the indent number
    let indent = &args.first().ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 2 arguments.".to_string(),
        2,
    ))?;

    let indent = indent.to_string();

    let indent = indent
        .parse::<usize>()
        .map_err(|_| FuncError::Generic("Invalid number".to_string()))?;

    // second argument is the value to indent

    let value = &args.get(1).ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 2 arguments.".to_string(),
        2,
    ))?;

    let value = value.to_string();

    let indent_string = " ".repeat(indent);

    let mut result = String::new();
    let lines = value.split('\n');
    let line_count = lines.clone().count();
    for (index, line) in lines.enumerate() {
        result.push_str(&indent_string);
        result.push_str(line);

        if index != line_count - 1 {
            result.push('\n');
        }
    }
    Ok(Value::from(result))
}

pub fn replace(args: &[Value]) -> Result<Value, FuncError> {
    let old = &args.first().ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 3 arguments.".to_string(),
        3,
    ))?;
    let old = old.to_string();
    let new = &args.get(1).ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 3 arguments.".to_string(),
        3,
    ))?;
    let new = new.to_string();
    let value = &args.get(2).ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 3 arguments.".to_string(),
        3,
    ))?;
    let value = value.to_string();
    let result = value.replace(&old, &new);
    Ok(Value::from(result))
}

pub fn plural(args: &[Value]) -> Result<Value, FuncError> {
    let singular = &args.first().ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 3 arguments.".to_string(),
        3,
    ))?;
    let singular = singular.to_string();
    let plural = &args.get(1).ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 3 arguments.".to_string(),
        3,
    ))?;
    let plural = plural.to_string();
    let length = &args.get(2).ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 3 arguments.".to_string(),
        3,
    ))?;
    let length = length.to_string();
    let length = length
        .parse::<usize>()
        .map_err(|_| FuncError::Generic("Invalid number".to_string()))?;

    let result = if length == 1 { singular } else { plural };
    Ok(Value::from(result))
}

/// Convert string to snake_case
pub fn snakecase(args: &[Value]) -> Result<Value, FuncError> {
    let value = &args.first().ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 1 argument.".to_string(),
        1,
    ))?;
    let value = value.to_string();

    let mut result = String::new();
    let mut prev_is_lower = false;

    for (i, ch) in value.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 && prev_is_lower {
                result.push('_');
            }
            result.push(ch.to_lowercase().next().unwrap());
            prev_is_lower = false;
        } else if ch.is_whitespace() || ch == '-' {
            result.push('_');
            prev_is_lower = false;
        } else {
            result.push(ch);
            prev_is_lower = ch.is_lowercase();
        }
    }

    Ok(Value::from(result))
}

/// Convert string to camelCase
pub fn camelcase(args: &[Value]) -> Result<Value, FuncError> {
    let value = &args.first().ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 1 argument.".to_string(),
        1,
    ))?;
    let value = value.to_string();

    let mut result = String::new();
    let mut capitalize_next = false;
    let mut first_char = true;

    for ch in value.chars() {
        if ch.is_whitespace() || ch == '_' || ch == '-' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(ch.to_uppercase().next().unwrap());
            capitalize_next = false;
            first_char = false;
        } else if first_char {
            result.push(ch.to_lowercase().next().unwrap());
            first_char = false;
        } else {
            result.push(ch);
        }
    }

    Ok(Value::from(result))
}

/// Convert string to kebab-case
pub fn kebabcase(args: &[Value]) -> Result<Value, FuncError> {
    let value = &args.first().ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 1 argument.".to_string(),
        1,
    ))?;
    let value = value.to_string();

    let mut result = String::new();
    let mut prev_is_lower = false;

    for (i, ch) in value.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 && prev_is_lower {
                result.push('-');
            }
            result.push(ch.to_lowercase().next().unwrap());
            prev_is_lower = false;
        } else if ch.is_whitespace() || ch == '_' {
            result.push('-');
            prev_is_lower = false;
        } else {
            result.push(ch);
            prev_is_lower = ch.is_lowercase();
        }
    }

    Ok(Value::from(result))
}

/// Swap the case of each character in the string
pub fn swapcase(args: &[Value]) -> Result<Value, FuncError> {
    let value = &args.first().ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 1 argument.".to_string(),
        1,
    ))?;
    let value = value.to_string();

    let result: String = value
        .chars()
        .map(|ch| {
            if ch.is_uppercase() {
                ch.to_lowercase().next().unwrap()
            } else if ch.is_lowercase() {
                ch.to_uppercase().next().unwrap()
            } else {
                ch
            }
        })
        .collect();

    Ok(Value::from(result))
}

/// Shuffle the characters in the string
pub fn shuffle(args: &[Value]) -> Result<Value, FuncError> {
    let value = &args.first().ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 1 argument.".to_string(),
        1,
    ))?;
    let value = value.to_string();

    let mut chars: Vec<char> = value.chars().collect();
    let mut rng = rand::rng();
    chars.shuffle(&mut rng);

    let result: String = chars.into_iter().collect();
    Ok(Value::from(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snakecase() {
        let result = snakecase(&[Value::String("HelloWorld".to_string())]).unwrap();
        assert_eq!(result.to_string(), "hello_world");
    }

    #[test]
    fn test_snakecase_with_spaces() {
        let result = snakecase(&[Value::String("Hello World Test".to_string())]).unwrap();
        assert_eq!(result.to_string(), "hello_world_test");
    }

    #[test]
    fn test_camelcase() {
        let result = camelcase(&[Value::String("hello_world".to_string())]).unwrap();
        assert_eq!(result.to_string(), "helloWorld");
    }

    #[test]
    fn test_camelcase_with_spaces() {
        let result = camelcase(&[Value::String("hello world test".to_string())]).unwrap();
        assert_eq!(result.to_string(), "helloWorldTest");
    }

    #[test]
    fn test_kebabcase() {
        let result = kebabcase(&[Value::String("HelloWorld".to_string())]).unwrap();
        assert_eq!(result.to_string(), "hello-world");
    }

    #[test]
    fn test_kebabcase_with_underscores() {
        let result = kebabcase(&[Value::String("hello_world_test".to_string())]).unwrap();
        assert_eq!(result.to_string(), "hello-world-test");
    }

    #[test]
    fn test_swapcase() {
        let result = swapcase(&[Value::String("Hello World".to_string())]).unwrap();
        assert_eq!(result.to_string(), "hELLO wORLD");
    }

    #[test]
    fn test_shuffle() {
        let result = shuffle(&[Value::String("abc".to_string())]).unwrap();
        let result_str = result.to_string();

        // Should be 3 characters (shuffled)
        assert_eq!(result_str.len(), 3);
        // Should contain all original characters
        assert!(result_str.contains('a'));
        assert!(result_str.contains('b'));
        assert!(result_str.contains('c'));
    }

    #[test]
    fn test_trim() {
        let result = trim(&[Value::String("  hello  ".to_string())]).unwrap();
        assert_eq!(result.to_string(), "hello");
    }

    #[test]
    fn test_upper() {
        let result = upper(&[Value::String("hello".to_string())]).unwrap();
        assert_eq!(result.to_string(), "HELLO");
    }

    #[test]
    fn test_lower() {
        let result = lower(&[Value::String("HELLO".to_string())]).unwrap();
        assert_eq!(result.to_string(), "hello");
    }

    #[test]
    fn test_contains() {
        let result = contains(&[
            Value::String("test".to_string()),
            Value::String("hello test world".to_string()),
        ])
        .unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_replace() {
        let result = replace(&[
            Value::String("old".to_string()),
            Value::String("new".to_string()),
            Value::String("old value old".to_string()),
        ])
        .unwrap();
        assert_eq!(result.to_string(), "new value new");
    }

    #[test]
    fn test_indent() {
        let multiline = r#"123
abc
xyz"#;
        let result = indent(&[Value::from(6), Value::String(multiline.to_string())]).unwrap();
        assert_eq!(result.to_string(), "      123\n      abc\n      xyz");
    }

    #[test]
    fn test_trim_all() {
        let result = trim_all(&[
            Value::String("-".to_string()),
            Value::String("--hello--".to_string()),
        ])
        .unwrap();
        assert_eq!(result.to_string(), "hello");
    }

    #[test]
    fn test_trim_prefix() {
        let result = trim_prefix(&[
            Value::String("hello".to_string()),
            Value::String("hello world".to_string()),
        ])
        .unwrap();
        assert_eq!(result.to_string(), " world");
    }

    #[test]
    fn test_trim_suffix() {
        let result = trim_suffix(&[
            Value::String("world".to_string()),
            Value::String("hello world".to_string()),
        ])
        .unwrap();
        assert_eq!(result.to_string(), "hello ");
    }

    #[test]
    fn test_repeat() {
        let result = repeat(&[Value::from(3), Value::String("ab".to_string())]).unwrap();
        assert_eq!(result.to_string(), "ababab");
    }

    #[test]
    fn test_substr() {
        let result = substr(&[
            Value::from(0),
            Value::from(5),
            Value::String("hello world".to_string()),
        ])
        .unwrap();
        assert_eq!(result.to_string(), "hello");
    }

    #[test]
    fn test_nospace() {
        let result = nospace(&[Value::String("hello world test".to_string())]).unwrap();
        assert_eq!(result.to_string(), "helloworldtest");
    }

    #[test]
    fn test_trunc() {
        let result = trunc(&[Value::from(5), Value::String("hello world".to_string())]).unwrap();
        assert_eq!(result.to_string(), "hello");
    }

    #[test]
    fn test_quote() {
        let result = quote(&[Value::String("hello".to_string())]).unwrap();
        assert_eq!(result.to_string(), "\"hello\"");
    }

    #[test]
    fn test_squote() {
        let result = squote(&[Value::String("hello".to_string())]).unwrap();
        assert_eq!(result.to_string(), "'hello'");
    }

    #[test]
    fn test_cat() {
        let result = cat(&[
            Value::String("hello".to_string()),
            Value::String("world".to_string()),
        ])
        .unwrap();
        assert_eq!(result.to_string(), "hello world");
    }

    #[test]
    fn test_has_prefix() {
        let result = has_prefix(&[
            Value::String("hello".to_string()),
            Value::String("hello world".to_string()),
        ])
        .unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_has_suffix() {
        let result = has_suffix(&[
            Value::String("world".to_string()),
            Value::String("hello world".to_string()),
        ])
        .unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_plural() {
        let result = plural(&[
            Value::String("item".to_string()),
            Value::String("items".to_string()),
            Value::from(1),
        ])
        .unwrap();
        assert_eq!(result.to_string(), "item");

        let result = plural(&[
            Value::String("item".to_string()),
            Value::String("items".to_string()),
            Value::from(2),
        ])
        .unwrap();
        assert_eq!(result.to_string(), "items");
    }

    #[test]
    fn test_initials() {
        let result = initials(&[Value::String("John Smith".to_string())]).unwrap();
        assert_eq!(result.to_string(), "JS");
    }

    #[test]
    fn test_rand_alpha() {
        let result = rand_alpha(&[Value::from(10)]).unwrap();
        assert_eq!(result.to_string().len(), 10);
        // Should only contain alphabetic characters
        assert!(result.to_string().chars().all(|c| c.is_alphabetic()));
    }

    #[test]
    fn test_rand_numeric() {
        let result = rand_numeric(&[Value::from(10)]).unwrap();
        assert_eq!(result.to_string().len(), 10);
        // Should only contain numeric characters
        assert!(result.to_string().chars().all(|c| c.is_numeric()));
    }

    #[test]
    fn test_rand_alpha_num() {
        let result = rand_alpha_num(&[Value::from(10)]).unwrap();
        assert_eq!(result.to_string().len(), 10);
        // Should only contain alphanumeric characters
        assert!(result.to_string().chars().all(|c| c.is_alphanumeric()));
    }
}
