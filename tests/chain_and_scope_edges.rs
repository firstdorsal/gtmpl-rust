//! Edges of field-chain resolution and variable scoping that work today but had
//! nothing holding them in place.
//!
//! Expectations were taken from Go 1.26.6 where Go has an equivalent; a function
//! stored *in* a map is a gtmpl extension with no Go counterpart, so those cases are
//! pinned against the documented behaviour instead.

use std::collections::HashMap;

use gtmpl_ng::{Context, ExecError, Func, FuncError, Template, Value};

/// A function-valued field in the *middle* of a chain, so the resolution has to invoke
/// it and then keep walking. Every other test puts the function last, which only
/// exercises the other branch of `invoke_chain`.
#[test]
fn a_function_in_the_middle_of_a_chain_is_invoked_and_walked_past() {
    let make_map: Func = |_args: &[Value]| -> Result<Value, FuncError> {
        let mut leaf = HashMap::new();
        leaf.insert("leaf".to_owned(), Value::from("found"));
        let mut deeper = HashMap::new();
        deeper.insert("deeper".to_owned(), Value::Map(leaf));
        Ok(Value::Map(deeper))
    };

    let mut root = HashMap::new();
    root.insert("fn".to_owned(), Value::from(make_map));
    let data = Value::Map(root);

    for source in [
        "{{ .fn.deeper.leaf }}",
        "{{ $.fn.deeper.leaf }}",
        "{{ $d := . }}{{ $d.fn.deeper.leaf }}",
    ] {
        let mut template = Template::default();
        template.parse(source).expect("parses");
        assert_eq!(
            template
                .render(&Context::from(data.clone()))
                .map_err(|e| e.to_string())
                .as_deref(),
            Ok("found"),
            "{}",
            source
        );
    }
}

/// `$` is bound in the outermost scope rather than aliasing the root, so reassigning
/// it must not cross a template boundary in either direction. Go agrees.
#[test]
fn reassigning_the_root_does_not_cross_a_template_boundary() {
    let source = concat!(
        r#"{{ define "d" }}[{{ $ }}{{ $ = "callee" }}{{ $ }}]{{ end }}"#,
        r#"{{ $ = "caller" }}{{ template "d" "arg" }}|{{ $ }}"#
    );
    let mut template = Template::default();
    template.parse(source).expect("parses");
    assert_eq!(
        template
            .render(&Context::from(Value::from("dot")))
            .map_err(|e| e.to_string())
            .as_deref(),
        Ok("[argcallee]|caller")
    );
}

/// Strips the `template: <name>:<line>:<col>:<len>:` prefix off a rendered error.
fn reason(error: &str) -> &str {
    error.splitn(6, ':').nth(5).unwrap_or(error)
}

/// The error variants whose payloads are boxed are reachable, and their messages
/// survived the boxing. Nothing asserted this, which is exactly where a mechanical
/// refactor can produce an empty or wrong message unnoticed.
#[test]
fn boxed_error_variants_keep_their_messages() {
    for (source, expected) in [
        ("{{ (nil).foo }}", "cannot evaluate command: nil"),
        (
            r#"{{ true "x" }}"#,
            "can't give argument to non-function true",
        ),
        ("{{ 1 2 }}", "can't give argument to non-function 1"),
    ] {
        let mut template = Template::default();
        template.parse(source).expect("parses");
        let error = template
            .render(&Context::from(Value::Nil))
            .expect_err("should fail");
        let message = match error {
            ExecError::Structured(ref structured) => structured.message.clone(),
            other => panic!("expected a structured error, got {:?}", other),
        };
        assert_eq!(message, expected, "{}", source);
        assert!(!reason(&message).is_empty());
    }
}
