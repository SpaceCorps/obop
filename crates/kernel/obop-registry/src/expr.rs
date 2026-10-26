//! The expression language of computed fields, action assertions and effect conditions
//! (D56, D57, D72). It is deliberately tiny and total: integers, exact decimals, text and
//! booleans, five arithmetic operators, comparisons, `if`, `concat`, `is_set` and four aggregates
//! over child records. There are no floats, no loops, no function definitions and no way to call
//! anything. An expression is parsed once, type-checked against the model it belongs to, and then
//! evaluated over data it is handed; evaluation cannot fail except by dividing by zero or
//! overflowing 64 bits (128 bits and 38 digits for decimals).
//!
//! ```text
//! or     := and ( "||" and )*
//! and    := not ( "&&" not )*
//! not    := "!" not | cmp
//! cmp    := add ( ("==" | "!=" | "<" | "<=" | ">" | ">=") add )?
//! add    := mul ( ("+" | "-") mul )*
//! mul    := unary ( ("*" | "/") unary )*
//! unary  := "-" unary | primary
//! primary:= INTEGER | DECIMAL | "text" | true | false | field | "(" or ")"
//!         | if(or, or, or) | concat(or, ...) | is_set(field)
//!         | sum(child.field) | min(child.field) | max(child.field) | count(child)
//! child  := name [ "[" ref_field "]" ]
//! ```
//!
//! Rules that are the same everywhere: an empty (null) value is `0`, `""` or `false` according to
//! the other operand; integer `/` rounds half away from zero; `min` and `max` of no children are
//! empty; `sum` and `count` of no children are `0`.
//!
//! *Decimals.* A literal is digits, a point and digits (`0.25`, `12.50`); a `decimal` field is a
//! decimal. A whole number mixed with a decimal becomes a decimal, so `qty * 0.25` and `rate > 1`
//! work, and `0.1 + 0.2 == 0.3` is true: `+`, `-` and `*` are exact (the scale of a sum is the
//! larger, of a product the sum of the two) and comparison is numeric. `/` of decimals is the one
//! operation that has to cut digits: its result has the scale of the computed field being
//! evaluated (12 in an assertion) and is rounded once with that field's `round` mode (default
//! `half_even`). The value of a computed `decimal` field is then rounded to the field's scale the
//! same way. A decimal does not mix with money or number fields in a computed field of that kind:
//! a decimal expression needs a `decimal` result.
//!
//! *Instants.* A `datetime` field is an instant. Instants compare with instants and with a text
//! literal that is a valid RFC 3339 instant (`created_at < "2026-01-01T00:00:00Z"`); they do not
//! compare with dates or other text, and have no arithmetic.

use crate::{FieldKind, ModelDef};
use obop_values::{Decimal, Instant, Round};
use std::sync::Arc;

/// One aggregate of an expression, as written.
#[derive(Debug, Clone, Copy)]
pub struct AggRef<'a> {
    pub func: AggFn,
    pub child: &'a str,
    pub via: Option<&'a str>,
    pub field: Option<&'a str>,
    pub filter: Option<&'a Expr>,
}

/// The source of an expression is at most this many characters.
pub const MAX_LEN: usize = 400;
const MAX_NODES: usize = 80;
const MAX_DEPTH: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ty {
    Int,
    /// An exact decimal (D72).
    Dec,
    /// An instant (D72).
    Instant,
    Text,
    Bool,
}

impl Ty {
    pub fn name(self) -> &'static str {
        match self {
            Ty::Int => "a number",
            Ty::Dec => "a decimal number",
            Ty::Instant => "a date and time",
            Ty::Text => "text",
            Ty::Bool => "a true/false value",
        }
    }

    /// The expression type of a stored field kind.
    pub fn of_kind(kind: &FieldKind) -> Ty {
        match kind {
            FieldKind::Number | FieldKind::Money => Ty::Int,
            FieldKind::Decimal { .. } => Ty::Dec,
            FieldKind::Datetime => Ty::Instant,
            FieldKind::Bool => Ty::Bool,
            _ => Ty::Text,
        }
    }

    /// Whole numbers and decimals: the types arithmetic applies to.
    fn numeric(self) -> bool {
        matches!(self, Ty::Int | Ty::Dec)
    }
}

/// What a decimal division produces and how it rounds: the scale and mode of the field being
/// computed (12 and `half_even` when the expression is not a field's, such as an assertion).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecCtx {
    pub scale: u8,
    pub round: Round,
}

