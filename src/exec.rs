use std::collections::VecDeque;
use std::io::Write;

use crate::error::ExecError;
use crate::node::*;
use crate::template::Template;
use crate::utils::{is_absent, is_true};

use gtmpl_value::{Func, Value};

const MAX_TEMPLATE_DEPTH: usize = 100_000;

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
    writer: &'b mut T,
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

        let mut state = State {
            template: self,
            template_name: self.name.clone(),
            writer,
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

    fn walk_list(&mut self, ctx: &Context, node: &'a ListNode) -> Result<(), ExecError> {
        for n in &node.nodes {
            self.walk(ctx, n)?;
        }
        Ok(())
    }

    // Top level walk function. Steps through the major parts for the template strcuture and
    // writes to the output.
    fn walk(&mut self, ctx: &Context, node: &'a Nodes) -> Result<(), ExecError> {
        self.node = Some(node);
        match *node {
            Nodes::Action(ref n) => {
                let val = self
                    .eval_pipeline(ctx, &n.pipe)
                    .map_err(|e| self.wrap_error(e, node))?;
                if n.pipe.decl.is_empty() {
                    self.print_value(&val)?;
                }
                Ok(())
            }
            Nodes::If(_) | Nodes::With(_) => self.walk_if_or_with(node, ctx),
            Nodes::Range(ref n) => self
                .walk_range(ctx, n)
                .map_err(|e| self.wrap_error(e, node)),
            Nodes::List(ref n) => self.walk_list(ctx, n),
            Nodes::Text(ref n) => write!(self.writer, "{}", n).map_err(ExecError::IOError),
            Nodes::Template(ref n) => self
                .walk_template(ctx, n)
                .map_err(|e| self.wrap_error(e, node)),
            _ => Err(ExecError::UnknownNode(node.clone())),
        }
    }

    fn walk_template(&mut self, ctx: &Context, template: &TemplateNode) -> Result<(), ExecError> {
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
        if self.depth >= MAX_TEMPLATE_DEPTH {
            return Err(ExecError::MaxTemplateDepth);
        }
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
                    depth: self.depth + 1,
                };
                return new_state.walk(&context, root);
            }
        }
        Err(ExecError::TemplateNotDefined(name))
    }

    fn eval_pipeline(&mut self, ctx: &Context, pipe: &PipeNode) -> Result<Value, ExecError> {
        let mut val: Option<Value> = None;
        for cmd in &pipe.cmds {
            val = Some(self.eval_command(ctx, cmd, &val)?);
            // TODO
        }
        let val = val.ok_or_else(|| ExecError::ErrorEvaluatingPipe(pipe.clone()))?;
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
            .ok_or_else(|| ExecError::NoArgsForCommandNode(cmd.clone()))?;

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
            _ => Err(ExecError::CannotEvaluateCommand((*first_word).clone())),
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
        let function = self
            .template
            .funcs
            .get(name.as_str())
            .ok_or_else(|| ExecError::UndefinedFunction(name.to_string()))?;
        self.eval_call(ctx, *function, args, fin)
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

        function(&arg_vals).map_err(Into::into)
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
        if let Nodes::Nil(_) = *chain.node {
            return Err(ExecError::NullInChain(chain.clone()));
        }
        let pipe = self.eval_arg(ctx, &*chain.node)?;
        self.eval_field_chain(&pipe, &chain.field, args, fin)
    }

    fn eval_arg(&mut self, ctx: &Context, node: &Nodes) -> Result<Value, ExecError> {
        let result = match *node {
            Nodes::Dot(_) => Ok(ctx.dot.clone()),
            //Nodes::Nil
            Nodes::Field(ref n) => self.eval_field_node(ctx, n, &[], &None), // args?
            Nodes::Variable(ref n) => self.eval_variable_node(n, &[], &None),
            Nodes::Pipe(ref n) => self.eval_pipeline(ctx, n),
            // Nodes::Identifier
            Nodes::Identifier(ref n) => self.eval_function(ctx, n, &[], &None),
            Nodes::Chain(ref n) => self.eval_chain_node(ctx, n, &[], &None),
            Nodes::String(ref n) => Ok(n.value.clone()),
            Nodes::Bool(ref n) => Ok(n.value.clone()),
            Nodes::Number(ref n) => Ok(n.value.clone()),
            _ => Err(ExecError::InvalidArgument(node.clone())),
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
                (f.f)(&[receiver.clone()]).map_err(Into::into)
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
    fn walk_if_or_with(&mut self, node: &'a Nodes, ctx: &Context) -> Result<(), ExecError> {
        let pipe = match *node {
            Nodes::If(ref n) | Nodes::With(ref n) => &n.pipe,
            _ => return Err(ExecError::ExpectedIfOrWith(node.clone())),
        };
        self.vars.push_back(VecDeque::new());
        let result = (|| {
            let val = self
                .eval_pipeline(ctx, pipe)
                .map_err(|e| self.wrap_error(e, node))?;
            let truth = is_true(&val);
            if truth {
                match *node {
                    Nodes::If(ref n) => self.walk_list(ctx, &n.list)?,
                    Nodes::With(ref n) => {
                        let ctx = Context { dot: val };
                        self.walk_list(&ctx, &n.list)?;
                    }
                    _ => {}
                }
            } else {
                match *node {
                    Nodes::If(ref n) | Nodes::With(ref n) => {
                        if let Some(ref otherwise) = n.else_list {
                            self.walk_list(ctx, otherwise)?;
                        }
                    }
                    _ => {}
                }
            }
            Ok(())
        })();
        self.vars.pop_back();
        result
    }

    fn one_iteration(
        &mut self,
        key: Value,
        val: Value,
        range: &'a RangeNode,
    ) -> Result<(), ExecError> {
        match range.pipe.decl.as_slice() {
            [value] => self.assign_var(&value.ident[0], val.clone())?,
            [key_var, value] => {
                self.assign_var(&key_var.ident[0], key)?;
                self.assign_var(&value.ident[0], val.clone())?;
            }
            _ => {}
        }
        let vars = VecDeque::new();
        self.vars.push_back(vars);
        let ctx = Context { dot: val };
        let result = self.walk_list(&ctx, &range.list);
        self.vars.pop_back();
        result
    }

    fn walk_range(&mut self, ctx: &Context, range: &'a RangeNode) -> Result<(), ExecError> {
        self.vars.push_back(VecDeque::new());
        let result = (|| {
            let val = self.eval_pipeline(ctx, &range.pipe)?;
            let empty = match val {
                Value::Object(ref map) | Value::Map(ref map) => {
                    let mut entries: Vec<_> = map.iter().collect();
                    entries.sort_by_key(|(key, _)| *key);
                    for (key, value) in entries {
                        self.one_iteration(Value::from(key.clone()), value.clone(), range)?;
                    }
                    map.is_empty()
                }
                Value::Array(ref values) => {
                    for (index, value) in values.iter().enumerate() {
                        self.one_iteration(Value::from(index), value.clone(), range)?;
                    }
                    values.is_empty()
                }
                Value::Nil | Value::NoValue => true,
                _ => return Err(ExecError::InvalidRange(val)),
            };
            if empty {
                if let Some(ref otherwise) = range.else_list {
                    self.walk_list(ctx, otherwise)?;
                }
            }
            Ok(())
        })();
        self.vars.pop_back();
        result
    }

    fn print_value(&mut self, val: &Value) -> Result<(), ExecError> {
        write!(self.writer, "{}", val).map_err(ExecError::IOError)?;
        Ok(())
    }
}

fn not_a_function(args: &[Nodes], val: &Option<Value>) -> Result<(), ExecError> {
    if args.len() > 1 || val.is_some() {
        return Err(ExecError::ArgumentForNonFunction(args[0].clone()));
    }
    Ok(())
}

#[cfg(test)]
mod tests_mocked {
    use super::*;
    use anyhow::anyhow;
    use gtmpl_derive::Gtmpl;
    use gtmpl_value::FuncError;
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
