use std::fmt::{Display, Formatter};

use crate::error::NodeError;
use crate::lexer::ItemType;
use crate::utils::unquote_char;

use gtmpl_value::Value;

macro_rules! nodes {
    ($($node:ident, $name:ident),*) => {
        #[derive(Debug)]
        #[derive(Clone)]
        #[derive(PartialEq)]
        pub enum NodeType {
           $($name,)*
        }

        #[derive(Clone)]
        #[derive(Debug)]
        pub enum Nodes {
            $($name($node),)*
        }

        impl Display for Nodes {
            fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
                match *self {
                    $(Nodes::$name(ref t) => t.fmt(f),)*
                }
            }
        }

        impl Nodes {
            pub fn typ(&self) -> &NodeType {
                match *self {
                    $(Nodes::$name(ref t) => t.typ(),)*
                }
            }
            pub fn pos(&self) -> Pos {
                match *self {
                    $(Nodes::$name(ref t) => t.pos(),)*
                }
            }
            pub fn line(&self) -> usize {
                match *self {
                    $(Nodes::$name(ref t) => t.line(),)*
                }
            }
            pub fn col(&self) -> usize {
                match *self {
                    $(Nodes::$name(ref t) => t.col(),)*
                }
            }
            pub fn len(&self) -> usize {
                match *self {
                    $(Nodes::$name(ref t) => t.len(),)*
                }
            }
            pub fn tree(&self) -> TreeId {
                match *self {
                    $(Nodes::$name(ref t) => t.tree(),)*
                }
            }
        }
    }
}

nodes!(
    ListNode,
    List,
    TextNode,
    Text,
    PipeNode,
    Pipe,
    ActionNode,
    Action,
    CommandNode,
    Command,
    IdentifierNode,
    Identifier,
    VariableNode,
    Variable,
    DotNode,
    Dot,
    NilNode,
    Nil,
    FieldNode,
    Field,
    ChainNode,
    Chain,
    BoolNode,
    Bool,
    NumberNode,
    Number,
    StringNode,
    String,
    EndNode,
    End,
    ElseNode,
    Else,
    IfNode,
    If,
    WithNode,
    With,
    RangeNode,
    Range,
    TemplateNode,
    Template
);

pub type Pos = usize;

pub type TreeId = usize;

pub trait Node: Display {
    fn typ(&self) -> &NodeType;
    fn pos(&self) -> Pos;
    fn line(&self) -> usize;
    fn col(&self) -> usize;
    fn len(&self) -> usize;
    fn tree(&self) -> TreeId;
}

macro_rules! node {
    ($name:ident {
        $($field:ident : $typ:ty),* $(,)*
    }) => {
        #[derive(Clone)]
        #[derive(Debug)]
        pub struct $name {
            typ: NodeType,
            pos: Pos,
            line: usize,
            col: usize,
            len: usize,
            tr: TreeId,
            $(pub $field: $typ,)*
        }
        impl $name {
            /// Widens this node's recorded source span once its extent is known. A
            /// node that is built incrementally cannot know its span at construction
            /// time: it ends at the last token the node consumes, which is only
            /// reached after its children are parsed.
            pub fn set_len(&mut self, len: usize) {
                self.len = len;
            }
        }
        impl Node for $name {
            fn typ(&self) -> &NodeType {
                &self.typ
            }
            fn pos(&self) -> Pos {
                self.pos
            }
            fn line(&self) -> usize {
                self.line
            }
            fn col(&self) -> usize {
                self.col
            }
            fn len(&self) -> usize {
                self.len
            }
            fn tree(&self) -> TreeId {
                self.tr
            }
        }
    }
}

impl Nodes {
    pub fn is_empty_tree(&self) -> Result<bool, NodeError> {
        match *self {
            Nodes::List(ref n) => n.is_empty_tree(),
            Nodes::Text(ref n) => Ok(n.text.is_empty()),
            Nodes::Action(_)
            | Nodes::If(_)
            | Nodes::Range(_)
            | Nodes::Template(_)
            | Nodes::With(_) => Ok(false),
            _ => Err(NodeError::NaTN),
        }
    }
}

