use std::collections::VecDeque;
use std::fmt;
use std::io::{self, Write};
use std::time::{Duration, Instant};

use crate::error::ExecError;
use crate::limits::{self, Limits};
use crate::node::*;
use crate::template::Template;
use crate::utils::{is_absent, is_true};

use gtmpl_value_ng::{Func, NumberKind, Value};

/// What a list did: ran to its end, or was cut short by `{{break}}` or `{{continue}}`.
///
/// The signal travels outwards through `{{if}}`, `{{with}}` and nested lists until a
/// `{{range}}` acts on it. The parser has already refused one that no `{{range}}`
/// encloses, so it can never reach the top.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Flow {
    Done,
    Break,
    Continue,
}

/// How deep the engine may recurse through nested bodies.
///
/// Go's own limit is 100 000 and counts `{{template}}` nesting alone, which it can
/// afford because a goroutine stack grows on demand. Ours does not: nesting is
/// recursion in this process, and `{{define "r"}}{{template "r" .}}{{end}}{{template
/// "r" .}}` overflowed the stack and aborted -- a core dump out of fifty-seven
/// characters, past the `Result` this API returns.
///
/// It counts every nested body rather than only templates, because the stack does not
/// care which kind of nesting put a frame on it. Counting templates could not work:
/// measured against a template-only counter, a body of sixty nested `{{if}}` reached
/// the same stack usage at a fortieth of the template depth, so any value safe for one
/// shape was either useless or fatal for the other.
///
/// The number is measured, in the most expensive configuration a consumer actually
/// runs: an unoptimised build on a 2 MiB stack, which is what `std::thread`, a Tokio
/// worker and the test harness all give -- against 8 MiB for a main thread, and
/// roughly eight times less stack per level once optimised.
///
/// | Shape, debug, 2 MiB stack | Aborted at |
/// |---|---|
/// | `{{range}}` bodies, with or without variables, pipelines or calls | 220 |
/// | one body per level | 320 |
/// | `{{if}}` and `{{with}}` bodies | past 400 |
///
/// 100 therefore sits at less than half the worst case measured, and more than twelve
/// times above what real templates reach: of the 157 templates in the deployment
/// corpus this engine renders, the deepest does not pass 8.
///
/// `tests/malicious_templates.rs` holds the margin rather than trusting it. It drives
/// the `{{range}}` shape on a 2 MiB thread under `cargo test`, which is the worst case
/// in the table, so a change that makes a level dearer takes the test binary down
/// instead of reaching a consumer.
///
/// Not part of [`Limits`] for the same reason: no value above this one is safe, so the
/// knob would only be a way to abort the process.
const MAX_NESTING_DEPTH: usize = 100;

/// How many loop iterations pass between two readings of the clock.
///
/// Measured over a million iterations of `{{ range }}x{{ end }}`: an iteration costs
/// 26.9 ns, and reading the clock on every one made it 40.4 -- a 50% tax on a render
/// that asked for a time bound, not for a slower loop. Every 64th puts it inside the noise
/// and costs at most 64 iterations of overshoot, which is microseconds for a body
/// cheap enough for the stride to matter at all. Anything expensive in a body is a
/// function call, and those are checked one for one.
const ITERATIONS_PER_CLOCK_READ: u32 = 64;

/// The writer, plus what is left of the render's budget.
///
/// The whole render shares one of these. A nested `{{template}}` builds its own
/// [`State`] but borrows this, so no amount of nesting hands a template a fresh
/// allowance -- which is the point, because nesting is how three loops that each obey
/// [`Limits::iterations`] come to 10¹² iterations together.
///
/// Wrapping the writer rather than counting at each call site is deliberate: every
/// byte the engine emits goes through `Write`, so a write path added later is counted
/// without anyone having to remember to.
struct Budget<T: Write> {
    writer: T,
    limits: Limits,
    bytes_left: usize,
    iterations_left: usize,
    /// The instant the deadline falls on and the span it was, together, so the error
    /// can name the configured span without a fallback for a state that cannot occur.
    /// The instant is held so a check is a comparison; `None` means the clock is
    /// never read.
    deadline: Option<(Instant, Duration)>,
    /// Iterations still to go before the clock is read again. See
    /// `ITERATIONS_PER_CLOCK_READ`.
    until_clock_read: u32,
}

/// The marker [`Budget::write`] refuses with, so [`Budget::explain`] can tell a
/// budget refusal from a real I/O failure by downcasting rather than by a flag that
/// would outlive the write it described.
#[derive(Debug, thiserror::Error)]
#[error("the render is over its output budget")]
struct OverBudget;

impl<T: Write> Budget<T> {
    fn new(writer: T, limits: Limits) -> Budget<T> {
        Budget {
            writer,
            limits,
            bytes_left: limits.output_bytes.unwrap_or(usize::MAX),
            iterations_left: limits.total_iterations.unwrap_or(usize::MAX),
            // Paired with the span out of the same `Limits`, and only when the
            // installed deadline produced an instant at all -- an unrepresentable
            // span is no deadline, and the pair keeps the two from drifting apart.
            deadline: limits::expires().zip(limits.deadline),
            // One, not the stride: a deadline that has already passed has to stop the
            // first iteration, not the sixty-fifth.
            until_clock_read: 1,
        }
    }

    /// Refuses to carry on once the render's deadline has passed.
    ///
    /// Costs nothing when none is set -- the clock is not read at all -- which is what
    /// lets this sit on the per-iteration path without charging every render for a
    /// feature most do not use.
    fn check_deadline(&self) -> Result<(), ExecError> {
        if let Some((expires, deadline)) = self.deadline {
            if Instant::now() >= expires {
                return Err(ExecError::DeadlineExceeded { deadline });
            }
        }
        Ok(())
    }

    /// Charges one loop iteration against the render's total, and reads the clock
    /// every so often.
    fn spend_iteration(&mut self) -> Result<(), ExecError> {
        if self.deadline.is_some() {
            self.until_clock_read -= 1;
            if self.until_clock_read == 0 {
                self.until_clock_read = ITERATIONS_PER_CLOCK_READ;
                self.check_deadline()?;
            }
        }
        match self.iterations_left.checked_sub(1) {
            Some(left) => {
                self.iterations_left = left;
                Ok(())
            }
            // Unreachable with no total set: the counter starts at `usize::MAX`.
            None => Err(ExecError::TooManyIterations {
                limit: self.limits.total_iterations.unwrap_or(usize::MAX),
            }),
        }
    }

    /// Names the real cause of a failed write.
    fn explain(&self, error: io::Error) -> ExecError {
        if error
            .get_ref()
            .is_some_and(|inner| inner.is::<OverBudget>())
        {
            ExecError::OutputTooLarge {
                limit: self.limits.output_bytes.unwrap_or(usize::MAX),
            }
        } else {
            ExecError::IOError(error)
        }
    }
}

impl<T: Write> Write for Budget<T> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.len() > self.bytes_left {
            return Err(io::Error::other(OverBudget));
        }
        let written = self.writer.write(buf)?;
        self.bytes_left -= written;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

/// Which of the two lazily evaluated builtins a call is, and so which truth value
/// ends its evaluation early.
#[derive(Clone, Copy)]
enum Logic {
    And,
    Or,
}

