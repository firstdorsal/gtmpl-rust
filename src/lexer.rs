use lazy_static::lazy_static;
use std::collections::HashMap;
use std::fmt;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

type Pos = usize;

/// The character that turns a delimiter into a trimming one.
const TRIM_MARKER: char = '-';
/// A trim marker plus the whitespace character that has to accompany it.
const TRIM_MARKER_LEN: usize = 2;

/// Whitespace as Go's template lexer defines it. Deliberately not
/// `char::is_whitespace`: Go treats anything else, a non-breaking space included, as
/// an unrecognized character inside an action.
fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

/// Whether `s` starts with `- ` -- a trim marker followed by whitespace, which is how
/// a left delimiter asks for the preceding text to be trimmed.
fn has_left_trim_marker(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next() == Some(TRIM_MARKER) && chars.next().map(is_space).unwrap_or(false)
}

/// Whether `s` starts with ` -` -- whitespace followed by a trim marker, which is how
/// a right delimiter asks for the following text to be trimmed. The whitespace belongs
/// to the delimiter, not to the action.
fn has_right_trim_marker(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().map(is_space).unwrap_or(false) && chars.next() == Some(TRIM_MARKER)
}
static LEFT_DELIM: &str = "{{";
static RIGHT_DELIM: &str = "}}";
static LEFT_COMMENT: &str = "/*";
static RIGHT_COMMENT: &str = "*/";

lazy_static! {
    static ref KEY: HashMap<&'static str, ItemType> = {
        let mut m = HashMap::new();
        m.insert(".", ItemType::ItemDot);
        m.insert("block", ItemType::ItemBlock);
        m.insert("define", ItemType::ItemDefine);
        m.insert("end", ItemType::ItemEnd);
        m.insert("else", ItemType::ItemElse);
        m.insert("if", ItemType::ItemIf);
        m.insert("range", ItemType::ItemRange);
        m.insert("nil", ItemType::ItemNil);
        m.insert("template", ItemType::ItemTemplate);
        m.insert("with", ItemType::ItemWith);
        m
    };
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, PartialEq)]
pub enum ItemType {
    ItemError,        // error occurred; value is text of error
    ItemBool,         // boolean constant
    ItemChar,         // printable ASCII character; grab bag for comma etc.
    ItemCharConstant, // character constant
    ItemComplex,      // complex constant (1+2i); imaginary is just a number
    ItemAssign,       // assignment to an existing variable
    ItemColonEquals,  // colon-equals (':=') introducing a declaration
    ItemEOF,
    ItemField,      // alphanumeric identifier starting with '.'
    ItemIdentifier, // alphanumeric identifier not starting with '.'
    ItemLeftDelim,  // left action delimiter
    ItemLeftParen,  // '(' inside action
    ItemNumber,     // simple number, including imaginary
    ItemPipe,       // pipe symbol
    ItemRawString,  // raw quoted string (includes quotes)
    ItemRightDelim, // right action delimiter
    ItemRightParen, // ')' inside action
    ItemSpace,      // run of spaces separating arguments
    ItemString,     // quoted string (includes quotes)
    ItemText,       // plain text
    ItemVariable,   // variable starting with '$', such as '$' or  '$1' or '$hello'
    // Keywords, appear after all the rest.
    ItemKeyword,  // used only to delimit the keywords
    ItemBlock,    // block keyword
    ItemDot,      // the cursor, spelled '.'
    ItemDefine,   // define keyword
    ItemElse,     // else keyword
    ItemEnd,      // end keyword
    ItemIf,       // if keyword
    ItemNil,      // the untyped nil constant, easiest to treat as a keyword
    ItemRange,    // range keyword
    ItemTemplate, // template keyword
    ItemWith,     // with keyword
}

#[derive(Debug)]
pub struct Item {
    pub typ: ItemType,
    pub pos: Pos,
    pub val: String,
    pub line: usize,
    pub col: usize,
}

