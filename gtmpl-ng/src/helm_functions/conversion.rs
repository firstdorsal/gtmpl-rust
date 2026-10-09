use crate::Value;
use std::collections::HashMap;

pub fn gtmpl_value_to_serde_yaml_value(value: &Value) -> anyhow::Result<serde_yaml_neo::Value> {
    Ok(match value {
        Value::Nil => serde_yaml_neo::Value::Null,
        Value::Bool(b) => serde_yaml_neo::Value::Bool(*b),
        Value::Number(n) => {
            let number = n
                .as_f64()
                .ok_or(anyhow::anyhow!("Cannot convert number to YAML"))?;

            serde_yaml_neo::Value::Number(number.into())
        }
        Value::String(s) => serde_yaml_neo::Value::String(s.clone()),
        Value::Array(a) => {
            let mut array = Vec::new();
            for v in a {
                array.push(gtmpl_value_to_serde_yaml_value(v)?);
            }
            serde_yaml_neo::Value::Sequence(array)
        }
        Value::Object(o) => {
            let mut object = serde_yaml_neo::Mapping::new();
            for (k, v) in o {
                let key = serde_yaml_neo::Value::String(k.clone());
                object.insert(key, gtmpl_value_to_serde_yaml_value(v)?);
            }
            serde_yaml_neo::Value::Mapping(object)
        }
        Value::Map(m) => {
            let mut object = serde_yaml_neo::Mapping::new();
            for (k, v) in m {
                let key = serde_yaml_neo::Value::String(k.clone());
                object.insert(key, gtmpl_value_to_serde_yaml_value(v)?);
            }
            serde_yaml_neo::Value::Mapping(object)
        }
        Value::Function(_) => serde_yaml_neo::Value::Null,
        Value::NoValue => serde_yaml_neo::Value::Null,
    })
}

pub fn gtmpl_value_to_serde_json_value(value: &Value) -> anyhow::Result<serde_json::Value> {
    Ok(match value {
        Value::Nil => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Number(n) => {
            // Try i64 first, then u64, then f64
            if let Some(i) = n.as_i64() {
                serde_json::Value::Number(i.into())
            } else if let Some(u) = n.as_u64() {
                serde_json::Value::Number(u.into())
            } else if let Some(f) = n.as_f64() {
                serde_json::Value::Number(
                    serde_json::Number::from_f64(f)
                        .ok_or(anyhow::anyhow!("Cannot convert float to JSON"))?,
                )
            } else {
                // Fallback: convert to string and parse
                let s = format!("{}", n);
                serde_json::from_str(&s)?
            }
        }
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::Array(a) => {
            let mut array = Vec::new();
            for v in a {
                array.push(gtmpl_value_to_serde_json_value(v)?);
            }
            serde_json::Value::Array(array)
        }
        Value::Object(o) => {
            let mut object = serde_json::Map::new();
            for (k, v) in o {
                object.insert(k.clone(), gtmpl_value_to_serde_json_value(v)?);
            }
            serde_json::Value::Object(object)
        }
        Value::Map(m) => {
            let mut object = serde_json::Map::new();
            for (k, v) in m {
                object.insert(k.clone(), gtmpl_value_to_serde_json_value(v)?);
            }
            serde_json::Value::Object(object)
        }
        Value::Function(_) => return Err(anyhow::anyhow!("Cannot convert function to JSON")),
        Value::NoValue => serde_json::Value::Null,
    })
}

pub fn serde_json_hashmap_to_gtmpl_hashmap(
    hashmap: &HashMap<String, serde_json::Value>,
) -> HashMap<String, Value> {
    let mut gtmpl_hashmap = HashMap::new();
    for (key, value) in hashmap {
        gtmpl_hashmap.insert(
            key.to_string(),
            Value::from(serde_json_value_to_gtmpl_value(value.clone())),
        );
    }
    gtmpl_hashmap
}

pub fn serde_json_value_to_gtmpl_value(value: serde_json::Value) -> Value {
    match value {
        serde_json::Value::Null => Value::Nil,
        serde_json::Value::Bool(b) => Value::Bool(b),
        // Preserve the narrowest numeric representation so downstream math
        // functions see an integer as an integer (not a float). Without
        // this, `fromJson "{\"n\":5}" | .n | add 1` would go through a
        // float round-trip and lose the integer identity.
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Number(i.into())
            } else if let Some(u) = n.as_u64() {
                Value::Number(u.into())
            } else if let Some(f) = n.as_f64() {
                Value::Number(f.into())
            } else {
                Value::Nil
            }
        }
        serde_json::Value::String(s) => Value::String(s),
        serde_json::Value::Array(a) => {
            Value::Array(a.into_iter().map(serde_json_value_to_gtmpl_value).collect())
        }
        serde_json::Value::Object(o) => Value::Object(
            o.into_iter()
                .map(|(k, v)| (k, serde_json_value_to_gtmpl_value(v)))
                .collect(),
        ),
    }
}

pub fn serde_yaml_hashmap_to_gtmpl_hashmap(
    hashmap: HashMap<String, serde_yaml_neo::Value>,
) -> HashMap<String, Value> {
    let mut gtmpl_hashmap = HashMap::new();
    for (key, value) in hashmap {
        gtmpl_hashmap.insert(key, Value::from(serde_yaml_value_to_gtmpl_value(value)));
    }
    gtmpl_hashmap
}

pub fn serde_yaml_value_to_gtmpl_value(value: serde_yaml_neo::Value) -> Value {
    match value {
        serde_yaml_neo::Value::Null => Value::Nil,
        serde_yaml_neo::Value::Bool(b) => Value::Bool(b),
        // Same rationale as serde_json: preserve integer identity so that
        // math functions behave consistently regardless of numeric source.
        serde_yaml_neo::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Number(i.into())
            } else if let Some(u) = n.as_u64() {
                Value::Number(u.into())
            } else if let Some(f) = n.as_f64() {
                Value::Number(f.into())
            } else {
                Value::Nil
            }
        }
        serde_yaml_neo::Value::String(s) => Value::String(s),
        serde_yaml_neo::Value::Sequence(a) => {
            Value::Array(a.into_iter().map(serde_yaml_value_to_gtmpl_value).collect())
        }
        serde_yaml_neo::Value::Mapping(serde_mapping) => Value::Object({
            let mut gtmpl_object: HashMap<String, Value> = HashMap::new();
            for (key, value) in serde_mapping {
                gtmpl_object.insert(
                    serde_yaml_value_to_gtmpl_value(key).to_string(),
                    serde_yaml_value_to_gtmpl_value(value),
                );
            }
            gtmpl_object
        }),
        // `serde_yaml_neo::Value::Tagged` wraps an inner value with a user tag
        // (e.g. `!MyTag hello`). Previously this path hit `unreachable!()`
        // and panicked. Recurse into the inner value, discarding the tag.
        serde_yaml_neo::Value::Tagged(tagged) => serde_yaml_value_to_gtmpl_value(tagged.value),
    }
}
