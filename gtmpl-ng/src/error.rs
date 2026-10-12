use gtmpl_value_ng::{FuncError, Value};
use std::{fmt, string::FromUtf8Error};
use thiserror::Error;

/// Where in a template an error came from, so a caller can point at it.
///
/// `line` and `col` are 1-based, `col` and `len` are byte offsets into that line.
/// Together they delimit the construct that failed, which is what makes
/// `line[col - 1..col - 1 + len]` the offending expression.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ErrorContext {
    /// Name of the template the error came from; empty for an unnamed one.
    pub name: String,
    /// 1-based line number.
    pub line: usize,
    /// 1-based byte column of the first character of the construct.
    ///
    /// Go's own error text reports this column 0-based, so a message built from this
    /// field reads one higher than the equivalent Go message. The character it points
    /// at is the same.
    pub col: usize,
    /// Byte length of the whole construct.
    ///
    /// Since 0.8.0 this spans the entire construct. Before that it was the length
    /// of the construct's first token only, so `{{.a.b}}` reported `2` rather
    /// than `4`.
    pub len: usize,
}

impl fmt::Display for ErrorContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}:{}", self.name, self.line, self.col, self.len)
    }
}

/// A structured error with location and optional cause chain
#[derive(Debug)]
#[non_exhaustive]
pub struct StructuredError {
    pub context: ErrorContext,
    pub message: String,
    pub cause: Option<Box<StructuredError>>,
}

impl StructuredError {
    pub fn new(
        name: impl ToString,
        line: usize,
        col: usize,
        len: usize,
        message: impl ToString,
    ) -> Self {
        Self {
            context: ErrorContext {
                name: name.to_string(),
                line,
                col,
                len,
            },
            message: message.to_string(),
            cause: None,
        }
    }

    pub fn with_cause(mut self, cause: StructuredError) -> Self {
        self.cause = Some(Box::new(cause));
        self
    }

    /// Iterate through the error chain
    pub fn chain(&self) -> ErrorChainIter<'_> {
        ErrorChainIter {
            current: Some(self),
        }
    }
}

pub struct ErrorChainIter<'a> {
    current: Option<&'a StructuredError>,
}

impl<'a> Iterator for ErrorChainIter<'a> {
    type Item = &'a StructuredError;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.current?;
        self.current = current.cause.as_deref();
        Some(current)
    }
}

