//! Whitespace trimming must not depend on how much whitespace there is. The space
//! in front of a `-}}` belongs to the delimiter, so a run of spaces has to hand its
//! last one back instead of swallowing it.
//!
//! Expectations were taken from Go 1.26.6 `text/template`.

use std::collections::HashMap;

mod common;

use gtmpl_ng::Value;

fn fixture() -> Value {
    let mut root = HashMap::new();
    root.insert("a".to_owned(), Value::from("A"));
    root.insert("n".to_owned(), Value::from(7));
    root.insert("l".to_owned(), Value::from(vec![1, 2]));
    Value::Map(root)
}

fn render(source: &str) -> Result<String, String> {
    common::render(source, fixture())
}

/// Any number of spaces around the markers trims to the same result.
#[test]
fn trimming_is_independent_of_the_amount_of_whitespace() {
    for source in [
        "x{{- .a -}}y",
        "x{{-  .a  -}}y",
        "x{{-   .a   -}}y",
        "x{{- .a  -}}y",
        "x{{-  .a -}}y",
        "x{{  .a  -}}y",
        "x  {{-  .a  -}}  y",
        "x\n{{-  .a  -}}\ny",
    ] {
        assert_eq!(render(source).as_deref(), Ok("xAy"), "{}", source);
    }
}

/// A right marker trims on its own, with no left marker in the action.
#[test]
fn a_right_marker_works_without_a_left_one() {
    assert_eq!(render("x{{.a  -}}y").as_deref(), Ok("xAy"));
    assert_eq!(render("x{{.a -}}y").as_deref(), Ok("xAy"));
}

/// A left marker trims on its own.
#[test]
fn a_left_marker_works_without_a_right_one() {
    assert_eq!(render("x{{-  .a  }}y").as_deref(), Ok("xAy"));
    assert_eq!(render("x{{- .a}}y").as_deref(), Ok("xAy"));
}

/// The delimiter must not eat into the action: whatever follows the trimmed space
/// still has to parse as the action's content.
#[test]
fn the_action_content_survives_trimming() {
    assert_eq!(render("x{{-  printf \"%v\" .n  -}}y").as_deref(), Ok("x7y"));
    assert_eq!(render("x{{-  $v := .a  -}}{{ $v }}y").as_deref(), Ok("xAy"));
    assert_eq!(
        render("x{{-  if .a  -}}A{{-  end  -}}y").as_deref(),
        Ok("xAy")
    );
    assert_eq!(
        render("x{{-  range .l  -}}{{ . }}{{-  end  -}}y").as_deref(),
        Ok("x12y")
    );
}

/// Adjacent trimmed actions do not interfere.
#[test]
fn adjacent_actions_trim_independently() {
    assert_eq!(render("x{{ .a -}}{{- .a }}y").as_deref(), Ok("xAAy"));
    assert_eq!(render("x{{ .a  -}}{{-  .a }}y").as_deref(), Ok("xAAy"));
}
