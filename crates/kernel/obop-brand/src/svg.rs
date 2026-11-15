//! The strict SVG reader and writer of the logo (docs/spec/branding.md, section 8).
//!
//! Input is parsed by a hand-written reader into typed values; what is stored and served is a
//! re-serialisation from those values, never the input. So a parser bug can refuse or mis-read a
//! logo but cannot let hostile markup through: no attribute value, element name or number is ever
//! copied from the input text. The reader is a total function: whatever the bytes (malformed,
//! truncated, deeply nested, huge) it returns a refusal or an output and never panics, and its
//! depth and size are bounded (16 KiB, 128 elements, groups nested at most 4 deep).
//!
//! Accepted: an optional `<?xml version ... ?>` line (not stored); one root `<svg>` with a
//! `viewBox` of four numbers (`width` and `height` are ignored, not stored); at most 128 elements
//! below it, of the kinds `g`, `path`, `rect`, `circle`, `ellipse`, `line` and `polygon`, with the
//! attributes `fill`, `stroke`, `fill-rule`, `stroke-width` and `transform` and the geometry of
//! each kind. Everything else is refused with a reason that names it.
//!
//! Three rules are stricter than a plain reading of that sentence, on purpose (they fail closed and
//! cost a design-tool export one edit): a root `width` or `height`, though ignored, must be a plain
//! number with an optional unit (`px`, `pt`, `em`, `cm`, `mm`, `in` or `%`), so `width="auto"` is
//! refused; the width and the height of the `viewBox` must be positive; and whatever is accepted is
//! read back (see `read`): a size below 0.0001 prints as 0, and a compact list that prints longer
//! than 8 KiB written out (`.5.5` is four bytes, `0.5 0.5` seven) is refused.
//!
//! Two more bounds refuse before any rule about the content is applied (they are in the limits of
//! section 8 of the spec): elements nested more than 16 levels deep, `<svg>` included, and more than
//! 32 attributes on one element. (The element limit alone would also bound a file,
//! but only after the reader has built the tree; these two keep the tree shallow and narrow.)
//!
//! The canonical text is one line made only of the bytes `A-Z a-z 0-9` and space `, . : / # ( ) = < >
//! " -`, and every `"` delimits an attribute value that the writer produced.

use crate::refusal::{Refusal, show_char, show_text};

/// Input over this many bytes is refused; so is a canonical text over it.
pub const MAX_BYTES: usize = 16 * 1024;
/// Elements below the root, at most.
pub const MAX_ELEMENTS: usize = 128;
/// `<g>` inside `<g>`, at most this deep.
pub const MAX_GROUP_DEPTH: usize = 4;
/// The `d` of a path and the `points` of a polygon, at most this many bytes each.
pub const MAX_LIST_BYTES: usize = 8 * 1024;
/// Functions in one `transform`, at most.
pub const MAX_TRANSFORMS: usize = 4;
/// A number on input is at most this many characters.
pub const MAX_NUMBER_CHARS: usize = 24;
/// A number on input is at most this in absolute value.
pub const MAX_ABS: f64 = 100_000.0;
/// The refusals one reading reports, at most (a hostile file cannot make a long list).
pub const MAX_REFUSALS: usize = 20;
/// Elements nested deeper than this are refused while reading, before anything is checked.
const MAX_NESTING: usize = 16;
/// Attributes on one element, at most.
const MAX_ATTRIBUTES: usize = 32;

const SVG_NAMESPACE: &str = "http://www.w3.org/2000/svg";
const ALLOWED_ELEMENTS: &str = "g, path, rect, circle, ellipse, line and polygon";

// ---------------------------------------------------------------------------------------------
// The typed result.
// ---------------------------------------------------------------------------------------------

/// A parsed logo. Print it with [`Svg::print`]; there is no other way to get text out.
#[derive(Clone, Debug, PartialEq)]
pub struct Svg {
    view_box: [f64; 4],
    children: Vec<Node>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    G,
    Path,
    Rect,
    Circle,
    Ellipse,
    Line,
    Polygon,
}

impl Kind {
    fn of(name: &str) -> Option<Kind> {
        Some(match name {
            "g" => Kind::G,
            "path" => Kind::Path,
            "rect" => Kind::Rect,
            "circle" => Kind::Circle,
            "ellipse" => Kind::Ellipse,
            "line" => Kind::Line,
            "polygon" => Kind::Polygon,
            _ => return None,
        })
    }

