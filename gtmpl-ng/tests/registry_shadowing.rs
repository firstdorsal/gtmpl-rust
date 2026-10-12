//! Which function answers a name when the Helm registry is loaded on top of the
//! builtins. Helm itself has no `index` and no `len` -- in a chart they are Go's
//! builtins -- so this engine's registry must leave them alone, and these tests hold
//! that: re-adding a shadow passes every unit test of the shadow itself, and only an
//! end-to-end render through `all_functions()` notices.

#![cfg(feature = "all-functions")]

use gtmpl_ng::all_functions::all_functions;
use gtmpl_ng::{Context, Template, Value};
use std::collections::HashMap;

fn render(source: &str, context: Value) -> Result<String, String> {
    let mut template = Template::default();
    template.add_funcs(&all_functions());
    template.parse(source).map_err(|e| e.to_string())?;
    template
        .render(&Context::from(context))
        .map_err(|e| e.to_string())
}

fn map(entries: &[(&str, &str)]) -> Value {
    Value::Map(
        entries
            .iter()
            .map(|(k, v)| (k.to_string(), Value::from(*v)))
            .collect::<HashMap<_, _>>(),
    )
}

/// The canonical Helm pattern: `index` reads a key out of a map. The old Helm-side
/// `index` was array-only and refused this.
#[test]
fn index_reads_a_map_under_all_functions() {
    let context = map(&[("app", "filez")]);
    assert_eq!(render(r#"{{ index . "app" }}"#, context).unwrap(), "filez");

    // And the rest of the builtin's reach stays: strings by byte, chained keys.
    assert_eq!(render(r#"{{ index "abc" 1 }}"#, Value::Nil).unwrap(), "98");
    let nested = Value::Map(HashMap::from([(
        "outer".to_string(),
        map(&[("inner", "found")]),
    )]));
    assert_eq!(
        render(r#"{{ index . "outer" "inner" }}"#, nested).unwrap(),
        "found"
    );
}

/// `len` counts a map, with or without the Helm registry, because the builtin serves
/// in both cases.
#[test]
fn len_counts_a_map_under_all_functions() {
    let context = map(&[("a", "1"), ("b", "2")]);
    assert_eq!(render("{{ len . }}", context.clone()).unwrap(), "2");

    let mut plain = Template::default();
    plain.parse("{{ len . }}").unwrap();
    assert_eq!(plain.render(&Context::from(context)).unwrap(), "2");
}

/// The registry itself carries neither name, so the builtins cannot be shadowed back
/// by accident.
#[test]
fn the_registry_does_not_shadow_gos_index_or_len() {
    let names: Vec<&str> = all_functions().iter().map(|(name, _)| *name).collect();
    assert!(!names.contains(&"index"), "helm has no index of its own");
    assert!(!names.contains(&"len"), "helm has no len of its own");
    // `slice` is the one deliberate shadow: Sprig defines it, so Helm's wins.
    assert!(names.contains(&"slice"));
}