impl Logic {
    /// The truth value that settles the answer: `and` is decided by its first falsy
    /// argument, `or` by its first truthy one.
    fn short_circuits_on(self) -> bool {
        matches!(self, Logic::Or)
    }
}

/// How deep a value handed back by a function may nest.
///
/// `Clone`, `Drop`, `Display` and the serializers walk a value recursively, so a
/// value's depth is stack usage the same way nesting depth is -- measured, an array
/// 10 001 levels deep, built by `{{ $x = list $x }}` in a loop, aborted a 2 MiB
/// thread inside `Display`. 128 is far above any data a template legitimately builds
/// and far below the measured abort, and it is not a [`Limits`] knob for the same
/// reason `MAX_NESTING_DEPTH` is not: there is no safe value above it to offer.
/// Host data is never walked -- the host built it, so it is theirs to size -- only
/// what a function hands back is.
const MAX_VALUE_DEPTH: usize = 128;

/// Refuses a function result that would endanger the process by being *held*.
///
/// [`Limits::function_output`] bounds what the generating functions produce, but a
/// value can also grow by feeding a function its own output: `{{ $s = print $s $s }}`
/// doubles per loop iteration and reaches 2^43 bytes inside forty of them, with every
/// counter content, until the allocator aborts. Every such growth passes through a
/// function call, so this is the one choke point: the result's weight -- string bytes
/// plus container elements and keys -- is judged against the same `function_output`
/// budget, and its depth against [`MAX_VALUE_DEPTH`].
///
/// The walk is iterative, so measuring a hostile value cannot itself overflow the
/// stack, and it stops at the first violation rather than sizing the whole value.
fn check_produced_value(value: &Value, limits: &Limits) -> Result<(), ExecError> {
    let budget = limits.function_output;
    let mut weight: usize = 0;
    let mut stack: Vec<(usize, &Value)> = vec![(1, value)];
    while let Some((depth, value)) = stack.pop() {
        if depth > MAX_VALUE_DEPTH {
            return Err(ExecError::ValueTooDeep {
                limit: MAX_VALUE_DEPTH,
            });
        }
        weight = weight.saturating_add(match value {
            Value::String(text) => text.len(),
            Value::Array(items) => {
                for item in items {
                    stack.push((depth + 1, item));
                }
                items.len()
            }
            Value::Map(entries) | Value::Object(entries) => {
                let mut key_bytes = 0usize;
                for (key, item) in entries {
                    key_bytes = key_bytes.saturating_add(key.len());
                    stack.push((depth + 1, item));
                }
                entries.len().saturating_add(key_bytes)
            }
            _ => 1,
        });
        if let Some(budget) = budget {
            if weight > budget {
                return Err(ExecError::ValueTooLarge { limit: budget });
            }
        }
    }
    Ok(())
}

struct Variable {
    name: String,
    value: Value,
}

/// How a single `Value` answers a field lookup. This is the one place that decides
/// it, so the borrowing walk and the invoking walk cannot drift apart.
enum FieldOutcome<'v> {
    /// The field is present and holds this value.
    Found(&'v Value),
    /// The receiver is absent, or a map that has no such key. Go produces an invalid
    /// value here and every further field access on it stays invalid, however deep,
    /// so this ends the chain and renders as `<no value>`.
    Absent,
}

/// Resolves one field of one receiver. A struct-like receiver must have the field; a
/// map need not; anything else cannot have fields at all.
fn resolve_field<'v>(receiver: &'v Value, field_name: &str) -> Result<FieldOutcome<'v>, ExecError> {
    match receiver {
        Value::Object(object) => match object.get(field_name) {
            Some(field) => Ok(FieldOutcome::Found(field)),
            None => Err(ExecError::NoFieldFor(
                field_name.to_string(),
                receiver.clone(),
            )),
        },
        Value::Map(map) => match map.get(field_name) {
            Some(field) => Ok(FieldOutcome::Found(field)),
            None => Ok(FieldOutcome::Absent),
        },
        // Reached when an earlier miss was carried here, possibly through a variable.
        Value::NoValue => Ok(FieldOutcome::Absent),
        _ => Err(ExecError::OnlyMapsAndObjectsHaveFields),
    }
}

/// Outcome of walking a whole field chain through borrowed values.
enum ChainLookup<'v> {
    /// The chain resolved to this value; only the final value has to be cloned.
    Found(&'v Value),
    /// The chain ran into an absent receiver and ends as `<no value>`.
    NoValue,
    /// The chain reached a function-valued field, which only `eval_field` may
    /// invoke. `at` is how many fields were already resolved, so the caller can
    /// resume there instead of walking the prefix again.
    Function { at: usize },
}

/// Walks a field chain without cloning the intermediate values. Resolving a field is
/// a pure lookup, so this borrows no state and works on any receiver -- dot as well
/// as a variable's value.
fn lookup_field_chain<'v>(
    receiver: &'v Value,
    ident: &[String],
) -> Result<ChainLookup<'v>, ExecError> {
    let mut value = receiver;

    for (index, field_name) in ident.iter().enumerate() {
        let field = match resolve_field(value, field_name)? {
            FieldOutcome::Found(field) => field,
            FieldOutcome::Absent => return Ok(ChainLookup::NoValue),
        };

        if matches!(field, Value::Function(_)) {
            return Ok(ChainLookup::Function { at: index });
        }
        value = field;
    }

    Ok(ChainLookup::Found(value))
}

struct State<'a, 'b, 'c, T: Write> {
    template: &'a Template,
    template_name: String,
    writer: &'b mut Budget<T>,
    root: &'c Value,
    node: Option<&'a Nodes>,
    vars: VecDeque<VecDeque<Variable>>,
    depth: usize,
}

/// A Context for the template. Passed to the template exectution.
pub struct Context {
    dot: Value,
}

impl Context {
    pub fn empty() -> Context {
        Context { dot: Value::Nil }
    }

    pub fn from<T>(value: T) -> Context
    where
        T: Into<Value>,
    {
        let serialized: Value = value.into();
        Context { dot: serialized }
    }
}

impl<'b> Template {
    pub fn execute<T: Write>(&self, writer: &'b mut T, data: &Context) -> Result<(), ExecError> {
        let mut vars: VecDeque<VecDeque<Variable>> = VecDeque::new();
        vars.push_back(VecDeque::new());

        // Template functions are bare `fn` pointers with nowhere to pass a limit, so
        // the ones that turn a number into an allocation read it from the thread for
        // as long as this render runs. Dropped at the end of the call, restoring
        // whatever was there -- which is what makes a render inside a function safe.
        let _active = limits::Active::install(self.limits);
        let mut budget = Budget::new(writer, self.limits);

        let mut state = State {
            template: self,
            template_name: self.name.clone(),
            writer: &mut budget,
            root: &data.dot,
            node: None,
            vars,
            depth: 0,
        };

        let root = self
            .tree_set
            .get(&self.name)
            .and_then(|tree| tree.root.as_ref())
            .ok_or_else(|| ExecError::IncompleteTemplate(self.name.clone()))?;
        state.walk(data, root)?;

        Ok(())
    }

    pub fn render(&self, data: &Context) -> Result<String, ExecError> {
        let mut w: Vec<u8> = vec![];
        self.execute(&mut w, data)?;
        String::from_utf8(w).map_err(ExecError::Utf8ConversionFailed)
    }
}