    fn name(self) -> &'static str {
        match self {
            Kind::G => "g",
            Kind::Path => "path",
            Kind::Rect => "rect",
            Kind::Circle => "circle",
            Kind::Ellipse => "ellipse",
            Kind::Line => "line",
            Kind::Polygon => "polygon",
        }
    }

    /// The geometry attributes of the kind, in the order the writer prints them.
    fn geometry(self) -> &'static [&'static str] {
        match self {
            Kind::G => &[],
            Kind::Path => &["d"],
            Kind::Rect => &["x", "y", "width", "height", "rx"],
            Kind::Circle => &["cx", "cy", "r"],
            Kind::Ellipse => &["cx", "cy", "rx", "ry"],
            Kind::Line => &["x1", "y1", "x2", "y2"],
            Kind::Polygon => &["points"],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Paint {
    None,
    CurrentColor,
    Rgb(u8, u8, u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FillRule {
    NonZero,
    EvenOdd,
}

#[derive(Clone, Debug, PartialEq)]
enum Transform {
    Matrix([f64; 6]),
    Translate(f64, Option<f64>),
    Scale(f64, Option<f64>),
}

#[derive(Clone, Debug, PartialEq)]
enum PathItem {
    Command(u8),
    Number(f64),
}

#[derive(Clone, Debug, PartialEq)]
enum Geometry {
    Number(f64),
    Path(Vec<PathItem>),
    Points(Vec<f64>),
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Style {
    fill: Option<Paint>,
    fill_rule: Option<FillRule>,
    stroke: Option<Paint>,
    stroke_width: Option<f64>,
    transform: Vec<Transform>,
}

#[derive(Clone, Debug, PartialEq)]
struct Node {
    kind: Kind,
    geometry: Vec<(&'static str, Geometry)>,
    style: Style,
    children: Vec<Node>,
}

// ---------------------------------------------------------------------------------------------
// The refusals of one reading.
// ---------------------------------------------------------------------------------------------

struct Sink {
    list: Vec<Refusal>,
}

impl Sink {
    fn new() -> Sink {
        Sink { list: Vec::new() }
    }

    fn push(&mut self, code: &'static str, message: impl Into<String>) {
        let message = message.into();
        if self.list.len() < MAX_REFUSALS && !self.list.iter().any(|r| r.message == message) {
            self.list.push(Refusal::new(code, message));
        }
    }

    fn full(&self) -> bool {
        self.list.len() >= MAX_REFUSALS
    }
}

// ---------------------------------------------------------------------------------------------
// Entry points.
// ---------------------------------------------------------------------------------------------

/// Reads an SVG file. Input must be UTF-8 with no byte order mark, at most 16 KiB. On refusal the
/// answer lists every reason found (at most [`MAX_REFUSALS`]), each naming the element, attribute
/// or number, so that a design-tool export says what to delete.
pub fn parse(input: &[u8]) -> Result<Svg, Vec<Refusal>> {
    let refuse = |code: &'static str, message: &str| Err(vec![Refusal::new(code, message)]);
    if input.len() > MAX_BYTES {
        return refuse(
            "svg_too_large",
            &format!(
                "the file is {} bytes; the limit is {MAX_BYTES} bytes (16 KiB)",
                input.len()
            ),
        );
    }
    if input.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return refuse(
            "svg_byte_order_mark",
            "the file starts with a byte order mark; save it as UTF-8 without one",
        );
    }
    if input.starts_with(&[0xFF, 0xFE]) || input.starts_with(&[0xFE, 0xFF]) {
        return refuse(
            "svg_not_utf8",
            "the file is UTF-16 (it starts with a byte order mark); save it as UTF-8",
        );
    }
    match std::str::from_utf8(input) {
        Ok(text) => read(text),
        Err(_) => refuse("svg_not_utf8", "the file is not valid UTF-8"),
    }
}

/// Reads an SVG text, as it is stored. The same rules as [`parse`].
pub fn parse_str(input: &str) -> Result<Svg, Vec<Refusal>> {
    if input.len() > MAX_BYTES {
        return parse(input.as_bytes());
    }
    if input.starts_with('\u{FEFF}') {
        return parse(&[0xEF, 0xBB, 0xBF]);
    }
    read(input)
}

/// The canonical text of an SVG file: `parse` then `print`.
pub fn canonical(input: &[u8]) -> Result<String, Vec<Refusal>> {
    parse(input).map(|svg| svg.print())
}

impl Svg {
    /// The canonical text: what the logo route serves, what `v` hashes and what is stored.
    pub fn print(&self) -> String {
        self.write(None)
    }

    /// The same, with `width` and `height` of `size` on the root, for the inline logo of a page:
    /// the stored artwork has none, and an inline `<svg>` with only a `viewBox` grows to the width
    /// of its container.
    pub fn print_inline(&self, size: u32) -> String {
        self.write(Some(size))
    }

    fn write(&self, size: Option<u32>) -> String {
        let mut out = String::with_capacity(256);
        out.push_str("<svg xmlns=\"");
        out.push_str(SVG_NAMESPACE);
        out.push('"');
        if let Some(n) = size {
            out.push_str(&format!(" width=\"{n}\" height=\"{n}\""));
        }
        out.push_str(" viewBox=\"");
        for (i, v) in self.view_box.iter().enumerate() {
            if i > 0 {
                out.push(' ');
            }
            out.push_str(&number(*v));
        }
        out.push_str("\">");
        for child in &self.children {
            child.write(&mut out);
        }
        out.push_str("</svg>");
        out
    }
}

impl Node {
    fn write(&self, out: &mut String) {
        let name = self.kind.name();
        out.push('<');
        out.push_str(name);
        for key in self.kind.geometry() {
            if let Some((_, value)) = self.geometry.iter().find(|(k, _)| k == key) {
                out.push(' ');
                out.push_str(key);
                out.push_str("=\"");
                match value {
                    Geometry::Number(n) => out.push_str(&number(*n)),
                    Geometry::Path(items) => {
                        for (i, item) in items.iter().enumerate() {
                            if i > 0 {
                                out.push(' ');
                            }
                            match item {
                                PathItem::Command(c) => out.push(*c as char),
                                PathItem::Number(n) => out.push_str(&number(*n)),
                            }
                        }
                    }
                    Geometry::Points(ns) => {
                        for (i, n) in ns.iter().enumerate() {
                            if i > 0 {
                                out.push(' ');
                            }
                            out.push_str(&number(*n));
                        }
                    }
                }
                out.push('"');
            }
        }
        let s = &self.style;
        if let Some(p) = s.fill {
            out.push_str(" fill=\"");
            out.push_str(&paint(p));
            out.push('"');
        }
        if let Some(r) = s.fill_rule {
            out.push_str(match r {
                FillRule::NonZero => " fill-rule=\"nonzero\"",
                FillRule::EvenOdd => " fill-rule=\"evenodd\"",
            });
        }
        if let Some(p) = s.stroke {
            out.push_str(" stroke=\"");
            out.push_str(&paint(p));
            out.push('"');
        }
        if let Some(w) = s.stroke_width {
            out.push_str(" stroke-width=\"");
            out.push_str(&number(w));
            out.push('"');
        }
        if !s.transform.is_empty() {
            out.push_str(" transform=\"");
            for (i, t) in s.transform.iter().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                match t {
                    Transform::Matrix(m) => {
                        out.push_str("matrix(");
                        for (j, v) in m.iter().enumerate() {
                            if j > 0 {
                                out.push(' ');
                            }
                            out.push_str(&number(*v));
                        }
                        out.push(')');
                    }
                    Transform::Translate(x, y) => {
                        out.push_str("translate(");
                        out.push_str(&number(*x));
                        if let Some(y) = y {
                            out.push(' ');
                            out.push_str(&number(*y));
                        }
                        out.push(')');
                    }
                    Transform::Scale(x, y) => {
                        out.push_str("scale(");
                        out.push_str(&number(*x));
                        if let Some(y) = y {
                            out.push(' ');
                            out.push_str(&number(*y));
                        }
                        out.push(')');
                    }
                }
            }
            out.push('"');
        }
        if self.children.is_empty() {
            out.push_str("/>");
        } else {
            out.push('>');
            for c in &self.children {
                c.write(out);
            }
            out.push_str("</");
            out.push_str(name);
            out.push('>');
        }
    }
}

/// A number as the writer prints it: rounded to 4 decimals, trailing zeros and a trailing point
/// dropped, never `-0`, never the raw `f64`. At most 12 characters for a number the reader
/// accepts (a sign, six digits, the point and four decimals).
fn number(v: f64) -> String {
    let mut s = format!("{v:.4}");
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    if s == "-0" {
        s = "0".to_string();
    }
    s
}

fn paint(p: Paint) -> String {
    match p {
        Paint::None => "none".to_string(),
        Paint::CurrentColor => "currentColor".to_string(),
        Paint::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
    }
}

// ---------------------------------------------------------------------------------------------
// Stage 1: the markup. A small reader of the XML the subset needs, producing a plain tree.
// ---------------------------------------------------------------------------------------------

struct Elem {
    name: String,
    attributes: Vec<(String, String)>,
    children: Vec<Elem>,
}

fn is_xml_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r')
}

fn is_list_space(b: u8) -> bool {
    is_xml_space(b) || b == 0x0C
}

fn is_name_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b == b':'
}

fn is_name_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b':' | b'.' | b'-')
}

