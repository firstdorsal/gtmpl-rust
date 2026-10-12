//! Deeply nested control structures must report an error rather than exhaust the
//! stack: the parser is recursive descent, and overflowing aborts the process
//! outright, which an untrusted template must not be able to do.

mod common;

use gtmpl_ng::{Template, Value};

fn nested(levels: usize) -> String {
    format!("{}x{}", "{{if 1}}".repeat(levels), "{{end}}".repeat(levels))
}

#[test]
fn nesting_within_the_limit_parses() {
    for levels in [1, 8, 32, 64] {
        let mut template = Template::default();
        assert!(
            template.parse(nested(levels)).is_ok(),
            "{} levels should parse",
            levels
        );
    }
    assert_eq!(common::render(&nested(32), Value::Nil).as_deref(), Ok("x"));
}

#[test]
fn nesting_past_the_limit_is_an_error() {
    for levels in [65, 100, 1_000, 10_000] {
        let mut template = Template::default();
        let error = template
            .parse(nested(levels))
            .expect_err("should be rejected");
        assert!(
            error.to_string().contains("nested more than"),
            "{} levels gave {:?}",
            levels,
            error.to_string()
        );
    }
}

/// The limit counts every construct that opens a body, not just `if`.
#[test]
fn the_limit_covers_every_nesting_construct() {
    for opener in ["{{if 1}}", "{{with 1}}", "{{range .}}"] {
        let closer = "{{end}}";
        let source = format!("{}x{}", opener.repeat(200), closer.repeat(200));
        let mut template = Template::default();
        assert!(
            template.parse(source).is_err(),
            "200 levels of {} should be rejected",
            opener
        );
    }
}