impl<'a, 'b, 'c, T: Write> State<'a, 'b, 'c, T> {
    fn wrap_error(&self, err: ExecError, node: &Nodes) -> ExecError {
        ExecError::with_context(
            &self.template_name,
            node.line(),
            node.col(),
            node.len(),
            err,
        )
    }

    /// Runs `body` in a fresh variable scope and restores the previous one, whichever
    /// way the body returns.
    ///
    /// Go writes this as `defer s.pop(s.mark())`. A `Drop` guard cannot do it here:
    /// the guard would have to hold the scope stack while the body still needs
    /// `&mut self` for `walk_list`. Owning the push and the pop in one place has the
    /// same effect -- a caller has no pop to forget.
    fn in_new_scope<R>(&mut self, body: impl FnOnce(&mut Self) -> R) -> R {
        self.vars.push_back(VecDeque::new());
        let result = body(self);
        self.vars.pop_back();
        result
    }

    fn assign_var(&mut self, name: &str, value: Value) -> Result<(), ExecError> {
        for context in self.vars.iter_mut().rev() {
            for variable in context.iter_mut().rev() {
                if variable.name == name {
                    variable.value = value;
                    return Ok(());
                }
            }
        }

        // `$` is not kept in the stack -- it is read straight off the root -- but Go
        // allows reassigning it, and the new value has to outlive the scope doing the
        // assignment. Binding it in the outermost scope gives exactly that: later
        // lookups find it before falling back to the root, and it survives every pop.
        if name == "$" {
            if let Some(outermost) = self.vars.front_mut() {
                outermost.push_front(Variable {
                    name: name.to_owned(),
                    value,
                });
                return Ok(());
            }
        }

        Err(ExecError::VariableNotFound(name.to_owned()))
    }

    fn var_ref(&self, key: &str) -> Result<&Value, ExecError> {
        for context in self.vars.iter().rev() {
            for var in context.iter().rev() {
                if var.name == key {
                    return Ok(&var.value);
                }
            }
        }
        if key == "$" {
            return Ok(self.root);
        }
        Err(ExecError::VariableNotFound(key.to_string()))
    }

    fn var_value(&self, key: &str) -> Result<Value, ExecError> {
        self.var_ref(key).cloned()
    }

    fn walk_list(&mut self, ctx: &Context, node: &'a ListNode) -> Result<Flow, ExecError> {
        // Every nested body is entered through here -- an `{{if}}` branch, a loop
        // body, a template being called -- so this is the one place that has to count
        // for the stack to stay bounded. See `MAX_NESTING_DEPTH`.
        if self.depth >= MAX_NESTING_DEPTH {
            return Err(ExecError::MaxTemplateDepth {
                limit: MAX_NESTING_DEPTH,
            });
        }
        self.depth += 1;
        let result = self.walk_nodes(ctx, node);
        self.depth -= 1;
        result
    }

    fn walk_nodes(&mut self, ctx: &Context, node: &'a ListNode) -> Result<Flow, ExecError> {
        for n in &node.nodes {
            // A `{{break}}` or `{{continue}}` abandons the rest of the list and keeps
            // travelling outwards until a `{{range}}` catches it.
            match self.walk(ctx, n)? {
                Flow::Done => {}
                interrupted => return Ok(interrupted),
            }
        }
        Ok(Flow::Done)
    }

    // Top level walk function. Steps through the major parts for the template strcuture and
    // writes to the output.
    fn walk(&mut self, ctx: &Context, node: &'a Nodes) -> Result<Flow, ExecError> {
        self.node = Some(node);
        match *node {
            Nodes::Action(ref n) => {
                let val = self
                    .eval_pipeline(ctx, &n.pipe)
                    .map_err(|e| self.wrap_error(e, node))?;
                if n.pipe.decl.is_empty() {
                    self.print_value(&val)?;
                }
                Ok(Flow::Done)
            }
            Nodes::If(_) | Nodes::With(_) => self.walk_if_or_with(node, ctx),
            // The range is where the signal stops: it acts on it and reports a
            // finished list to whatever encloses it.
            Nodes::Range(ref n) => self
                .walk_range(ctx, n)
                .map_err(|e| self.wrap_error(e, node))
                .map(|()| Flow::Done),
            Nodes::List(ref n) => self.walk_list(ctx, n),
            Nodes::Text(ref n) => self
                .write_output(format_args!("{}", n))
                .map(|()| Flow::Done),
            // A `{{define}}` body is its own template, so the parser has already
            // refused a `{{break}}` in one: nothing can escape through here.
            Nodes::Template(ref n) => self
                .walk_template(ctx, n)
                .map_err(|e| self.wrap_error(e, node))
                .map(|()| Flow::Done),
            Nodes::Break(_) => Ok(Flow::Break),
            Nodes::Continue(_) => Ok(Flow::Continue),
            _ => Err(ExecError::UnknownNode(node.to_string())),
        }
    }

    fn walk_template(&mut self, ctx: &Context, template: &TemplateNode) -> Result<(), ExecError> {
        // Charged like a loop iteration: the depth cap bounds how deep calls go, but
        // nothing else bounds how *many* there are, and two calls per level is 2^100
        // calls inside a depth of 100. Drawing on the shared total closes that.
        self.writer.spend_iteration()?;
        let name = match template.name {
            PipeOrString::String(ref name) => name.to_owned(),
            PipeOrString::Pipe(ref pipe) => {
                if let Value::String(s) = self.eval_pipeline(ctx, pipe)? {
                    s
                } else {
                    return Err(ExecError::PipelineMustYieldString);
                }
            }
        };
        let tree = self.template.tree_set.get(&name);
        if let Some(tree) = tree {
            if let Some(ref root) = tree.root {
                let value = if let Some(ref pipe) = template.pipe {
                    self.eval_pipeline(ctx, pipe)?
                } else {
                    Value::NoValue
                };
                let context = Context::from(value);
                let mut vars = VecDeque::new();
                vars.push_back(VecDeque::new());
                let mut new_state = State {
                    template: self.template,
                    template_name: name.clone(),
                    writer: self.writer,
                    root: &context.dot,
                    node: None,
                    vars,
                    // Carried on, not restarted: the frames this template's body puts
                    // on the stack sit on top of the ones already there, so a fresh
                    // count per template would be no count at all.
                    depth: self.depth,
                };
                // A `{{break}}` cannot occur in a template body -- the parser
                // refuses one outside a `{{range}}`, and a body is its own template.
                return new_state.walk(&context, root).map(|_| ());
            }
        }
        Err(ExecError::TemplateNotDefined(name))
    }

    fn eval_pipeline(&mut self, ctx: &Context, pipe: &PipeNode) -> Result<Value, ExecError> {
        let mut val: Option<Value> = None;
        for cmd in &pipe.cmds {
            val = Some(self.eval_command(ctx, cmd, &val)?);
        }
        let val = val.ok_or_else(|| ExecError::ErrorEvaluatingPipe(pipe.to_string()))?;
        for var in &pipe.decl {
            if pipe.is_assign {
                self.assign_var(&var.ident[0], val.clone())?;
                continue;
            }
            self.vars
                .back_mut()
                .map(|v| {
                    v.push_back(Variable {
                        name: var.ident[0].clone(),
                        value: val.clone(),
                    })
                })
                .ok_or(ExecError::EmptyStack)?;
        }
        Ok(val)
    }

