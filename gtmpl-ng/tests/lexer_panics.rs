//! Regression tests for lexer panics that previously crashed the parse instead of
//! returning a clean error.
//!
//! Deliberately not gated behind a feature: these exercise the core lexer, which is
//! always compiled. The older `panic_audit.rs`/`panic_regressions.rs` suites are
//! gated because their cases run through the helm function registry.

mod common;

use gtmpl_ng::Value;

fn parse_and_render(source: &str) -> Result<String, String> {
    common::render(source, Value::Nil)
}

/// Previously panicked with "byte index N is not a char boundary" because
/// `backup()` subtracted a single byte while `next()` had advanced by the
/// character's UTF-8 width. `peek()` is `next()` + `backup()`, so peeking at any
/// multi-byte character left the position inside its encoding and the next slice
/// panicked -- on the lexer's own thread, which surfaced to the caller as the
/// unrelated "unexpected receiving on a closed channel".
#[test]
fn multibyte_characters_in_an_action_do_not_panic() {
    for source in [
        "{{ü}}",
        "{{ é}}",
        "{{.café→}}",
        "{{\u{00A0}x}}",
        "{{\u{00A0}\u{00A0}x}}",
        "{{\u{3000}x}}",
        "{{- é -}}",
        "{{ .a.é }}",
    ] {
        let result = parse_and_render(source);
        // Go rejects these too; the point is that it is a reported error, not a crash
        // and not a bogus channel message.
        assert!(result.is_err(), "{:?} should be an error", source);
        let message = result.unwrap_err();
        assert!(
            !message.contains("closed channel"),
            "{:?} failed via a lexer thread crash: {}",
            source,
            message
        );
    }
}

/// Previously panicked the same way from the other side: `rtrim_len` measured the
/// trailing whitespace a `{{-` trims as `len - 1 - index_of_last_non_space`, which
/// assumes every character is one byte. With a multi-byte character immediately
/// before the marker the trim position landed inside it.
#[test]
fn multibyte_text_before_a_left_trim_marker_does_not_panic() {
    for (source, expected) in [
        ("\u{e9}{{- \"x\" }}", "\u{e9}x"),
        ("\u{e9}  {{- \"x\" }}", "\u{e9}x"),
        ("a\u{e9}  {{- \"x\" }}", "a\u{e9}x"),
        ("Gr\u{f6}\u{df}e{{- \"x\" }}", "Gr\u{f6}\u{df}ex"),
        ("\u{2192}\t{{- \"x\" }}", "\u{2192}x"),
        ("x  {{- \"y\" }}", "xy"),
    ] {
        let result = parse_and_render(source);
        assert_eq!(result.as_deref(), Ok(expected), "{:?}", source);
    }
}

/// Go trims only the four ASCII space characters around a marker, so other whitespace
/// is text and has to survive.
#[test]
fn a_trim_marker_leaves_non_ascii_whitespace_alone() {
    assert_eq!(
        parse_and_render("a\u{a0}{{- \"x\" }}").as_deref(),
        Ok("a\u{a0}x")
    );
}

/// Non-ASCII text that is legal stays legal -- the fix must not reject it.
#[test]
fn legal_non_ascii_templates_still_render() {
    for (source, expected) in [
        (r#"{{ "Übergröße" }}"#, "Übergröße"),
        ("Größe: ok", "Größe: ok"),
        (r#"{{ "🎉" }}"#, "🎉"),
        ("{{/* größer */}}ok", "ok"),
        (r#"{{ printf "%s ä" "Wert" }}"#, "Wert ä"),
    ] {
        assert_eq!(
            parse_and_render(source).as_deref(),
            Ok(expected),
            "{source:?}"
        );
    }
}
