//! Screens as outlines (ledger #3365): an author WRITES a short outline of references; the model
//! is what it COMPILES to.
//!
//! ```text
//! @prefixes urn:iki:uidemo:prefixes
//! group label="Contact"
//!   heading content="Write to us" level=1
//!   ui:form:resource:urn:iki:uidemo:contact  description="We answer within a day"
//!   ui:screen:name:recent
//! ```
//!
//! ## The outline
//!
//! - One line per node. A line is a CORE KIND (`heading`, `group`, …) or a structural node
//!   (`reference`, `region`: [`model::STRUCTURES`], which a face resolves) with `key=value`
//!   properties; a custom element's full IRI (`urn:{ns}:ui:element:name:{name}`), which stays an
//!   element node for a face to take down its ladder ([`crate::element`]); or a REFERENCE: any
//!   other name, whose Source answers a UI node or subtree (a generated form, another screen, a
//!   curated view), followed by `key=value` OVERRIDES.
//! - Indentation (spaces) nests: a line's children are the more-indented lines below it. A
//!   reference's children follow the referenced node's own. One line is the root.
//! - A value is a bare word or `"quoted"` (`\"` and `\\` escape). It is typed by the property
//!   it sets: a heading's `level` is a number, a `required` a flag, everything else text. On an
//!   element a bare integer is a number, a bare `true`/`false` a flag.
//! - `#` starts a comment line; `@prefixes <iri>`, before the first node, names the screen's
//!   prefix table.
//!
//! **Overrides** merge into the referenced node's properties and are checked against its kind:
//! a generated form can be relabeled, but a property its kind does not have is refused. Every
//! error is `InvalidArgument` naming the screen and the source line. A reference line inside a
//! `region` is refused: it would be read here, in the screen's space under the caller's whole
//! grant; a `reference` node there resolves in the region's narrowed space ([`crate::region`]).
//!
//! ## Prefixes: ikigai-fn's, not a second expander
//!
//! Short names are ikigai-fn's prefix tables ([`ikigai_fn::prefix_space`]): a table is data (an
//! `ik:Alias` of prefix rules over an empty space), checked by [`ikigai_fn::prefix_check`] (no
//! prefix under `urn:`, `http:` and the other reserved schemes), and the HOST injects its space
//! as a corridor with `Kernel::issue_in`. The compiler never rewrites a name: it issues each
//! reference as written, so a short name resolves inside the corridor, under the caller's own
//! capability, and nowhere else. [`PREFIXES`] is the default table (`ui:` for `urn:iki:ui:`),
//! served here as data; a screen's `@prefixes` table adds to it, and the compiler refuses a
//! screen whose table is not in force (its corridor, built by [`prefix_space`], is not in the
//! request's chain) rather than resolve its short names against whatever else is.
//!
//! ## Resources
//!
//! | name | kind | what it is |
//! |---|---|---|
//! | `urn:iki:ui:screen:name:{screen}:source` | a host's resource | the outline, `text/plain` |
//! | `urn:iki:ui:screen:name:{screen}` | composition ([`screens`]) | the source compiled by [`COMPILER`]: cached, hanging from the source and every reference it read |
//! | [`COMPILER`] | transreptor | an outline (`content`) to `application/vnd.ikigai.ui+json` |
//! | [`PREFIXES`] | data | the default prefix table, Turtle ([`PREFIX_TABLE`]) |
//!
//! ## Bounds
//!
//! The source is bounded BEFORE it is parsed ([`bound`]: [`MAX_SOURCE_BYTES`], [`MAX_LINES`],
//! [`MAX_INDENT`] columns), and the parser is iterative and refuses nesting past
//! [`MAX_NESTING`]. A referenced screen is compiled by its own request, which carries the
//! screens open above it (`open`): a reference to one of them is a cycle, refused at its line,
//! and the chain is bounded by [`MAX_DEPTH`]. Shared references can explode a small source, so
//! every compile counts the nodes it PRODUCES, the referenced subtrees included, and refuses past
//! [`MAX_NODES`] before it copies the subtree that would cross it. A reference to a screen
//! through a prefix that hides its `screen:name:` is not seen as one; the kernel's own depth
//! budget is its bound.

