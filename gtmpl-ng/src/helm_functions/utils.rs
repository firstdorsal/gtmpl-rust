use crate::{FuncError, Value};
use std::collections::HashMap;

use super::conversion::{
    gtmpl_value_to_serde_json_value, gtmpl_value_to_serde_yaml_value,
    serde_json_value_to_gtmpl_value, serde_yaml_value_to_gtmpl_value,
};

pub fn dict(args: &[Value]) -> Result<Value, FuncError> {
    let mut dict = HashMap::new();
    let mut args = args.iter();
    while let Some(key) = args.next() {
        let value = args
            .next()
            .ok_or(FuncError::Generic("No value found".to_string()))?;
        dict.insert(key.to_string(), value.clone());
    }
    Ok(Value::from(dict))
}

pub fn list(args: &[Value]) -> Result<Value, FuncError> {
    Ok(Value::Array(args.to_vec()))
}

pub fn from_yaml(args: &[Value]) -> Result<Value, FuncError> {
    let value = &args.first().ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 1 argument.".to_string(),
        1,
    ))?;
    let value = value.to_string();
    let value: serde_yaml_neo::Value =
        serde_yaml_neo::from_str(&value).map_err(|e| FuncError::Generic(e.to_string()))?;
    let value = serde_yaml_value_to_gtmpl_value(value);
    Ok(Value::from(value))
}

pub fn to_yaml(args: &[Value]) -> Result<Value, FuncError> {
    let value = &args.first().ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 1 argument.".to_string(),
        1,
    ))?;

    let yaml = serde_yaml_neo::to_string(
        &gtmpl_value_to_serde_yaml_value(value)
            .map_err(|e| FuncError::Generic(format!("Error converting to yaml: {}", e)))?,
    )
    .map_err(|e| FuncError::Generic(e.to_string()))?;

    Ok(Value::String(yaml))
}

pub fn from_json(args: &[Value]) -> Result<Value, FuncError> {
    let value = &args.first().ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 1 argument.".to_string(),
        1,
    ))?;
    let value = value.to_string();
    let value: serde_json::Value =
        serde_json::from_str(&value).map_err(|e| FuncError::Generic(e.to_string()))?;
    let value = serde_json_value_to_gtmpl_value(value);
    Ok(Value::from(value))
}

pub fn to_json(args: &[Value]) -> Result<Value, FuncError> {
    let value = &args.first().ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 1 argument.".to_string(),
        1,
    ))?;

    let json = serde_json::to_string(
        &gtmpl_value_to_serde_json_value(value)
            .map_err(|e| FuncError::Generic(format!("Error converting to json: {}", e)))?,
    )
    .map_err(|e| FuncError::Generic(e.to_string()))?;

    Ok(Value::String(json))
}

pub fn to_pretty_json(args: &[Value]) -> Result<Value, FuncError> {
    let value = &args.first().ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 1 argument.".to_string(),
        1,
    ))?;

    let json = serde_json::to_string_pretty(
        &gtmpl_value_to_serde_json_value(value)
            .map_err(|e| FuncError::Generic(format!("Error converting to json: {}", e)))?,
    )
    .map_err(|e| FuncError::Generic(e.to_string()))?;

    Ok(Value::String(json))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_dict() {
        let result = dict(&[
            Value::String("key1".to_string()),
            Value::String("value1".to_string()),
            Value::String("key2".to_string()),
            Value::Number(42.into()),
        ])
        .unwrap();

        if let Value::Map(m) = result {
            assert_eq!(m.len(), 2);
            assert_eq!(m.get("key1").unwrap().to_string(), "value1");
            assert_eq!(m.get("key2").unwrap().to_string(), "42");
        } else {
            panic!("Expected map");
        }
    }

    #[test]
    fn test_list() {
        let result = list(&[
            Value::Number(1.into()),
            Value::Number(2.into()),
            Value::Number(3.into()),
        ])
        .unwrap();

        if let Value::Array(arr) = result {
            assert_eq!(arr.len(), 3);
            assert_eq!(arr[0].to_string(), "1");
        } else {
            panic!("Expected array");
        }
    }

    #[test]
    fn test_to_json_from_json() {
        let mut map = HashMap::new();
        map.insert("foo".to_string(), Value::String("bar".to_string()));
        let dict = Value::Map(map);

        let json_str = to_json(&[dict]).unwrap();
        let parsed = from_json(&[json_str]).unwrap();

        if let Value::Object(obj) = parsed {
            assert_eq!(obj.get("foo").unwrap().to_string(), "bar");
        } else {
            panic!("Expected object");
        }
    }

    #[test]
    fn test_to_yaml_from_yaml() {
        let mut map = HashMap::new();
        map.insert("foo".to_string(), Value::String("bar".to_string()));
        let dict = Value::Map(map);

        let yaml_str = to_yaml(&[dict]).unwrap();
        let parsed = from_yaml(&[yaml_str]).unwrap();

        if let Value::Object(obj) = parsed {
            assert_eq!(obj.get("foo").unwrap().to_string(), "bar");
        } else {
            panic!("Expected object");
        }
    }

    /// `toYaml` read numbers through `as_f64`, which answers only for the float
    /// variant, so it failed outright on any integer -- `{{ toYaml .ports }}` over a
    /// list of port numbers, which is most of what a chart passes it. Expectations
    /// from `gopkg.in/yaml.v3`, which is what Helm marshals with.
    #[test]
    fn to_yaml_writes_integers() {
        for (value, expected) in [
            (Value::from(vec![1u64, 2, 3]), "- 1\n- 2\n- 3\n"),
            (Value::from(7u64), "7\n"),
            (Value::from(-5i64), "-5\n"),
            (Value::from(u64::MAX), "18446744073709551615\n"),
            (Value::from(1.5f64), "1.5\n"),
            (Value::from("s"), "s\n"),
        ] {
            assert_eq!(
                to_yaml(std::slice::from_ref(&value)).unwrap().to_string(),
                expected,
                "toYaml of {:?}",
                value
            );
        }
    }

    /// `serde_yaml` read a leading-zero integer as a string; `serde_yaml_neo` reads it
    /// as octal, which is what Go's YAML readers and Helm do -- so a file mode comes
    /// through as the number it means. Expectations from `gopkg.in/yaml.v3`.
    #[test]
    fn from_yaml_reads_a_leading_zero_integer_as_octal() {
        for (document, expected) in [
            ("mode: 0644", "420"),
            ("mode: 00123", "83"),
            ("mode: 0o644", "420"),
            ("mode: 0x1f", "31"),
            ("mode: 123", "123"),
        ] {
            let parsed = from_yaml(&[Value::from(document)]).unwrap();
            let mode = match parsed {
                Value::Object(ref fields) => fields.get("mode").cloned().unwrap(),
                ref other => panic!("expected an object, got {:?}", other),
            };
            assert_eq!(mode.to_string(), expected, "fromYaml of {:?}", document);
        }
    }

    #[test]
    fn to_json_writes_integers_as_integers() {
        for (value, expected) in [
            (Value::from(vec![1u64, 2, 3]), "[1,2,3]"),
            (Value::from(7u64), "7"),
            (Value::from(-5i64), "-5"),
            (Value::from(1.5f64), "1.5"),
        ] {
            assert_eq!(
                to_json(std::slice::from_ref(&value)).unwrap().to_string(),
                expected,
                "toJson of {:?}",
                value
            );
        }
    }

    /// JSON has no infinity, so there is nothing to write rather than something wrong.
    #[test]
    fn to_json_refuses_an_infinity() {
        assert!(to_json(&[Value::from(f64::INFINITY)]).is_err());
    }
}