/// Reads a text, and holds what it accepts to what the read side will accept: the canonical text
/// is what is stored, and the read side parses the stored text again. The writer rounds every
/// number to 4 decimals and writes every list in full with one space between two items, so a size
/// below 0.0001 prints as 0 and a compact list (`.5.5`) can print longer than its input; either
/// would pass the checks on the input and be refused when it is read back. Reading the printed
/// text once here makes accepting and re-reading agree by construction, and the refusal names what
/// the re-reading refused.
fn read(text: &str) -> Result<Svg, Vec<Refusal>> {
    let svg = read_once(text)?;
    let printed = svg.print();
    if printed.len() > MAX_BYTES {
        return Err(vec![Refusal::new(
            "svg_too_large",
            format!(
                "the canonical text is {} bytes; the limit is {MAX_BYTES} bytes (16 KiB)",
                printed.len()
            ),
        )]);
    }
    if let Err(again) = read_once(&printed) {
        let first = again.first().map_or("", |r| r.message.as_str());
        return Err(vec![Refusal::new(
            "svg_canonical",
            format!(
                "the canonical text of the file would be refused when it is read back ({first}); \
                 the writer prints every number in full, rounded to 4 decimals, with one space \
                 between two, so make the size larger or the list shorter"
            ),
        )]);
    }
    Ok(svg)
}

/// The reading itself, with no check of the canonical text.
fn read_once(text: &str) -> Result<Svg, Vec<Refusal>> {
    let mut sink = Sink::new();
    let tree = match tokenize(text, &mut sink) {
        Ok(tree) => tree,
        Err(Hard) => return Err(sink.list),
    };
    let svg = match tree {
        Some(root) => build(&root, &mut sink),
        None => {
            sink.push("svg_malformed", "the file holds no <svg> element");
            None
        }
    };
    if !sink.list.is_empty() {
        return Err(sink.list);
    }
    let Some(svg) = svg else {
        return Err(vec![Refusal::new(
            "svg_malformed",
            "the file holds no <svg> element",
        )]);
    };
    Ok(svg)
}

/// A hard failure: the reason is in the sink and nothing more can be read.
struct Hard;