use std::collections::BTreeMap;

use ikigai_core::{
    ArgRef, ArgSpec, AsyncFnEndpoint, Bindings, Description, DynamicSpace, EndpointSpace, Error,
    Grammar, Invocation, InvokeFuture, Iri, ReprType, Representation, Request, Verb,
};

use crate::model::{self, Children, Node, Type, Value};

/// A screen's name is this plus `{screen}`.
pub const SCREEN_PREFIX: &str = "urn:iki:ui:screen:name:";
/// A screen's source is its name plus this.
pub const SOURCE_SUFFIX: &str = ":source";
/// The outline compiler, a transreptor from [`OUTLINE`] to [`model::MEDIA_TYPE`].
pub const COMPILER: &str = "urn:iki:ui:outline:compile";
/// What an outline is served as.
pub const OUTLINE: &str = "text/plain";
/// The default prefix table: `ui:` for `urn:iki:ui:`.
pub const PREFIXES: &str = "urn:iki:ui:prefixes";
/// [`PREFIXES`]' Turtle, an `ik:Alias` that [`ikigai_fn::prefix_check`] accepts.
pub const PREFIX_TABLE: &str = include_str!("../assets/prefixes.ttl");

/// The largest outline read, in bytes.
pub const MAX_SOURCE_BYTES: usize = 64 * 1024;
/// The most lines an outline holds.
pub const MAX_LINES: usize = 4096;
/// The deepest indentation, in columns.
pub const MAX_INDENT: usize = 128;
/// The deepest nesting of lines in one outline.
pub const MAX_NESTING: usize = 32;
/// The most properties one line sets.
pub const MAX_PROPERTIES: usize = 32;
/// The most distinct references one outline reads.
pub const MAX_REFERENCES: usize = 256;
/// The longest chain of screens referencing screens.
pub const MAX_DEPTH: usize = crate::element::MAX_DEPTH;
/// The most nodes one compiled screen holds, referenced subtrees included.
pub const MAX_NODES: usize = crate::element::MAX_NODES;

/// The screens this crate ships as outlines, each overridable from `screen/{name}.outline` in
/// the UI directory.
pub const SOURCES: [crate::Artifact; 1] = [crate::Artifact {
    iri: "urn:iki:ui:screen:name:gallery:source",
    id: "ui-screen-gallery-source",
    file: "screen/gallery.outline",
    media_type: "text/plain; charset=utf-8",
    bytes: include_bytes!("../assets/screen/gallery.outline"),
}];

/// A value as written: its text, and whether it was quoted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Raw {
    /// The text, unescaped.
    pub text: String,
    /// Whether it was `"quoted"` (quoted text is always text).
    pub quoted: bool,
}

/// What a line names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Head {
    /// A core kind.
    Kind(&'static str),
    /// A custom element's IRI ([`model::is_element`]).
    Element(String),
    /// Any other name: its answer is the node.
    Reference(String),
}

/// One line of an outline and the lines nested under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// Its line number in the source, from 1.
    pub number: usize,
    /// What it names.
    pub head: Head,
    /// Its properties or overrides, in order.
    pub properties: Vec<(String, Raw)>,
    /// The lines nested under it.
    pub children: Vec<Line>,
}

/// A parsed outline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outline {
    /// The `@prefixes` table, and its line.
    pub prefixes: Option<(usize, String)>,
    /// The root line.
    pub root: Line,
    /// What a [`Line::number`] counts, for errors: `"line"` for an outline, `"form"` for an
    /// s-expression ([`crate::sexpr`]), whose forms are numbered in reading order.
    pub unit: &'static str,
}

fn at(screen: &str, line: usize, detail: impl std::fmt::Display) -> Error {
    at_unit(screen, "line", line, detail)
}

