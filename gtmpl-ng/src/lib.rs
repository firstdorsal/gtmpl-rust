//! The Golang Templating Language for Rust.
//!
//! ## Example
//! ```rust
//! use gtmpl_ng;
//!
//! let output = gtmpl_ng::template("Finally! Some {{ . }} for Rust", "gtmpl");
//! assert_eq!(&output.unwrap(), "Finally! Some gtmpl for Rust");
//! ```

pub mod error;
mod exec;
pub mod funcs;
mod go_number;
mod lexer;
mod node;
mod parse;
mod print_verb;
mod printf;
mod template;
mod utils;

#[cfg(feature = "helm-functions")]
pub mod helm_functions;

#[cfg(feature = "mows-functions")]
pub mod mows_functions;

#[cfg(feature = "all-functions")]
pub mod all_functions;

#[doc(inline)]
pub use crate::template::Template;

#[doc(inline)]
pub use crate::exec::Context;

#[doc(inline)]
pub use gtmpl_value_ng::Func;

pub use gtmpl_value_ng::FuncError;

#[doc(inline)]
pub use gtmpl_value_ng::from_value;

pub use error::{ErrorContext, ExecError, ParseError, StructuredError, TemplateError};
pub use gtmpl_value_ng::Value;

/// Provides simple basic templating given just a template sting and context.
///
/// ## Example
/// ```rust
/// let output = gtmpl_ng::template("Finally! Some {{ . }} for Rust", "gtmpl");
/// assert_eq!(&output.unwrap(), "Finally! Some gtmpl for Rust");
/// ```
pub fn template<T: Into<Value>>(template_str: &str, context: T) -> Result<String, TemplateError> {
    let mut tmpl = Template::default();
    tmpl.parse(template_str)?;
    tmpl.render(&Context::from(context)).map_err(Into::into)
}