impl Default for DecCtx {
    fn default() -> Self {
        Self {
            scale: obop_values::MAX_SCALE,
            round: Round::HalfEven,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggFn {
    Sum,
    Min,
    Max,
    Count,
}

impl AggFn {
    pub fn name(self) -> &'static str {
        match self {
            AggFn::Sum => "sum",
            AggFn::Min => "min",
            AggFn::Max => "max",
            AggFn::Count => "count",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Int(i64),
    Dec(Decimal),
    Text(String),
    Bool(bool),
    /// A field of the record the expression belongs to.
    Field(String),
    IsSet(String),
    /// `child` is written as in the source: a model name, or a suffix of one (see
    /// `resolve_child`).
    Agg {
        func: AggFn,
        child: String,
        via: Option<String>,
        field: Option<String>,
        /// `where <condition on the child>`: only children for which it holds count.
        filter: Option<Box<Expr>>,
    },
    Neg(Box<Expr>),
    Not(Box<Expr>),
    Bin(BinOp, Box<Expr>, Box<Expr>),
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    Concat(Vec<Expr>),
}

// ---- lexing and parsing -------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    Int(i64),
    Dec(Decimal),
    Str(String),
    Ident(String),
    Op(&'static str),
}

fn lex(src: &str) -> Result<Vec<(Tok, usize)>, String> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let at = i;
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() {
            let mut n: i64 = 0;
            let mut overflow = false;
            while i < chars.len() && chars[i].is_ascii_digit() {
                match n
                    .checked_mul(10)
                    .and_then(|n| n.checked_add(i64::from(chars[i] as u8 - b'0')))
                {
                    Some(m) => n = m,
                    None => overflow = true,
                }
                i += 1;
            }
            // `12.50`: a decimal literal is digits, a point and at least one digit.
            if i + 1 < chars.len() && chars[i] == '.' && chars[i + 1].is_ascii_digit() {
                i += 1;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                let text: String = chars[at..i].iter().collect();
                if i < chars.len() && (chars[i].is_ascii_alphabetic() || chars[i] == '_') {
                    return Err(format!(
                        "a number cannot be followed by letters (no units) at position {}",
                        at + 1
                    ));
                }
                let d = Decimal::parse(&text).map_err(|e| {
                    format!(
                        "the decimal {text} at position {} is not valid: {e}",
                        at + 1
                    )
                })?;
                out.push((Tok::Dec(d), at));
                continue;
            }
            if overflow {
                return Err(format!("number too large at position {}", at + 1));
            }
            if i < chars.len()
                && (chars[i].is_ascii_alphabetic() || chars[i] == '.' || chars[i] == '_')
            {
                return Err(format!(
                    "a number cannot be followed by letters or a lone point (write 1.5, not 1. or 1x) at position {}",
                    at + 1
                ));
            }
            out.push((Tok::Int(n), at));
        } else if c == '"' {
            let mut s = String::new();
            i += 1;
            loop {
                match chars.get(i) {
                    None => {
                        return Err(format!("unterminated text starting at position {}", at + 1));
                    }
                    Some('"') => {
                        i += 1;
                        break;
                    }
                    Some('\\') => match chars.get(i + 1) {
                        Some('"') => {
                            s.push('"');
                            i += 2;
                        }
                        Some('\\') => {
                            s.push('\\');
                            i += 2;
                        }
                        _ => {
                            return Err(format!(
                                "only \\\" and \\\\ may be escaped (position {})",
                                i + 1
                            ));
                        }
                    },
                    Some(ch) if ch.is_control() => {
                        return Err(format!("control character in text at position {}", i + 1));
                    }
                    Some(ch) => {
                        s.push(*ch);
                        i += 1;
                    }
                }
            }
            out.push((Tok::Str(s), at));
        } else if c.is_ascii_lowercase() || c == '_' {
            let mut s = String::new();
            while i < chars.len()
                && (chars[i].is_ascii_lowercase() || chars[i].is_ascii_digit() || chars[i] == '_')
            {
                s.push(chars[i]);
                i += 1;
            }
            out.push((Tok::Ident(s), at));
        } else {
            let two: String = chars[i..chars.len().min(i + 2)].iter().collect();
            let op: &'static str = match two.as_str() {
                "==" => "==",
                "!=" => "!=",
                "<=" => "<=",
                ">=" => ">=",
                "&&" => "&&",
                "||" => "||",
                _ => match c {
                    '+' => "+",
                    '-' => "-",
                    '*' => "*",
                    '/' => "/",
                    '<' => "<",
                    '>' => ">",
                    '!' => "!",
                    '(' => "(",
                    ')' => ")",
                    '[' => "[",
                    ']' => "]",
                    ',' => ",",
                    '.' => ".",
                    _ => return Err(format!("unexpected {c:?} at position {}", at + 1)),
                },
            };
            i += op.len();
            out.push((Tok::Op(op), at));
        }
    }
    Ok(out)
}

struct Parser {
    toks: Vec<(Tok, usize)>,
    pos: usize,
    nodes: usize,
    depth: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|(t, _)| t)
    }
    fn here(&self) -> usize {
        self.toks.get(self.pos).map_or(usize::MAX, |(_, p)| *p + 1)
    }
    fn eat_op(&mut self, op: &str) -> bool {
        if matches!(self.peek(), Some(Tok::Op(o)) if *o == op) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn expect_op(&mut self, op: &str) -> Result<(), String> {
        if self.eat_op(op) {
            Ok(())
        } else {
            Err(self.unexpected(&format!("`{op}`")))
        }
    }
    fn unexpected(&self, wanted: &str) -> String {
        match self.toks.get(self.pos) {
            None => format!("the expression ends where {wanted} was expected"),
            Some((t, p)) => format!("expected {wanted} at position {}, found {}", p + 1, show(t)),
        }
    }
    fn ident(&mut self) -> Result<String, String> {
        match self.peek() {
            Some(Tok::Ident(s)) => {
                let s = s.clone();
                self.pos += 1;
                Ok(s)
            }
            _ => Err(self.unexpected("a name")),
        }
    }
    fn node(&mut self) -> Result<(), String> {
        self.nodes += 1;
        if self.nodes > MAX_NODES {
            return Err(format!(
                "the expression is too large (at most {MAX_NODES} parts)"
            ));
        }
        Ok(())
    }
    fn enter(&mut self) -> Result<(), String> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(format!(
                "the expression is nested too deeply (at most {MAX_DEPTH})"
            ));
        }
        Ok(())
    }

    fn or(&mut self) -> Result<Expr, String> {
        self.enter()?;
        let mut l = self.and()?;
        while self.eat_op("||") {
            self.node()?;
            let r = self.and()?;
            l = Expr::Bin(BinOp::Or, Box::new(l), Box::new(r));
        }
        self.depth -= 1;
        Ok(l)
    }
    fn and(&mut self) -> Result<Expr, String> {
        let mut l = self.not()?;
        while self.eat_op("&&") {
            self.node()?;
            let r = self.not()?;
            l = Expr::Bin(BinOp::And, Box::new(l), Box::new(r));
        }
        Ok(l)
    }
    fn not(&mut self) -> Result<Expr, String> {
        if self.eat_op("!") {
            self.node()?;
            self.enter()?;
            let e = self.not()?;
            self.depth -= 1;
            return Ok(Expr::Not(Box::new(e)));
        }
        self.cmp()
    }
    fn cmp(&mut self) -> Result<Expr, String> {
        let l = self.add()?;
        for (s, op) in [
            ("==", BinOp::Eq),
            ("!=", BinOp::Ne),
            ("<=", BinOp::Le),
            (">=", BinOp::Ge),
            ("<", BinOp::Lt),
            (">", BinOp::Gt),
        ] {
            if self.eat_op(s) {
                self.node()?;
                let r = self.add()?;
                return Ok(Expr::Bin(op, Box::new(l), Box::new(r)));
            }
        }
        Ok(l)
    }
    fn add(&mut self) -> Result<Expr, String> {
        let mut l = self.mul()?;
        loop {
            let op = if self.eat_op("+") {
                BinOp::Add
            } else if self.eat_op("-") {
                BinOp::Sub
            } else {
                return Ok(l);
            };
            self.node()?;
            let r = self.mul()?;
            l = Expr::Bin(op, Box::new(l), Box::new(r));
        }
    }
    fn mul(&mut self) -> Result<Expr, String> {
        let mut l = self.unary()?;
        loop {
            let op = if self.eat_op("*") {
                BinOp::Mul
            } else if self.eat_op("/") {
                BinOp::Div
            } else {
                return Ok(l);
            };
            self.node()?;
            let r = self.unary()?;
            l = Expr::Bin(op, Box::new(l), Box::new(r));
        }
    }
    fn unary(&mut self) -> Result<Expr, String> {
        if self.eat_op("-") {
            self.node()?;
            self.enter()?;
            let e = self.unary()?;
            self.depth -= 1;
            return Ok(Expr::Neg(Box::new(e)));
        }
        self.primary()
    }
    fn primary(&mut self) -> Result<Expr, String> {
        self.node()?;
        match self.peek().cloned() {
            Some(Tok::Int(n)) => {
                self.pos += 1;
                Ok(Expr::Int(n))
            }
            Some(Tok::Dec(d)) => {
                self.pos += 1;
                Ok(Expr::Dec(d))
            }
            Some(Tok::Str(s)) => {
                self.pos += 1;
                Ok(Expr::Text(s))
            }
            Some(Tok::Op("(")) => {
                self.pos += 1;
                let e = self.or()?;
                self.expect_op(")")?;
                Ok(e)
            }
            Some(Tok::Ident(name)) => {
                self.pos += 1;
                match name.as_str() {
                    "true" => Ok(Expr::Bool(true)),
                    "false" => Ok(Expr::Bool(false)),
                    "sum" | "min" | "max" | "count"
                        if matches!(self.peek(), Some(Tok::Op("("))) =>
                    {
                        self.pos += 1;
                        let func = match name.as_str() {
                            "sum" => AggFn::Sum,
                            "min" => AggFn::Min,
                            "max" => AggFn::Max,
                            _ => AggFn::Count,
                        };
                        let child = self.ident()?;
                        let via = if self.eat_op("[") {
                            let v = self.ident()?;
                            self.expect_op("]")?;
                            Some(v)
                        } else {
                            None
                        };
                        let field = if func == AggFn::Count {
                            None
                        } else {
                            self.expect_op(".")?;
                            Some(self.ident()?)
                        };
                        let filter = if matches!(self.peek(), Some(Tok::Ident(w)) if w == "where") {
                            self.pos += 1;
                            Some(Box::new(self.or()?))
                        } else {
                            None
                        };
                        self.expect_op(")")?;
                        Ok(Expr::Agg {
                            func,
                            child,
                            via,
                            field,
                            filter,
                        })
                    }
                    "is_set" if matches!(self.peek(), Some(Tok::Op("("))) => {
                        self.pos += 1;
                        let f = self.ident()?;
                        self.expect_op(")")?;
                        Ok(Expr::IsSet(f))
                    }
                    "if" if matches!(self.peek(), Some(Tok::Op("("))) => {
                        self.pos += 1;
                        let c = self.or()?;
                        self.expect_op(",")?;
                        let a = self.or()?;
                        self.expect_op(",")?;
                        let b = self.or()?;
                        self.expect_op(")")?;
                        Ok(Expr::If(Box::new(c), Box::new(a), Box::new(b)))
                    }
                    "concat" if matches!(self.peek(), Some(Tok::Op("("))) => {
                        self.pos += 1;
                        let mut parts = vec![self.or()?];
                        while self.eat_op(",") {
                            parts.push(self.or()?);
                        }
                        self.expect_op(")")?;
                        Ok(Expr::Concat(parts))
                    }
                    _ => {
                        if matches!(self.peek(), Some(Tok::Op("("))) {
                            return Err(format!(
                                "unknown function `{name}` (the functions are sum, min, max, count, if, concat and is_set)"
                            ));
                        }
                        Ok(Expr::Field(name))
                    }
                }
            }
            _ => Err(self.unexpected("a value")),
        }
    }
}