pub(crate) fn at_unit(screen: &str, unit: &str, n: usize, detail: impl std::fmt::Display) -> Error {
    Error::InvalidArgument {
        name: "screen".into(),
        detail: format!("{screen} {unit} {n}: {detail}"),
    }
}

/// What a written name is: a core kind or structure, a custom element's IRI, or a reference
/// (any other IRI). `None`: none of them. The one rule every front end reads a head by.
pub(crate) fn head(name: &str) -> Option<Head> {
    if let Some(k) = model::kind(name).or_else(|| model::structure(name)) {
        Some(Head::Kind(k.name))
    } else if model::is_element(name) {
        Some(Head::Element(name.to_string()))
    } else if name.contains(':') && Iri::parse(name).is_ok() {
        Some(Head::Reference(name.to_string()))
    } else {
        None
    }
}

/// Refuse a source past [`MAX_SOURCE_BYTES`], [`MAX_LINES`] or [`MAX_INDENT`], or not UTF-8,
/// before anything parses it.
pub fn bound<'a>(screen: &str, bytes: &'a [u8]) -> Result<&'a str, Error> {
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err(at(
            screen,
            0,
            format!(
                "{} bytes is past the bound of {MAX_SOURCE_BYTES}",
                bytes.len()
            ),
        ));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| at(screen, 0, "the outline is not UTF-8"))?;
    for (i, line) in text.lines().enumerate() {
        if i >= MAX_LINES {
            return Err(at(
                screen,
                i + 1,
                format!("past the bound of {MAX_LINES} lines"),
            ));
        }
        let indent = line.len() - line.trim_start_matches(' ').len();
        if indent > MAX_INDENT {
            return Err(at(
                screen,
                i + 1,
                format!("indented past the bound of {MAX_INDENT} columns"),
            ));
        }
    }
    Ok(text)
}

fn word_ok(key: &str) -> bool {
    key.starts_with(|c: char| c.is_ascii_lowercase())
        && key
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// One line's head and properties.
fn tokens(screen: &str, number: usize, text: &str) -> Result<(Head, Vec<(String, Raw)>), Error> {
    let (head, mut rest) = text.split_once(char::is_whitespace).unwrap_or((text, ""));
    let Some(head) = self::head(head) else {
        return Err(at(
            screen,
            number,
            format!("`{head}` is not a kind, an element or a name"),
        ));
    };
    let mut properties: Vec<(String, Raw)> = Vec::new();
    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            break;
        }
        let Some((key, after)) = rest.split_once('=') else {
            return Err(at(screen, number, format!("`{rest}` is not key=value")));
        };
        if !word_ok(key) {
            return Err(at(
                screen,
                number,
                format!("`{key}` is not a property name"),
            ));
        }
        let (raw, tail) = if let Some(quoted) = after.strip_prefix('"') {
            let mut text = String::new();
            let mut chars = quoted.char_indices();
            let mut end = None;
            while let Some((i, c)) = chars.next() {
                match c {
                    '"' => {
                        end = Some(i + 1);
                        break;
                    }
                    '\\' => match chars.next() {
                        Some((_, e @ ('"' | '\\'))) => text.push(e),
                        _ => {
                            return Err(at(
                                screen,
                                number,
                                format!("`{key}`: only \\\" and \\\\ escape in a quoted value"),
                            ))
                        }
                    },
                    c => text.push(c),
                }
            }
            let Some(end) = end else {
                return Err(at(
                    screen,
                    number,
                    format!("`{key}`: the quote is not closed"),
                ));
            };
            let tail = &quoted[end..];
            if !tail.is_empty() && !tail.starts_with(char::is_whitespace) {
                return Err(at(screen, number, format!("`{key}`: text after the quote")));
            }
            (Raw { text, quoted: true }, tail)
        } else {
            let (value, tail) = after.split_once(char::is_whitespace).unwrap_or((after, ""));
            if value.is_empty() || value.contains('"') {
                return Err(at(
                    screen,
                    number,
                    format!("`{key}` has no value (quote one with spaces)"),
                ));
            }
            (
                Raw {
                    text: value.to_string(),
                    quoted: false,
                },
                tail,
            )
        };
        if properties.iter().any(|(k, _)| k == key) {
            return Err(at(screen, number, format!("`{key}` is given twice")));
        }
        if properties.len() >= MAX_PROPERTIES {
            return Err(at(
                screen,
                number,
                format!("more than {MAX_PROPERTIES} properties"),
            ));
        }
        properties.push((key.to_string(), raw));
        rest = tail;
    }
    Ok((head, properties))
}