    fn eval_command(
        &mut self,
        ctx: &Context,
        cmd: &CommandNode,
        val: &Option<Value>,
    ) -> Result<Value, ExecError> {
        let first_word = &cmd
            .args
            .first()
            .ok_or_else(|| ExecError::NoArgsForCommandNode(cmd.to_string()))?;

        match *(*first_word) {
            Nodes::Field(ref n) => return self.eval_field_node(ctx, n, &cmd.args, val),
            Nodes::Variable(ref n) => return self.eval_variable_node(n, &cmd.args, val),
            Nodes::Pipe(ref n) => return self.eval_pipeline(ctx, n),
            Nodes::Chain(ref n) => return self.eval_chain_node(ctx, n, &cmd.args, val),
            Nodes::Identifier(ref n) => return self.eval_function(ctx, n, &cmd.args, val),
            _ => {}
        }
        not_a_function(&cmd.args, val)?;
        match *(*first_word) {
            Nodes::Bool(ref n) => Ok(n.value.clone()),
            Nodes::Dot(_) => Ok(ctx.dot.clone()),
            Nodes::Number(ref n) => Ok(n.value.clone()),
            Nodes::String(ref n) => Ok(n.value.clone()),
            _ => Err(ExecError::CannotEvaluateCommand(first_word.to_string())),
        }
    }

    fn eval_function(
        &mut self,
        ctx: &Context,
        ident: &IdentifierNode,
        args: &[Nodes],
        fin: &Option<Value>,
    ) -> Result<Value, ExecError> {
        let name = &ident.ident;
        let function = *self
            .template
            .funcs
            .get(name.as_str())
            .ok_or_else(|| ExecError::UndefinedFunction(name.to_string()))?;
        // Go evaluates `and` and `or` lazily since 1.18: the arguments after the one
        // that decides are never evaluated, so `{{ and false (len nil) }}` is `false`
        // rather than an error. Recognised by name *and* address: the name so the
        // builtin registered under another name keeps ordinary call semantics, the
        // address so a caller's own function registered *as* "and" or "or" does too.
        // Addresses of distinct functions are not guaranteed distinct across every
        // codegen, so if the match ever fails the fallback is eager evaluation --
        // Go's own semantics before 1.18: the same values, more work, never less.
        let lazy = match name.as_str() {
            "and" if std::ptr::fn_addr_eq(function, crate::funcs::and as Func) => Some(Logic::And),
            "or" if std::ptr::fn_addr_eq(function, crate::funcs::or as Func) => Some(Logic::Or),
            _ => None,
        };
        if let Some(logic) = lazy {
            return self.eval_lazy_logic(ctx, logic, function, args, fin);
        }
        self.eval_call(ctx, function, args, fin)
    }

    fn eval_call(
        &mut self,
        ctx: &Context,
        function: Func,
        args: &[Nodes],
        fin: &Option<Value>,
    ) -> Result<Value, ExecError> {
        let mut arg_vals = vec![];
        if !args.is_empty() {
            for arg in &args[1..] {
                let val = self.eval_arg(ctx, arg)?;
                arg_vals.push(val);
            }
        }
        if let Some(ref f) = *fin {
            arg_vals.push(f.clone());
        }

        // Before the call, not after: a function runs to its end whatever the deadline
        // says, so what a check here buys is that the *next* one does not start.
        self.writer.check_deadline()?;
        let value = function(&arg_vals)?;
        check_produced_value(&value, &self.template.limits)?;
        Ok(value)
    }

    /// Go's lazy `and`/`or`: arguments evaluate left to right only until one whose
    /// truth short-circuits the [`Logic`], and the last one evaluated is returned.
    ///
    /// A piped value (`fin`) is the final argument, already evaluated by the pipe.
    /// With no arguments at all the real function is called empty, so the arity error
    /// stays the function's own.
    fn eval_lazy_logic(
        &mut self,
        ctx: &Context,
        logic: Logic,
        function: Func,
        args: &[Nodes],
        fin: &Option<Value>,
    ) -> Result<Value, ExecError> {
        self.writer.check_deadline()?;
        let mut last: Option<Value> = None;
        for arg in args.iter().skip(1) {
            let val = self.eval_arg(ctx, arg)?;
            if is_true(&val) == logic.short_circuits_on() {
                return Ok(val);
            }
            last = Some(val);
        }
        if let Some(ref f) = *fin {
            return Ok(f.clone());
        }
        match last {
            Some(val) => Ok(val),
            None => function(&[]).map_err(Into::into),
        }
    }

    fn eval_chain_node(
        &mut self,
        ctx: &Context,
        chain: &ChainNode,
        args: &[Nodes],
        fin: &Option<Value>,
    ) -> Result<Value, ExecError> {
        if chain.field.is_empty() {
            return Err(ExecError::NoFieldsInEvalChainNode);
        }
        let pipe = self.eval_arg(ctx, &chain.node)?;
        self.eval_field_chain(&pipe, &chain.field, args, fin)
    }

    fn eval_arg(&mut self, ctx: &Context, node: &Nodes) -> Result<Value, ExecError> {
        let result = match *node {
            Nodes::Dot(_) => Ok(ctx.dot.clone()),
            Nodes::Nil(_) => Ok(Value::Nil),
            Nodes::Field(ref n) => self.eval_field_node(ctx, n, &[], &None), // args?
            Nodes::Variable(ref n) => self.eval_variable_node(n, &[], &None),
            Nodes::Pipe(ref n) => self.eval_pipeline(ctx, n),
            // Nodes::Identifier
            Nodes::Identifier(ref n) => self.eval_function(ctx, n, &[], &None),
            Nodes::Chain(ref n) => self.eval_chain_node(ctx, n, &[], &None),
            Nodes::String(ref n) => Ok(n.value.clone()),
            Nodes::Bool(ref n) => Ok(n.value.clone()),
            Nodes::Number(ref n) => Ok(n.value.clone()),
            _ => Err(ExecError::InvalidArgument(node.to_string())),
        };
        // Wrap errors with the argument node's position for better error reporting
        result.map_err(|e| self.wrap_error(e, node))
    }

    fn eval_field_node(
        &mut self,
        ctx: &Context,
        field: &FieldNode,
        args: &[Nodes],
        fin: &Option<Value>,
    ) -> Result<Value, ExecError> {
        self.eval_field_chain(&ctx.dot, &field.ident, args, fin)
    }

    fn eval_field_chain(
        &mut self,
        receiver: &Value,
        ident: &[String],
        args: &[Nodes],
        fin: &Option<Value>,
    ) -> Result<Value, ExecError> {
        let n = ident.len();
        if n < 1 {
            return Err(ExecError::FieldChainWithoutFields);
        }

        // Only `eval_field` may invoke a function-valued field, and a call with
        // arguments has to go through it as well. Everything else is a pure lookup
        // that the borrowing walk can answer without cloning anything but the result.
        if args.len() <= 1 && fin.is_none() {
            match lookup_field_chain(receiver, ident)? {
                ChainLookup::Found(value) => return Ok(value.clone()),
                ChainLookup::NoValue => return Ok(Value::NoValue),
                ChainLookup::Function { at } => {
                    return self.invoke_chain(receiver, ident, at, args, fin)
                }
            }
        }

        self.invoke_chain(receiver, ident, 0, args, fin)
    }

