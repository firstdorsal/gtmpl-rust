//! Field chains rooted at a variable (`$`, `$var`) must behave exactly like chains
//! rooted at dot. They are walked through borrowed values to avoid cloning the whole
//! variable, so every outcome of that walk needs a test.

use std::collections::HashMap;

mod common;

use gtmpl_ng::{Context, Func, FuncError, Template, Value};

fn fixture() -> Value {
    let mut metadata = HashMap::new();
    metadata.insert("name".to_owned(), Value::from("example"));

    let mut deep = HashMap::new();
    deep.insert("leaf".to_owned(), Value::from("found"));
    let mut mid = HashMap::new();
    mid.insert("deep".to_owned(), Value::Map(deep));

    let greet: Func = |args: &[Value]| -> Result<Value, FuncError> {
        let who = match &args[0] {
            Value::Map(m) | Value::Object(m) => match m.get("who") {
                Some(Value::String(s)) => s.clone(),
                _ => "nobody".to_owned(),
            },
            _ => "nobody".to_owned(),
        };
        Ok(Value::from(format!("hello {who}")))
    };
    let mut callable = HashMap::new();
    callable.insert("who".to_owned(), Value::from("world"));
    callable.insert("greet".to_owned(), Value::from(greet));

    let mut object = HashMap::new();
    object.insert("field".to_owned(), Value::from("objfield"));

    let mut root = HashMap::new();
    root.insert("metadata".to_owned(), Value::from(metadata));
    root.insert("mid".to_owned(), Value::Map(mid));
    root.insert("callable".to_owned(), Value::Map(callable));
    root.insert("object".to_owned(), Value::Object(object));
    root.insert("empty".to_owned(), Value::Map(HashMap::new()));
    root.insert("scalar".to_owned(), Value::from(7));
    Value::Map(root)
}

fn render(source: &str) -> Result<String, String> {
    common::render(source, fixture())
}

/// A chain rooted at a variable must resolve the same value as the equivalent
/// chain rooted at dot.
#[test]
fn variable_chains_agree_with_dot_chains() {
    for (dot, dollar, var) in [
        (
            "{{.metadata.name}}",
            "{{$.metadata.name}}",
            "{{$d.metadata.name}}",
        ),
        (
            "{{.mid.deep.leaf}}",
            "{{$.mid.deep.leaf}}",
            "{{$d.mid.deep.leaf}}",
        ),
        (
            "{{.object.field}}",
            "{{$.object.field}}",
            "{{$d.object.field}}",
        ),
    ] {
        let expected = render(dot).expect("dot chain renders");
        assert_eq!(
            render(dollar).as_deref(),
            Ok(expected.as_str()),
            "{}",
            dollar
        );
        let scoped = format!("{{{{$d := .}}}}{var}");
        assert_eq!(
            render(&scoped).as_deref(),
            Ok(expected.as_str()),
            "{scoped}"
        );
    }
}

/// A function-valued field is invoked with its receiver, also when it is reached
/// through a variable chain rather than through dot.
#[test]
fn function_fields_are_invoked_through_variable_chains() {
    assert_eq!(render("{{.callable.greet}}").as_deref(), Ok("hello world"));
    assert_eq!(render("{{$.callable.greet}}").as_deref(), Ok("hello world"));
    assert_eq!(
        render("{{$d := .}}{{$d.callable.greet}}").as_deref(),
        Ok("hello world")
    );
}

/// A map miss renders as `<no value>` and ends the chain, through a variable exactly
/// as through dot. `tests/missing_map_key.rs` has the full comparison against Go.
#[test]
fn missing_map_key_yields_no_value() {
    for source in [
        "{{.empty.missing}}",
        "{{$.empty.missing}}",
        "{{$d := .}}{{$d.empty.missing}}",
        "{{.empty.missing.deeper}}",
        "{{$.empty.missing.deeper}}",
        "{{$d := .}}{{$d.empty.missing.deeper}}",
    ] {
        assert_eq!(render(source).as_deref(), Ok("<no value>"), "{}", source);
    }
}

/// Strips the `template: <name>:<line>:<col>:<len>:` prefix off a rendered error.
/// The source span of a `$var.a.b` chain is its `VariableNode` and the span of a
/// `.a.b` chain is its `FieldNode`, so the positions differ by design; only the
/// failure reason has to agree.
fn reason(error: &str) -> &str {
    error.splitn(6, ':').nth(5).unwrap_or(error)
}

/// The failure modes have to stay identical between dot- and variable-rooted chains.
#[test]
fn failures_agree_between_dot_and_variable_chains() {
    // a missing struct field, and a receiver that is not a map or object
    for (dot, dollar, var) in [
        (
            "{{.object.nope}}",
            "{{$.object.nope}}",
            "{{$d.object.nope}}",
        ),
        (
            "{{.scalar.nope}}",
            "{{$.scalar.nope}}",
            "{{$d.scalar.nope}}",
        ),
    ] {
        let expected = render(dot).expect_err("dot chain fails");
        let expected = reason(&expected);
        assert!(!expected.is_empty(), "no reason in error for {}", dot);

        let got = render(dollar).unwrap_err();
        assert_eq!(reason(&got), expected, "{}", dollar);

        let scoped = format!("{{{{$d := .}}}}{var}");
        let got = render(&scoped).unwrap_err();
        assert_eq!(reason(&got), expected, "{}", scoped);
    }
}

/// `$` stays bound to the root while dot moves.
#[test]
fn dollar_stays_root_inside_range_and_with() {
    let mut template = Template::default();
    template
        .parse("{{with .metadata}}{{$.scalar}}/{{.name}}{{end}}")
        .unwrap();
    assert_eq!(
        template.render(&Context::from(fixture())).unwrap(),
        "7/example"
    );
}

#[test]
fn chain_on_undefined_variable_fails() {
    let error = render("{{$nope.field}}").unwrap_err();
    assert!(error.contains("nope"), "unexpected error: {}", error);
}
