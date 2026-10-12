//! A template is not always written by the program that renders it, and these are the
//! cases where that matters.
//!
//! Each template below ended the process before the limits in `src/limits.rs` existed:
//! two by exhausting something, two by never finishing. None of them could be caught,
//! because a stack overflow and a failed allocation both abort rather than return, so
//! the `Result` the API hands back was never reached.
//!
//! Deliberately **not** gated on a feature. The rest of the resource suite needs the
//! function registries and says so in `feature_gates.rs`, but nothing here does: a
//! plain `cargo test` has to run the file that proves a hostile template cannot take
//! the process down.

use gtmpl_ng::{Context, ExecError, FuncError, Function, Limits, Template, Value};
use std::collections::HashMap;
use std::time::Duration;

/// Renders with builtins only, under the given limits.
fn render_with(source: &str, limits: Limits) -> Result<String, String> {
    let mut template = Template::with_limits(limits);
    template.parse(source).map_err(|e| format!("parse: {e}"))?;
    template
        .render(&Context::from(Value::Nil))
        .map_err(|e| e.to_string())
}

fn render(source: &str) -> Result<String, String> {
    render_with(source, Limits::default())
}

/// A template that calls itself. Fifty-six characters, and the stack was gone:
/// `fatal runtime error: stack overflow, aborting`.
#[test]
fn a_recursive_template_is_refused_rather_than_overflowing_the_stack() {
    let error = render(r#"{{define "r"}}{{template "r" .}}{{end}}{{template "r" .}}"#)
        .expect_err("unbounded recursion");
    assert!(
        error.contains("exceeded the maximum template nesting depth of 100"),
        "{error}"
    );
}

/// The limit has to hold on the smallest stack a caller is likely to render on:
/// `std::thread`'s default is 2 MiB, and so is a Tokio worker's, against 8 MiB for the
/// main thread. If the limit is ever raised past what the stack can take, this test
/// does not fail politely -- it aborts the whole test binary, which is the loudest
/// signal available and a great deal better than a consumer finding out.
///
/// The five shapes differ twenty-fold in how much nesting they put between two
/// template calls, which is why the limit counts every nested body rather than only
/// templates: counting templates alone would have let the widest shape overflow at a
/// fortieth of the allowance.
#[test]
fn the_nesting_limit_holds_on_a_two_mebibyte_stack() {
    let deeply_nested = |action: &str, depth: usize| {
        format!(
            r#"{{{{define "r"}}}}{}{{{{template "r" .}}}}{}{{{{end}}}}{{{{template "r" .}}}}"#,
            action.repeat(depth),
            "{{end}}".repeat(depth)
        )
    };
    let shapes = [
        r#"{{define "r"}}{{template "r" .}}{{end}}{{template "r" .}}"#.to_owned(),
        deeply_nested("{{if 1}}", 60),
        deeply_nested("{{with 1}}", 55),
        deeply_nested("{{range 1}}", 55),
        format!(
            r#"{{{{define "r"}}}}{{{{if {}1{}}}}}{{{{template "r" .}}}}{{{{end}}}}{{{{end}}}}{{{{template "r" .}}}}"#,
            "(".repeat(30),
            ")".repeat(30)
        ),
    ];

    let worker = std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(move || {
            shapes
                .iter()
                .map(|source| render(source).expect_err("unbounded recursion"))
                .collect::<Vec<_>>()
        })
        .expect("spawning a worker");

    for error in worker.join().expect("the worker must not overflow") {
        assert!(
            error.contains("exceeded the maximum template nesting depth of 100"),
            "{error}"
        );
    }
}

/// `{{range 1000000000}}{{end}}` ran until it was killed. The count is the template's
/// own, so this is the limit that belongs on it.
#[test]
fn a_huge_range_is_refused() {
    let error = render("{{range 1000000000}}{{end}}").expect_err("a billion iterations");
    assert!(
        error.contains("more than the 10000 iterations allowed"),
        "{error}"
    );
}

/// Three loops that each obey the per-range limit are 10¹² together, so the per-range
/// limit cannot be the only one.
#[test]
fn nesting_cannot_multiply_its_way_past_the_total() {
    let error = render("{{range 10000}}{{range 10000}}{{range 10000}}{{end}}{{end}}{{end}}")
        .expect_err("10^12 iterations");
    assert!(
        error.contains("more than the 1000000 iterations allowed"),
        "{error}"
    );
}

/// The allocator failed and the process aborted. Counted as it is written, so the
/// render stops at the limit rather than allocating past it and failing afterwards.
#[test]
fn a_render_cannot_write_past_its_output_budget() {
    let limits = Limits::default().with_output_bytes(1024);
    let error = render_with("{{range 2000}}xx{{end}}", limits).expect_err("4000 bytes of output");
    assert!(
        error.contains("more than the 1024 bytes allowed"),
        "{error}"
    );

    // Right up to the limit still goes through, so the bound is the number it says.
    assert_eq!(
        render_with("{{range 512}}xx{{end}}", limits).unwrap().len(),
        1024
    );
}

/// Every limit is separate, so raising one does not quietly widen another.
#[test]
fn each_limit_can_be_raised_on_its_own() {
    let roomier = Limits::default().with_iterations(200_000);
    assert_eq!(
        render_with("{{range 100000}}x{{end}}", roomier)
            .unwrap()
            .len(),
        100_000
    );

    // The total is untouched by that, and still catches the nested shape.
    let error = render_with("{{range 10000}}{{range 10000}}{{end}}{{end}}", roomier)
        .expect_err("10^8 iterations");
    assert!(error.contains("1000000 iterations"), "{error}");
}

/// Opting out has to actually opt out, or a program that renders only its own
/// templates would have no way to use the engine as it used to behave.
#[test]
fn unlimited_lifts_the_iteration_limits() {
    assert_eq!(
        render_with("{{range 50000}}x{{end}}", Limits::unlimited())
            .unwrap()
            .len(),
        50_000
    );
}

/// The stack limit is not part of `Limits` and cannot be lifted: there is no value
/// above it that is safe, so the knob would only be a way to abort the process.
#[test]
fn unlimited_does_not_lift_the_nesting_limit() {
    let error = render_with(
        r#"{{define "r"}}{{template "r" .}}{{end}}{{template "r" .}}"#,
        Limits::unlimited(),
    )
    .expect_err("recursion is bounded whatever the limits say");
    assert!(
        error.contains("exceeded the maximum template nesting depth of 100"),
        "{error}"
    );
}

/// Ordinary templates must not notice any of this. Depth here is what a real chart
/// reaches; the deployment corpus this was measured on stays in single digits.
#[test]
fn ordinary_templates_are_untouched() {
    assert_eq!(render("{{range 3}}{{.}}{{end}}").unwrap(), "012");
    assert_eq!(render("{{range 10000}}x{{end}}").unwrap().len(), 10_000);
    assert_eq!(
        render(r#"{{define "row"}}[{{.}}]{{end}}{{range 3}}{{template "row" .}}{{end}}"#).unwrap(),
        "[0][1][2]"
    );
    assert_eq!(
        render("{{if 1}}{{with 2}}{{range 2}}{{.}}{{end}}{{end}}{{end}}").unwrap(),
        "01"
    );
}

/// Ranging over data the host supplied is not capped per range -- that length is the
/// program's own and a template cannot inflate it -- but it still draws on the total.
#[test]
fn a_long_array_from_the_context_is_not_capped_per_range() {
    let long: Vec<Value> = (0..50_000).map(Value::from).collect();
    let mut template = Template::default();
    template.parse("{{range .}}x{{end}}").unwrap();
    let output = template
        .render(&Context::from(Value::Array(long)))
        .expect("the host's own data is not the template's doing");
    assert_eq!(output.len(), 50_000);
}

/// The limits report themselves as their own error variants.
///
/// Only where nothing wraps them: an error raised inside a `{{range}}` is given its
/// position on the way out, and `with_context` flattens whatever it wraps into a
/// message. That is how every exec error in this engine already behaves, so the
/// assertion here is split -- the typed variant from a top-level write, and the
/// message plus position from inside a range.
#[test]
fn the_limits_report_themselves_as_errors() {
    let mut template = Template::with_limits(Limits::default().with_output_bytes(2));
    template.parse("xxxx").unwrap();
    assert!(
        matches!(
            template.render(&Context::empty()).unwrap_err(),
            ExecError::OutputTooLarge { limit: 2, .. }
        ),
        "a top-level write is not wrapped, so the variant survives"
    );

    let mut template = Template::default();
    template.parse("{{range 1000000000}}{{end}}").unwrap();
    let error = template.render(&Context::empty()).unwrap_err();
    let structured = error
        .as_structured()
        .expect("errors from inside a range carry their position");
    assert_eq!(
        structured.message,
        "range over 1000000000 is more than the 10000 iterations allowed"
    );
    assert_eq!(structured.context.line, 1);
}

/// A value can grow by feeding a function its own output: forty doublings of eight
/// characters are 2^43 bytes, with every counter content, and the allocator's abort
/// cannot be caught. The produced-value check stops the doubling at the
/// `function_output` budget instead -- and fast, because refusal happens at the
/// first result past the budget, not after forty of them.
#[test]
fn a_value_cannot_double_its_way_past_the_limits() {
    let mut template = Template::default();
    template
        .parse(r#"{{ $s := "xxxxxxxx" }}{{ range 40 }}{{ $s = print $s $s }}{{ end }}{{ len $s }}"#)
        .unwrap();
    let error = template.render(&Context::empty()).unwrap_err();
    let structured = error.as_structured().expect("raised inside the range");
    assert_eq!(
        structured.message,
        "a function produced a value larger than the 1000000 allowed"
    );
}

/// Depth is capped too, and `Limits::unlimited()` does not lift it: a deep value is
/// stack usage in every recursive walk (`Clone`, `Drop`, `Display`), the same way
/// nested bodies are, and no budget makes that safe.
#[test]
fn a_value_cannot_nest_its_way_off_the_stack() {
    fn nest(args: &[Value]) -> Result<Value, FuncError> {
        Ok(Value::Array(vec![args[0].clone()]))
    }
    let mut template = Template::with_limits(Limits::unlimited());
    template.add_func("nest", nest);
    template
        .parse("{{ $x := 1 }}{{ range 10000 }}{{ $x = nest $x }}{{ end }}ok")
        .unwrap();
    let error = template.render(&Context::empty()).unwrap_err();
    let structured = error.as_structured().expect("raised inside the range");
    assert_eq!(
        structured.message,
        "a function produced a value nested deeper than the 128 levels allowed"
    );
}

/// `{{template}}` fan-out: two calls per level is 2^40 calls inside a depth of 40,
/// with no loop, no output and no function budget touched. Calls draw on
/// `total_iterations`, so the total stops it.
#[test]
fn template_calls_cannot_fan_out_past_the_iteration_total() {
    let mut template = Template::with_limits(Limits::default().with_total_iterations(1_000));
    template
        .parse(
            r#"{{define "f"}}{{if .}}{{template "f" (slice . 1)}}{{template "f" (slice . 1)}}{{end}}{{end}}{{template "f" "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"}}"#,
        )
        .unwrap();
    let started = std::time::Instant::now();
    let error = template.render(&Context::empty()).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("more than the 1000 iterations allowed"),
        "{error}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "refusal must not take meaningfully longer than the budget itself"
    );
}

// ---------------------------------------------------------------------------
// The deadline, which is the one limit that is opt-in.
// ---------------------------------------------------------------------------

/// The counters bound memory, not time. The template that shows it is the nested one,
/// which spends the whole million-iteration budget on bodies that each build a megabyte
/// and throw it away: 13.8 seconds, with every count in it allowed. It is not rendered
/// here -- a test is not the place to burn fourteen seconds -- so this uses the cheap
/// shape and an expired deadline to exercise the same check.
#[test]
fn a_deadline_stops_a_render_that_stays_inside_every_counter() {
    // The template is inside every counter, so without a deadline it is allowed.
    let mut allowed = Template::default();
    allowed.parse("{{ range 10000 }}x{{ end }}").unwrap();
    assert_eq!(allowed.render(&Context::empty()).unwrap().len(), 10_000);

    // With a deadline already past, the same template stops at the first iteration.
    let expired = Limits::default().with_deadline(Duration::ZERO);
    let mut template = Template::with_limits(expired);
    template.parse("{{ range 10000 }}x{{ end }}").unwrap();
    let error = template.render(&Context::empty()).unwrap_err();
    assert!(
        error.to_string().contains("did not finish within"),
        "{error}"
    );
}

/// A loop long enough that no machine finishes it inside the deadline, against one
/// short enough that every machine finishes it inside a generous one. The margins are
/// four orders of magnitude so the test cannot depend on how busy the box is.
#[test]
fn the_deadline_is_the_duration_it_was_given() {
    let tight = Limits::unlimited().with_deadline(Duration::from_millis(20));
    let mut template = Template::with_limits(tight);
    template.parse("{{ range 1000000000 }}{{ end }}").unwrap();
    let started = std::time::Instant::now();
    let error = template.render(&Context::empty()).unwrap_err();
    assert!(
        error.to_string().contains("did not finish within 20ms"),
        "{error}"
    );
    // It stopped at the deadline rather than running the billion iterations out.
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );

    let roomy = Limits::default().with_deadline(Duration::from_secs(60));
    let mut template = Template::with_limits(roomy);
    template.parse("{{ range 100 }}x{{ end }}").unwrap();
    assert_eq!(template.render(&Context::empty()).unwrap().len(), 100);
}

/// Checked before a function call as well as before an iteration, so a template with
/// no loop in it is covered too. A function runs to its end whatever the deadline says
/// -- there is no cancellation point inside an ordinary call -- so what the check buys
/// is that the next one does not start.
#[test]
fn the_deadline_is_checked_before_a_function_call() {
    let expired = Limits::default().with_deadline(Duration::ZERO);
    let mut template = Template::with_limits(expired);
    template.parse(r#"{{ printf "%s" "x" }}"#).unwrap();
    let error = template.render(&Context::empty()).unwrap_err();
    assert!(
        error.to_string().contains("did not finish within"),
        "{error}"
    );
}

/// And before a `Func` field called as a method, which is the other place a function
/// runs.
#[test]
fn the_deadline_is_checked_before_a_method_call() {
    fn greet(_args: &[Value]) -> Result<Value, FuncError> {
        Ok(Value::from("hello"))
    }
    let mut context: HashMap<String, Value> = HashMap::new();
    context.insert("greet".to_owned(), Value::Function(Function { f: greet }));

    let mut template = Template::with_limits(Limits::default().with_deadline(Duration::ZERO));
    template.parse("{{ .greet }}").unwrap();
    let error = template
        .render(&Context::from(Value::from(context.clone())))
        .unwrap_err();
    assert!(
        error.to_string().contains("did not finish within"),
        "{error}"
    );

    // Without a deadline the same call is ordinary.
    let mut template = Template::default();
    template.parse("{{ .greet }}").unwrap();
    assert_eq!(
        template
            .render(&Context::from(Value::from(context)))
            .unwrap(),
        "hello"
    );
}

/// Between units of work, which means a template that is only text has nothing to
/// check and renders however long it has been. Worth pinning, because it is the
/// difference between what this bounds and what a timeout around the call would.
#[test]
fn a_template_with_no_loop_or_call_is_not_interrupted() {
    let mut template = Template::with_limits(Limits::default().with_deadline(Duration::ZERO));
    template.parse("just text").unwrap();
    assert_eq!(template.render(&Context::empty()).unwrap(), "just text");
}

/// Off by default, so no existing render starts failing on a slow machine.
#[test]
fn there_is_no_deadline_unless_one_is_asked_for() {
    assert_eq!(Limits::default().deadline, None);
    assert_eq!(Limits::unlimited().deadline, None);
    assert_eq!(gtmpl_ng::limits::remaining(), None);

    let mut template = Template::default();
    template.parse("{{ range 10000 }}x{{ end }}").unwrap();
    assert_eq!(template.render(&Context::empty()).unwrap().len(), 10_000);
}

/// A custom function with work it can do in pieces can see how much time is left, the
/// way one that allocates can see the size it is allowed.
#[test]
fn a_function_can_read_the_time_it_has_left() {
    fn how_long(_args: &[Value]) -> Result<Value, FuncError> {
        Ok(match gtmpl_ng::limits::remaining() {
            Some(left) if left > Duration::from_secs(30) => Value::from("plenty"),
            Some(_) => Value::from("little"),
            None => Value::from("none"),
        })
    }

    let mut template =
        Template::with_limits(Limits::default().with_deadline(Duration::from_secs(60)));
    template.add_func("howLong", how_long);
    template.parse("{{ howLong }}").unwrap();
    assert_eq!(template.render(&Context::empty()).unwrap(), "plenty");

    // And `None` outside a deadline, which is how a function tells the two apart.
    let mut template = Template::default();
    template.add_func("howLong", how_long);
    template.parse("{{ howLong }}").unwrap();
    assert_eq!(template.render(&Context::empty()).unwrap(), "none");
}

/// The deadline is restored like the rest, so a render started inside a template
/// function does not inherit or clobber the outer one's clock.
#[test]
fn a_nested_render_gets_its_own_deadline() {
    fn inner(_args: &[Value]) -> Result<Value, FuncError> {
        // Its own render, with its own limits, while the outer one is still running.
        let mut template = Template::with_limits(Limits::default().with_deadline(Duration::ZERO));
        template.parse("{{ range 10 }}x{{ end }}").unwrap();
        let refused = template.render(&Context::empty()).is_err();
        Ok(Value::from(refused))
    }

    let mut template =
        Template::with_limits(Limits::default().with_deadline(Duration::from_secs(60)));
    template.add_func("inner", inner);
    template
        .parse("{{ inner }}{{ range 3 }}.{{ end }}")
        .unwrap();
    // The inner render hit its own expired deadline; the outer one carried on under
    // its sixty seconds afterwards, which is what the range after it proves.
    assert_eq!(template.render(&Context::empty()).unwrap(), "true...");
}

/// How the deadline reports itself.
///
/// Always with its position, unlike the output limit: the two places it is checked --
/// a function call and a loop iteration -- are both inside nodes that add context on
/// the way out, and `with_context` flattens what it wraps into a message. That is how
/// every exec error in this engine already behaves.
#[test]
fn the_deadline_reports_itself_with_its_position() {
    let mut template =
        Template::with_limits(Limits::unlimited().with_deadline(Duration::from_millis(20)));
    template.parse("{{ range 1000000000 }}{{ end }}").unwrap();
    let error = template.render(&Context::empty()).unwrap_err();
    let structured = error
        .as_structured()
        .expect("raised inside a range, which carries its position");
    assert_eq!(structured.message, "the render did not finish within 20ms");
    assert_eq!(structured.context.line, 1);
}

/// The clock starts when `render` is called, not when the `Template` is built, so a
/// template held in a cache or a `static` does not arrive with its deadline already
/// spent -- and each render of it gets the whole span.
#[test]
fn the_deadline_is_measured_from_the_start_of_each_render() {
    let mut template =
        Template::with_limits(Limits::default().with_deadline(Duration::from_millis(250)));
    template.parse("{{ range 3 }}x{{ end }}").unwrap();

    // Built well before the first render, and reused after a wait longer than the
    // deadline: both renders have their own 250 ms.
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(template.render(&Context::empty()).unwrap(), "xxx");
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(template.render(&Context::empty()).unwrap(), "xxx");
}
