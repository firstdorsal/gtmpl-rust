use std::collections::HashMap;

use gtmpl_derive_ng::Gtmpl;
use gtmpl_value_ng::{Func, FuncError, Value};

/// Unwraps the `Value::Object` a derived `From` produces.
fn fields(value: Value) -> HashMap<String, Value> {
    match value {
        Value::Object(map) => map,
        other => panic!("expected an object, got {:?}", other),
    }
}

#[test]
fn a_string_field_becomes_a_string() {
    #[derive(Gtmpl)]
    struct Foo {
        bar: String,
    }

    let fields = fields(Value::from(Foo {
        bar: "2000".to_owned(),
    }));
    assert_eq!(fields.get("bar"), Some(&Value::String("2000".to_owned())));
}

#[test]
fn a_struct_field_nests_another_object() {
    #[derive(Gtmpl)]
    struct Bar {
        i: i64,
    }
    #[derive(Gtmpl)]
    struct Foo {
        bar: Bar,
    }

    let fields = fields(Value::from(Foo { bar: Bar { i: 23 } }));
    let inner = match fields.get("bar") {
        Some(Value::Object(map)) => map,
        other => panic!("expected a nested object, got {:?}", other),
    };
    assert_eq!(inner.get("i"), Some(&Value::from(23i64)));
}

#[test]
fn a_function_field_stays_callable() {
    fn bar(a: &[Value]) -> Result<Value, FuncError> {
        Ok(a[0].clone())
    }

    #[derive(Gtmpl)]
    struct Foo {
        bar: Func,
    }

    let fields = fields(Value::from(Foo { bar }));
    let function = match fields.get("bar") {
        Some(Value::Function(function)) => function,
        other => panic!("expected a function, got {:?}", other),
    };
    let returned = (function.f)(&[Value::from(23i64)]).unwrap();
    assert_eq!(returned, Value::from(23i64));
}

#[test]
fn a_hashmap_field_becomes_a_map() {
    #[derive(Gtmpl)]
    struct Foo {
        field: HashMap<String, u64>,
    }

    let field: HashMap<String, u64> = [("2000".to_owned(), 23u64)].into_iter().collect();
    let fields = fields(Value::from(Foo {
        field: field.clone(),
    }));
    // A `HashMap` converts to `Value::Map`, which is distinct from the `Value::Object`
    // a derived struct produces.
    let expected: Value = field.into();
    assert!(matches!(expected, Value::Map(_)));
    assert_eq!(fields.get("field"), Some(&expected));
}

#[test]
fn a_vec_of_structs_becomes_an_array_of_objects() {
    // Deliberately not `Clone`: a field of `Vec<T>` used to demand it, so a struct
    // holding a list of another struct had to derive `Clone` through the whole tree
    // for a conversion that owns its input and never needed a copy.
    #[derive(Gtmpl)]
    struct Foo {
        name: String,
        value: i64,
    }
    #[derive(Gtmpl)]
    struct Bar {
        fields: Vec<Foo>,
    }

    let fields = fields(Value::from(Bar {
        fields: vec![Foo {
            name: "something".to_owned(),
            value: 23i64,
        }],
    }));
    let items = match fields.get("fields") {
        Some(Value::Array(items)) => items,
        other => panic!("expected an array, got {:?}", other),
    };
    assert_eq!(items.len(), 1);
    let first = match &items[0] {
        Value::Object(map) => map,
        other => panic!("expected an object, got {:?}", other),
    };
    assert_eq!(first.get("name"), Some(&Value::from("something")));
    assert_eq!(first.get("value"), Some(&Value::from(23i64)));
}

#[test]
fn an_option_field_carries_its_value() {
    #[derive(Gtmpl)]
    struct Foo {
        field: Option<String>,
    }

    let fields = fields(Value::from(Foo {
        field: Some("something".to_owned()),
    }));
    assert_eq!(fields.get("field"), Some(&Value::from("something")));
}

#[test]
fn an_empty_option_field_becomes_no_value() {
    #[derive(Gtmpl)]
    struct Foo {
        field: Option<String>,
    }

    let fields = fields(Value::from(Foo { field: None }));
    assert_eq!(fields.get("field"), Some(&Value::NoValue));
}

#[test]
fn a_unit_struct_becomes_an_empty_object() {
    #[derive(Gtmpl)]
    struct Foo;

    assert!(fields(Value::from(Foo)).is_empty());
}

#[test]
fn a_field_that_cannot_be_cloned_still_converts() {
    // `File` is neither `Clone` nor `Into<Value>`, so it stands in for any type a
    // consumer wraps: what matters is that holding one does not stop the struct around
    // it from being a context.
    struct NotClonable(#[allow(dead_code)] std::fs::File);

    impl From<NotClonable> for Value {
        fn from(_: NotClonable) -> Value {
            Value::from("opaque")
        }
    }

    #[derive(Gtmpl)]
    struct Foo {
        items: Vec<NotClonable>,
        maybe: Option<NotClonable>,
        named: HashMap<String, NotClonable>,
    }

    let file = || NotClonable(std::fs::File::open("/dev/null").expect("/dev/null opens"));
    let fields = fields(Value::from(Foo {
        items: vec![file()],
        maybe: Some(file()),
        named: [("key".to_owned(), file())].into_iter().collect(),
    }));

    assert_eq!(
        fields.get("items"),
        Some(&Value::Array(vec![Value::from("opaque")]))
    );
    assert_eq!(fields.get("maybe"), Some(&Value::from("opaque")));
    assert_eq!(
        fields.get("named"),
        Some(&Value::Map(
            [("key".to_owned(), Value::from("opaque"))]
                .into_iter()
                .collect()
        ))
    );
}
