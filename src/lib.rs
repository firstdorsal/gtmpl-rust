//! The Golang Templating Language for Rust.
//!
//! ## Example
//! ```rust
//! use gtmpl;
//!
//! let output = gtmpl::template("Finally! Some {{ . }} for Rust", "gtmpl");
//! assert_eq!(&output.unwrap(), "Finally! Some gtmpl for Rust");
//! ```

// Allow pre-existing clippy warnings from upstream
#![allow(clippy::result_large_err)]
#![allow(clippy::explicit_auto_deref)]
#![allow(clippy::cloned_ref_to_slice_refs)]
#![allow(clippy::iter_overeager_cloned)]
#![allow(clippy::double_ended_iterator_last)]
#![allow(clippy::to_string_in_format_args)]
#![allow(clippy::needless_raw_string_hashes)]
#![allow(clippy::doc_lazy_continuation)]

pub mod error;
mod exec;
pub mod funcs;
mod lexer;
mod node;
mod parse;
mod print_verb;
mod printf;
mod template;
mod utils;

#[doc(inline)]
pub use crate::template::Template;

#[doc(inline)]
pub use crate::exec::Context;

#[doc(inline)]
pub use gtmpl_value::Func;

pub use gtmpl_value::FuncError;

#[doc(inline)]
pub use gtmpl_value::from_value;

pub use error::TemplateError;
pub use gtmpl_value::Value;

/// Provides simple basic templating given just a template sting and context.
///
/// ## Example
/// ```rust
/// let output = gtmpl::template("Finally! Some {{ . }} for Rust", "gtmpl");
/// assert_eq!(&output.unwrap(), "Finally! Some gtmpl for Rust");
/// ```
pub fn template<T: Into<Value>>(template_str: &str, context: T) -> Result<String, TemplateError> {
    let mut tmpl = Template::default();
    tmpl.parse(template_str)?;
    tmpl.render(&Context::from(context)).map_err(Into::into)
}
