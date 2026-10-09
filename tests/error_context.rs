//! `ErrorContext` has to point at the construct that failed, so a caller can
//! underline it. The contract tested here is that slicing the template source at
//! `col` for `len` characters yields exactly the offending expression.

use std::collections::HashMap;

use gtmpl_ng::{Context, ExecError, Template, Value};

fn fixture() -> Value {
    let mut root = HashMap::new();
    root.insert("aaaa".to_owned(), Value::from(7));
    root.insert("a".to_owned(), Value::from(7));
    root.insert("scalar".to_owned(), Value::from(7));
    Value::Map(root)
}

/// Renders `source`, expects it to fail, and returns the source text the reported
/// context points at.
fn offending_text(source: &str) -> String {
    let mut template = Template::default();
    template.parse(source).expect("template parses");
    let error = template
        .render(&Context::from(fixture()))
        .expect_err("template fails");

    let context = match error {
        ExecError::Structured(ref structured) => structured.context.clone(),
        other => panic!("expected a structured error, got {:?}", other),
    };

    let line = source
        .lines()
        .nth(context.line - 1)
        .unwrap_or_else(|| panic!("line {} not in source", context.line));
    let start = context.col - 1;
    let end = (start + context.len).min(line.len());
    assert!(
        start <= line.len(),
        "col {} past end of line for {source:?}",
        context.col
    );
    line[start..end].to_owned()
}

#[test]
fn the_context_spans_the_whole_field_chain() {
    assert_eq!(offending_text("{{.aaaa.b}}"), ".aaaa.b");
    assert_eq!(offending_text("{{.a.bbbb}}"), ".a.bbbb");
    assert_eq!(offending_text("{{.scalar.nope}}"), ".scalar.nope");
    assert_eq!(offending_text("{{.scalar.a.b.c}}"), ".scalar.a.b.c");
}

/// A variable-rooted chain spans from the `$` through the last field.
#[test]
fn the_context_spans_a_variable_rooted_chain() {
    assert_eq!(offending_text("{{$.scalar.nope}}"), "$.scalar.nope");
    assert_eq!(
        offending_text("{{$d := .}}{{$d.scalar.nope}}"),
        "$d.scalar.nope"
    );
}

/// Surrounding whitespace and trim markers stay out of the span.
#[test]
fn the_context_excludes_delimiters_and_whitespace() {
    assert_eq!(offending_text("{{ .scalar.nope }}"), ".scalar.nope");
    assert_eq!(offending_text("{{-  .scalar.nope  -}}"), ".scalar.nope");
}

/// An argument inside a call is reported, not the whole call.
#[test]
fn the_context_points_at_the_failing_argument() {
    assert_eq!(
        offending_text("{{printf \"%v\" .scalar.nope}}"),
        ".scalar.nope"
    );
}

/// `if`, `with` and `range` take their span from their pipeline.
#[test]
fn the_context_spans_control_pipelines() {
    assert_eq!(
        offending_text("{{if .scalar.nope}}x{{end}}"),
        ".scalar.nope"
    );
    assert_eq!(
        offending_text("{{with .scalar.nope}}x{{end}}"),
        ".scalar.nope"
    );
    assert_eq!(
        offending_text("{{range .scalar.nope}}x{{end}}"),
        ".scalar.nope"
    );
}

/// The line number and column survive leading lines and text.
#[test]
fn the_context_reports_the_right_line_and_column() {
    assert_eq!(
        offending_text("first line\nsecond\nprefix {{.scalar.nope}} suffix"),
        ".scalar.nope"
    );
}
