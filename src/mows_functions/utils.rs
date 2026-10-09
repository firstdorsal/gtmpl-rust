//! Mows-specific utility functions

use crate::{FuncError, Value};

pub fn join_domain(args: &[Value]) -> Result<Value, FuncError> {
    if args.is_empty() {
        return Err(FuncError::AtLeastXArgs(
            "joindomain requires at least 1 argument".to_string(),
            1,
        ));
    }

    let parts: Vec<String> = args
        .iter()
        .filter_map(|v| {
            // Skip nil/empty values
            if crate::utils::is_absent_or_nil(v) {
                return None;
            }
            let s = v.to_string();
            if s.is_empty() || s == "<no value>" {
                return None;
            }
            // Trim leading and trailing dots from each part
            let trimmed = s.trim_matches('.');
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .collect();

    if parts.is_empty() {
        return Ok(Value::String(String::new()));
    }

    Ok(Value::String(parts.join(".")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_join_domain_basic() {
        let result = join_domain(&[
            Value::String("abc".to_string()),
            Value::String("localhost".to_string()),
        ])
        .unwrap();
        assert_eq!(result.to_string(), "abc.localhost");
    }

    #[test]
    fn test_join_domain_with_trailing_dot() {
        let result = join_domain(&[
            Value::String("abc.".to_string()),
            Value::String("example.com".to_string()),
        ])
        .unwrap();
        assert_eq!(result.to_string(), "abc.example.com");
    }

    #[test]
    fn test_join_domain_with_both_dots() {
        let result = join_domain(&[
            Value::String("abc.".to_string()),
            Value::String("example.com.".to_string()),
        ])
        .unwrap();
        assert_eq!(result.to_string(), "abc.example.com");
    }

    #[test]
    fn test_join_domain_multiple_parts() {
        let result = join_domain(&[
            Value::String("abc".to_string()),
            Value::String("def".to_string()),
            Value::String("example.com.".to_string()),
        ])
        .unwrap();
        assert_eq!(result.to_string(), "abc.def.example.com");
    }

    #[test]
    fn test_join_domain_single() {
        let result = join_domain(&[Value::String("example.com".to_string())]).unwrap();
        assert_eq!(result.to_string(), "example.com");
    }

    #[test]
    fn test_join_domain_with_nil() {
        let result = join_domain(&[Value::Nil, Value::String("example.com".to_string())]).unwrap();
        assert_eq!(result.to_string(), "example.com");
    }

    #[test]
    fn test_join_domain_with_empty() {
        let result = join_domain(&[
            Value::String("".to_string()),
            Value::String("example.com".to_string()),
        ])
        .unwrap();
        assert_eq!(result.to_string(), "example.com");
    }
}
