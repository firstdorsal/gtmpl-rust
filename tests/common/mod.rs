//! Helpers shared by the integration suites.
//!
//! Lives in a subdirectory so Cargo compiles it as a module of each test binary
//! rather than as a test target of its own.

use gtmpl_ng::{Context, Template, Value};

/// Parses and renders `source` against `data`, flattening a parse and an exec
/// failure into one message so a test can assert on the outcome without having to
/// match two error types.
pub fn render(source: &str, data: Value) -> Result<String, String> {
    let mut template = Template::default();
    template.parse(source).map_err(|e| e.to_string())?;
    template
        .render(&Context::from(data))
        .map_err(|e| e.to_string())
}