fn tokenize(s: &str, sink: &mut Sink) -> Result<Option<Elem>, Hard> {
    let b = s.as_bytes();
    let mut i = 0usize;
    let mut stack: Vec<Elem> = Vec::new();
    let mut root: Option<Elem> = None;
    let mut elements = 0usize;
    let hard = |sink: &mut Sink, code: &'static str, message: String| -> Hard {
        sink.push(code, message);
        Hard
    };

    // The optional first line.
    if s.starts_with("<?xml") && b.get(5).is_some_and(|c| is_xml_space(*c) || *c == b'?') {
        let Some(end) = s.find("?>") else {
            return Err(hard(
                sink,
                "svg_malformed",
                "the XML declaration at the start is not closed".into(),
            ));
        };
        declaration(&s[5..end], sink);
        i = end + 2;
    }

    while i < b.len() {
        if sink.full() {
            return Err(Hard);
        }
        if b[i] != b'<' {
            let end = s[i..].find('<').map_or(b.len(), |p| i + p);
            let text = &s[i..end];
            if text.bytes().any(|c| !is_xml_space(c)) {
                if text.contains('&') {
                    sink.push(
                        "svg_entity",
                        "an entity or & in the text is not allowed (and no text is)",
                    );
                } else {
                    sink.push(
                        "svg_text",
                        "text outside the elements is not allowed (<text> and <title> are not either)",
                    );
                }
            }
            i = end;
            continue;
        }
        let rest = &s[i..];
        if let Some(after) = rest.strip_prefix("<!--") {
            sink.push("svg_comment", "a comment is not allowed; delete it");
            match after.find("-->") {
                Some(p) => i += 4 + p + 3,
                None => {
                    return Err(hard(
                        sink,
                        "svg_malformed",
                        "a comment is not closed".into(),
                    ));
                }
            }
            continue;
        }
        if rest.starts_with("<![CDATA[") {
            sink.push("svg_cdata", "CDATA is not allowed; delete it");
            match rest.find("]]>") {
                Some(p) => i += p + 3,
                None => {
                    return Err(hard(
                        sink,
                        "svg_malformed",
                        "a CDATA section is not closed".into(),
                    ));
                }
            }
            continue;
        }
        if rest.starts_with("<!") {
            let what = if rest.len() >= 9 && rest.as_bytes()[..9].eq_ignore_ascii_case(b"<!DOCTYPE")
            {
                "a DOCTYPE (and with it any entity) is not allowed; delete it"
            } else {
                "a markup declaration is not allowed; delete it"
            };
            return Err(hard(sink, "svg_doctype", what.into()));
        }
        if rest.starts_with("<?") {
            sink.push(
                "svg_processing_instruction",
                "a processing instruction other than the first line is not allowed; delete it",
            );
            match rest.find("?>") {
                Some(p) => i += p + 2,
                None => {
                    return Err(hard(
                        sink,
                        "svg_malformed",
                        "a processing instruction is not closed".into(),
                    ));
                }
            }
            continue;
        }
        if rest.starts_with("</") {
            i += 2;
            let (name, next) = read_name(s, i);
            i = next;
            while i < b.len() && is_xml_space(b[i]) {
                i += 1;
            }
            if name.is_empty() || b.get(i) != Some(&b'>') {
                return Err(hard(
                    sink,
                    "svg_malformed",
                    "an end tag is malformed".into(),
                ));
            }
            i += 1;
            match stack.pop() {
                Some(done) if done.name == name => attach(&mut stack, &mut root, done),
                Some(_) => {
                    return Err(hard(
                        sink,
                        "svg_malformed",
                        format!("the end tag </{}> does not match", show_text(name, 40)),
                    ));
                }
                None => {
                    return Err(hard(
                        sink,
                        "svg_malformed",
                        format!("the end tag </{}> has no start tag", show_text(name, 40)),
                    ));
                }
            }
            continue;
        }
        // A start tag.
        if root.is_some() && stack.is_empty() {
            return Err(hard(
                sink,
                "svg_second_root",
                "a second root element is not allowed".into(),
            ));
        }
        i += 1;
        let (name, next) = read_name(s, i);
        i = next;
        if name.is_empty() {
            return Err(hard(
                sink,
                "svg_malformed",
                "a start tag has no name".into(),
            ));
        }
        elements += 1;
        if elements > MAX_ELEMENTS + 1 {
            return Err(hard(
                sink,
                "svg_limit",
                format!("more than {MAX_ELEMENTS} elements; the limit is {MAX_ELEMENTS}"),
            ));
        }
        let mut element = Elem {
            name: name.to_string(),
            attributes: Vec::new(),
            children: Vec::new(),
        };
        let mut self_closing = false;
        loop {
            while i < b.len() && is_xml_space(b[i]) {
                i += 1;
            }
            match b.get(i) {
                None => {
                    return Err(hard(
                        sink,
                        "svg_malformed",
                        format!("the file ends inside the tag <{}", show_text(name, 40)),
                    ));
                }
                Some(b'>') => {
                    i += 1;
                    break;
                }
                Some(b'/') if b.get(i + 1) == Some(&b'>') => {
                    i += 2;
                    self_closing = true;
                    break;
                }
                Some(_) => {}
            }
            let (attribute, next) = read_name(s, i);
            if attribute.is_empty() {
                return Err(hard(
                    sink,
                    "svg_malformed",
                    format!("a tag <{}> is malformed", show_text(name, 40)),
                ));
            }
            i = next;
            while i < b.len() && is_xml_space(b[i]) {
                i += 1;
            }
            if b.get(i) != Some(&b'=') {
                return Err(hard(
                    sink,
                    "svg_malformed",
                    format!("the attribute {} has no value", show_text(attribute, 40)),
                ));
            }
            i += 1;
            while i < b.len() && is_xml_space(b[i]) {
                i += 1;
            }
            let quote = match b.get(i) {
                Some(q @ (b'"' | b'\'')) => *q,
                _ => {
                    return Err(hard(
                        sink,
                        "svg_malformed",
                        format!("the value of {} is not quoted", show_text(attribute, 40)),
                    ));
                }
            };
            i += 1;
            let Some(len) = b[i..].iter().position(|c| *c == quote) else {
                return Err(hard(
                    sink,
                    "svg_malformed",
                    format!("the value of {} is not closed", show_text(attribute, 40)),
                ));
            };
            // The value is cut at ASCII quotes, so both ends are character boundaries.
            let value = &s[i..i + len];
            i += len + 1;
            if element.attributes.len() >= MAX_ATTRIBUTES {
                return Err(hard(
                    sink,
                    "svg_limit",
                    format!(
                        "more than {MAX_ATTRIBUTES} attributes on <{}>",
                        show_text(name, 40)
                    ),
                ));
            }
            element
                .attributes
                .push((attribute.to_string(), value.to_string()));
        }
        if self_closing {
            attach(&mut stack, &mut root, element);
        } else {
            if stack.len() >= MAX_NESTING {
                return Err(hard(
                    sink,
                    "svg_limit",
                    format!("elements nested more than {MAX_NESTING} deep"),
                ));
            }
            stack.push(element);
        }
    }
    if let Some(open) = stack.last() {
        return Err(hard(
            sink,
            "svg_malformed",
            format!("the file ends inside <{}>", show_text(&open.name, 40)),
        ));
    }
    Ok(root)
}

