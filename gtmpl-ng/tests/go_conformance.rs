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
    // Pinned rather than bounded: a case commented out with the file's own `#` would
    // otherwise vanish from the run without a trace, which is the one way this fixture
    // can quietly stop covering what it says it covers.
    const RECORDED_CASES: usize = 237;
    const RECORDED_ERROR_CASES: usize = 12;

    let mut checked = 0;
    let mut error_cases = 0;
    let mut failures: Vec<String> = Vec::new();

    for line in FIXTURE.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        // Exactly three: a stray tab inside an expectation would otherwise truncate it
        // and the case would pass against a prefix.
        assert_eq!(fields.len(), 3, "malformed fixture line: {:?}", line);
        let (source, outcome, expected) = (fields[0], fields[1], fields[2]);
        checked += 1;
        if outcome == "ERR" {
            error_cases += 1;
        }

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

    assert_eq!(
        checked, RECORDED_CASES,
        "the fixture's case count changed; update RECORDED_CASES deliberately"
    );
    assert_eq!(
        error_cases, RECORDED_ERROR_CASES,
        "an expectation changed between output and error; update RECORDED_ERROR_CASES deliberately"
    );
    assert!(
        failures.is_empty(),
        "{} of {} cases drifted:\n\n{}",
        failures.len(),
        checked,
        failures.join("\n\n")
    );
}