fn show(t: &Tok) -> String {
    match t {
        Tok::Int(n) => n.to_string(),
        Tok::Dec(d) => d.to_string(),
        Tok::Str(s) => format!("{s:?}"),
        Tok::Ident(s) => format!("`{s}`"),
        Tok::Op(o) => format!("`{o}`"),
    }
}

impl Expr {
    /// Parse the source text. Syntax only; `check` does the types.
    pub fn parse(src: &str) -> Result<Expr, String> {
        if src.trim().is_empty() {
            return Err("the expression is empty".into());
        }
        if src.chars().count() > MAX_LEN {
            return Err(format!(
                "the expression is longer than {MAX_LEN} characters"
            ));
        }
        let toks = lex(src)?;
        let mut p = Parser {
            toks,
            pos: 0,
            nodes: 0,
            depth: 0,
        };
        let e = p.or()?;
        if p.pos < p.toks.len() {
            return Err(p.unexpected("the end of the expression"));
        }
        let _ = p.here();
        Ok(e)
    }

    /// Fields of the record this expression reads, in order of appearance.
    pub fn fields(&self) -> Vec<&str> {
        let mut v = Vec::new();
        self.walk(&mut |e| match e {
            Expr::Field(f) | Expr::IsSet(f) => v.push(f.as_str()),
            _ => {}
        });
        v
    }