impl Item {
    pub fn new<T: Into<String>>(typ: ItemType, pos: Pos, val: T, line: usize, col: usize) -> Item {
        Item {
            typ,
            pos,
            val: val.into(),
            line,
            col,
        }
    }
}

impl fmt::Display for Item {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.typ {
            ItemType::ItemEOF => write!(f, "EOF"),
            ItemType::ItemKeyword => write!(f, "<{}>", self.val),
            _ => write!(f, "{}", self.val),
        }
    }
}

pub struct Lexer {
    last_pos: Pos,                  // position of most recent item returned by nextItem
    items_receiver: Receiver<Item>, // channel of scanned items
    finished: bool,                 // flag if lexer is finished
}

/// Resolves the line and column of a byte offset on demand.
///
/// Tokens are emitted in increasing offset order, so each lookup continues from the
/// previous one and the input is walked once overall. Deriving the position beats
/// maintaining it incrementally: a token that spans a newline -- a raw string, a
/// comment, a multi-line action -- used to report the line *after* itself, and the
/// column subtraction underflowed and panicked outright.
struct Position {
    offset: Pos,
    line: usize,
    line_start: Pos,
}

impl Position {
    fn new() -> Position {
        Position {
            offset: 0,
            line: 1,
            line_start: 0,
        }
    }

    /// 1-based line and column of `offset`.
    fn at(&mut self, input: &str, offset: Pos) -> (usize, usize) {
        // Offsets only ever move forward: every caller passes `self.start`, which is
        // assigned from `self.pos` after the scan has already advanced past it.
        debug_assert!(
            offset >= self.offset,
            "position asked to walk backwards, from {} to {}",
            self.offset,
            offset
        );
        for (index, c) in input[self.offset..offset].char_indices() {
            if c == '\n' {
                self.line += 1;
                self.line_start = self.offset + index + 1;
            }
        }
        self.offset = offset;
        (self.line, offset - self.line_start + 1)
    }
}

struct LexerStateMachine {
    input: String,              // the string being scanned
    state: State,               // the next lexing function to enter
    pos: Pos,                   // current position in the input
    start: Pos,                 // start position of this item
    width: Pos,                 // width of last rune read from input
    items_sender: Sender<Item>, // channel of scanned items
    paren_depth: usize,         // nesting depth of ( ) exprs
    position: Position,         // resolves line/column for emitted items
}

#[derive(Debug)]
enum State {
    End,
    LexText,
    LexLeftDelim,
    LexComment,
    LexRightDelim,
    LexInsideAction,
    LexSpace,
    LexIdentifier,
    LexField,
    LexVariable,
    LexChar,
    LexNumber,
    LexQuote,
    LexRawQuote,
}

impl Iterator for Lexer {
    type Item = Item;
    fn next(&mut self) -> Option<Item> {
        if self.finished {
            return None;
        }
        let item = match self.items_receiver.recv() {
            Ok(item) => {
                self.last_pos = item.pos;
                if item.typ == ItemType::ItemError || item.typ == ItemType::ItemEOF {
                    self.finished = true;
                }
                item
            }
            Err(e) => {
                self.finished = true;
                Item::new(ItemType::ItemError, 0, format!("{}", e), 0, 0)
            }
        };
        Some(item)
    }
}

impl Lexer {
    pub fn new(input: String) -> Lexer {
        let (tx, rx) = channel();
        let mut l = LexerStateMachine {
            input,
            state: State::LexText,
            pos: 0,
            start: 0,
            width: 0,
            items_sender: tx,
            paren_depth: 0,
            position: Position::new(),
        };
        thread::spawn(move || l.run());
        Lexer {
            last_pos: 0,
            items_receiver: rx,
            finished: false,
        }
    }

    pub fn drain(&mut self) {
        for _ in self.items_receiver.iter() {}
    }
}

impl Drop for Lexer {
    fn drop(&mut self) {
        self.drain();
    }
}