/// Parse an outline, bounded ([`bound`]) and iteratively: the tree is built with an explicit
/// stack of the lines still open, never by recursion.
pub fn parse(screen: &str, bytes: &[u8]) -> Result<Outline, Error> {
    let text = bound(screen, bytes)?;
    let mut prefixes = None;
    let mut root: Option<Line> = None;
    // The lines still open, outermost first, each with its indentation.
    let mut open: Vec<(usize, Line)> = Vec::new();
    for (i, source) in text.lines().enumerate() {
        let number = i + 1;
        if source.contains('\t') {
            return Err(at(screen, number, "indent with spaces, not tabs"));
        }
        let trimmed = source.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = source.len() - source.trim_start_matches(' ').len();
        if let Some(table) = trimmed.strip_prefix("@prefixes") {
            let table = table.trim();
            if indent != 0 || prefixes.is_some() || root.is_some() || !open.is_empty() {
                return Err(at(
                    screen,
                    number,
                    "@prefixes comes once, unindented, before the first node",
                ));
            }
            if Iri::parse(table).is_err() || table.contains(char::is_whitespace) {
                return Err(at(screen, number, format!("`{table}` is not an IRI")));
            }
            prefixes = Some((number, table.to_string()));
            continue;
        }
        if trimmed.starts_with('@') {
            return Err(at(
                screen,
                number,
                format!("`{trimmed}` is not a directive"),
            ));
        }
        let (head, properties) = tokens(screen, number, trimmed)?;
        let line = Line {
            number,
            head,
            properties,
            children: Vec::new(),
        };
        // Close every open line at this indentation or deeper.
        let mut closed: Option<usize> = None;
        while open.last().is_some_and(|(n, _)| *n >= indent) {
            let (n, done) = open.pop().expect("an open line");
            closed = Some(n);
            match open.last_mut() {
                Some((_, parent)) => parent.children.push(done),
                None => root = Some(done),
            }
        }
        if closed.is_some_and(|n| n != indent) {
            return Err(at(
                screen,
                number,
                "the indentation matches no line above it",
            ));
        }
        if open.is_empty() && root.is_some() {
            return Err(at(
                screen,
                number,
                "an outline has one root line; indent this one under it",
            ));
        }
        if open.len() >= MAX_NESTING {
            return Err(at(
                screen,
                number,
                format!("nested past the bound of {MAX_NESTING}"),
            ));
        }
        open.push((indent, line));
    }
    while let Some((_, done)) = open.pop() {
        match open.last_mut() {
            Some((_, parent)) => parent.children.push(done),
            None => root = Some(done),
        }
    }
    let root = root.ok_or_else(|| at(screen, 0, "the outline has no node"))?;
    Ok(Outline {
        prefixes,
        root,
        unit: "line",
    })
}