node!(
    ListNode {
        nodes: Vec<Nodes>
    }
);

impl ListNode {
    pub fn append(&mut self, n: Nodes) {
        self.nodes.push(n);
    }
    pub fn new(tr: TreeId, pos: Pos, line: usize, col: usize, len: usize) -> ListNode {
        ListNode {
            typ: NodeType::List,
            pos,
            line,
            col,
            len,
            tr,
            nodes: vec![],
        }
    }
    pub fn is_empty_tree(&self) -> Result<bool, NodeError> {
        for n in &self.nodes {
            match n.is_empty_tree() {
                Ok(true) => {}
                Ok(false) => return Ok(false),
                Err(s) => return Err(s),
            }
        }
        Ok(true)
    }
}

impl Display for ListNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        for n in &self.nodes {
            if let Err(e) = n.fmt(f) {
                return Err(e);
            }
        }
        Ok(())
    }
}

node!(TextNode { text: String });

impl TextNode {
    pub fn new(
        tr: TreeId,
        pos: Pos,
        line: usize,
        col: usize,
        len: usize,
        text: String,
    ) -> TextNode {
        TextNode {
            typ: NodeType::Text,
            pos,
            line,
            col,
            len,
            tr,
            text,
        }
    }
}

impl Display for TextNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "{}", self.text)
    }
}

node!(
    PipeNode {
        decl: Vec<VariableNode>,
        is_assign: bool,
        cmds: Vec<CommandNode>
    }
);

impl PipeNode {
    pub fn new(
        tr: TreeId,
        pos: Pos,
        line: usize,
        col: usize,
        len: usize,
        decl: Vec<VariableNode>,
    ) -> PipeNode {
        PipeNode {
            typ: NodeType::Pipe,
            tr,
            pos,
            line,
            col,
            len,
            decl,
            is_assign: false,
            cmds: vec![],
        }
    }

    pub fn append(&mut self, cmd: CommandNode) {
        self.cmds.push(cmd);
    }
}

impl Display for PipeNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        let decl = if self.decl.is_empty() {
            Ok(())
        } else {
            write!(
                f,
                "{} {} ",
                self.decl
                    .iter()
                    .map(|n| n.to_string())
                    .collect::<Vec<String>>()
                    .join(", "),
                if self.is_assign { "=" } else { ":=" }
            )
        };
        decl.and_then(|_| {
            write!(
                f,
                "{}",
                self.cmds
                    .iter()
                    .map(|cmd| cmd.to_string())
                    .collect::<Vec<String>>()
                    .join(" | ")
            )
        })
    }
}

node!(ActionNode { pipe: PipeNode });

impl ActionNode {
    pub fn new(
        tr: TreeId,
        pos: Pos,
        line: usize,
        col: usize,
        len: usize,
        pipe: PipeNode,
    ) -> ActionNode {
        ActionNode {
            typ: NodeType::Action,
            tr,
            pos,
            line,
            col,
            len,
            pipe,
        }
    }
}

impl Display for ActionNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "{{{{{}}}}}", self.pipe)
    }
}

node!(
    CommandNode {
        args: Vec<Nodes>
    }
);

impl CommandNode {
    pub fn new(tr: TreeId, pos: Pos, line: usize, col: usize, len: usize) -> CommandNode {
        CommandNode {
            typ: NodeType::Command,
            pos,
            line,
            col,
            len,
            tr,
            args: vec![],
        }
    }

    pub fn append(&mut self, node: Nodes) {
        self.args.push(node);
    }
}

impl Display for CommandNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        let s = self
            .args
            .iter()
            .map(|n|
                // Handle PipeNode.
                n.to_string())
            .collect::<Vec<String>>()
            .join(" ");
        write!(f, "{}", s)
    }
}

node!(IdentifierNode { ident: String });

impl IdentifierNode {
    pub fn new(ident: String) -> IdentifierNode {
        IdentifierNode {
            typ: NodeType::Identifier,
            tr: 0,
            pos: 0,
            line: 0,
            col: 0,
            len: 0,
            ident,
        }
    }