    /// Walks a field chain through `eval_field`, which is the only place allowed to
    /// invoke a function-valued field and therefore needs each receiver owned.
    ///
    /// `from` is the index of the first field that actually needs that treatment.
    /// Everything before it was already resolved by the borrowing walk and is known
    /// to be function-free, so it is resolved by reference here too rather than
    /// cloned step by step.
    fn invoke_chain(
        &mut self,
        receiver: &Value,
        ident: &[String],
        from: usize,
        args: &[Nodes],
        fin: &Option<Value>,
    ) -> Result<Value, ExecError> {
        let n = ident.len();

        let mut borrowed = receiver;
        for field_name in ident.iter().take(from) {
            match resolve_field(borrowed, field_name)? {
                FieldOutcome::Found(field) => borrowed = field,
                FieldOutcome::Absent => return Ok(Value::NoValue),
            }
        }

        if from + 1 >= n {
            return self.eval_field(borrowed, &ident[n - 1], args, fin);
        }

        let mut owned = self.eval_field(borrowed, &ident[from], &[], &None)?;
        for field_name in ident.iter().take(n - 1).skip(from + 1) {
            owned = self.eval_field(&owned, field_name, &[], &None)?;
        }
        self.eval_field(&owned, &ident[n - 1], args, fin)
    }

    fn eval_field(
        &mut self,
        receiver: &Value,
        field_name: &str,
        args: &[Nodes],
        fin: &Option<Value>,
    ) -> Result<Value, ExecError> {
        // An absent receiver answers before the argument check, as Go does: it has no
        // fields and no methods, so `{{.map.missing.deeper "x"}}` is `<no value>`
        // rather than "cannot be invoked as function".
        if is_absent(receiver) {
            return Ok(Value::NoValue);
        }
        if args.len() > 1 || fin.is_some() {
            return Err(ExecError::NotAFunctionButArguments(field_name.to_string()));
        }

        match resolve_field(receiver, field_name)? {
            FieldOutcome::Found(Value::Function(f)) => {
                self.writer.check_deadline()?;
                (f.f)(std::slice::from_ref(receiver)).map_err(Into::into)
            }
            FieldOutcome::Found(field) => Ok(field.clone()),
            FieldOutcome::Absent => Ok(Value::NoValue),
        }
    }

    fn eval_variable_node(
        &mut self,
        variable: &VariableNode,
        args: &[Nodes],
        fin: &Option<Value>,
    ) -> Result<Value, ExecError> {
        if variable.ident.len() == 1 {
            let val = self.var_value(&variable.ident[0])?;
            not_a_function(args, fin)?;
            return Ok(val);
        }

        // Walk `$var.a.b` straight out of the variable. Going through `var_value`
        // would clone the whole variable -- for `$` that is the entire root object --
        // only to throw away everything but the field at the end of the chain.
        let mut resume_at = 0;
        if args.len() <= 1 && fin.is_none() {
            match lookup_field_chain(self.var_ref(&variable.ident[0])?, &variable.ident[1..])? {
                ChainLookup::Found(value) => return Ok(value.clone()),
                ChainLookup::NoValue => return Ok(Value::NoValue),
                ChainLookup::Function { at } => resume_at = at,
            }
        }

        let val = self.var_value(&variable.ident[0])?;
        self.invoke_chain(&val, &variable.ident[1..], resume_at, args, fin)
    }

    // Walks an `if` or `with` node. They behave the same, except that `with` sets dot.
    fn walk_if_or_with(&mut self, node: &'a Nodes, ctx: &Context) -> Result<Flow, ExecError> {
        let pipe = match *node {
            Nodes::If(ref n) | Nodes::With(ref n) => &n.pipe,
            _ => return Err(ExecError::ExpectedIfOrWith(node.to_string())),
        };
        self.in_new_scope(|state| {
            let val = state
                .eval_pipeline(ctx, pipe)
                .map_err(|e| state.wrap_error(e, node))?;
            // Neither construct is a barrier: a `{{break}}` inside a `{{with}}` ends
            // the `{{range}}` around it, which is Go's behaviour.
            if is_true(&val) {
                match *node {
                    Nodes::If(ref n) => state.walk_list(ctx, &n.list),
                    Nodes::With(ref n) => {
                        let ctx = Context { dot: val };
                        state.walk_list(&ctx, &n.list)
                    }
                    _ => Ok(Flow::Done),
                }
            } else {
                match *node {
                    Nodes::If(ref n) | Nodes::With(ref n) => match n.else_list {
                        Some(ref otherwise) => state.walk_list(ctx, otherwise),
                        None => Ok(Flow::Done),
                    },
                    _ => Ok(Flow::Done),
                }
            }
        })
    }

    fn one_iteration(
        &mut self,
        key: Value,
        val: Value,
        range: &'a RangeNode,
    ) -> Result<Flow, ExecError> {
        // Charged here rather than in the three loops above it, so iterating a map,
        // an array or an integer all draw on the same total and a new kind of range
        // cannot be added without one.
        self.writer.spend_iteration()?;
        match range.pipe.decl.as_slice() {
            [value] => self.assign_var(&value.ident[0], val.clone())?,
            [key_var, value] => {
                self.assign_var(&key_var.ident[0], key)?;
                self.assign_var(&value.ident[0], val.clone())?;
            }
            _ => {}
        }
        let ctx = Context { dot: val };
        self.in_new_scope(|state| state.walk_list(&ctx, &range.list))
            // `{{continue}}` has done its work by ending the body; only `{{break}}`
            // still concerns the loop.
            .map(|flow| match flow {
                Flow::Continue => Flow::Done,
                other => other,
            })
    }