    /// The aggregates of the expression.
    pub fn aggregates(&self) -> Vec<AggRef<'_>> {
        let mut v = Vec::new();
        self.walk(&mut |e| {
            if let Expr::Agg {
                func,
                child,
                via,
                field,
                filter,
            } = e
            {
                v.push(AggRef {
                    func: *func,
                    child: child.as_str(),
                    via: via.as_deref(),
                    field: field.as_deref(),
                    filter: filter.as_deref(),
                });
            }
        });
        v
    }

    fn walk<'a>(&'a self, f: &mut dyn FnMut(&'a Expr)) {
        f(self);
        match self {
            Expr::Neg(a) | Expr::Not(a) => a.walk(f),
            Expr::Bin(_, a, b) => {
                a.walk(f);
                b.walk(f);
            }
            Expr::If(a, b, c) => {
                a.walk(f);
                b.walk(f);
                c.walk(f);
            }
            Expr::Concat(v) => v.iter().for_each(|e| e.walk(f)),
            _ => {}
        }
    }
}

// ---- resolving children and type checking -------------------------------------------------

/// How to find the child model an aggregate names. `name` is a model of the registry, or the part
/// after this model's own name, singular or plural: `lines` in `accounting_move` is
/// `accounting_move_line`. The child must have a `ref` to this model; when it has several, the
/// expression says which with `lines[move_id]`.
pub fn resolve_child(
    this: &ModelDef,
    name: &str,
    via: Option<&str>,
    lookup: &dyn Fn(&str) -> Option<Arc<ModelDef>>,
) -> Result<(Arc<ModelDef>, &'static str), String> {
    let mut candidates = vec![name.to_string(), format!("{}_{name}", this.name)];
    if let Some(s) = name.strip_suffix('s') {
        candidates.push(format!("{}_{s}", this.name));
        candidates.push(s.to_string());
    }
    let Some(child) = candidates.iter().find_map(|c| lookup(c)) else {
        return Err(format!(
            "`{name}` is not a model that refers to {} (tried {})",
            this.name,
            candidates.join(", ")
        ));
    };
    let refs: Vec<&'static str> = child
        .fields
        .iter()
        .filter(|f| f.kind.target() == Some(this.name))
        .map(|f| f.name)
        .collect();
    match (via, refs.as_slice()) {
        (Some(v), _) => match refs.iter().find(|r| **r == v) {
            Some(r) => Ok((child, r)),
            None => Err(format!("{}.{v} is not a ref to {}", child.name, this.name)),
        },
        (None, []) => Err(format!(
            "{} has no ref field to {}, so it has no records \"of\" one {}",
            child.name, this.name, this.name
        )),
        (None, [one]) => Ok((child.clone(), one)),
        (None, many) => Err(format!(
            "{} has several refs to {} ({}); say which: {name}[{}]",
            child.name,
            this.name,
            many.join(", "),
            many[0]
        )),
    }
}

/// Everything the checker needs to know about the world around an expression.
pub struct Scope<'a> {
    pub model: &'a ModelDef,
    pub lookup: &'a dyn Fn(&str) -> Option<Arc<ModelDef>>,
    /// When true, field references to `computed` fields are allowed (they are inputs like any
    /// other). Assertions and conditions allow it too.
    pub allow_computed: bool,
}

impl Scope<'_> {
    fn field_ty(&self, name: &str) -> Result<Ty, String> {
        match self.model.fields.iter().find(|f| f.name == name) {
            Some(f) => Ok(Ty::of_kind(&f.kind)),
            None => Err(format!("{} has no field `{name}`", self.model.name)),
        }
    }
}

/// Whether two types may be compared: equal types, a number with a decimal, or an instant with a
/// text literal that is a valid instant (the only text an instant compares with).
fn comparable(ta: Ty, tb: Ty, a: &Expr, b: &Expr) -> bool {
    let instant_text = |t: Ty, other: &Expr| {
        t == Ty::Instant && matches!(other, Expr::Text(s) if Instant::parse(s).is_ok())
    };
    ta == tb
        || (ta.numeric() && tb.numeric())
        || (instant_text(ta, b) && tb == Ty::Text)
        || (instant_text(tb, a) && ta == Ty::Text)
}