    pub fn set_pos(&mut self, pos: Pos) -> &IdentifierNode {
        self.pos = pos;
        self
    }

    pub fn set_line(&mut self, line: usize) -> &IdentifierNode {
        self.line = line;
        self
    }

    pub fn set_col(&mut self, col: usize) -> &IdentifierNode {
        self.col = col;
        self
    }

    pub fn set_tree(&mut self, tr: TreeId) -> &IdentifierNode {
        self.tr = tr;
        self
    }
}

impl Display for IdentifierNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "{}", self.ident)
    }
}

node!(
    VariableNode {
        ident: Vec<String>
    }
);

impl VariableNode {
    pub fn new(
        tr: TreeId,
        pos: Pos,
        line: usize,
        col: usize,
        len: usize,
        ident: &str,
    ) -> VariableNode {
        VariableNode {
            typ: NodeType::Variable,
            tr,
            pos,
            line,
            col,
            len,
            ident: ident.split('.').map(|s| s.to_owned()).collect(),
        }
    }
}

impl Display for VariableNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "{}", self.ident.join("."))
    }
}

node!(DotNode {});

impl DotNode {
    pub fn new(tr: TreeId, pos: Pos, line: usize, col: usize, len: usize) -> DotNode {
        DotNode {
            typ: NodeType::Dot,
            tr,
            pos,
            line,
            col,
            len,
        }
    }
}

impl Display for DotNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, ".")
    }
}

node!(NilNode {});

impl Display for NilNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "nil")
    }
}

impl NilNode {
    pub fn new(tr: TreeId, pos: Pos, line: usize, col: usize, len: usize) -> NilNode {
        NilNode {
            typ: NodeType::Nil,
            tr,
            pos,
            line,
            col,
            len,
        }
    }
}

node!(
    FieldNode {
        ident: Vec<String>
    }
);

impl FieldNode {
    pub fn new(
        tr: TreeId,
        pos: Pos,
        line: usize,
        col: usize,
        len: usize,
        ident: &str,
    ) -> FieldNode {
        FieldNode {
            typ: NodeType::Field,
            tr,
            pos,
            line,
            col,
            len,
            ident: ident[..]
                .split('.')
                .filter_map(|s| {
                    if s.is_empty() {
                        None
                    } else {
                        Some(s.to_owned())
                    }
                })
                .collect(),
        }
    }
}

impl Display for FieldNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "{}", self.ident.join("."))
    }
}

node!(
    ChainNode {
        node: Box<Nodes>,
        field: Vec<String>
    }
);

impl ChainNode {
    pub fn new(
        tr: TreeId,
        pos: Pos,
        line: usize,
        col: usize,
        len: usize,
        node: Nodes,
    ) -> ChainNode {
        ChainNode {
            typ: NodeType::Chain,
            tr,
            pos,
            line,
            col,
            len,
            node: Box::new(node),
            field: vec![],
        }
    }

    pub fn add(&mut self, val: &str) {
        let val = val.trim_start_matches('.').to_owned();
        self.field.push(val);
    }
}

impl Display for ChainNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        if let Err(e) = {
            // Handle PipeNode.
            write!(f, "{}", self.node)
        } {
            return Err(e);
        }
        for field in &self.field {
            if let Err(e) = write!(f, ".{}", field) {
                return Err(e);
            }
        }
        Ok(())
    }
}

node!(BoolNode { value: Value });

impl BoolNode {
    pub fn new(tr: TreeId, pos: Pos, line: usize, col: usize, len: usize, val: bool) -> BoolNode {
        BoolNode {
            typ: NodeType::Bool,
            tr,
            pos,
            line,
            col,
            len,
            value: Value::from(val),
        }
    }
}

impl Display for BoolNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "{}", self.value)
    }
}

#[derive(Clone, Debug)]
pub enum NumberType {
    U64,
    I64,
    Float,
    Char,
}