impl Iterator for LexerStateMachine {
    type Item = char;
    fn next(&mut self) -> Option<char> {
        match self.input[self.pos..].chars().next() {
            Some(c) => {
                self.width = c.len_utf8();
                self.pos += self.width;
                Some(c)
            }
            None => {
                self.width = 0;
                None
            }
        }
    }
}

impl LexerStateMachine {
    fn run(&mut self) {
        loop {
            self.state = match self.state {
                State::LexText => self.lex_text(),
                State::LexComment => self.lex_comment(),
                State::LexLeftDelim => self.lex_left_delim(),
                State::LexRightDelim => self.lex_right_delim(),
                State::LexInsideAction => self.lex_inside_action(),
                State::LexSpace => self.lex_space(),
                State::LexIdentifier => self.lex_identifier(),
                State::LexField => self.lex_field(),
                State::LexVariable => self.lex_variable(),
                State::LexChar => self.lex_char(),
                State::LexNumber => self.lex_number(),
                State::LexQuote => self.lex_quote(),
                State::LexRawQuote => self.lex_raw_quote(),
                State::End => {
                    return;
                }
            }
        }
    }

    fn backup(&mut self) {
        // `next` advanced by the character's UTF-8 width, so give back the same
        // amount. Subtracting one byte leaves `pos` inside a multi-byte character
        // and the next slice panics on a non-char-boundary index.
        self.pos -= self.width;
    }

    fn peek(&mut self) -> Option<char> {
        let c = self.next();
        self.backup();
        c
    }

    fn emit(&mut self, t: ItemType) {
        let (line, col) = self.position.at(&self.input, self.start);
        let value = &self.input[self.start..self.pos];
        self.items_sender
            .send(Item::new(t, self.start, value, line, col))
            .unwrap();
        self.start = self.pos;
    }

    fn ignore(&mut self) {
        self.start = self.pos;
    }

    fn accept(&mut self, valid: &str) -> bool {
        if self.next().map(|s| valid.contains(s)).unwrap_or_default() {
            return true;
        }
        self.backup();
        false
    }

    fn accept_run(&mut self, valid: &str) {
        while self.accept(valid) {}
    }

    fn errorf(&mut self, msg: &str) -> State {
        let (line, col) = self.position.at(&self.input, self.start);
        self.items_sender
            .send(Item::new(ItemType::ItemError, self.start, msg, line, col))
            .unwrap();
        State::End
    }

    fn lex_text(&mut self) -> State {
        self.width = 0;
        let x = self.input[self.pos..].find(LEFT_DELIM);
        match x {
            Some(x) => {
                self.pos += x;
                let ld = self.pos + LEFT_DELIM.len();
                let trim = if has_left_trim_marker(&self.input[ld..]) {
                    rtrim_len(&self.input[self.start..self.pos])
                } else {
                    0
                };
                self.pos -= trim;
                if self.pos > self.start {
                    self.emit(ItemType::ItemText);
                }
                self.pos += trim;
                self.ignore();
                State::LexLeftDelim
            }
            None => {
                self.pos = self.input.len();
                if self.pos > self.start {
                    self.emit(ItemType::ItemText);
                }
                self.emit(ItemType::ItemEOF);
                State::End
            }
        }
    }

    /// Whether a trim-marked right delimiter begins at `at`, i.e. whitespace, the
    /// marker, then the delimiter. One definition so the delimiter scan and the
    /// whitespace run cannot disagree about what counts.
    fn at_right_trim_delim(&self, at: usize) -> bool {
        let rest = &self.input[at..];
        has_right_trim_marker(rest) && rest[TRIM_MARKER_LEN..].starts_with(RIGHT_DELIM)
    }

    fn at_right_delim(&mut self) -> (bool, bool) {
        if self.input[self.pos..].starts_with(RIGHT_DELIM) {
            return (true, false);
        }
        if self.at_right_trim_delim(self.pos) {
            return (true, true);
        }
        (false, false)
    }

