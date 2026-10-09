//! A reported position has to name the line the construct is actually on, including
//! when an earlier token spanned newlines.
//!
//! Expectations were taken from Go 1.26.6, which reports the same lines.

use std::collections::HashMap;

use gtmpl_ng::{Context, ExecError, Template, Value};

fn failing_line(source: &str) -> usize {
    let mut root = HashMap::new();
    root.insert("s".to_owned(), Value::from(7));
    let mut template = Template::default();
    template.parse(source).expect("template parses");
    match template.render(&Context::from(Value::Map(root))) {
        Err(ExecError::Structured(structured)) => structured.context.line,
        Err(other) => panic!("expected a structured error, got {:?}", other),
        Ok(output) => panic!("expected an error, rendered {:?}", output),
    }
}

#[test]
fn plain_text_before_the_error_counts_lines() {
    assert_eq!(failing_line("{{ .s.nope }}"), 1);
    assert_eq!(failing_line("a\n{{ .s.nope }}"), 2);
    assert_eq!(failing_line("a\nb\n{{ .s.nope }}"), 3);
    assert_eq!(failing_line("a\n\n\n{{ .s.nope }}"), 4);
}

/// A token that itself spans newlines used to leave the counter on the line *after*
/// it, or to panic while computing the column.
#[test]
fn tokens_that_span_newlines_count_lines() {
    assert_eq!(failing_line("{{ `raw` }}\n{{ .s.nope }}"), 2);
    assert_eq!(failing_line("{{ `ra\nw` }}\n{{ .s.nope }}"), 3);
    assert_eq!(failing_line("{{ `ra\nw\nw` }}{{ .s.nope }}"), 3);
    assert_eq!(failing_line("{{/* c */}}\n{{ .s.nope }}"), 2);
    assert_eq!(failing_line("{{/* c\nc */}}{{ .s.nope }}"), 2);
    assert_eq!(failing_line("{{ `a\nb` }}{{ .s.nope }}"), 2);
}

/// A raw string may span lines, a double-quoted one may not -- as in Go. The second
/// form is what keeps an unterminated string from swallowing the rest of a file.
#[test]
fn only_raw_strings_may_span_lines() {
    let mut template = Template::default();
    assert!(
        template.parse("{{ `a\nb` }}").is_ok(),
        "a raw string may span lines"
    );

    let mut template = Template::default();
    assert!(
        template.parse("{{ \"a\nb\" }}").is_err(),
        "a quoted string may not span lines"
    );

    let mut template = Template::default();
    assert!(
        template.parse("{{ \"a\\nb\" }}").is_ok(),
        "an escaped newline is fine"
    );
}

#[test]
fn a_multi_line_action_reports_its_own_line() {
    assert_eq!(failing_line("a\n{{\n.s.nope\n}}"), 3);
    assert_eq!(failing_line("a\n{{ printf \"%v\"\n  .s.nope }}"), 3);
}

/// Trimmed whitespace still counts: the newlines a `-}}` removes happened.
#[test]
fn trimmed_newlines_still_count() {
    assert_eq!(failing_line("a\n{{- \"x\" -}}\n\n{{ .s.nope }}"), 4);
}

#[test]
fn a_multi_line_template_definition_counts_lines() {
    let source = "{{ define \"d\" }}\n\n{{ .s.nope }}\n{{ end }}{{ template \"d\" . }}";
    assert_eq!(failing_line(source), 3);
}