/// The dynamic-space name a prefix table's corridor carries: `ui-prefixes:{rest}` for the
/// table `urn:{rest}`. A table's IRI is `urn:` and segments of `[a-z0-9-]`, so its space is
/// named by it.
pub fn prefix_space_name(table: &str) -> Option<String> {
    let rest = table.strip_prefix("urn:")?;
    rest.split(':')
        .all(|s| {
            !s.is_empty()
                && s.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
        .then(|| format!("ui-prefixes:{rest}"))
}

/// The corridor for the prefix table `table`: [`ikigai_fn::prefix_space`] (and its
/// [`prefix_check`](ikigai_fn::prefix_check)) under [`prefix_space_name`]. The host binds the
/// table (`refresh`, or `drive` to follow its edits, behind ikigai-core's `declare` feature) and
/// injects the space with `Kernel::issue_in` around the requests that read screens.
///
/// # Panics
///
/// When `table` is not a `urn:` IRI of `[a-z0-9-]` segments: it is a name in the host's code.
pub fn prefix_space(table: &str) -> DynamicSpace {
    let name = prefix_space_name(table)
        .unwrap_or_else(|| panic!("`{table}` is not urn: and segments of [a-z0-9-]"));
    ikigai_fn::prefix_space(&name, Iri::parse(table).expect("a urn: IRI"))
}

fn space_id(table: &str) -> Option<String> {
    prefix_space_name(table).map(|name| format!("urn:iki:space:dynamic:{name}"))
}

/// The screen a written reference names, when it names one through `…screen:name:{screen}`.
fn screen_of(reference: &str) -> Option<&str> {
    let (_, screen) = reference.rsplit_once("screen:name:")?;
    (!screen.is_empty() && !screen.contains(':')).then_some(screen)
}

fn size(node: &Node) -> usize {
    let mut n = 0;
    let mut stack = vec![node];
    while let Some(node) = stack.pop() {
        n += 1;
        stack.extend(node.children.iter());
        stack.extend(node.replacement.as_deref());
    }
    n
}

/// A value for a property of type `t` (`None`: an element's argument, typed by how it reads).
fn coerce(t: Option<Type>, raw: &Raw) -> Result<Value, String> {
    match t {
        Some(Type::Level) => raw
            .text
            .parse::<i64>()
            .map(Value::Number)
            .map_err(|_| format!("`{}` is not a heading level", raw.text)),
        Some(Type::Flag) => match raw.text.as_str() {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            other => Err(format!("`{other}` is not true or false")),
        },
        Some(_) => Ok(Value::Text(raw.text.clone())),
        None if raw.quoted => Ok(Value::Text(raw.text.clone())),
        None => Ok(match raw.text.as_str() {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            t => t
                .parse::<i64>()
                .map(Value::Number)
                .unwrap_or_else(|_| Value::Text(t.to_string())),
        }),
    }
}

struct Build<'a> {
    screen: &'a str,
    unit: &'static str,
    references: &'a BTreeMap<String, (Node, usize)>,
    produced: usize,
}

impl Build<'_> {
    fn count(&mut self, line: usize, n: usize) -> Result<(), Error> {
        self.produced += n;
        if self.produced > MAX_NODES {
            return Err(at_unit(
                self.screen,
                self.unit,
                line,
                format!("the screen produces more than {MAX_NODES} nodes"),
            ));
        }
        Ok(())
    }

    /// The node `line` compiles to. Recursion is bounded by [`MAX_NESTING`], which the parser
    /// holds the outline to.
    fn node(&mut self, line: &Line) -> Result<Node, Error> {
        let mut node = match &line.head {
            Head::Kind(_) | Head::Element(_) => {
                self.count(line.number, 1)?;
                let kind = match &line.head {
                    Head::Kind(k) => k.to_string(),
                    Head::Element(e) => e.clone(),
                    Head::Reference(_) => unreachable!("matched above"),
                };
                Node {
                    kind,
                    hint: Default::default(),
                    replacement: None,
                    properties: Default::default(),
                    children: Vec::new(),
                }
            }
            Head::Reference(name) => {
                let (node, n) = &self.references[name];
                self.count(line.number, *n)?;
                node.clone()
            }
        };
        let kind = model::kind(&node.kind).or_else(|| model::structure(&node.kind));
        for (key, raw) in &line.properties {
            let t = match kind {
                Some(k) => Some(k.property(key).ok_or_else(|| {
                    at_unit(
                        self.screen,
                        self.unit,
                        line.number,
                        format!("a {} has no property `{key}`", k.name),
                    )
                })?),
                None => None,
            };
            let value =
                coerce(t, raw).map_err(|e| at_unit(self.screen, self.unit, line.number, e))?;
            node.properties.insert(key.clone(), value);
        }
        for child in &line.children {
            let child = self.node(child)?;
            if let Some(k) = kind {
                match k.children {
                    Children::Leaf => {
                        return Err(at_unit(
                            self.screen,
                            self.unit,
                            line.number,
                            format!("a {} takes no children", k.name),
                        ))
                    }
                    Children::Only(only) if child.kind != only => {
                        return Err(at_unit(
                            self.screen,
                            self.unit,
                            line.number,
                            format!("a {} takes only {only} children", k.name),
                        ))
                    }
                    _ => {}
                }
            }
            node.children.push(child);
        }
        // The children are valid already (built first), so a fault here is this line's own.
        let shallow = Node {
            children: Vec::new(),
            ..node.clone()
        };
        model::validate(&shallow).map_err(|e| match e {
            Error::InvalidArgument { detail, .. } => at_unit(
                self.screen,
                self.unit,
                line.number,
                detail.strip_prefix("/: ").unwrap_or(&detail),
            ),
            other => other,
        })?;
        Ok(node)
    }
}