    fn lex_left_delim(&mut self) -> State {
        self.pos += LEFT_DELIM.len();
        let trim = has_left_trim_marker(&self.input[self.pos..]);
        let after_marker = if trim { TRIM_MARKER_LEN } else { 0 };
        if self.input[(self.pos + after_marker)..].starts_with(LEFT_COMMENT) {
            self.pos += after_marker;
            self.ignore();
            State::LexComment
        } else {
            self.emit(ItemType::ItemLeftDelim);
            self.pos += after_marker;
            self.ignore();
            self.paren_depth = 0;
            State::LexInsideAction
        }
    }

    fn lex_comment(&mut self) -> State {
        self.pos += LEFT_COMMENT.len();
        let i = match self.input[self.pos..].find(RIGHT_COMMENT) {
            Some(i) => i,
            None => {
                return self.errorf("unclosed comment");
            }
        };

        self.pos += i + RIGHT_COMMENT.len();
        let (delim, trim) = self.at_right_delim();

        if !delim {
            return self.errorf("comment end before closing delimiter");
        }

        if trim {
            self.pos += TRIM_MARKER_LEN;
        }

        self.pos += RIGHT_DELIM.len();

        if trim {
            self.pos += ltrim_len(&self.input[self.pos..]);
        }

        self.ignore();
        State::LexText
    }

    fn lex_right_delim(&mut self) -> State {
        let trim = has_right_trim_marker(&self.input[self.pos..]);
        if trim {
            self.pos += TRIM_MARKER_LEN;
            self.ignore();
        }
        self.pos += RIGHT_DELIM.len();
        self.emit(ItemType::ItemRightDelim);
        if trim {
            self.pos += ltrim_len(&self.input[self.pos..]);
            self.ignore();
        }
        State::LexText
    }

    fn lex_inside_action(&mut self) -> State {
        let (delim, _) = self.at_right_delim();
        if delim {
            if self.paren_depth == 0 {
                return State::LexRightDelim;
            }
            return self.errorf("unclosed left paren");
        }

        match self.next() {
            None => self.errorf("unclosed action"),
            Some(c) => {
                match c {
                    '"' => State::LexQuote,
                    '`' => State::LexRawQuote,
                    '$' => State::LexVariable,
                    '\'' => State::LexChar,
                    '(' => {
                        self.emit(ItemType::ItemLeftParen);
                        self.paren_depth += 1;
                        State::LexInsideAction
                    }
                    ')' => {
                        self.emit(ItemType::ItemRightParen);
                        if self.paren_depth == 0 {
                            return self.errorf(&format!("unexpected right paren {}", c));
                        }
                        self.paren_depth -= 1;
                        State::LexInsideAction
                    }
                    '=' => {
                        self.emit(ItemType::ItemAssign);
                        State::LexInsideAction
                    }
                    ':' => match self.next() {
                        Some('=') => {
                            self.emit(ItemType::ItemColonEquals);
                            State::LexInsideAction
                        }
                        _ => self.errorf("expected :="),
                    },
                    '|' => {
                        self.emit(ItemType::ItemPipe);
                        State::LexInsideAction
                    }
                    '.' => match self.input[self.pos..].chars().next() {
                        Some('0'..='9') => {
                            self.backup();
                            State::LexNumber
                        }
                        _ => State::LexField,
                    },
                    '+' | '-' | '0'..='9' => {
                        self.backup();
                        State::LexNumber
                    }
                    _ if is_space(c) => State::LexSpace,
                    _ if c.is_alphanumeric() || c == '_' => {
                        self.backup();
                        State::LexIdentifier
                    }
                    _ if c.is_ascii() => {
                        // figure out a way to check for unicode.isPrint ?!
                        self.emit(ItemType::ItemChar);
                        State::LexInsideAction
                    }
                    _ => self.errorf(&format!("unrecognized character in action {}", c)),
                }
            }
        }
    }

