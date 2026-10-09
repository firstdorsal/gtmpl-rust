//! A missing map key ends a field chain and renders as `<no value>`, however deep,
//! and also when the missed value is carried through a variable. A receiver of the
//! wrong type, or a nil receiver, still errors.
//!
//! Every expectation here was taken from Go 1.26.6 `text/template` rendering the
//! equivalent `map[string]any`.

use std::collections::HashMap;

use gtmpl_ng::{Context, Template, Value};

fn fixture() -> Value {
    let mut inner = HashMap::new();
    inner.insert("leaf".to_owned(), Value::from("L"));
    let mut nested = HashMap::new();
    nested.insert("inner".to_owned(), Value::Map(inner));

    let mut root = HashMap::new();
    root.insert("emptyMap".to_owned(), Value::Map(HashMap::new()));
    root.insert("nested".to_owned(), Value::Map(nested));
    root.insert("scalar".to_owned(), Value::from(7));
    root.insert("str".to_owned(), Value::from("s"));
    root.insert("list".to_owned(), Value::from(vec![1, 2]));
    root.insert("nilval".to_owned(), Value::Nil);
    Value::Map(root)
}

fn render(source: &str) -> Result<String, String> {
    let mut template = Template::default();
    template.parse(source).map_err(|e| e.to_string())?;
    template
        .render(&Context::from(fixture()))
        .map_err(|e| e.to_string())
}

#[test]
fn a_missing_key_ends_the_chain() {
    for source in [
        "{{.emptyMap.missing}}",
        "{{.emptyMap.missing.deeper}}",
        "{{.emptyMap.missing.a.b.c}}",
        "{{.missingTop}}",
        "{{.missingTop.deeper}}",
        "{{.nested.nope.leaf}}",
    ] {
        assert_eq!(render(source).as_deref(), Ok("<no value>"), "{}", source);
    }
}

/// The miss survives being stored in a variable and keeps ending chains.
#[test]
fn a_missing_key_stays_missing_through_a_variable() {
    for source in [
        "{{$x := .emptyMap.missing}}{{$x}}",
        "{{$x := .emptyMap.missing}}{{$x.field}}",
        "{{$x := .emptyMap.missing}}{{$x.a.b}}",
        "{{$n := .nested}}{{$n.nope.leaf}}",
    ] {
        assert_eq!(render(source).as_deref(), Ok("<no value>"), "{}", source);
    }
}

/// A present chain still resolves, so the short-circuit does not swallow real values.
#[test]
fn present_chains_still_resolve() {
    assert_eq!(render("{{.nested.inner.leaf}}").as_deref(), Ok("L"));
    assert_eq!(render("{{$.nested.inner.leaf}}").as_deref(), Ok("L"));
}

/// A missing value is falsy and empty, as in Go.
#[test]
fn a_missing_key_is_falsy_and_empty() {
    assert_eq!(
        render("{{if .emptyMap.missing.deeper}}y{{else}}n{{end}}").as_deref(),
        Ok("n")
    );
    assert_eq!(
        render("{{with .emptyMap.missing}}y{{else}}n{{end}}").as_deref(),
        Ok("n")
    );
    assert_eq!(
        render("{{range .emptyMap.missing}}y{{else}}n{{end}}").as_deref(),
        Ok("n")
    );
}

/// A receiver that is not a map or object is still an error -- the short-circuit must
/// not turn a genuine type mismatch into `<no value>`.
#[test]
fn wrong_receiver_types_still_error() {
    for source in [
        "{{.scalar.nope}}",
        "{{.str.nope}}",
        "{{.list.nope}}",
        "{{.nilval.nope}}",
    ] {
        assert!(render(source).is_err(), "{} should fail", source);
    }
}