/// Type-check `e` in `scope` and return its type.
pub fn check(e: &Expr, scope: &Scope<'_>) -> Result<Ty, String> {
    let want = |t: Ty, got: Ty, what: &str| -> Result<(), String> {
        if t == got {
            Ok(())
        } else {
            Err(format!("{what} must be {}, not {}", t.name(), got.name()))
        }
    };
    let number = |got: Ty, what: &str| -> Result<(), String> {
        if got.numeric() {
            Ok(())
        } else {
            Err(format!(
                "{what} must be a number, not {} (decimals are numbers too)",
                got.name()
            ))
        }
    };
    match e {
        Expr::Int(_) => Ok(Ty::Int),
        Expr::Dec(_) => Ok(Ty::Dec),
        Expr::Text(_) => Ok(Ty::Text),
        Expr::Bool(_) => Ok(Ty::Bool),
        Expr::Field(f) => scope.field_ty(f),
        Expr::IsSet(f) => scope.field_ty(f).map(|_| Ty::Bool),
        Expr::Neg(a) => {
            let t = check(a, scope)?;
            number(t, "the operand of `-`")?;
            Ok(t)
        }
        Expr::Not(a) => {
            want(Ty::Bool, check(a, scope)?, "the operand of `!`")?;
            Ok(Ty::Bool)
        }
        Expr::Bin(op, a, b) => {
            let (ta, tb) = (check(a, scope)?, check(b, scope)?);
            match op {
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
                    number(ta, "the left side of an arithmetic operator")?;
                    number(tb, "the right side of an arithmetic operator")?;
                    Ok(if ta == Ty::Dec || tb == Ty::Dec {
                        Ty::Dec
                    } else {
                        Ty::Int
                    })
                }
                BinOp::And | BinOp::Or => {
                    want(Ty::Bool, ta, "the left side of `&&`/`||`")?;
                    want(Ty::Bool, tb, "the right side of `&&`/`||`")?;
                    Ok(Ty::Bool)
                }
                BinOp::Eq | BinOp::Ne => {
                    if !comparable(ta, tb, a, b) {
                        return Err(format!("cannot compare {} with {}", ta.name(), tb.name()));
                    }
                    Ok(Ty::Bool)
                }
                BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                    if !comparable(ta, tb, a, b) || ta == Ty::Bool {
                        return Err(format!("cannot order {} against {}", ta.name(), tb.name()));
                    }
                    Ok(Ty::Bool)
                }
            }
        }
        Expr::If(c, a, b) => {
            want(Ty::Bool, check(c, scope)?, "the condition of `if`")?;
            let (ta, tb) = (check(a, scope)?, check(b, scope)?);
            if ta != tb {
                return Err(format!(
                    "both branches of `if` must have the same type, not {} and {}",
                    ta.name(),
                    tb.name()
                ));
            }
            Ok(ta)
        }
        Expr::Concat(parts) => {
            for p in parts {
                check(p, scope)?;
            }
            Ok(Ty::Text)
        }
        Expr::Agg {
            func,
            child,
            via,
            field,
            filter,
        } => {
            let (cm, _) = resolve_child(scope.model, child, via.as_deref(), scope.lookup)?;
            if let Some(w) = filter {
                let inner = Scope {
                    model: &cm,
                    lookup: scope.lookup,
                    allow_computed: scope.allow_computed,
                };
                if check(w, &inner).map_err(|e| format!("where: {e}"))? != Ty::Bool {
                    return Err("where must be a comparison or other true/false expression".into());
                }
            }
            let mut ty = Ty::Int;
            if let Some(f) = field {
                let Some(def) = cm.fields.iter().find(|d| d.name == f) else {
                    return Err(format!("{} has no field `{f}`", cm.name));
                };
                ty = Ty::of_kind(&def.kind);
                if !ty.numeric() {
                    return Err(format!(
                        "{}() needs a number, money or decimal field; {}.{f} is {}",
                        func.name(),
                        cm.name,
                        def.kind.name()
                    ));
                }
            }
            Ok(ty)
        }
    }
}

// ---- evaluation ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum V {
    Null,
    Int(i64),
    /// An exact decimal (D72).
    Dec(Decimal),
    /// An instant, microseconds since the epoch (D72).
    Instant(i64),
    Text(String),
    Bool(bool),
}

impl V {
    fn int(&self) -> i64 {
        match self {
            V::Int(n) => *n,
            V::Bool(b) => i64::from(*b),
            _ => 0,
        }
    }
    fn truthy(&self) -> bool {
        match self {
            V::Bool(b) => *b,
            V::Int(n) => *n != 0,
            _ => false,
        }
    }
    fn text(&self) -> String {
        match self {
            V::Null => String::new(),
            V::Int(n) => n.to_string(),
            V::Dec(d) => d.to_string(),
            V::Instant(us) => Instant::from_micros(*us)
                .map(Instant::canonical)
                .unwrap_or_default(),
            V::Text(s) => s.clone(),
            V::Bool(b) => b.to_string(),
        }
    }
    fn is_dec(&self) -> bool {
        matches!(self, V::Dec(_))
    }
    /// As a decimal: a whole number is a decimal of scale 0, empty is zero. Text is accepted when
    /// it is a decimal in canonical form (a stored decimal read without its kind).
    fn dec(&self) -> Result<Decimal, String> {
        match self {
            V::Dec(d) => Ok(*d),
            V::Int(n) => Ok(Decimal::from_i64(*n)),
            V::Null => Ok(Decimal::zero(0)),
            V::Bool(b) => Ok(Decimal::from_i64(i64::from(*b))),
            V::Text(s) => Decimal::parse(s).map_err(|e| format!("{s:?} is not a decimal: {e}")),
            V::Instant(_) => Err("an instant is not a number".into()),
        }
    }
    /// As microseconds; empty is before everything.
    fn instant(&self) -> Result<i64, String> {
        match self {
            V::Instant(us) => Ok(*us),
            V::Null => Ok(i64::MIN),
            V::Text(s) => Instant::parse(s)
                .map(Instant::micros)
                .map_err(|e| format!("{s:?} is not an instant: {e}")),
            _ => Err("that is not an instant".into()),
        }
    }
}

/// What an expression reads. Implemented by the command layer over the store.
pub trait Env {
    fn field(&self, name: &str) -> Result<V, String>;
    /// `child` and `via` as written in the expression; the implementation resolves them with
    /// `resolve_child`, exactly as the checker did.
    fn aggregate(
        &self,
        func: AggFn,
        child: &str,
        via: Option<&str>,
        field: Option<&str>,
        filter: Option<&Expr>,
    ) -> Result<V, String>;
}

fn big(n: i128) -> Result<i64, String> {
    i64::try_from(n).map_err(|_| "the result does not fit in 64 bits".to_string())
}

/// Integer division rounded half away from zero.
pub fn div_round(n: i64, d: i64) -> Result<i64, String> {
    if d == 0 {
        return Err("division by zero".into());
    }
    let (n, d) = (i128::from(n), i128::from(d));
    let sign = if (n < 0) != (d < 0) { -1 } else { 1 };
    big(sign * ((n.abs() * 2 + d.abs()) / (2 * d.abs())))
}

/// Evaluate with the default decimal context (an assertion or condition).
pub fn eval(e: &Expr, env: &dyn Env) -> Result<V, String> {
    eval_in(e, env, DecCtx::default())
}