    fn lex_space(&mut self) -> State {
        // The first whitespace character was already consumed by `lex_inside_action`,
        // which checked for a delimiter before doing so, so it cannot be the one that
        // belongs to a `-}}`.
        //
        // The whitespace in front of a trim-marked right delimiter is part of the
        // delimiter, so the run stops before it rather than swallowing it and leaving
        // the `-` to be lexed as a number.
        while !self.at_right_trim_delim(self.pos) && self.peek().map(is_space).unwrap_or(false) {
            self.next();
        }

        self.emit(ItemType::ItemSpace);
        State::LexInsideAction
    }

    fn lex_identifier(&mut self) -> State {
        let c = self.find(|c| !(c.is_alphanumeric() || *c == '_'));
        self.backup();
        if !self.at_terminator() {
            return self.errorf(&format!("bad character {}", c.unwrap_or_default()));
        }
        let item_type = match &self.input[self.start..self.pos] {
            "true" | "false" => ItemType::ItemBool,
            word if KEY.contains_key(word) => (*KEY.get(word).unwrap()).clone(),
            word if word.starts_with('.') => ItemType::ItemField,
            _ => ItemType::ItemIdentifier,
        };
        self.emit(item_type);
        State::LexInsideAction
    }

    fn lex_field(&mut self) -> State {
        self.lex_field_or_variable(ItemType::ItemField)
    }

    fn lex_variable(&mut self) -> State {
        self.lex_field_or_variable(ItemType::ItemVariable)
    }

    fn lex_field_or_variable(&mut self, typ: ItemType) -> State {
        if self.at_terminator() {
            self.emit(match typ {
                ItemType::ItemVariable => ItemType::ItemVariable,
                _ => ItemType::ItemDot,
            });
            return State::LexInsideAction;
        }
        let c = self.find(|c| !(c.is_alphanumeric() || *c == '_'));
        self.backup();

        if !self.at_terminator() {
            return self.errorf(&format!("bad character {}", c.unwrap_or_default()));
        }
        self.emit(typ);
        State::LexInsideAction
    }

    fn at_terminator(&mut self) -> bool {
        match self.peek() {
            Some(c) => {
                match c {
                    '.' | ',' | '|' | ':' | ')' | '(' | ' ' | '\t' | '\r' | '\n' => true,
                    // this is what golang does to detect a delimiter
                    _ => RIGHT_DELIM.starts_with(c),
                }
            }
            // Go counts end of input as a terminator, so an unclosed action is
            // reported as exactly that rather than as a bad character.
            None => true,
        }
    }

    fn lex_char(&mut self) -> State {
        let mut escaped = false;
        loop {
            let c = self.next();
            match c {
                Some('\\') => {
                    escaped = true;
                    continue;
                }
                Some('\n') | None => {
                    return self.errorf("unterminated character constant");
                }
                Some('\'') if !escaped => {
                    break;
                }
                _ => {}
            };
            escaped = false;
        }
        self.emit(ItemType::ItemCharConstant);
        State::LexInsideAction
    }

    fn lex_number(&mut self) -> State {
        if self.scan_number() {
            // Let's ingnore complex numbers here.
            self.emit(ItemType::ItemNumber);
            State::LexInsideAction
        } else {
            let msg = &format!("bad number syntax: {}", &self.input[self.start..self.pos]);
            self.errorf(msg)
        }
    }

    /// Scans the shape of a number without judging it. Go's integer literals allow
    /// a base prefix and `_` separators, so both are accepted here and validated in
    /// `NumberNode::new`, which is also where Go decides whether a literal is legal.
    fn scan_number(&mut self) -> bool {
        const DECIMAL: &str = "0123456789_";
        const HEXADECIMAL: &str = "0123456789abcdefABCDEF_";
        const OCTAL: &str = "01234567_";
        const BINARY: &str = "01_";

        self.accept("+-");

        let mut digits = DECIMAL;
        if self.accept("0") {
            // A leading zero is an octal prefix for an integer but not for a float,
            // so the decimal set stays until a prefix letter says otherwise.
            if self.accept("xX") {
                digits = HEXADECIMAL;
            } else if self.accept("oO") {
                digits = OCTAL;
            } else if self.accept("bB") {
                digits = BINARY;
            }
        }
        self.accept_run(digits);

        if self.accept(".") {
            self.accept_run(digits);
        }
        if digits == DECIMAL && self.accept("eE") {
            self.accept("+-");
            self.accept_run(DECIMAL);
        }
        // Go also takes a `p` exponent on a hexadecimal float. Scanning one here
        // without being able to evaluate it would only turn a clear lexer error into
        // a confusing parse error, so it stays unsupported on both sides.
        // Let's ignore imaginary numbers for now.
        //
        // End of input ends the number, as in Go: treating it as another character
        // reports a bad number where the real problem is the missing `}}`.
        if self.peek().map(|c| c.is_alphanumeric()).unwrap_or(false) {
            self.next();
            return false;
        }
        true
    }