node!(NumberNode {
    is_i64: bool,
    is_u64: bool,
    is_f64: bool,
    text: String,
    number_typ: NumberType,
    value: Value,
});

/// A numeric literal, classified the way Go's `strconv.ParseInt(text, 0, 64)` does.
enum IntegerLiteral {
    /// A legal integer literal, split into the parts `from_str_radix` needs.
    Legal {
        negative: bool,
        digits: String,
        radix: u32,
    },
    /// Not an integer literal at all, so the floating point syntax should be tried.
    NotInteger,
    /// Shaped like an integer literal but not a legal one.
    Malformed,
}

/// Removes Go's `_` separators from a run of digits.
///
/// Go requires every separator to sit between two digits, with one extra allowed
/// directly after a base prefix, so `1_000` and `0x_ff` are legal while `_1`, `1_`
/// and `1__0` are not.
fn strip_digit_separators(digits: &str, after_base_prefix: bool) -> Option<String> {
    if digits.contains("__") || digits.ends_with('_') {
        return None;
    }
    if digits.starts_with('_') && !after_base_prefix {
        return None;
    }
    let stripped: String = digits.chars().filter(|c| *c != '_').collect();
    if stripped.is_empty() {
        None
    } else {
        Some(stripped)
    }
}

/// Classifies `text` as a Go integer literal.
///
/// Rust's `from_str_radix` takes neither a base prefix nor `_` separators, so the
/// prefix is resolved and the separators are checked and removed here. A bare leading
/// zero means octal for an integer but not for a float, which is why `0.5` and `0e1`
/// come back as `NotInteger` while `017` is octal and `08` is malformed.
fn classify_integer_literal(text: &str) -> IntegerLiteral {
    let (negative, body) = match text.chars().next() {
        Some('-') => (true, &text[1..]),
        Some('+') => (false, &text[1..]),
        _ => (false, text),
    };
    if body.is_empty() {
        return IntegerLiteral::Malformed;
    }

    let prefixed = body
        .strip_prefix('0')
        .and_then(|rest| match rest.chars().next() {
            Some('x') | Some('X') => Some((16, &rest[1..])),
            Some('o') | Some('O') => Some((8, &rest[1..])),
            Some('b') | Some('B') => Some((2, &rest[1..])),
            _ => None,
        });

    if let Some((radix, digits)) = prefixed {
        return match strip_digit_separators(digits, true) {
            Some(digits) if digits.chars().all(|c| c.is_digit(radix)) => IntegerLiteral::Legal {
                negative,
                digits,
                radix,
            },
            _ => IntegerLiteral::Malformed,
        };
    }

    // A fraction or an exponent makes this a float, whatever the leading digit is.
    if body.contains(['.', 'e', 'E']) {
        return IntegerLiteral::NotInteger;
    }

    let digits = match strip_digit_separators(body, false) {
        Some(digits) => digits,
        None => return IntegerLiteral::Malformed,
    };

    if let Some(octal) = digits.strip_prefix('0') {
        if !octal.is_empty() {
            return if octal.chars().all(|c| c.is_digit(8)) {
                IntegerLiteral::Legal {
                    negative,
                    digits: octal.to_owned(),
                    radix: 8,
                }
            } else {
                IntegerLiteral::Malformed
            };
        }
    }

    if digits.chars().all(|c| c.is_ascii_digit()) {
        IntegerLiteral::Legal {
            negative,
            digits,
            radix: 10,
        }
    } else {
        IntegerLiteral::Malformed
    }
}