/// Evaluate; a decimal division produces `ctx.scale` digits after the point, rounded by
/// `ctx.round`.
pub fn eval_in(e: &Expr, env: &dyn Env, ctx: DecCtx) -> Result<V, String> {
    let ev = |e: &Expr| eval_in(e, env, ctx);
    Ok(match e {
        Expr::Int(n) => V::Int(*n),
        Expr::Dec(d) => V::Dec(*d),
        Expr::Text(s) => V::Text(s.clone()),
        Expr::Bool(b) => V::Bool(*b),
        Expr::Field(f) => env.field(f)?,
        Expr::IsSet(f) => V::Bool(match env.field(f)? {
            V::Null => false,
            V::Text(s) => !s.is_empty(),
            _ => true,
        }),
        Expr::Neg(a) => match ev(a)? {
            V::Dec(d) => V::Dec(d.negated()),
            v => V::Int(big(-i128::from(v.int()))?),
        },
        Expr::Not(a) => V::Bool(!ev(a)?.truthy()),
        Expr::Bin(op, a, b) => {
            // `&&` and `||` stop early, so a guard such as `is_set(x) && ...` is meaningful.
            if *op == BinOp::And {
                return Ok(V::Bool(ev(a)?.truthy() && ev(b)?.truthy()));
            }
            if *op == BinOp::Or {
                return Ok(V::Bool(ev(a)?.truthy() || ev(b)?.truthy()));
            }
            let (x, y) = (ev(a)?, ev(b)?);
            let arithmetic = matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div);
            if arithmetic && (x.is_dec() || y.is_dec()) {
                let (p, q) = (x.dec()?, y.dec()?);
                let r = match op {
                    BinOp::Add => p.checked_add(q),
                    BinOp::Sub => p.checked_sub(q),
                    BinOp::Mul => p.checked_mul(q),
                    _ => p.checked_div(q, ctx.scale, ctx.round),
                };
                return r.map(V::Dec).map_err(|e| e.to_string());
            }
            match op {
                BinOp::Add => V::Int(big(i128::from(x.int()) + i128::from(y.int()))?),
                BinOp::Sub => V::Int(big(i128::from(x.int()) - i128::from(y.int()))?),
                BinOp::Mul => V::Int(big(i128::from(x.int()) * i128::from(y.int()))?),
                BinOp::Div => V::Int(div_round(x.int(), y.int())?),
                _ => {
                    let ord = if x.is_dec() || y.is_dec() {
                        x.dec()?.cmp(&y.dec()?)
                    } else if matches!(x, V::Instant(_)) || matches!(y, V::Instant(_)) {
                        x.instant()?.cmp(&y.instant()?)
                    } else {
                        match (&x, &y) {
                            (V::Text(_), _) | (_, V::Text(_)) => x.text().cmp(&y.text()),
                            _ => x.int().cmp(&y.int()),
                        }
                    };
                    V::Bool(match op {
                        BinOp::Eq => ord.is_eq(),
                        BinOp::Ne => ord.is_ne(),
                        BinOp::Lt => ord.is_lt(),
                        BinOp::Le => ord.is_le(),
                        BinOp::Gt => ord.is_gt(),
                        _ => ord.is_ge(),
                    })
                }
            }
        }
        Expr::If(c, a, b) => {
            if ev(c)?.truthy() {
                ev(a)?
            } else {
                ev(b)?
            }
        }
        Expr::Concat(parts) => {
            let mut s = String::new();
            for p in parts {
                s.push_str(&ev(p)?.text());
            }
            V::Text(s)
        }
        Expr::Agg {
            func,
            child,
            via,
            field,
            filter,
        } => env.aggregate(
            *func,
            child,
            via.as_deref(),
            field.as_deref(),
            filter.as_deref(),
        )?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CommandDef, FieldDef};
    use obop_types::DataClass;
    use std::collections::HashMap;

    struct Rec(HashMap<&'static str, V>, Vec<i64>);
    impl Env for Rec {
        fn field(&self, name: &str) -> Result<V, String> {
            Ok(self.0.get(name).cloned().unwrap_or(V::Null))
        }
        fn aggregate(
            &self,
            func: AggFn,
            _: &str,
            _: Option<&str>,
            _: Option<&str>,
            _: Option<&Expr>,
        ) -> Result<V, String> {
            let v = &self.1;
            Ok(match func {
                AggFn::Sum => V::Int(v.iter().sum()),
                AggFn::Count => V::Int(v.len() as i64),
                AggFn::Min => v.iter().min().map_or(V::Null, |n| V::Int(*n)),
                AggFn::Max => v.iter().max().map_or(V::Null, |n| V::Int(*n)),
            })
        }
    }

    fn run(src: &str, rec: &Rec) -> Result<V, String> {
        eval(&Expr::parse(src)?, rec)
    }

    fn rec() -> Rec {
        Rec(
            HashMap::from([
                ("qty", V::Int(3)),
                ("price", V::Int(1999)),
                ("name", V::Text("Acme".into())),
                ("flag", V::Bool(true)),
            ]),
            vec![5, 10, 20],
        )
    }

    #[test]
    fn arithmetic_comparison_and_precedence() {
        let r = rec();
        assert_eq!(run("qty * price", &r), Ok(V::Int(5997)));
        assert_eq!(run("1 + 2 * 3 - 4", &r), Ok(V::Int(3)));
        assert_eq!(run("-(qty - 10)", &r), Ok(V::Int(7)));
        assert_eq!(run("qty * 100 / 8", &r), Ok(V::Int(38))); // 37.5 rounds away from zero
        assert_eq!(run("-qty * 100 / 8", &r), Ok(V::Int(-38)));
        assert_eq!(run("7 / 3", &r), Ok(V::Int(2)));
        assert_eq!(run("8 / 3", &r), Ok(V::Int(3)));
        assert_eq!(run("qty >= 3 && name == \"Acme\"", &r), Ok(V::Bool(true)));
        assert_eq!(run("!flag || qty < 2", &r), Ok(V::Bool(false)));
        assert_eq!(run("missing + 1", &r), Ok(V::Int(1)), "null is zero");
        assert_eq!(run("missing == 0", &r), Ok(V::Bool(true)));
        assert_eq!(run("missing == \"\"", &r), Ok(V::Bool(true)));
    }

    fn dec(s: &str) -> V {
        V::Dec(Decimal::parse(s).unwrap())
    }

    #[test]
    fn decimals_are_exact() {
        let mut r = rec();
        r.0.insert("rate", dec("0.075"));
        r.0.insert("net", dec("19.99"));
        r.0.insert("seen_at", V::Instant(1_000_000));
        // The classic.
        assert_eq!(run("0.1 + 0.2", &r), Ok(dec("0.3")));
        assert_eq!(run("0.1 + 0.2 == 0.3", &r), Ok(V::Bool(true)));
        assert_eq!(run("0.30 == 0.3", &r), Ok(V::Bool(true)));
        // A whole number mixes in; the scale of a sum is the larger, of a product the sum.
        assert_eq!(run("net * qty", &r), Ok(dec("59.97")));
        assert_eq!(run("net * rate", &r), Ok(dec("1.49925")));
        assert_eq!(run("net + 1", &r), Ok(dec("20.99")));
        assert_eq!(run("-net", &r), Ok(dec("-19.99")));
        assert_eq!(run("net - 20", &r), Ok(dec("-0.01")));
        // Order is numeric: "9.5" < "10.25".
        assert_eq!(run("9.5 < 10.25", &r), Ok(V::Bool(true)));
        assert_eq!(run("net > 19.9 && net < 20", &r), Ok(V::Bool(true)));
        assert_eq!(run("rate > 0", &r), Ok(V::Bool(true)));
        assert_eq!(run("missing * 2.5", &r), Ok(dec("0.0")), "null is zero");
        // Division uses the context: 12 digits and half_even unless a field says otherwise.
        assert_eq!(run("1 / 3.0", &r), Ok(dec("0.333333333333")));
        let ctx = DecCtx {
            scale: 2,
            round: Round::HalfUp,
        };
        let e = Expr::parse("net / 3").unwrap();
        assert_eq!(eval_in(&e, &r, ctx), Ok(dec("6.66")));
        let e = Expr::parse("2 / 3.0").unwrap();
        assert_eq!(eval_in(&e, &r, ctx), Ok(dec("0.67")));
        let down = DecCtx {
            scale: 2,
            round: Round::Down,
        };
        assert_eq!(eval_in(&e, &r, down), Ok(dec("0.66")));
        // The result of a whole-number division is still a whole number.
        assert_eq!(run("7 / 2", &r), Ok(V::Int(4)));
        assert!(run("net / 0", &r).unwrap_err().contains("division by zero"));
        assert!(
            run("9999999999999999999999999999999999999.0 * 100", &r)
                .unwrap_err()
                .contains("too large")
        );
        assert_eq!(
            run("concat(net, \"|\", 1.50)", &r),
            Ok(V::Text("19.99|1.50".into()))
        );
        assert_eq!(run("is_set(net)", &r), Ok(V::Bool(true)));
        // Instants compare with instants and with an instant written as text.
        assert_eq!(
            run("seen_at < \"2026-01-01T00:00:00Z\"", &r),
            Ok(V::Bool(true))
        );
        assert_eq!(run("seen_at == seen_at", &r), Ok(V::Bool(true)));
        assert_eq!(
            run("missing_at < seen_at", &r),
            Ok(V::Bool(true)),
            "empty is before everything"
        );
    }

    #[test]
    fn functions() {
        let r = rec();
        assert_eq!(run("sum(lines.amount)", &r), Ok(V::Int(35)));
        assert_eq!(run("count(lines)", &r), Ok(V::Int(3)));
        assert_eq!(
            run("max(lines.amount) - min(lines[move_id].amount)", &r),
            Ok(V::Int(15))
        );
        assert_eq!(
            run("if(flag, \"yes\", \"no\")", &r),
            Ok(V::Text("yes".into()))
        );
        assert_eq!(
            run("concat(name, \"-\", qty, \"/\", flag)", &r),
            Ok(V::Text("Acme-3/true".into()))
        );
        assert_eq!(
            run("is_set(name) && !is_set(missing)", &r),
            Ok(V::Bool(true))
        );
        // Short-circuit: the division is never evaluated.
        assert_eq!(run("is_set(missing) && 1 / 0 == 1", &r), Ok(V::Bool(false)));
    }

    #[test]
    fn arithmetic_errors_do_not_panic() {
        let r = rec();
        assert!(run("1 / 0", &r).unwrap_err().contains("division by zero"));
        assert!(
            run("9223372036854775807 + 1", &r)
                .unwrap_err()
                .contains("64 bits")
        );
        assert!(run("9223372036854775807 * 2", &r).is_err());
        assert!(
            Expr::parse("99999999999999999999")
                .unwrap_err()
                .contains("too large")
        );
        assert!(div_round(i64::MIN, -1).is_err());
        assert_eq!(div_round(-5, 2), Ok(-3));
        assert_eq!(div_round(5, -2), Ok(-3));
        assert_eq!(div_round(4, 3), Ok(1));
    }

    #[test]
    fn syntax_errors_are_explained() {
        for (src, want) in [
            ("", "empty"),
            ("1 +", "ends"),
            ("1.", "lone point"),
            ("2x", "followed by letters"),
            ("a b", "end of the expression"),
            ("\"abc", "unterminated"),
            ("\"a\\n\"", "escaped"),
            ("foo(1)", "unknown function"),
            ("sum(", "ends"),
            ("sum(lines)", "`.`"),
            ("@", "unexpected"),
            ("(1", "ends"),
            ("if(true, 1)", "expected `,`"),
            ("sum(a.b.c)", "`)`"),
        ] {
            let e = Expr::parse(src).unwrap_err();
            assert!(e.contains(want), "{src:?} -> {e}");
        }
        assert!(
            Expr::parse(&"1+".repeat(201))
                .unwrap_err()
                .contains("longer")
        );
        assert!(
            Expr::parse(&format!("{}1{}", "(".repeat(40), ")".repeat(40)))
                .unwrap_err()
                .contains("nested")
        );
        let wide = (0..60).map(|_| "1").collect::<Vec<_>>().join("+");
        assert!(Expr::parse(&wide).unwrap_err().contains("too large"));
    }

    fn f(name: &'static str) -> FieldDef {
        FieldDef::new(name, "d", DataClass::None, false, "t.user")
    }

    fn models() -> (Arc<ModelDef>, Arc<ModelDef>) {
        let parent = ModelDef {
            name: "t_move",
            description: "d",
            read_group: "t.user",
            write_group: "t.user",
            access: None,
            fields: vec![
                f("n").kind(FieldKind::Number),
                f("m").kind(FieldKind::Money),
                f("title"),
                f("ok").kind(FieldKind::Bool),
                f("state").select(&["a", "b"]),
                f("rate").decimal(10, 4),
                f("seen").datetime(),
                f("due").kind(FieldKind::Date),
            ],
            commands: Vec::<CommandDef>::new(),
            ext: Default::default(),
        };
        let line = ModelDef {
            name: "t_move_line",
            description: "d",
            read_group: "t.user",
            write_group: "t.user",
            access: None,
            fields: vec![
                f("move_id").reference("t_move"),
                f("other_id").reference("t_move"),
                f("debit").kind(FieldKind::Money),
                f("label"),
                f("weight").decimal(12, 3),
            ],
            commands: vec![],
            ext: Default::default(),
        };
        (Arc::new(parent), Arc::new(line))
    }

    fn typed(src: &str) -> Result<Ty, String> {
        let (p, l) = models();
        let lookup = move |n: &str| match n {
            "t_move" => Some(p.clone()),
            "t_move_line" => Some(l.clone()),
            _ => None,
        };
        let (p, _) = models();
        let e = Expr::parse(src)?;
        check(
            &e,
            &Scope {
                model: &p,
                lookup: &lookup,
                allow_computed: true,
            },
        )
    }

    #[test]
    fn types_are_checked_against_the_model() {
        assert_eq!(typed("n * m + 1"), Ok(Ty::Int));
        assert_eq!(typed("if(ok, title, \"x\")"), Ok(Ty::Text));
        assert_eq!(typed("state == \"a\""), Ok(Ty::Bool));
        assert_eq!(typed("concat(n, title, ok)"), Ok(Ty::Text));
        assert_eq!(typed("sum(t_move_line[move_id].debit)"), Ok(Ty::Int));
        assert_eq!(typed("sum(t_move_line[move_id].debit) == m"), Ok(Ty::Bool));
        assert_eq!(typed("count(t_move_lines[other_id])"), Ok(Ty::Int));
        // Decimals (D72): a number mixed with a decimal is a decimal.
        assert_eq!(typed("rate * 2"), Ok(Ty::Dec));
        assert_eq!(typed("rate / 3 + 0.5"), Ok(Ty::Dec));
        assert_eq!(typed("rate > 1"), Ok(Ty::Bool));
        assert_eq!(typed("n < rate"), Ok(Ty::Bool));
        assert_eq!(typed("-rate"), Ok(Ty::Dec));
        assert_eq!(typed("sum(t_move_line[move_id].weight)"), Ok(Ty::Dec));
        assert_eq!(
            typed("max(t_move_line[move_id].weight) * rate"),
            Ok(Ty::Dec)
        );
        assert_eq!(typed("if(ok, rate, 0.0)"), Ok(Ty::Dec));
        assert_eq!(typed("seen"), Ok(Ty::Instant));
        assert_eq!(typed("seen < seen"), Ok(Ty::Bool));
        assert_eq!(typed("seen >= \"2026-03-29T01:30:00Z\""), Ok(Ty::Bool));
        for (src, want) in [
            ("ghost + 1", "no field `ghost`"),
            ("title + 1", "must be a number, not text"),
            ("n && ok", "must be a true/false value"),
            ("n == title", "cannot compare"),
            ("ok < ok", "cannot order"),
            ("if(n, 1, 2)", "condition"),
            ("if(ok, 1, \"x\")", "same type"),
            ("-title", "operand of `-`"),
            ("!n", "operand of `!`"),
            ("sum(nothing.x)", "not a model that refers to t_move"),
            ("sum(lines.debit)", "several refs"),
            ("sum(zzz.debit)", "tried zzz, t_move_zzz"),
            ("sum(t_move_line.debit)", "several refs"),
            ("sum(t_move_line[title].debit)", "is not a ref"),
            (
                "sum(t_move_line[move_id].label)",
                "needs a number, money or decimal field",
            ),
            ("sum(t_move_line[move_id].nope)", "has no field"),
            ("rate == title", "cannot compare"),
            ("rate + title", "must be a number"),
            ("seen + 1", "must be a number"),
            ("seen < due", "cannot order a date and time against text"),
            ("seen == title", "cannot compare"),
            ("seen < \"2026-03-29\"", "cannot order"),
            ("seen < \"yesterday\"", "cannot order"),
            ("seen < 5", "cannot order"),
            ("rate && ok", "must be a true/false value"),
            ("sum(t_move_line[move_id].label)", "decimal field"),
        ] {
            let e = typed(src).unwrap_err();
            assert!(e.contains(want), "{src:?} -> {e}");
        }
    }

    #[test]
    fn child_names_resolve_by_suffix_singular_or_plural() {
        let (p, l) = models();
        let lookup = |n: &str| (n == "t_move_line").then(|| l.clone());
        for name in ["t_move_line", "line", "lines"] {
            let (m, via) = resolve_child(&p, name, Some("move_id"), &lookup).unwrap();
            assert_eq!((m.name, via), ("t_move_line", "move_id"), "{name}");
        }
        let _ = typed("1");
    }

    #[test]
    fn fields_and_aggregates_are_listed() {
        let e = Expr::parse("sum(lines.debit) + qty * 2 - count(rows[r])").unwrap();
        assert_eq!(e.fields(), ["qty"]);
        let a = e.aggregates();
        assert_eq!(a.len(), 2);
        assert_eq!(
            (a[1].func, a[1].child, a[1].via),
            (AggFn::Count, "rows", Some("r"))
        );
        let w = Expr::parse("count(lines where state == \"done\" && qty > 1) * 100 / count(lines)")
            .unwrap();
        let a = w.aggregates();
        assert_eq!(a.len(), 2);
        assert!(a[0].filter.is_some() && a[1].filter.is_none());
        assert!(
            w.fields().is_empty(),
            "a child's fields are not fields of the record"
        );
    }
}