/// Compile `outline` (the source of `screen`) in `inv`: check its prefix table is in force, read
/// every reference once through the kernel (a referenced screen with `open` and this screen),
/// and build and validate the model.
pub async fn compile(
    inv: &Invocation<'_>,
    screen: &str,
    outline: &Outline,
    open: &[String],
) -> Result<Node, Error> {
    if let Some((line, table)) = &outline.prefixes {
        let id = space_id(table).ok_or_else(|| {
            at_unit(
                screen,
                outline.unit,
                *line,
                format!("`{table}` is not a prefix table's name (urn: and segments of [a-z0-9-])"),
            )
        })?;
        let in_force = inv
            .scope()
            .spaces()
            .iter()
            .any(|s| s.id().is_some_and(|i| i.as_str() == id));
        if !in_force {
            return Err(at_unit(
                screen,
                outline.unit,
                *line,
                format!(
                    "the prefix table `{table}` is not in force: the host injects its space \
                     (ikigai_ui::screen::prefix_space) as a corridor with Kernel::issue_in"
                ),
            ));
        }
    }
    let this = screen.strip_prefix(SCREEN_PREFIX).unwrap_or(screen);
    let mut wanted: Vec<(usize, &str)> = Vec::new();
    let mut stack = vec![(&outline.root, false)];
    while let Some((line, in_region)) = stack.pop() {
        if let Head::Reference(name) = &line.head {
            // Read here, it would be read in the screen's space under the caller's whole grant,
            // and a region's interior resolves in its narrowed space (ledger #3410).
            if in_region {
                return Err(at_unit(
                    screen,
                    outline.unit,
                    line.number,
                    format!(
                        "`{name}` inside a region would be read outside it; write a \
                         `reference` node, which resolves in the region's space"
                    ),
                ));
            }
            wanted.push((line.number, name));
        }
        let in_region = in_region || line.head == Head::Kind(crate::region::KIND);
        stack.extend(line.children.iter().map(|c| (c, in_region)));
    }
    wanted.sort();
    let mut references: BTreeMap<String, (Node, usize)> = BTreeMap::new();
    for (line, name) in wanted {
        if references.contains_key(name) {
            continue;
        }
        if references.len() >= MAX_REFERENCES {
            return Err(at_unit(
                screen,
                outline.unit,
                line,
                format!("past {MAX_REFERENCES} references"),
            ));
        }
        let mut request = Request::new(
            Verb::Source,
            Iri::parse(name).map_err(|e| at_unit(screen, outline.unit, line, e))?,
        );
        if let Some(other) = screen_of(name) {
            if other == this || open.iter().any(|o| o == other) {
                let chain: Vec<&str> = open
                    .iter()
                    .map(String::as_str)
                    .chain([this, other])
                    .collect();
                return Err(at_unit(
                    screen,
                    outline.unit,
                    line,
                    format!("a cycle of screens: {}", chain.join(" → ")),
                ));
            }
            if open.len() + 1 >= MAX_DEPTH {
                return Err(at_unit(
                    screen,
                    outline.unit,
                    line,
                    format!("screens nested past the bound of {MAX_DEPTH}"),
                ));
            }
            let chain: Vec<&str> = open.iter().map(String::as_str).chain([this]).collect();
            request = request.with_arg("open", ArgRef::Inline(chain.join(" ").into_bytes()));
        }
        let rep = inv.issue(request).await.map_err(|e| match e {
            Error::InvalidArgument { name: n, detail } if n == "screen" => {
                at_unit(screen, outline.unit, line, format!("`{name}`: {detail}"))
            }
            other => other,
        })?;
        let node = model::parse(&rep.bytes).map_err(|e| {
            at_unit(
                screen,
                outline.unit,
                line,
                format!(
                    "`{name}` does not answer a UI node (an element is written by its full \
                     IRI): {e}"
                ),
            )
        })?;
        let n = size(&node);
        references.insert(name.to_string(), (node, n));
    }
    let mut build = Build {
        screen,
        unit: outline.unit,
        references: &references,
        produced: 0,
    };
    let root = build.node(&outline.root)?;
    model::validate(&root)?;
    Ok(root)
}