impl NumberNode {
    #[allow(clippy::float_cmp)]
    pub fn new(
        tr: TreeId,
        pos: Pos,
        line: usize,
        col: usize,
        len: usize,
        text: String,
        item_typ: &ItemType,
    ) -> Result<NumberNode, NodeError> {
        match *item_typ {
            ItemType::ItemCharConstant => unquote_char(&text, '\'')
                .map(|c| NumberNode {
                    typ: NodeType::Number,
                    tr,
                    pos,
                    line,
                    col,
                    len,
                    is_i64: true,
                    is_u64: true,
                    is_f64: true,
                    text,
                    number_typ: NumberType::Char,
                    value: Value::from(c as u64),
                })
                .ok_or(NodeError::UnquoteError),
            _ => {
                // Mirrors Go's `newNumber`: read it as an integer first so a base
                // prefix is honoured, and fall back to the float syntax only when no
                // integer reading applies.
                let mut as_i64 = 0i64;
                let mut as_u64 = 0u64;
                let mut is_i64 = false;
                let mut is_u64 = false;

                match classify_integer_literal(&text) {
                    IntegerLiteral::Malformed => return Err(NodeError::NaN),
                    IntegerLiteral::Legal {
                        negative,
                        digits,
                        radix,
                    } => {
                        if !negative {
                            if let Ok(parsed) = u64::from_str_radix(&digits, radix) {
                                as_u64 = parsed;
                                is_u64 = true;
                            }
                        }
                        let signed = if negative {
                            format!("-{}", digits)
                        } else {
                            digits
                        };
                        if let Ok(parsed) = i64::from_str_radix(&signed, radix) {
                            as_i64 = parsed;
                            is_i64 = true;
                            if parsed == 0 {
                                // In case of -0.
                                as_u64 = 0;
                                is_u64 = true;
                            }
                        }
                        if !is_i64 && !is_u64 {
                            // Too large for either, which Go reports as an overflow.
                            return Err(NodeError::NaN);
                        }
                    }
                    IntegerLiteral::NotInteger => {}
                }

                // `is_f64` records that the literal was written as a fraction, which
                // is what decides how it renders; an integer is promoted to a float
                // value as well, as Go does, but keeps its integer shape.
                let (as_f64, is_f64) = if is_i64 || is_u64 {
                    let promoted = if is_i64 { as_i64 as f64 } else { as_u64 as f64 };
                    (promoted, false)
                } else {
                    let stripped: String = text.chars().filter(|c| *c != '_').collect();
                    match stripped.parse::<f64>() {
                        Ok(parsed) => (parsed, true),
                        Err(_) => return Err(NodeError::NaN),
                    }
                };

                // A whole float still reads as an integer, so `{{ 1e3 }}` renders
                // `1000` rather than `1000.0`.
                if !is_i64 && ((as_f64 as i64) as f64) == as_f64 {
                    as_i64 = as_f64 as i64;
                    is_i64 = true;
                }
                if !is_u64 && ((as_f64 as u64) as f64) == as_f64 {
                    as_u64 = as_f64 as u64;
                    is_u64 = true;
                }

                let number_typ = if is_f64 {
                    NumberType::Float
                } else if is_u64 {
                    NumberType::U64
                } else {
                    NumberType::I64
                };

                let value = if is_u64 {
                    Value::from(as_u64)
                } else if is_i64 {
                    Value::from(as_i64)
                } else {
                    Value::from(as_f64)
                };

                Ok(NumberNode {
                    typ: NodeType::Number,
                    tr,
                    pos,
                    line,
                    col,
                    len,
                    is_i64,
                    is_u64,
                    is_f64,
                    text,
                    number_typ,
                    value,
                })
            }
        }
    }
}

impl Display for NumberNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "{}", self.text)
    }
}

node!(StringNode {
    quoted: String,
    value: Value,
});

impl StringNode {
    pub fn new(
        tr: TreeId,
        pos: Pos,
        line: usize,
        col: usize,
        len: usize,
        orig: String,
        text: String,
    ) -> StringNode {
        StringNode {
            typ: NodeType::String,
            tr,
            pos,
            line,
            col,
            len,
            quoted: orig,
            value: Value::from(text),
        }
    }
}

impl Display for StringNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "{}", self.quoted)
    }
}

node!(EndNode {});

impl EndNode {
    pub fn new(tr: TreeId, pos: Pos, line: usize, col: usize, len: usize) -> EndNode {
        EndNode {
            typ: NodeType::End,
            tr,
            pos,
            line,
            col,
            len,
        }
    }
}