impl fmt::Display for StructuredError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "template: {}:{}", self.context, self.message)?;
        if let Some(ref cause) = self.cause {
            write!(f, "\n  caused by: {}", cause)?;
        }
        Ok(())
    }
}

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum ParseError {
    #[error("unexpected {0} in define clause")]
    UnexpectedInDefineClause(String),
    #[error("unexpected end")]
    UnexpectedEnd,
    #[error("template nested more than {0} levels deep")]
    MaxParseDepth(usize),
    #[error("template: {0}:{1}")]
    WithContext(ErrorContext, String),
    #[error("no tree")]
    NoTree,
    #[error(transparent)]
    NodeError(#[from] NodeError),
    #[error("enable gtmpl_dynamic_template to use a pipeline as name")]
    NoDynamicTemplate,
    #[error("unable to parse string: {0}")]
    UnableToParseString(String),
}

impl ParseError {
    pub fn with_context(
        name: impl ToString,
        line: usize,
        col: usize,
        len: usize,
        msg: impl ToString,
    ) -> Self {
        Self::WithContext(
            ErrorContext {
                name: name.to_string(),
                line,
                col,
                len,
            },
            msg.to_string(),
        )
    }
}

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum NodeError {
    #[error("unable to unquote")]
    UnquoteError,
    /// Go's wording, which names the literal it could not read.
    #[error("illegal number syntax: {0:?}")]
    IllegalNumberSyntax(String),
    #[error("not a tree node")]
    NaTN,
}

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum ExecError {
    #[error("{0}")]
    Structured(StructuredError),
    #[error("{0} is an incomplete or empty template")]
    IncompleteTemplate(String),
    #[error("{0}")]
    IOError(#[from] std::io::Error),
    #[error("unknown node: {0}")]
    UnknownNode(String),
    #[error("expected if or with node, got {0}")]
    ExpectedIfOrWith(String),
    #[error("unable to convert output to uft-8: {0}")]
    Utf8ConversionFailed(FromUtf8Error),
    #[error("empty var stack")]
    EmptyStack,
    // Go's wording, and through `Display` rather than `Debug`: the message used to
    // read `invalid range Number(Number { n: U(3) })`, which hands the reader this
    // crate's internals instead of the value they wrote.
    #[error("range can't iterate over {0}")]
    InvalidRange(Value),
    #[error("{0} overflows int")]
    RangeOverflowsInt(Value),
    #[error("can't use {0} to iterate over more than one variable")]
    RangeOverOneVariable(Value),
    #[error("pipeline must yield a String")]
    PipelineMustYieldString,
    #[error("template {0} not defined")]
    TemplateNotDefined(String),
    #[error("exceeded the maximum template nesting depth of {limit}")]
    #[non_exhaustive]
    MaxTemplateDepth { limit: usize },
    #[error("range over {asked} is more than the {limit} iterations allowed")]
    #[non_exhaustive]
    RangeTooLong { asked: i64, limit: usize },
    #[error("the render would run more than the {limit} iterations allowed")]
    #[non_exhaustive]
    TooManyIterations { limit: usize },
    #[error("the render would write more than the {limit} bytes allowed")]
    #[non_exhaustive]
    OutputTooLarge { limit: usize },
    #[error("a function produced a value larger than the {limit} allowed")]
    #[non_exhaustive]
    ValueTooLarge { limit: usize },
    #[error("a function produced a value nested deeper than the {limit} levels allowed")]
    #[non_exhaustive]
    ValueTooDeep { limit: usize },
    #[error("the render did not finish within {deadline:?}")]
    #[non_exhaustive]
    DeadlineExceeded { deadline: std::time::Duration },
    #[error("error evaluating pipe: {0}")]
    ErrorEvaluatingPipe(String),
    #[error("no arguments for command node: {0}")]
    NoArgsForCommandNode(String),
    #[error("cannot evaluate command: {0}")]
    CannotEvaluateCommand(String),
    #[error("field chain without fields :/")]
    FieldChainWithoutFields,
    #[error("{0} has arguments but cannot be invoked as function")]
    NotAFunctionButArguments(String),
    #[error("no fields in eval_chain_node")]
    NoFieldsInEvalChainNode,
    #[error("cannot handle {0} as argument")]
    InvalidArgument(String),
    #[error("{0} is not a defined function")]
    UndefinedFunction(String),
    #[error(transparent)]
    FuncError(#[from] FuncError),
    #[error("can't give argument to non-function {0}")]
    ArgumentForNonFunction(String),
    #[error("only maps and objects have fields")]
    OnlyMapsAndObjectsHaveFields,
    #[error("no field `{0}` in {1}")]
    NoFieldFor(String, Value),
    #[error("variable {0} not found")]
    VariableNotFound(String),
}

impl ExecError {
    /// Wrap an error with location context, preserving the error chain.
    /// The innermost (original) error location stays as primary,
    /// outer contexts are added to the cause chain.
    pub fn with_context(
        name: impl ToString,
        line: usize,
        col: usize,
        len: usize,
        error: ExecError,
    ) -> Self {
        let new_context = ErrorContext {
            name: name.to_string(),
            line,
            col,
            len,
        };

        match error {
            // If already structured, keep original context as primary,
            // add new context to the cause chain
            ExecError::Structured(mut inner) => {
                let outer_cause = StructuredError {
                    context: new_context,
                    message: "while evaluating".to_string(),
                    cause: inner.cause.take(),
                };
                inner.cause = Some(Box::new(outer_cause));
                ExecError::Structured(inner)
            }
            // Convert other errors to structured with this context
            other => {
                let structured = StructuredError {
                    context: new_context,
                    message: other.to_string(),
                    cause: None,
                };
                ExecError::Structured(structured)
            }
        }
    }

    /// Get the structured error if this is one
    pub fn as_structured(&self) -> Option<&StructuredError> {
        match self {
            ExecError::Structured(s) => Some(s),
            _ => None,
        }
    }
}

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum TemplateError {
    #[error(transparent)]
    ExecError(#[from] ExecError),
    #[error(transparent)]
    ParseError(#[from] ParseError),
}
