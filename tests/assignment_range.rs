use std::collections::HashMap;

use gtmpl_ng::{Context, Template, Value};

fn render(source: &str, context: impl Into<Value>) -> String {
    let mut template = Template::default();
    template.parse(source).unwrap();
    template.render(&Context::from(context)).unwrap()
}

#[test]
fn assignment_accumulates_across_iterations() {
    assert_eq!(
        render(
            r#"{{ $acc := "seed" }}{{ range . }}{{ $acc = printf "%s-%d" $acc . }}{{ end }}{{ $acc }}"#,
            vec![1, 2, 3],
        ),
        "seed-1-2-3"
    );
}

#[test]
fn assignment_updates_the_nearest_binding() {
    assert_eq!(
        render(
            r#"{{ $x := "outer" }}{{ range . }}{{ $x := "inner" }}{{ $x = "changed" }}{{ $x }}{{ end }}{{ $x }}"#,
            vec![1, 2],
        ),
        "changedchangedouter"
    );
}

#[test]
fn conditionals_restore_the_outer_binding() {
    for block in ["if true", "with ."] {
        let source = format!(
            r#"{{{{ $x := "outer" }}}}{{{{ {block} }}}}{{{{ $x := "inner" }}}}{{{{ $x = "changed" }}}}{{{{ $x }}}}{{{{ end }}}}{{{{ $x }}}}"#
        );
        assert_eq!(render(&source, "context"), "changedouter");
    }
}

#[test]
fn range_assignment_updates_existing_index_and_value() {
    let source = r#"{{ $key := -1 }}{{ $value := "initial" }}{{ range $key, $value = . }}{{ end }}{{ $key }}:{{ $value }}"#;
    assert_eq!(render(source, vec![10, 20, 30]), "2:30");
    assert_eq!(render(source, Vec::<i32>::new()), "[]:[]");
    assert_eq!(
        render(
            "{{ $value := 9 }}{{ range $value = . }}{{ end }}{{ $value }}",
            vec![1, 2]
        ),
        "2"
    );
}

#[test]
fn range_declarations_do_not_replace_outer_variables() {
    assert_eq!(
        render(
            "{{ $x := 99 }}{{ range $x := . }}{{ $x }}{{ end }}|{{ $x }}",
            vec![1, 2]
        ),
        "12|99"
    );
}

#[test]
fn range_else_only_runs_for_empty_collections() {
    let source = "{{ range . }}{{ . }}{{ else }}empty{{ end }}";
    assert_eq!(render(source, vec![1, 2]), "12");
    assert_eq!(render(source, Vec::<i32>::new()), "empty");
    assert_eq!(render(source, Value::Nil), "empty");
    assert_eq!(render(source, Value::NoValue), "empty");
    assert_eq!(
        render(
            "{{ range $x := . }}bad{{ else }}{{ len $x }}{{ end }}",
            Vec::<i32>::new()
        ),
        "0"
    );
}

#[test]
fn map_ranges_use_sorted_keys() {
    let values = HashMap::from([
        ("z".to_owned(), Value::from(3)),
        ("a".to_owned(), Value::from(1)),
        ("b".to_owned(), Value::from(2)),
    ]);
    for context in [Value::Map(values.clone()), Value::Object(values)] {
        assert_eq!(
            render(
                "{{ range $key, $value := . }}{{ $key }}={{ $value }};{{ else }}empty{{ end }}",
                context
            ),
            "a=1;b=2;z=3;"
        );
    }
}

#[test]
fn assignment_to_an_undefined_variable_fails() {
    let mut template = Template::default();
    template.parse("{{ $missing = 1 }}").unwrap();
    let error = template.render(&Context::empty()).unwrap_err();
    assert!(error.to_string().contains("missing"));
}