impl Display for EndNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "{{{{end}}}}")
    }
}

node!(ElseNode {});

impl ElseNode {
    pub fn new(pos: Pos, line: usize, col: usize, len: usize) -> ElseNode {
        ElseNode {
            typ: NodeType::Else,
            tr: 0,
            pos,
            line,
            col,
            len,
        }
    }
}

impl Display for ElseNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "{{{{else}}}}")
    }
}

node!(
    BranchNode {
        pipe: PipeNode,
        list: ListNode,
        else_list: Option<ListNode>
    }
);

pub type IfNode = BranchNode;
pub type WithNode = BranchNode;
pub type RangeNode = BranchNode;

impl BranchNode {
    pub fn new_if(
        tr: TreeId,
        pos: Pos,
        line: usize,
        col: usize,
        len: usize,
        pipe: PipeNode,
        list: ListNode,
        else_list: Option<ListNode>,
    ) -> IfNode {
        IfNode {
            typ: NodeType::If,
            tr,
            pos,
            line,
            col,
            len,
            pipe,
            list,
            else_list,
        }
    }

    pub fn new_with(
        tr: TreeId,
        pos: Pos,
        line: usize,
        col: usize,
        len: usize,
        pipe: PipeNode,
        list: ListNode,
        else_list: Option<ListNode>,
    ) -> WithNode {
        WithNode {
            typ: NodeType::With,
            tr,
            pos,
            line,
            col,
            len,
            pipe,
            list,
            else_list,
        }
    }

    pub fn new_range(
        tr: TreeId,
        pos: Pos,
        line: usize,
        col: usize,
        len: usize,
        pipe: PipeNode,
        list: ListNode,
        else_list: Option<ListNode>,
    ) -> RangeNode {
        RangeNode {
            typ: NodeType::Range,
            tr,
            pos,
            line,
            col,
            len,
            pipe,
            list,
            else_list,
        }
    }
}

impl Display for BranchNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        let name = match self.typ {
            NodeType::If => "if",
            NodeType::Range => "range",
            NodeType::With => "with",
            _ => {
                return Err(std::fmt::Error);
            }
        };
        if let Some(ref else_list) = self.else_list {
            return write!(
                f,
                "{{{{{} {}}}}}{}{{{{else}}}}{}{{{{end}}}}",
                name, self.pipe, self.list, else_list
            );
        }
        write!(f, "{{{{{} {}}}}}{}{{{{end}}}}", name, self.pipe, self.list)
    }
}

node!(
    TemplateNode {
        name: PipeOrString,
        pipe: Option<PipeNode>
    }
);

impl TemplateNode {
    pub fn new(
        tr: TreeId,
        pos: Pos,
        line: usize,
        col: usize,
        len: usize,
        name: PipeOrString,
        pipe: Option<PipeNode>,
    ) -> TemplateNode {
        TemplateNode {
            typ: NodeType::Template,
            tr,
            pos,
            line,
            col,
            len,
            name,
            pipe,
        }
    }
}

impl Display for TemplateNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self.pipe {
            Some(ref pipe) => write!(f, "{{{{template {} {}}}}}", self.name, pipe),
            None => write!(f, "{{{{template {}}}}}", self.name),
        }
    }
}

#[derive(Clone, Debug)]
pub enum PipeOrString {
    Pipe(PipeNode),
    String(String),
}

impl Display for PipeOrString {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        match *self {
            PipeOrString::Pipe(ref pipe_node) => write!(f, "{}", pipe_node),
            PipeOrString::String(ref s) => write!(f, "{}", s),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clone() {
        let t1 = TextNode::new(1, 0, 1, 1, 3, "foo".to_owned());
        let mut t2 = t1.clone();
        t2.text = "bar".to_owned();
        assert_eq!(t1.to_string(), "foo");
        assert_eq!(t2.to_string(), "bar");
    }

    #[test]
    fn test_end() {
        let t1 = EndNode::new(1, 0, 1, 1, 3);
        assert_eq!(t1.to_string(), "{{end}}");
    }
}