    fn walk_range(&mut self, ctx: &Context, range: &'a RangeNode) -> Result<(), ExecError> {
        self.in_new_scope(|state| {
            let val = state.eval_pipeline(ctx, &range.pipe)?;
            let empty = match val {
                Value::Object(ref map) | Value::Map(ref map) => {
                    let mut entries: Vec<_> = map.iter().collect();
                    entries.sort_by_key(|(key, _)| *key);
                    for (key, value) in entries {
                        let flow =
                            state.one_iteration(Value::from(key.clone()), value.clone(), range)?;
                        if flow == Flow::Break {
                            break;
                        }
                    }
                    map.is_empty()
                }
                Value::Array(ref values) => {
                    for (index, value) in values.iter().enumerate() {
                        let flow = state.one_iteration(Value::from(index), value.clone(), range)?;
                        if flow == Flow::Break {
                            break;
                        }
                    }
                    values.is_empty()
                }
                // Go 1.22: an integer iterates from zero, with the index as dot. A
                // whole float is stored as an integer here, so `{{range 3.0}}`
                // iterates where Go refuses it -- the same collapse that lets
                // `printf "%f" 3` answer. A fractional one still refuses.
                Value::Number(number) => {
                    let count = match number.kind() {
                        NumberKind::Signed(signed) => signed,
                        NumberKind::Unsigned(unsigned) => i64::try_from(unsigned)
                            .map_err(|_| ExecError::RangeOverflowsInt(val.clone()))?,
                        NumberKind::Float(_) => return Err(ExecError::InvalidRange(val)),
                    };
                    // Go takes one variable from an integer range, not two: there is
                    // no value to pair the index with.
                    if range.pipe.decl.len() > 1 {
                        return Err(ExecError::RangeOverOneVariable(val));
                    }
                    // The count a template asked for, as opposed to the length of
                    // something the host handed it. `{{range 1000000000}}` ran until
                    // it was killed; a map or array from the context is not capped
                    // here, because the template cannot inflate it.
                    if let Some(limit) = state.template.limits.iterations {
                        // `map_or(true, ..)`: a count that does not fit a `usize` cannot be
                        // under a `usize` limit -- on 64-bit the branch is dead, on 32-bit it is
                        // the check. Negative counts iterate zero times and pass.
                        if count > 0 && usize::try_from(count).map_or(true, |count| count > limit) {
                            return Err(ExecError::RangeTooLong {
                                asked: count,
                                limit,
                            });
                        }
                    }
                    for index in 0..count {
                        let index = Value::from(index);
                        if state.one_iteration(index.clone(), index, range)? == Flow::Break {
                            break;
                        }
                    }
                    count <= 0
                }
                Value::Nil | Value::NoValue => true,
                _ => return Err(ExecError::InvalidRange(val)),
            };
            if empty {
                if let Some(ref otherwise) = range.else_list {
                    // The parser refuses a `{{break}}` here, as Go does: this branch
                    // runs precisely when there was nothing to iterate.
                    state.walk_list(ctx, otherwise)?;
                }
            }
            Ok(())
        })
    }

    fn print_value(&mut self, val: &Value) -> Result<(), ExecError> {
        // Go's `printValue` writes an untyped nil as `<no value>` -- `{{ and nil 1 }}`
        // prints `<no value>` there. `fmt.Sprint` writes `<nil>`, but that is the
        // `print` family's path, not an action's.
        if matches!(val, Value::Nil) {
            return self.write_output(format_args!("<no value>"));
        }
        self.write_output(format_args!("{}", val))
    }

    /// Writes through the budget, reporting a refusal as the limit it hit.
    fn write_output(&mut self, args: fmt::Arguments<'_>) -> Result<(), ExecError> {
        self.writer
            .write_fmt(args)
            .map_err(|error| self.writer.explain(error))
    }
}