fn attach(stack: &mut [Elem], root: &mut Option<Elem>, done: Elem) {
    match stack.last_mut() {
        Some(parent) => parent.children.push(done),
        None => *root = Some(done),
    }
}

/// An XML name at `i`: letters, digits and `_ : . -`, starting with a letter, `_` or `:`.
fn read_name(s: &str, i: usize) -> (&str, usize) {
    let b = s.as_bytes();
    if i >= b.len() || !is_name_start(b[i]) {
        return ("", i);
    }
    let mut j = i + 1;
    while j < b.len() && is_name_char(b[j]) && j - i < 64 {
        j += 1;
    }
    (&s[i..j], j)
}

/// The body of the first line, `version="1.0" encoding="UTF-8"`: only these three, and the
/// encoding must be UTF-8.
fn declaration(body: &str, sink: &mut Sink) {
    let b = body.as_bytes();
    let mut i = 0usize;
    loop {
        while i < b.len() && is_xml_space(b[i]) {
            i += 1;
        }
        if i >= b.len() {
            return;
        }
        let (name, next) = read_name(body, i);
        i = next;
        while i < b.len() && is_xml_space(b[i]) {
            i += 1;
        }
        if name.is_empty() || b.get(i) != Some(&b'=') {
            sink.push(
                "svg_malformed",
                "the XML declaration at the start is malformed",
            );
            return;
        }
        i += 1;
        while i < b.len() && is_xml_space(b[i]) {
            i += 1;
        }
        let Some(q @ (b'"' | b'\'')) = b.get(i).copied() else {
            sink.push(
                "svg_malformed",
                "the XML declaration at the start is malformed",
            );
            return;
        };
        i += 1;
        let Some(len) = b[i..].iter().position(|c| *c == q) else {
            sink.push(
                "svg_malformed",
                "the XML declaration at the start is malformed",
            );
            return;
        };
        let value = &body[i..i + len];
        i += len + 1;
        match name {
            "version" if value == "1.0" => {}
            "standalone" if value == "yes" || value == "no" => {}
            "encoding" if value.eq_ignore_ascii_case("utf-8") => {}
            "encoding" => sink.push(
                "svg_not_utf8",
                format!(
                    "the XML declaration says the encoding is {}; it must be UTF-8",
                    show_text(value, 24)
                ),
            ),
            _ => sink.push(
                "svg_malformed",
                format!(
                    "the XML declaration holds {}, which is not version, encoding or standalone",
                    show_text(name, 24)
                ),
            ),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Stage 2: the elements and their values.
// ---------------------------------------------------------------------------------------------

fn build(root: &Elem, sink: &mut Sink) -> Option<Svg> {
    if root.name != "svg" {
        sink.push(
            "svg_element",
            format!(
                "the root element is <{}>; it must be <svg>",
                show_text(&root.name, 40)
            ),
        );
        return None;
    }
    let mut view_box = None;
    let mut seen: Vec<&str> = Vec::new();
    for (name, value) in &root.attributes {
        if duplicate(&mut seen, name, "svg", sink) {
            continue;
        }
        match name.as_str() {
            "xmlns" => {
                if value != SVG_NAMESPACE {
                    sink.push(
                        "svg_attribute",
                        "xmlns on <svg> is not the SVG namespace; delete it or use \
                         http://www.w3.org/2000/svg",
                    );
                }
            }
            "viewBox" => match number_list(value) {
                Ok(v) if v.len() == 4 && v[2] > 0.0 && v[3] > 0.0 => {
                    view_box = Some([v[0], v[1], v[2], v[3]]);
                }
                Ok(v) if v.len() == 4 => sink.push(
                    "svg_value",
                    "viewBox on <svg>: the width and the height must be positive",
                ),
                Ok(_) => sink.push(
                    "svg_value",
                    "viewBox on <svg>: it must hold exactly four numbers",
                ),
                Err(why) => sink.push("svg_value", format!("viewBox on <svg>: {why}")),
            },
            // Ignored, not stored; still held to a plain number with an optional unit, so that
            // nothing odd is waved through.
            "width" | "height" => {
                if !plain_length(value) {
                    sink.push(
                        "svg_value",
                        format!(
                            "{name} on <svg> is not a plain number with an optional unit \
                             (px, pt, em, cm, mm, in or %)"
                        ),
                    );
                }
            }
            _ => not_allowed_attribute(sink, name, "svg"),
        }
    }
    if view_box.is_none() && !seen.contains(&"viewBox") {
        sink.push(
            "svg_value",
            "the root <svg> needs a viewBox of four numbers, such as viewBox=\"0 0 64 64\"",
        );
    }
    let mut children = Vec::new();
    for child in &root.children {
        if sink.full() {
            break;
        }
        if let Some(n) = node(child, 0, sink) {
            children.push(n);
        }
    }
    Some(Svg {
        view_box: view_box?,
        children,
    })
}

/// Whether the attribute was given before; says so once when it was.
fn duplicate<'a>(seen: &mut Vec<&'a str>, name: &'a str, element: &str, sink: &mut Sink) -> bool {
    if seen.contains(&name) {
        sink.push(
            "svg_attribute",
            format!(
                "the attribute {} is given twice on <{}>",
                show_text(name, 40),
                show_text(element, 40)
            ),
        );
        return true;
    }
    seen.push(name);
    false
}

fn not_allowed_attribute(sink: &mut Sink, name: &str, element: &str) {
    sink.push(
        "svg_attribute",
        format!(
            "the attribute {} on <{}> is not allowed; delete it",
            show_text(name, 40),
            show_text(element, 40)
        ),
    );
}

fn node(e: &Elem, groups: usize, sink: &mut Sink) -> Option<Node> {
    let Some(kind) = Kind::of(&e.name) else {
        sink.push(
            "svg_element",
            format!(
                "the element <{}> is not allowed (only {ALLOWED_ELEMENTS} are); delete it",
                show_text(&e.name, 40)
            ),
        );
        return None;
    };
    if kind == Kind::G && groups >= MAX_GROUP_DEPTH {
        sink.push(
            "svg_limit",
            format!("<g> nested more than {MAX_GROUP_DEPTH} deep"),
        );
        return None;
    }
    let element = kind.name();
    let mut out = Node {
        kind,
        geometry: Vec::new(),
        style: Style::default(),
        children: Vec::new(),
    };
    let mut seen: Vec<&str> = Vec::new();
    for (name, value) in &e.attributes {
        if sink.full() {
            break;
        }
        if duplicate(&mut seen, name, element, sink) {
            continue;
        }
        let name = name.as_str();
        if let Some(key) = kind.geometry().iter().copied().find(|k| *k == name) {
            if let Some(g) = geometry(kind, key, value, sink) {
                out.geometry.push((key, g));
            }
            continue;
        }
        let bad = |sink: &mut Sink, why: String| {
            sink.push("svg_value", format!("{name} on <{element}>: {why}"));
        };
        match name {
            "fill" | "stroke" => match parse_paint(value) {
                Ok(p) if name == "fill" => out.style.fill = Some(p),
                Ok(p) => out.style.stroke = Some(p),
                Err(why) => bad(sink, why),
            },
            "fill-rule" => match value.trim() {
                "nonzero" => out.style.fill_rule = Some(FillRule::NonZero),
                "evenodd" => out.style.fill_rule = Some(FillRule::EvenOdd),
                other => bad(
                    sink,
                    format!("{} is not nonzero or evenodd", show_text(other, 24)),
                ),
            },
            "stroke-width" => match single_number(value, true) {
                Ok(w) => out.style.stroke_width = Some(w),
                Err(why) => bad(sink, why),
            },
            "transform" => match parse_transform(value) {
                Ok(t) => out.style.transform = t,
                Err(why) => bad(sink, why),
            },
            _ => not_allowed_attribute(sink, name, element),
        }
    }
    if kind == Kind::G {
        for child in &e.children {
            if sink.full() {
                break;
            }
            if let Some(n) = node(child, groups + 1, sink) {
                out.children.push(n);
            }
        }
    } else if !e.children.is_empty() {
        sink.push(
            "svg_element",
            format!("<{element}> cannot hold other elements; delete them"),
        );
    }
    Some(out)
}

fn geometry(kind: Kind, key: &'static str, value: &str, sink: &mut Sink) -> Option<Geometry> {
    let element = kind.name();
    let mut bad = |why: String| {
        sink.push("svg_value", format!("{key} on <{element}>: {why}"));
        None
    };
    match key {
        "d" => match parse_path(value) {
            Ok(p) => Some(Geometry::Path(p)),
            Err(why) => bad(why),
        },
        "points" => match parse_points(value) {
            Ok(p) => Some(Geometry::Points(p)),
            Err(why) => bad(why),
        },
        // A size and a radius cannot be negative; a coordinate can.
        "width" | "height" | "r" | "rx" | "ry" => match single_number(value, true) {
            Ok(n) => Some(Geometry::Number(n)),
            Err(why) => bad(why),
        },
        _ => match single_number(value, false) {
            Ok(n) => Some(Geometry::Number(n)),
            Err(why) => bad(why),
        },
    }
}

// ---------------------------------------------------------------------------------------------
// Values.
// ---------------------------------------------------------------------------------------------

/// `5`, `5.5`, `.5`, `5.`, with an optional sign; no exponent. At most 24 characters, at most
/// 100000 in absolute value. Returns the number and where it ends.
fn scan_number(b: &[u8], start: usize) -> Result<(f64, usize), String> {
    let at = |i: usize| b.get(i).copied();
    let mut i = start;
    if matches!(at(i), Some(b'+' | b'-')) {
        i += 1;
    }
    let digits_start = i;
    while at(i).is_some_and(|c| c.is_ascii_digit()) {
        i += 1;
    }
    let mut digits = i - digits_start;
    if at(i) == Some(b'.') {
        i += 1;
        let fraction_start = i;
        while at(i).is_some_and(|c| c.is_ascii_digit()) {
            i += 1;
        }
        digits += i - fraction_start;
    }
    if digits == 0 {
        return Err(match at(start) {
            Some(c) if c.is_ascii() => format!("expected a number, found {}", show_char(c as char)),
            Some(_) => "expected a number, found a character that is not ASCII".to_string(),
            None => "expected a number, found the end".to_string(),
        });
    }
    if matches!(at(i), Some(b'e' | b'E')) {
        let mut j = i;
        while at(j)
            .is_some_and(|c| c.is_ascii_digit() || matches!(c, b'e' | b'E' | b'+' | b'-' | b'.'))
        {
            j += 1;
        }
        let shown = String::from_utf8_lossy(&b[start..j.min(start + 32)]).into_owned();
        return Err(format!(
            "the number {} has an exponent; write it out in full",
            show_text(&shown, 32)
        ));
    }
    let token = String::from_utf8_lossy(&b[start..i]).into_owned();
    if token.len() > MAX_NUMBER_CHARS {
        return Err(format!(
            "the number {} is longer than {MAX_NUMBER_CHARS} characters; shorten it",
            show_text(&token, 27)
        ));
    }
    let Ok(v) = token.parse::<f64>() else {
        return Err(format!("{} is not a number", show_text(&token, 27)));
    };
    if !v.is_finite() || v.abs() > MAX_ABS {
        return Err(format!(
            "the number {} is larger than {MAX_ABS} in absolute value",
            show_text(&token, 27)
        ));
    }
    Ok((v, i))
}

/// A value that is one number and nothing else, spaces around it allowed.
fn single_number(value: &str, non_negative: bool) -> Result<f64, String> {
    let b = value
        .trim_matches(|c: char| c.is_ascii() && is_list_space(c as u8))
        .as_bytes();
    let (v, end) = scan_number(b, 0)?;
    if end != b.len() {
        return Err(format!(
            "{} is not a plain number (no units or lists)",
            show_text(&String::from_utf8_lossy(b), 24)
        ));
    }
    if non_negative && v < 0.0 {
        return Err(format!("the number {} is negative", number(v)));
    }
    Ok(v)
}

/// A list of numbers separated by spaces and at most one comma between two of them. A sign or a
/// second point starts a new number (`10-5` is 10 and -5, `.5.5` is .5 and .5).
fn number_list(value: &str) -> Result<Vec<f64>, String> {
    let b = value.as_bytes();
    let mut i = 0usize;
    let mut out = Vec::new();
    skip_list_space(b, &mut i);
    while i < b.len() {
        let (v, end) = scan_number(b, i)?;
        out.push(v);
        i = end;
        separator(b, &mut i)?;
    }
    Ok(out)
}

fn skip_list_space(b: &[u8], i: &mut usize) {
    while *i < b.len() && is_list_space(b[*i]) {
        *i += 1;
    }
}

/// After an item: spaces, at most one comma, spaces. A comma must be followed by an item.
fn separator(b: &[u8], i: &mut usize) -> Result<(), String> {
    skip_list_space(b, i);
    if b.get(*i) == Some(&b',') {
        *i += 1;
        skip_list_space(b, i);
        if *i >= b.len() || b[*i] == b',' {
            return Err("a comma with no number after it".to_string());
        }
    }
    Ok(())
}

fn parse_points(value: &str) -> Result<Vec<f64>, String> {
    if value.len() > MAX_LIST_BYTES {
        return Err(format!("longer than {MAX_LIST_BYTES} bytes (8 KiB)"));
    }
    let list = number_list(value)?;
    if list.is_empty() {
        return Err("the list is empty".to_string());
    }
    if list.len() % 2 != 0 {
        return Err("the list has an odd count of numbers; points are pairs".to_string());
    }
    Ok(list)
}

fn parse_paint(value: &str) -> Result<Paint, String> {
    let v = value.trim_matches(|c: char| c.is_ascii() && is_list_space(c as u8));
    if v.to_ascii_lowercase().contains("url(") {
        return Err(
            "a reference with url( is not allowed (gradients, patterns and filters are not)"
                .to_string(),
        );
    }
    match v {
        "none" => return Ok(Paint::None),
        "currentColor" => return Ok(Paint::CurrentColor),
        _ => {}
    }
    if let Some(hex) = v.strip_prefix('#')
        && hex.bytes().all(|c| c.is_ascii_hexdigit())
        && (hex.len() == 3 || hex.len() == 6)
    {
        let digit = |c: u8| u8::try_from((c as char).to_digit(16).unwrap_or(0)).unwrap_or(0);
        let h = hex.as_bytes();
        return Ok(if h.len() == 3 {
            Paint::Rgb(digit(h[0]) * 17, digit(h[1]) * 17, digit(h[2]) * 17)
        } else {
            Paint::Rgb(
                digit(h[0]) * 16 + digit(h[1]),
                digit(h[2]) * 16 + digit(h[3]),
                digit(h[4]) * 16 + digit(h[5]),
            )
        });
    }
    Err(format!(
        "{} is not #rgb, #rrggbb, none or currentColor",
        show_text(v, 24)
    ))
}

/// The units a width or height of the root may carry, lower case, directly after the number.
const LENGTH_UNITS: [&str; 7] = ["px", "pt", "em", "cm", "mm", "in", "%"];

/// A width or height of the root: a plain number (no exponent) with an optional unit. Ignored, not
/// stored. The unit is cut off first: `em` begins with the letter that begins an exponent, so the
/// number reader, which refuses every exponent, would otherwise refuse `1em` before it saw the unit.
fn plain_length(value: &str) -> bool {
    let v = value.trim_matches(|c: char| c.is_ascii() && is_list_space(c as u8));
    let number = LENGTH_UNITS
        .iter()
        .find_map(|unit| v.strip_suffix(unit))
        .unwrap_or(v);
    // The number is all that is left, with no space between it and the unit.
    scan_number(number.as_bytes(), 0).is_ok_and(|(_, end)| end == number.len())
}

fn parse_transform(value: &str) -> Result<Vec<Transform>, String> {
    let b = value.as_bytes();
    let mut i = 0usize;
    let mut out = Vec::new();
    loop {
        // Between functions: spaces and at most one comma.
        skip_list_space(b, &mut i);
        if b.get(i) == Some(&b',') && !out.is_empty() {
            i += 1;
            skip_list_space(b, &mut i);
        }
        if i >= b.len() {
            return Ok(out);
        }
        let start = i;
        while i < b.len() && b[i].is_ascii_alphabetic() {
            i += 1;
        }
        let name = &value[start..i];
        if name.is_empty() {
            return Err(match value[i..].chars().next() {
                Some(c) => format!("unexpected {}", show_char(c)),
                None => "unexpected end".to_string(),
            });
        }
        if !matches!(name, "matrix" | "translate" | "scale") {
            return Err(format!(
                "the function {} is not allowed (only matrix, translate and scale; a rotation \
                 or a skew can be written as a matrix by the design tool)",
                show_text(name, 24)
            ));
        }
        skip_list_space(b, &mut i);
        if b.get(i) != Some(&b'(') {
            return Err(format!("{name} has no ("));
        }
        i += 1;
        let mut args = Vec::new();
        skip_list_space(b, &mut i);
        while b.get(i) != Some(&b')') {
            if i >= b.len() {
                return Err(format!("{name}( is not closed"));
            }
            let (v, end) = scan_number(b, i)?;
            args.push(v);
            i = end;
            separator(b, &mut i)?;
        }
        i += 1;
        let t = match (name, args.as_slice()) {
            ("matrix", [a, b, c, d, e, f]) => Transform::Matrix([*a, *b, *c, *d, *e, *f]),
            ("matrix", _) => return Err("matrix( needs exactly 6 numbers".to_string()),
            ("translate", [x]) => Transform::Translate(*x, None),
            ("translate", [x, y]) => Transform::Translate(*x, Some(*y)),
            ("translate", _) => return Err("translate( needs 1 or 2 numbers".to_string()),
            ("scale", [x]) => Transform::Scale(*x, None),
            ("scale", [x, y]) => Transform::Scale(*x, Some(*y)),
            _ => return Err("scale( needs 1 or 2 numbers".to_string()),
        };
        out.push(t);
        if out.len() > MAX_TRANSFORMS {
            return Err(format!("more than {MAX_TRANSFORMS} functions"));
        }
    }
}

/// The numbers a path command takes, and which of them (counting from 0) are arc flags.
fn command_arity(c: u8) -> Option<usize> {
    Some(match c {
        b'Z' | b'z' => 0,
        b'H' | b'h' | b'V' | b'v' => 1,
        b'M' | b'm' | b'L' | b'l' | b'T' | b't' => 2,
        b'S' | b's' | b'Q' | b'q' => 4,
        b'C' | b'c' => 6,
        b'A' | b'a' => 7,
        _ => return None,
    })
}

/// Reads the `d` of a path: command letters of `MmLlHhVvCcSsQqTtAaZz` and numbers, in the compact
/// grammar of SVG (spaces and commas separate, a sign or a second point starts a number, and each
/// arc flag is one `0` or `1` that may be packed with the next number). The command's own numbers
/// are counted, so the flags can be told from numbers. Anything else is refused.
fn parse_path(value: &str) -> Result<Vec<PathItem>, String> {
    if value.len() > MAX_LIST_BYTES {
        return Err(format!("longer than {MAX_LIST_BYTES} bytes (8 KiB)"));
    }
    let b = value.as_bytes();
    let mut i = 0usize;
    let mut out: Vec<PathItem> = Vec::new();
    skip_list_space(b, &mut i);
    if i >= b.len() {
        return Err("the path data is empty".to_string());
    }
    if !matches!(b[i], b'M' | b'm') {
        return Err("the path data must start with a moveto, M or m".to_string());
    }
    while i < b.len() {
        let c = b[i];
        let Some(arity) = command_arity(c) else {
            return Err(match value[i..].chars().next() {
                Some(c) => format!("unexpected {}", show_char(c)),
                None => "unexpected end".to_string(),
            });
        };
        i += 1;
        out.push(PathItem::Command(c));
        skip_list_space(b, &mut i);
        if arity == 0 {
            if i < b.len() && !b[i].is_ascii_alphabetic() {
                return Err("numbers after Z or z".to_string());
            }
            continue;
        }
        let mut groups = 0usize;
        loop {
            for k in 0..arity {
                if (c == b'A' || c == b'a') && (k == 3 || k == 4) {
                    match b.get(i) {
                        Some(f @ (b'0' | b'1')) => {
                            out.push(PathItem::Number(f64::from(*f - b'0')));
                            i += 1;
                        }
                        _ => return Err("an arc flag must be 0 or 1".to_string()),
                    }
                } else {
                    let (v, end) = scan_number(b, i)?;
                    out.push(PathItem::Number(v));
                    i = end;
                }
                separator(b, &mut i)?;
            }
            groups += 1;
            // The next group of the same command, if a number follows.
            match b.get(i) {
                Some(n) if !n.is_ascii_alphabetic() => {}
                _ => break,
            }
        }
        if groups == 0 {
            return Err("a command with no numbers".to_string());
        }
    }
    Ok(out)
}
