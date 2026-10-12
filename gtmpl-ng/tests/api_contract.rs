//! What the 1.0 API promises beyond rendering: thread-safety of the core types, the
//! thread-local limits surviving a panic, and lazy `and`/`or` staying tied to the
//! builtins themselves rather than to their names or addresses alone.

use gtmpl_ng::{funcs, Context, FuncError, Limits, Template, Value};

/// The README tells callers to render on a thread they can abandon, which only works
/// while these stay `Send`. A compile-time pin, so the claim cannot rot silently.
#[test]
fn the_core_types_are_send() {
    fn assert_send<T: Send>() {}
    assert_send::<Template>();
    assert_send::<Context>();
    assert_send::<Value>();
    assert_send::<Limits>();
}

/// A panic inside a registered function unwinds through the render. The thread-local
/// limits are restored by a drop guard, so the thread is not left carrying the
/// paniced render's limits.
#[test]
fn a_panic_inside_a_render_restores_the_thread_locals() {
    fn boom(_args: &[Value]) -> Result<Value, FuncError> {
        panic!("deliberately");
    }
    let before = gtmpl_ng::limits::active();
    let caught = std::panic::catch_unwind(|| {
        let mut template = Template::with_limits(Limits::unlimited());
        template.add_func("boom", boom);
        template.parse("{{ boom }}").unwrap();
        let _ = template.render(&Context::empty());
    });
    assert!(caught.is_err(), "the panic must reach us");
    assert_eq!(
        gtmpl_ng::limits::active(),
        before,
        "the limits in force before the render are back"
    );
    assert_eq!(
        gtmpl_ng::limits::remaining(),
        None,
        "and no deadline is left armed"
    );
}

/// Laziness belongs to the two builtins, not to their names: a caller's own function
/// registered as "and" must see every argument evaluated, the way any function does.
#[test]
fn a_user_function_named_and_is_called_eagerly() {
    fn count_args(args: &[Value]) -> Result<Value, FuncError> {
        Ok(Value::from(args.len() as u64))
    }
    let mut template = Template::default();
    template.add_func("and", count_args);
    template.parse(r#"{{ and false (lt "a" 1) }}"#).unwrap();
    let error = template
        .render(&Context::empty())
        .expect_err("the second argument is evaluated, and it fails");
    assert!(error.to_string().contains("unable to compare"), "{error}");

    let mut template = Template::default();
    template.add_func("and", count_args);
    template.parse("{{ and 1 2 3 5 }}").unwrap();
    assert_eq!(
        template.render(&Context::empty()).unwrap(),
        "4",
        "the user's function counts its four arguments; the builtin would have answered 5"
    );
}

/// And the other direction: the builtin registered under a different name loses the
/// laziness, because the executor recognises the pair of name and function, not the
/// function alone.
#[test]
fn the_builtin_under_another_name_is_eager() {
    let mut template = Template::default();
    template.add_func("both", funcs::and);
    template.parse(r#"{{ both false (lt "a" 1) }}"#).unwrap();
    let error = template
        .render(&Context::empty())
        .expect_err("no laziness, so the failing argument is evaluated");
    assert!(error.to_string().contains("unable to compare"), "{error}");
}