    fn lex_quote(&mut self) -> State {
        let mut escaped = false;
        loop {
            let c = self.next();
            match c {
                Some('\\') => {
                    escaped = true;
                    continue;
                }
                Some('\n') | None => {
                    return self.errorf("unterminated quoted string");
                }
                Some('"') if !escaped => {
                    break;
                }
                _ => {}
            };
            escaped = false;
        }
        self.emit(ItemType::ItemString);
        State::LexInsideAction
    }

    fn lex_raw_quote(&mut self) -> State {
        if !self.any(|c| c == '`') {
            return self.errorf("unterminated raw quoted string");
        }
        self.emit(ItemType::ItemRawString);
        State::LexInsideAction
    }
}

/// Byte length of the trailing run of whitespace in `s`.
///
/// Measured from the *end* of the last non-space character. Measuring from its start
/// and adding one assumes every character is a single byte, which puts the trim
/// position inside a multi-byte character and panics the next slice.
fn rtrim_len(s: &str) -> usize {
    match s.char_indices().rev().find(|(_, c)| !is_space(*c)) {
        Some((index, c)) => s.len() - (index + c.len_utf8()),
        None => s.len(),
    }
}

/// Byte length of the leading run of whitespace in `s`.
fn ltrim_len(s: &str) -> usize {
    s.find(|c: char| !is_space(c)).unwrap_or(s.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexer_run() {
        let mut l = Lexer::new("abc".to_owned());
        let i1 = l.next().unwrap();
        assert_eq!(i1.typ, ItemType::ItemText);
        assert_eq!(&i1.val, "abc");
    }

    #[test]
    fn lex_simple() {
        let s = r#"something {{ if eq "foo" "bar" }}"#;
        let l = Lexer::new(s.to_owned());
        assert_eq!(l.count(), 13);
    }

    #[test]
    fn test_whitespace() {
        let s = r#"something {{  .foo  }}"#;
        let l = Lexer::new(s.to_owned());
        let s_ = l.map(|i| i.val).collect::<Vec<String>>().join("");
        assert_eq!(s_, s);
    }

    #[test]
    fn test_input() {
        let s = r#"something {{ .foo }}"#;
        let l = Lexer::new(s.to_owned());
        let s_ = l.map(|i| i.val).collect::<Vec<String>>().join("");
        assert_eq!(s_, s);
    }

    #[test]
    fn test_underscore() {
        let s = r#"something {{ .foo_bar }}"#;
        let l = Lexer::new(s.to_owned());
        let s_ = l.map(|i| i.val).collect::<Vec<String>>().join("");
        assert_eq!(s_, s);
    }

    #[test]
    fn test_trim() {
        let s = r#"something {{- .foo -}} 2000"#;
        let l = Lexer::new(s.to_owned());
        let s_ = l.map(|i| i.val).collect::<Vec<String>>().join("");
        assert_eq!(s_, r#"something{{.foo}}2000"#);
    }

    #[test]
    fn test_comment() {
        let s = r#"something {{- /* foo */ -}} 2000"#;
        let l = Lexer::new(s.to_owned());
        let s_ = l.map(|i| i.val).collect::<Vec<String>>().join("");
        assert_eq!(s_, r#"something2000"#);
    }
}
