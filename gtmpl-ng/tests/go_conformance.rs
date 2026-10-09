//! Locks in how this engine formats against how Go does.
//!
//! `go_conformance.txt` holds a template per line with the output it must produce. The
//! expectations were diffed against real Go 1.26.8; the file's header records the few
//! places this engine deliberately differs and why, so changing any of them has to be
//! a conscious edit rather than an accident.

use gtmpl_ng::{template, Value};

const FIXTURE: &str = include_str!("go_conformance.txt");

#[test]
fn formatting_matches_the_recorded_go_behaviour() {
    let mut checked = 0;
    let mut failures: Vec<String> = Vec::new();

    for line in FIXTURE.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let mut fields = line.split('\t');
        let (Some(source), Some(outcome), Some(expected)) =
            (fields.next(), fields.next(), fields.next())
        else {
            panic!("malformed fixture line: {:?}", line);
        };
        checked += 1;

        let rendered = template(&source.replace("\\n", "\n"), Value::Nil);
        let actual = match (&rendered, outcome) {
            (Ok(out), "OK") => format!("{:?}", out),
            (Err(error), "ERR") => error
                .to_string()
                .lines()
                .next()
                .unwrap_or_default()
                .to_owned(),
            (Ok(out), _) => {
                failures.push(format!(
                    "{}\n  expected an error, rendered {:?}",
                    source, out
                ));
                continue;
            }
            (Err(error), _) => {
                failures.push(format!(
                    "{}\n  expected output, failed with {}",
                    source, error
                ));
                continue;
            }
        };
        if actual != expected {
            failures.push(format!(
                "{}\n  expected {}\n  actual   {}",
                source, expected, actual
            ));
        }
    }

    assert!(checked > 100, "the fixture went missing: {} cases", checked);
    assert!(
        failures.is_empty(),
        "{} of {} cases drifted:\n\n{}",
        failures.len(),
        checked,
        failures.join("\n\n")
    );
}