fn open_arg(inv: &Invocation<'_>) -> Vec<String> {
    inv.inline_str("open")
        .map(|s| s.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default()
}

/// [`COMPILER`]: an outline in `content` to the model, `application/vnd.ikigai.ui+json`.
pub(crate) fn compiler_endpoint() -> AsyncFnEndpoint {
    let description = Description::new("ui-outline-compile")
        .title("The UI outline compiler")
        .summary(
            "A screen outline (text/plain: one line per node, a kind or a reference with \
             key=value overrides, indentation nesting) compiled to the UI model, every \
             reference read through the kernel.",
        )
        .verb(Verb::Source)
        .verb(Verb::Meta)
        .transreptor([OUTLINE], [model::MEDIA_TYPE])
        .lossy()
        .input(
            ArgSpec::new("content")
                .summary("the outline")
                .class(crate::XSD_STRING),
        )
        .input(
            ArgSpec::new("screen")
                .summary("the screen the outline is the source of, for errors and cycles")
                .class(crate::XSD_STRING)
                .optional(),
        )
        .input(
            ArgSpec::new("open")
                .summary("the screens open above this one, space-separated")
                .class(crate::XSD_STRING)
                .optional(),
        )
        .output(model::MEDIA_TYPE);
    AsyncFnEndpoint::new(
        "ui-outline-compile",
        |inv: &Invocation<'_>| -> InvokeFuture<'_> {
            Box::pin(async move {
                let screen = inv.inline_str("screen").unwrap_or("outline").to_string();
                let outline = parse(&screen, inv.inline_arg("content")?)?;
                let node = compile(inv, &screen, &outline, &open_arg(inv)).await?;
                let bytes = serde_json::to_vec(&node)
                    .map_err(|e| Error::Endpoint(format!("a model serializes: {e}")))?;
                Ok(Representation::new(ReprType::new(model::MEDIA_TYPE), bytes).cacheable())
            })
        },
    )
    .with_description(description)
}

/// `urn:iki:ui:screen:name:{screen}`: its source, read through the kernel and compiled by
/// [`COMPILER`], in the face asked for. `id` names it; `binding` declares `{screen}` (an exact
/// name, the gallery, has none).
pub(crate) fn screen_endpoint(id: &'static str, binding: bool) -> AsyncFnEndpoint {
    let description = Description::new(id)
        .title("A UI screen")
        .summary(
            "A screen compiled from its source, urn:iki:ui:screen:name:{screen}:source (an \
             outline, an s-expression or a Lisp program): application/vnd.ikigai.ui+json, \
             JSON-LD with as=application/ld+json, or the s-expression face with \
             as=text/x-ikigai-ui-sexpr.",
        )
        .verb(Verb::Source)
        .verb(Verb::Meta)
        .input(
            ArgSpec::new("open")
                .summary("the screens open above this one, space-separated")
                .class(crate::XSD_STRING)
                .optional(),
        )
        .output(model::MEDIA_TYPE)
        .output(model::JSON_LD)
        .output(crate::sexpr::MEDIA_TYPE);
    let description = if binding {
        description.input(
            ArgSpec::new("screen")
                .summary("the screen's name, one segment")
                .class(crate::XSD_STRING)
                .binding()
                .example("gallery"),
        )
    } else {
        description
    };
    AsyncFnEndpoint::new(id, |inv: &Invocation<'_>| -> InvokeFuture<'_> {
        Box::pin(async move {
            let screen = inv.request.target.as_str();
            let source = Iri::parse(format!("{screen}{SOURCE_SUFFIX}"))
                .map_err(|e| Error::Endpoint(e.to_string()))?;
            let source = inv.issue(Request::new(Verb::Source, source)).await?;
            // The front end by what the source is (ledger #3367): every one compiles through
            // the same step ([`compile`]).
            let source_expiry = source.expiry;
            let compiler = match source.repr_type.media_type.as_str() {
                crate::sexpr::MEDIA_TYPE => crate::sexpr::COMPILER,
                crate::sexpr::LISP => crate::sexpr::PROGRAM,
                _ => COMPILER,
            };
            let mut request = Request::new(Verb::Source, Iri::parse(compiler).expect("an IRI"))
                .with_arg("as", ArgRef::Inline(model::MEDIA_TYPE.as_bytes().to_vec()))
                .with_arg("content", ArgRef::Inline(source.bytes))
                .with_arg("screen", ArgRef::Inline(screen.as_bytes().to_vec()));
            if let Ok(open) = inv.inline_str("open") {
                request = request.with_arg("open", ArgRef::Inline(open.as_bytes().to_vec()));
            }
            let compiled = inv.issue(request).await?;
            let node: Node = serde_json::from_slice(&compiled.bytes)
                .map_err(|e| Error::Endpoint(format!("the compiled model: {e}")))?;
            // No fresher than its source and its compile (a program's eval is live unless it
            // says otherwise, ledger #3367).
            let mut face = crate::model_face(inv, &node)?;
            face.expiry = face
                .expiry
                .most_restrictive(source_expiry)
                .most_restrictive(compiled.expiry);
            Ok(face)
        })
    })
    .with_description(description)
}

/// `urn:iki:ui:screen:name:{screen}` for a `{screen}` of one segment: `{…}:source` is a
/// different name, never a screen.
struct ScreenName;

impl Grammar for ScreenName {
    fn match_iri(&self, iri: &Iri) -> Option<Bindings> {
        let screen = iri.as_str().strip_prefix(SCREEN_PREFIX)?;
        if screen.is_empty() || screen.contains(':') {
            return None;
        }
        let mut b = Bindings::new();
        b.insert("screen", screen);
        Some(b)
    }

    fn pattern(&self) -> String {
        format!("{SCREEN_PREFIX}{{screen}}")
    }
}

/// Every screen compiled from its outline: `urn:iki:ui:screen:name:{screen}` reads
/// `urn:iki:ui:screen:name:{screen}:source`. A host places it LAST, so a screen it binds as a
/// model directly still answers first, and a screen with no source is the source's `NotFound`.
pub fn screens() -> EndpointSpace {
    EndpointSpace::new().bind(ScreenName, screen_endpoint("ui-screen-name", true))
}
