use std::collections::HashMap;

use crate::error::{ParseError, TemplateError};
use crate::funcs::BUILTINS;
use crate::limits::Limits;
use crate::parse::{parse, Tree};

use gtmpl_value_ng::Func;

/// The main template structure.
///
/// `#[non_exhaustive]`, so a later field -- the way [`limits`](Self::limits) arrived
/// -- is not a breaking change. Build one with [`Template::default`] or
/// [`Template::with_limits`].
#[non_exhaustive]
pub struct Template {
    pub name: String,
    pub funcs: HashMap<String, Func>,
    /// What one render of this template may consume. See [`Limits`].
    pub limits: Limits,
    /// The parsed trees by template name, in the `parse` module's own types -- which
    /// is why this cannot be public API.
    pub(crate) tree_set: HashMap<String, Tree>,
}

impl Default for Template {
    fn default() -> Template {
        Template {
            name: String::default(),
            funcs: BUILTINS.iter().map(|&(k, v)| (k.to_owned(), v)).collect(),
            tree_set: HashMap::default(),
            limits: Limits::default(),
        }
    }
}

impl Template {
    /// Creates a new empty template that renders under the given [`Limits`].
    ///
    /// The default limits are sized for templates a human wrote. Use this when the
    /// program needs more than that, or [`Limits::unlimited`] when every template it
    /// renders is one it wrote itself.
    ///
    /// ## Example
    ///
    /// ```rust
    /// use gtmpl_ng::{Context, Limits, Template};
    ///
    /// let mut tmpl = Template::with_limits(Limits::default().with_output_bytes(1024));
    /// tmpl.parse("{{ range 2000 }}xx{{ end }}").unwrap();
    /// let error = tmpl.render(&Context::empty()).unwrap_err();
    /// // Reported from where it happened, so the message carries the position too.
    /// assert!(error
    ///     .to_string()
    ///     .ends_with("the render would write more than the 1024 bytes allowed"));
    /// ```
    pub fn with_limits(limits: Limits) -> Template {
        Template {
            limits,
            ..Default::default()
        }
    }

    /// Creates a new empty template with a given `name`.
    pub fn with_name<T: Into<String>>(name: T) -> Template {
        Template {
            name: name.into(),
            ..Default::default()
        }
    }

    /// Adds a single custom function to the template.
    ///
    /// ## Example
    ///
    /// ```rust
    /// use gtmpl_ng::{Context, Func, FuncError, Value};
    ///
    /// fn hello_world(_args: &[Value]) -> Result<Value, FuncError> {
    ///   Ok(Value::from("Hello World!"))
    /// }
    ///
    /// let mut tmpl = gtmpl_ng::Template::default();
    /// tmpl.add_func("helloWorld", hello_world);
    /// tmpl.parse("{{ helloWorld }}").unwrap();
    /// let output = tmpl.render(&Context::empty());
    /// assert_eq!(&output.unwrap(), "Hello World!");
    /// ```
    pub fn add_func(&mut self, name: &str, func: Func) {
        self.funcs.insert(name.to_owned(), func);
    }

    /// Adds custom functions to the template.
    ///
    /// ## Example
    ///
    /// ```rust
    /// use std::collections::HashMap;
    ///
    /// use gtmpl_ng::{Context, Func, FuncError, Value};
    ///
    /// fn hello_world(_args: &[Value]) -> Result<Value, FuncError> {
    ///   Ok(Value::from("Hello World!"))
    /// }
    ///
    /// let funcs = vec![("helloWorld", hello_world as Func)];
    /// let mut tmpl = gtmpl_ng::Template::default();
    /// tmpl.add_funcs(&funcs);
    /// tmpl.parse("{{ helloWorld }}").unwrap();
    /// let output = tmpl.render(&Context::empty());
    /// assert_eq!(&output.unwrap(), "Hello World!");
    /// ```
    pub fn add_funcs<T: Into<String> + Clone>(&mut self, funcs: &[(T, Func)]) {
        self.funcs
            .extend(funcs.iter().cloned().map(|(k, v)| (k.into(), v)));
    }

    /// Parse the given `text` as template body.
    ///
    /// ## Example
    ///
    /// ```rust
    /// let mut tmpl = gtmpl_ng::Template::default();
    /// tmpl.parse("Hello World!").unwrap();
    /// ```
    pub fn parse<T: Into<String>>(&mut self, text: T) -> Result<(), ParseError> {
        let tree_set = parse(
            self.name.clone(),
            text.into(),
            self.funcs.keys().cloned().collect(),
        )?;
        self.tree_set.extend(tree_set);
        Ok(())
    }

    /// Add the given `text` as a template with a `name`.
    ///
    /// ## Example
    ///
    /// ```rust
    /// use gtmpl_ng::Context;
    ///
    /// let mut tmpl = gtmpl_ng::Template::default();
    /// tmpl.add_template("fancy", "{{ . }}");
    /// tmpl.parse(r#"{{ template "fancy" . }}!"#).unwrap();
    /// let output = tmpl.render(&Context::from("Hello World"));
    /// assert_eq!(&output.unwrap(), "Hello World!");
    /// ```
    pub fn add_template<N: Into<String>, T: Into<String>>(
        &mut self,
        name: N,
        text: T,
    ) -> Result<(), TemplateError> {
        let tree_set = parse(
            name.into(),
            text.into(),
            self.funcs.keys().cloned().collect(),
        )?;
        self.tree_set.extend(tree_set);
        Ok(())
    }
}

#[cfg(test)]
mod tests_mocked {
    use super::*;

    #[test]
    fn test_parse() {
        let mut t = Template::with_name("foo");
        assert!(t.parse(r#"{{ if eq "bar" "bar" }} 2000 {{ end }}"#).is_ok());
        assert!(t.tree_set.contains_key("foo"));
    }
}