fn not_a_function(args: &[Nodes], val: &Option<Value>) -> Result<(), ExecError> {
    if args.len() > 1 || val.is_some() {
        return Err(ExecError::ArgumentForNonFunction(args[0].to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests_mocked {
    use super::*;
    use anyhow::anyhow;
    use gtmpl_derive_ng::Gtmpl;
    use gtmpl_value_ng::FuncError;
    use std::collections::HashMap;

    #[test]
    fn simple_template() {
        let data = Context::from(1);
        let mut w: Vec<u8> = vec![];
        let mut t = Template::default();
        assert!(t.parse(r#"{{ if false }} 2000 {{ end }}"#).is_ok());
        let out = t.execute(&mut w, &data);
        assert!(out.is_ok());
        assert_eq!(String::from_utf8(w).unwrap(), "");

        let data = Context::from(1);
        let mut w: Vec<u8> = vec![];
        let mut t = Template::default();
        assert!(t.parse(r#"{{ if true }} 2000 {{ end }}"#).is_ok());
        let out = t.execute(&mut w, &data);
        assert!(out.is_ok());
        assert_eq!(String::from_utf8(w).unwrap(), " 2000 ");

        let data = Context::from(1);
        let mut w: Vec<u8> = vec![];
        let mut t = Template::default();
        assert!(t.parse(r#"{{ if true -}} 2000 {{- end }}"#).is_ok());
        let out = t.execute(&mut w, &data);
        assert!(out.is_ok());
        assert_eq!(String::from_utf8(w).unwrap(), "2000");

        let data = Context::from(1);
        let mut w: Vec<u8> = vec![];
        let mut t = Template::default();
        assert!(t
            .parse(r#"{{ if false -}} 2000 {{- else -}} 3000 {{- end }}"#)
            .is_ok());
        let out = t.execute(&mut w, &data);
        assert!(out.is_ok());
        assert_eq!(String::from_utf8(w).unwrap(), "3000");
    }

    #[test]
    fn test_dot() {
        let data = Context::from(1);
        let mut w: Vec<u8> = vec![];
        let mut t = Template::default();
        assert!(t
            .parse(r#"{{ if . -}} 2000 {{- else -}} 3000 {{- end }}"#)
            .is_ok());
        let out = t.execute(&mut w, &data);
        assert!(out.is_ok());
        assert_eq!(String::from_utf8(w).unwrap(), "2000");

        let data = Context::from(false);
        let mut w: Vec<u8> = vec![];
        let mut t = Template::default();
        assert!(t
            .parse(r#"{{ if . -}} 2000 {{- else -}} 3000 {{- end }}"#)
            .is_ok());
        let out = t.execute(&mut w, &data);
        assert!(out.is_ok());
        assert_eq!(String::from_utf8(w).unwrap(), "3000");
    }

    #[test]
    fn test_sub() {
        let data = Context::from(1u8);
        let mut w: Vec<u8> = vec![];
        let mut t = Template::default();
        assert!(t.parse(r#"{{.}}"#).is_ok());
        let out = t.execute(&mut w, &data);
        assert!(out.is_ok());
        assert_eq!(String::from_utf8(w).unwrap(), "1");

        #[derive(Gtmpl)]
        struct Foo {
            foo: u8,
        }
        let f = Foo { foo: 1 };
        let data = Context::from(f);
        let mut w: Vec<u8> = vec![];
        let mut t = Template::default();
        assert!(t.parse(r#"{{.foo}}"#).is_ok());
        let out = t.execute(&mut w, &data);
        assert!(out.is_ok());
        assert_eq!(String::from_utf8(w).unwrap(), "1");
    }

    #[test]
    fn test_novalue() {
        #[derive(Gtmpl)]
        struct Foo {
            foo: u8,
        }
        let f = Foo { foo: 1 };
        let data = Context::from(f);
        let mut w: Vec<u8> = vec![];
        let mut t = Template::default();
        assert!(t.parse(r#"{{.foobar}}"#).is_ok());
        let out = t.execute(&mut w, &data);
        assert!(out.is_err());

        let map: HashMap<String, u64> = [("foo".to_owned(), 23u64)].iter().cloned().collect();
        let data = Context::from(map);
        let mut w: Vec<u8> = vec![];
        let mut t = Template::default();
        assert!(t.parse(r#"{{.foo2}}"#).is_ok());
        let out = t.execute(&mut w, &data);
        assert!(out.is_ok());
        assert_eq!(String::from_utf8(w).unwrap(), Value::NoValue.to_string());
    }

    #[test]
    fn test_dollar_dot() {
        #[derive(Gtmpl, Clone)]
        struct Foo {
            foo: u8,
        }
        let data = Context::from(Foo { foo: 1u8 });
        let mut w: Vec<u8> = vec![];
        let mut t = Template::default();
        assert!(t.parse(r#"{{$.foo}}"#).is_ok());
        let out = t.execute(&mut w, &data);
        assert!(out.is_ok());
        assert_eq!(String::from_utf8(w).unwrap(), "1");

        let nested: HashMap<String, Value> = [("value".to_owned(), Value::from("nested"))].into();
        let root: HashMap<String, Value> = [
            ("root".to_owned(), Value::from("root")),
            ("nested".to_owned(), Value::from(nested)),
        ]
        .into();
        let data = Context::from(root);
        let mut w = Vec::new();
        let mut t = Template::default();
        assert!(t
            .parse(r#"{{with .nested}}{{$.root}}/{{.value}}{{end}}"#)
            .is_ok());
        let out = t.execute(&mut w, &data);
        assert!(out.is_ok());
        assert_eq!(String::from_utf8(w).unwrap(), "root/nested");
    }

    #[test]
    fn test_function_via_dot() {
        #[derive(Gtmpl)]
        struct Foo {
            foo: Func,
        }
        fn foo(_: &[Value]) -> Result<Value, FuncError> {
            Ok(Value::from("foobar"))
        }
        let data = Context::from(Foo { foo });
        let mut w: Vec<u8> = vec![];
        let mut t = Template::default();
        assert!(t.parse(r#"{{.foo}}"#).is_ok());
        let out = t.execute(&mut w, &data);
        assert!(out.is_ok());
        assert_eq!(String::from_utf8(w).unwrap(), "foobar");

        fn plus_one(args: &[Value]) -> Result<Value, FuncError> {
            if let Value::Object(ref o) = &args[0] {
                if let Some(Value::Number(ref n)) = o.get("num") {
                    if let Some(i) = n.as_i64() {
                        return Ok((i + 1).into());
                    }
                }
            }
            Err(anyhow!("integer required, got: {:?}", args).into())
        }

        #[derive(Gtmpl)]
        struct AddMe {
            num: u8,
            plus_one: Func,
        }
        let data = Context::from(AddMe { num: 42, plus_one });
        let mut w: Vec<u8> = vec![];
        let mut t = Template::default();
        assert!(t.parse(r#"{{.plus_one}}"#).is_ok());
        let out = t.execute(&mut w, &data);
        assert!(out.is_ok());
        assert_eq!(String::from_utf8(w).unwrap(), "43");
    }

    #[test]
    fn test_function_ret_map() {
        fn map(_: &[Value]) -> Result<Value, FuncError> {
            let mut h = HashMap::new();
            h.insert("field".to_owned(), 1);
            Ok(h.into())
        }

        let data = Context::empty();
        let mut w: Vec<u8> = vec![];
        let mut t = Template::default();
        t.add_func("map", map);
        assert!(t.parse(r#"{{map.field}}"#).is_ok());
        let out = t.execute(&mut w, &data);
        assert!(out.is_ok());
        assert_eq!(String::from_utf8(w).unwrap(), "1");
    }

    #[test]
    fn test_dot_value() {
        #[derive(Gtmpl, Clone)]
        struct Foo {
            foo: u8,
        }
        #[derive(Gtmpl)]
        struct Bar {
            bar: Foo,
        }
        let f = Foo { foo: 1 };
        let data = Context::from(f);
        let mut w: Vec<u8> = vec![];
        let mut t = Template::default();
        assert!(t
            .parse(r#"{{ if .foo -}} 2000 {{- else -}} 3000 {{- end }}"#)
            .is_ok());
        let out = t.execute(&mut w, &data);
        assert!(out.is_ok());
        assert_eq!(String::from_utf8(w).unwrap(), "2000");

        let f = Foo { foo: 0 };
        let data = Context::from(f);
        let mut w: Vec<u8> = vec![];
        let mut t = Template::default();
        assert!(t
            .parse(r#"{{ if .foo -}} 2000 {{- else -}} 3000 {{- end }}"#)
            .is_ok());
        let out = t.execute(&mut w, &data);
        assert!(out.is_ok());
        assert_eq!(String::from_utf8(w).unwrap(), "3000");

        let bar = Bar {
            bar: Foo { foo: 1 },
        };
        let data = Context::from(bar);
        let mut w: Vec<u8> = vec![];
        let mut t = Template::default();
        assert!(t
            .parse(r#"{{ if .bar.foo -}} 2000 {{- else -}} 3000 {{- end }}"#)
            .is_ok());
        let out = t.execute(&mut w, &data);
        assert!(out.is_ok());
        assert_eq!(String::from_utf8(w).unwrap(), "2000");

        let bar = Bar {
            bar: Foo { foo: 0 },
        };
        let data = Context::from(bar);
        let mut w: Vec<u8> = vec![];
        let mut t = Template::default();
        assert!(t
            .parse(r#"{{ if .bar.foo -}} 2000 {{- else -}} 3000 {{- end }}"#)
            .is_ok());
        let out = t.execute(&mut w, &data);
        assert!(out.is_ok());
        assert_eq!(String::from_utf8(w).unwrap(), "3000");
    }

    #[test]
    fn test_with() {
        #[derive(Gtmpl)]
        struct Foo {
            foo: u16,
        }
        let f = Foo { foo: 1000 };
        let data = Context::from(f);
        let mut w: Vec<u8> = vec![];
        let mut t = Template::default();
        assert!(t
            .parse(r#"{{ with .foo -}} {{.}} {{- else -}} 3000 {{- end }}"#)
            .is_ok());
        let out = t.execute(&mut w, &data);
        assert!(out.is_ok());
        assert_eq!(String::from_utf8(w).unwrap(), "1000");
    }

    #[test]
    fn test_range() {
        let mut map = HashMap::new();
        map.insert("a".to_owned(), 1);
        map.insert("b".to_owned(), 2);
        let data = Context::from(map);
        let mut w: Vec<u8> = vec![];
        let mut t = Template::default();
        assert!(t.parse(r#"{{ range . -}} {{.}} {{- end }}"#).is_ok());
        let out = t.execute(&mut w, &data);
        assert!(out.is_ok());
        assert_eq!(String::from_utf8(w).unwrap(), "12");

        let vec = vec!["foo", "bar", "2000"];
        let data = Context::from(vec);
        let mut w: Vec<u8> = vec![];
        let mut t = Template::default();
        assert!(t.parse(r#"{{ range . -}} {{.}} {{- end }}"#).is_ok());
        let out = t.execute(&mut w, &data);
        assert!(out.is_ok());
        assert_eq!(String::from_utf8(w).unwrap(), "foobar2000");
    }

    #[test]
    fn test_proper_range() {
        let vec = vec!["a".to_string(), "b".to_string()];
        let data = Context::from(vec);
        let mut w: Vec<u8> = vec![];
        let mut t = Template::default();
        assert!(t
            .parse(r#"{{ range $k, $v := . -}} {{ $k }}{{ $v }} {{- end }}"#)
            .is_ok());
        let out = t.execute(&mut w, &data);
        assert!(out.is_ok());
        assert_eq!(String::from_utf8(w).unwrap(), "0a1b");

        let mut map = HashMap::new();
        map.insert("a".to_owned(), 1);
        map.insert("b".to_owned(), 2);
        let data = Context::from(map);
        let mut w: Vec<u8> = vec![];
        let mut t = Template::default();
        assert!(t
            .parse(r#"{{ range $k, $v := . -}} {{ $v }} {{- end }}"#)
            .is_ok());
        let out = t.execute(&mut w, &data);
        assert!(out.is_ok());
        assert_eq!(String::from_utf8(w).unwrap(), "12");

        let mut map = HashMap::new();
        map.insert("a".to_owned(), "b");
        map.insert("c".to_owned(), "d");
        let data = Context::from(map);
        let mut w: Vec<u8> = vec![];
        let mut t = Template::default();
        assert!(t
            .parse(r#"{{ range $k, $v := . -}} {{ $k }}{{ $v }} {{- end }}"#)
            .is_ok());
        let out = t.execute(&mut w, &data);
        assert!(out.is_ok());
        assert_eq!(String::from_utf8(w).unwrap(), "abcd");

        let mut map = HashMap::new();
        map.insert("a".to_owned(), 1);
        map.insert("b".to_owned(), 2);
        let data = Context::from(map);
        let mut w: Vec<u8> = vec![];
        let mut t = Template::default();
        assert!(t
            .parse(r#"{{ range $k, $v := . -}} {{ $k }}{{ $v }} {{- end }}"#)
            .is_ok());
        let out = t.execute(&mut w, &data);
        assert!(out.is_ok());
        assert_eq!(String::from_utf8(w).unwrap(), "a1b2");

        let mut map = HashMap::new();
        map.insert("a".to_owned(), 1);
        map.insert("b".to_owned(), 2);
        #[derive(Gtmpl)]
        struct Foo {
            foo: HashMap<String, i32>,
        }
        let f = Foo { foo: map };
        let data = Context::from(f);
        let mut w: Vec<u8> = vec![];
        let mut t = Template::default();
        assert!(t
            .parse(r#"{{ range $k, $v := .foo -}} {{ $v }} {{- end }}"#)
            .is_ok());
        let out = t.execute(&mut w, &data);
        assert!(out.is_ok());
        assert_eq!(String::from_utf8(w).unwrap(), "12");

        let mut map = HashMap::new();
        #[derive(Gtmpl, Clone)]
        struct Bar {
            bar: i32,
        }
        map.insert("a".to_owned(), Bar { bar: 1 });
        map.insert("b".to_owned(), Bar { bar: 2 });
        let data = Context::from(map);
        let mut w: Vec<u8> = vec![];
        let mut t = Template::default();
        assert!(t
            .parse(r#"{{ range $k, $v := . -}} {{ $v.bar }} {{- end }}"#)
            .is_ok());
        let out = t.execute(&mut w, &data);
        assert!(out.is_ok());
        assert_eq!(String::from_utf8(w).unwrap(), "12");
    }

    #[test]
    fn test_len() {
        let mut w: Vec<u8> = vec![];
        let mut t = Template::default();
        assert!(t.parse(r#"my len is {{ len . }}"#).is_ok());
        let data = Context::from(vec![1, 2, 3]);
        let out = t.execute(&mut w, &data);
        assert!(out.is_ok());
        assert_eq!(String::from_utf8(w).unwrap(), "my len is 3");

        let mut w: Vec<u8> = vec![];
        let mut t = Template::default();
        assert!(t.parse(r#"{{ len . }}"#).is_ok());
        let data = Context::from("hello".to_owned());
        let out = t.execute(&mut w, &data);
        assert!(out.is_ok());
        assert_eq!(String::from_utf8(w).unwrap(), "5");
    }

    #[test]
    fn test_pipeline_function() {
        let mut w: Vec<u8> = vec![];
        let mut t = Template::default();
        assert!(t.parse(r#"{{ if ( 1 | eq . ) -}} 2000 {{- end }}"#).is_ok());
        let data = Context::from(1);
        let out = t.execute(&mut w, &data);
        assert!(out.is_ok());
        assert_eq!(String::from_utf8(w).unwrap(), "2000");
    }

    #[test]
    fn test_function() {
        let mut w: Vec<u8> = vec![];
        let mut t = Template::default();
        assert!(t.parse(r#"{{ if eq . . -}} 2000 {{- end }}"#).is_ok());
        let data = Context::from(1);
        let out = t.execute(&mut w, &data);
        assert!(out.is_ok());
        assert_eq!(String::from_utf8(w).unwrap(), "2000");
    }

    #[test]
    fn test_eq() {
        let mut w: Vec<u8> = vec![];
        let mut t = Template::default();
        assert!(t.parse(r#"{{ if eq "a" "a" -}} 2000 {{- end }}"#).is_ok());
        let data = Context::from(1);
        let out = t.execute(&mut w, &data);
        assert!(out.is_ok());
        assert_eq!(String::from_utf8(w).unwrap(), "2000");

        let mut w: Vec<u8> = vec![];
        let mut t = Template::default();
        assert!(t.parse(r#"{{ if eq "a" "b" -}} 2000 {{- end }}"#).is_ok());
        let data = Context::from(1);
        let out = t.execute(&mut w, &data);
        assert!(out.is_ok());
        assert_eq!(String::from_utf8(w).unwrap(), "");

        let mut w: Vec<u8> = vec![];
        let mut t = Template::default();
        assert!(t.parse(r#"{{ if eq true true -}} 2000 {{- end }}"#).is_ok());
        let data = Context::from(1);
        let out = t.execute(&mut w, &data);
        assert!(out.is_ok());
        assert_eq!(String::from_utf8(w).unwrap(), "2000");

        let mut w: Vec<u8> = vec![];
        let mut t = Template::default();
        assert!(t
            .parse(r#"{{ if eq true false -}} 2000 {{- end }}"#)
            .is_ok());
        let data = Context::from(1);
        let out = t.execute(&mut w, &data);
        assert!(out.is_ok());
        assert_eq!(String::from_utf8(w).unwrap(), "");

        let mut w: Vec<u8> = vec![];
        let mut t = Template::default();
        assert!(t
            .parse(r#"{{ if eq 23.42 23.42 -}} 2000 {{- end }}"#)
            .is_ok());
        let data = Context::from(1);
        let out = t.execute(&mut w, &data);
        assert!(out.is_ok());
        assert_eq!(String::from_utf8(w).unwrap(), "2000");

        let mut w: Vec<u8> = vec![];
        let mut t = Template::default();
        assert!(t.parse(r#"{{ if eq 1 . -}} 2000 {{- end }}"#).is_ok());
        let data = Context::from(1);
        let out = t.execute(&mut w, &data);
        assert!(out.is_ok());
        assert_eq!(String::from_utf8(w).unwrap(), "2000");
    }

    #[test]
    fn test_block() {
        let mut w: Vec<u8> = vec![];
        let mut t = Template::default();
        assert!(t
            .parse(r#"{{ block "foobar" true -}} {{ $ }} {{- end }}"#)
            .is_ok());
        let data = Context::from(2000);
        let out = t.execute(&mut w, &data);
        assert!(out.is_ok());
        assert_eq!(String::from_utf8(w).unwrap(), "true");
    }

    #[test]
    fn test_assign_string() {
        let mut w: Vec<u8> = vec![];
        let mut t = Template::default();
        assert!(t
            .parse(r#"{{ with $foo := "bar" }}{{ $foo }}{{ end }}"#)
            .is_ok());
        let data = Context::from(1);
        let out = t.execute(&mut w, &data);
        assert!(out.is_ok());
        assert_eq!(String::from_utf8(w).unwrap(), "bar");
    }
}
