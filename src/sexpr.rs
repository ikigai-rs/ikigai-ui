//! Screens as s-expressions (ledger #3367): another FRONT END onto the outline's model, never a
//! second compiler. A datum is read into the same [`Outline`] an outline parses to, and
//! [`screen::compile`] (references, overrides, prefixes, bounds, validation) does the rest.
//!
//! ```lisp
//! (@prefixes urn:iki:uidemo:prefixes)
//! (group :label "Contact"
//!   (heading :content "Write to us" :level 1)
//!   (ui:form:resource:urn:iki:uidemo:contact :description "We answer within a day")
//!   (acme:rating :value 4))
//! ```
//!
//! ## Structural: the screen as a datum
//!
//! - A list whose head is a core kind (or a structure, or a custom element's IRI) is a node; any
//!   other head is a REFERENCE (a full IRI, or a short name through the screen's prefix table,
//!   exactly as in an outline), and its `:key value` pairs are overrides. Nested lists are
//!   children, in order, wherever they stand among the pairs. A reference head may take one
//!   string right after it, the rest of its name: `(ui:view:resource "urn:…")` is the reference
//!   `ui:view:resource:urn:…`, so a program composes references from what it read.
//! - A value is a string (text), an integer, or a bare symbol (`true`/`false`, `#t`/`#f`, or a
//!   word), typed by the property it sets as in an outline.
//! - `(@prefixes <iri>)`, before the node, names the screen's prefix table.
//! - The reader is ikigai-sexpr's ([`ikigai_sexpr::parse_all`], the neutral datum every Lisp
//!   adapts into). Its own bounds are generous (512 levels); this front end bounds the source
//!   BEFORE it parses ([`bound`]: [`MAX_SOURCE_BYTES`] and [`MAX_NESTING`] lists), so the reader
//!   never recurses past what a screen may nest.
//!
//! Forms are numbered in reading order (the node lists, from 1), and every error is
//! `InvalidArgument` naming the screen and the form.
//!
//! The profile lives HERE, not in ikigai-sexpr beside its other profiles: what it means (which
//! heads are kinds, how a value is typed, what an override may set) is the UI model's, and the
//! compile step it hands to is this crate's. ikigai-sexpr keeps the neutral datum, its reader
//! and its printer, which is all this profile takes from it.
//!
//! ## Computed: a screen that is a Lisp program
//!
//! A screen's source may be a program ([`LISP`]) whose RESULT is the datum above. [`PROGRAM`]
//! issues it to `urn:lisp:eval` (ikigai-lisp, which the HOST binds: this crate links no Lisp
//! engine), carrying the caller's own capability, so the program's builtins are the kernel verbs
//! under that capability (and `urn:cap:lisp` gates running it at all), with ikigai-lisp's
//! budgets. Its result is DATA, read by the same front end and compiled by the same step: nothing
//! the program computes runs in a face. The compiled screen carries the eval's expiry (live
//! unless the program says `(cacheable …)`) as well as its own.
//!
//! ## The model's s-expression face
//!
//! [`print`] writes a model back as this datum (kinds and elements as heads, properties as
//! `:key value`, children nested and indented), so a generated view can be read and edited as an
//! s-expression; [`read`] of what it prints compiles to the same model. A screen answers it with
//! `as=text/x-ikigai-ui-sexpr`. Hints and replacements have no s-expression spelling (nor an
//! outline one): a model with either is refused, never printed lossily.
//!
//! ## Resources
//!
//! | name | kind | what it is |
//! |---|---|---|
//! | [`COMPILER`] | transreptor | a datum ([`MEDIA_TYPE`], `content`) to `application/vnd.ikigai.ui+json` |
//! | [`PROGRAM`] | transreptor | a program ([`LISP`], `content`), run by `urn:lisp:eval`, to the model |
//!
//! A screen whose `{screen}:source` answers [`MEDIA_TYPE`] or [`LISP`] is compiled by these
//! instead of the outline's [`screen::COMPILER`].

use ikigai_core::{
    ArgRef, ArgSpec, AsyncFnEndpoint, Description, Error, Invocation, InvokeFuture, Iri, ReprType,
    Representation, Request, Verb,
};
use ikigai_sexpr::Sexpr;

use crate::model::{self, Node, Value};
use crate::screen::{self, at_unit, Head, Line, Outline, Raw};

/// What a screen datum is served as.
pub const MEDIA_TYPE: &str = "text/x-ikigai-ui-sexpr";
/// What a screen program is served as: a Lisp program whose result is a [`MEDIA_TYPE`] datum.
pub const LISP: &str = "text/x-ikigai-ui-lisp";
/// The datum compiler, a transreptor from [`MEDIA_TYPE`] to [`model::MEDIA_TYPE`].
pub const COMPILER: &str = "urn:iki:ui:sexpr:compile";
/// The program compiler, a transreptor from [`LISP`] to [`model::MEDIA_TYPE`].
pub const PROGRAM: &str = "urn:iki:ui:lisp:compile";
/// Where a program runs: ikigai-lisp's eval, bound by the host.
pub const LISP_EVAL: &str = "urn:lisp:eval";

/// The largest datum or program read, in bytes: an outline's bound.
pub const MAX_SOURCE_BYTES: usize = screen::MAX_SOURCE_BYTES;
/// The deepest nesting of lists in one datum: an outline's bound on nested lines.
pub const MAX_NESTING: usize = screen::MAX_NESTING;

const UNIT: &str = "form";

fn at(screen: &str, form: usize, detail: impl std::fmt::Display) -> Error {
    at_unit(screen, UNIT, form, detail)
}

/// Refuse a datum past [`MAX_SOURCE_BYTES`] or nested past [`MAX_NESTING`] lists, or not UTF-8,
/// before the reader sees it. The scan skips strings and `;` comments as the reader does.
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
    let text =
        std::str::from_utf8(bytes).map_err(|_| at(screen, 0, "the s-expression is not UTF-8"))?;
    let mut depth = 0usize;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => {
                            chars.next();
                        }
                        '"' => break,
                        _ => {}
                    }
                }
            }
            ';' => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            '(' => {
                depth += 1;
                if depth > MAX_NESTING {
                    return Err(at(
                        screen,
                        0,
                        format!("nested past the bound of {MAX_NESTING} lists"),
                    ));
                }
            }
            ')' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    Ok(text)
}

struct Reader<'a> {
    screen: &'a str,
    forms: usize,
}

impl Reader<'_> {
    fn value(&self, form: usize, key: &str, value: &Sexpr) -> Result<Raw, Error> {
        let unquoted = |text: &str| Raw {
            text: text.to_string(),
            quoted: false,
        };
        match value {
            Sexpr::Str(text) => Ok(Raw {
                text: text.clone(),
                quoted: true,
            }),
            Sexpr::Int(n) => Ok(unquoted(&n.to_string())),
            Sexpr::Symbol(s) if s == "#t" => Ok(unquoted("true")),
            Sexpr::Symbol(s) if s == "#f" => Ok(unquoted("false")),
            Sexpr::Symbol(s) if !s.starts_with(':') => Ok(unquoted(s)),
            _ => Err(at(
                self.screen,
                form,
                format!("`:{key}` has no value (a string, an integer or a word)"),
            )),
        }
    }

    /// The line a node form reads as. Recursion is bounded by [`MAX_NESTING`], which [`bound`]
    /// held the source to.
    fn line(&mut self, form: &Sexpr) -> Result<Line, Error> {
        self.forms += 1;
        let number = self.forms;
        let Sexpr::List(items) = form else {
            return Err(at(
                self.screen,
                number,
                format!("`{}` is not a node (a list)", ikigai_sexpr::write(form)),
            ));
        };
        let head = match items.first() {
            Some(Sexpr::Symbol(name)) => screen::head(name).ok_or_else(|| {
                at(
                    self.screen,
                    number,
                    format!("`{name}` is not a kind, an element or a name"),
                )
            })?,
            Some(other) => {
                return Err(at(
                    self.screen,
                    number,
                    format!(
                        "`{}` is not a kind, an element or a name",
                        ikigai_sexpr::write(other)
                    ),
                ))
            }
            None => return Err(at(self.screen, number, "an empty list is not a node")),
        };
        // `(ui:view:resource "urn:…")`: a reference head and one string after it name the
        // reference `{head}:{string}`, so a program composes a reference from a resource it read.
        let mut items = &items[1..];
        let head = match (head, items.first()) {
            (Head::Reference(name), Some(Sexpr::Str(rest))) => {
                items = &items[1..];
                let joined = format!("{name}:{rest}");
                match screen::head(&joined) {
                    Some(Head::Reference(r)) => Head::Reference(r),
                    _ => return Err(at(self.screen, number, format!("`{joined}` is not a name"))),
                }
            }
            (head, _) => head,
        };
        let mut line = Line {
            number,
            head,
            properties: Vec::new(),
            children: Vec::new(),
        };
        let mut rest = items.iter();
        while let Some(item) = rest.next() {
            match item {
                Sexpr::Symbol(s) if s.starts_with(':') => {
                    let key = &s[1..];
                    if !word_ok(key) {
                        return Err(at(
                            self.screen,
                            number,
                            format!("`{key}` is not a property name"),
                        ));
                    }
                    let value = rest
                        .next()
                        .ok_or_else(|| at(self.screen, number, format!("`:{key}` has no value")))?;
                    let raw = self.value(number, key, value)?;
                    if line.properties.iter().any(|(k, _)| k == key) {
                        return Err(at(self.screen, number, format!("`:{key}` is given twice")));
                    }
                    if line.properties.len() >= screen::MAX_PROPERTIES {
                        return Err(at(
                            self.screen,
                            number,
                            format!("more than {} properties", screen::MAX_PROPERTIES),
                        ));
                    }
                    line.properties.push((key.to_string(), raw));
                }
                Sexpr::List(_) => {
                    let child = self.line(item)?;
                    line.children.push(child);
                }
                other => {
                    return Err(at(
                        self.screen,
                        number,
                        format!(
                            "`{}` is neither a `:key value` pair nor a child node",
                            ikigai_sexpr::write(other)
                        ),
                    ))
                }
            }
        }
        Ok(line)
    }
}

fn word_ok(key: &str) -> bool {
    key.starts_with(|c: char| c.is_ascii_lowercase())
        && key
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Read a screen datum into the outline it means, bounded ([`bound`]) before ikigai-sexpr's
/// reader sees it.
pub fn read(screen: &str, bytes: &[u8]) -> Result<Outline, Error> {
    let text = bound(screen, bytes)?;
    let forms = ikigai_sexpr::parse_all(text).map_err(|e| at(screen, 0, e.detail()))?;
    let mut forms = forms.iter().peekable();
    let mut prefixes = None;
    if let Some(Sexpr::List(items)) = forms.peek() {
        if matches!(items.first(), Some(Sexpr::Symbol(s)) if s == "@prefixes") {
            forms.next();
            let table = match &items[1..] {
                [Sexpr::Symbol(t)] | [Sexpr::Str(t)] => t.clone(),
                _ => return Err(at(screen, 0, "(@prefixes <iri>) names one table")),
            };
            if Iri::parse(&table).is_err() || table.contains(char::is_whitespace) {
                return Err(at(screen, 0, format!("`{table}` is not an IRI")));
            }
            prefixes = Some((0, table));
        }
    }
    let mut reader = Reader { screen, forms: 0 };
    let root = forms
        .next()
        .ok_or_else(|| at(screen, 0, "the s-expression has no node"))?;
    if let Sexpr::List(items) = root {
        if matches!(items.first(), Some(Sexpr::Symbol(s)) if s.starts_with('@')) {
            return Err(at(
                screen,
                0,
                "(@prefixes <iri>) comes once, before the node, and is the only directive",
            ));
        }
    }
    let root = reader.line(root)?;
    if forms.next().is_some() {
        return Err(at(
            screen,
            0,
            "a screen has one root node; nest the others in it",
        ));
    }
    Ok(Outline {
        prefixes,
        root,
        unit: UNIT,
    })
}

/// A model as the datum [`read`] reads back to it. Refused (`InvalidArgument`) when a node has
/// hints or a replacement, which no screen source spells. Recursion is bounded by the model's
/// own depth bound ([`model::MAX_DEPTH`]), which [`model::validate`] holds a model to.
pub fn datum(node: &Node) -> Result<Sexpr, Error> {
    if !node.hint.is_empty() || node.replacement.is_some() {
        return Err(Error::InvalidArgument {
            name: "model".into(),
            detail: format!(
                "a `{}` with hints or a replacement has no s-expression face",
                node.kind
            ),
        });
    }
    let mut items = vec![Sexpr::Symbol(node.kind.clone())];
    for (key, value) in &node.properties {
        items.push(Sexpr::Symbol(format!(":{key}")));
        items.push(match value {
            Value::Number(n) => Sexpr::Int(*n),
            Value::Bool(b) => Sexpr::Symbol(b.to_string()),
            Value::Text(t) => Sexpr::Str(t.clone()),
        });
    }
    for child in &node.children {
        items.push(datum(child)?);
    }
    Ok(Sexpr::List(items))
}

/// A model's s-expression face: [`datum`], one node per line, children indented two spaces.
pub fn print(node: &Node) -> Result<String, Error> {
    let mut out = String::new();
    layout(&datum(node)?, 0, &mut out);
    out.push('\n');
    Ok(out)
}

fn layout(datum: &Sexpr, indent: usize, out: &mut String) {
    let Sexpr::List(items) = datum else {
        out.push_str(&ikigai_sexpr::write(datum));
        return;
    };
    out.push('(');
    for (i, item) in items.iter().enumerate() {
        if let Sexpr::List(_) = item {
            out.push('\n');
            out.push_str(&" ".repeat(indent + 2));
            layout(item, indent + 2, out);
        } else {
            if i > 0 {
                out.push(' ');
            }
            out.push_str(&ikigai_sexpr::write(item));
        }
    }
    out.push(')');
}

fn open_arg(inv: &Invocation<'_>) -> Vec<String> {
    inv.inline_str("open")
        .map(|s| s.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default()
}

async fn compiled(
    inv: &Invocation<'_>,
    screen: &str,
    datum: &[u8],
) -> Result<Representation, Error> {
    let outline = read(screen, datum)?;
    let node = screen::compile(inv, screen, &outline, &open_arg(inv)).await?;
    let bytes = serde_json::to_vec(&node)
        .map_err(|e| Error::Endpoint(format!("a model serializes: {e}")))?;
    Ok(Representation::new(ReprType::new(model::MEDIA_TYPE), bytes).cacheable())
}

fn compiler_description(id: &str, title: &str, summary: &str, from: &str) -> Description {
    Description::new(id)
        .title(title)
        .summary(summary)
        .verb(Verb::Source)
        .verb(Verb::Meta)
        .transreptor([from], [model::MEDIA_TYPE])
        .lossy()
        .input(
            ArgSpec::new("content")
                .summary("the source")
                .class(crate::XSD_STRING),
        )
        .input(
            ArgSpec::new("screen")
                .summary("the screen this is the source of, for errors and cycles")
                .class(crate::XSD_STRING)
                .optional(),
        )
        .input(
            ArgSpec::new("open")
                .summary("the screens open above this one, space-separated")
                .class(crate::XSD_STRING)
                .optional(),
        )
        .output(model::MEDIA_TYPE)
}

/// [`COMPILER`]: a screen datum in `content` to the model.
pub(crate) fn compiler_endpoint() -> AsyncFnEndpoint {
    AsyncFnEndpoint::new(
        "ui-sexpr-compile",
        |inv: &Invocation<'_>| -> InvokeFuture<'_> {
            Box::pin(async move {
                let screen = inv
                    .inline_str("screen")
                    .unwrap_or("s-expression")
                    .to_string();
                compiled(inv, &screen, inv.inline_arg("content")?).await
            })
        },
    )
    .with_description(compiler_description(
        "ui-sexpr-compile",
        "The UI s-expression compiler",
        "A screen as an s-expression (text/x-ikigai-ui-sexpr: a list per node, a kind or a \
         reference head with :key value overrides, nested lists children), read by \
         ikigai-sexpr's reader and compiled by the outline's compile step to the UI model.",
        MEDIA_TYPE,
    ))
}

/// [`PROGRAM`]: a Lisp program in `content`, run by [`LISP_EVAL`] under the caller's
/// capability, its result read as a datum and compiled to the model.
pub(crate) fn program_endpoint() -> AsyncFnEndpoint {
    AsyncFnEndpoint::new(
        "ui-lisp-compile",
        |inv: &Invocation<'_>| -> InvokeFuture<'_> {
            Box::pin(async move {
                let screen = inv.inline_str("screen").unwrap_or("program").to_string();
                let program = inv.inline_arg("content")?;
                if program.len() > MAX_SOURCE_BYTES {
                    return Err(at(
                        &screen,
                        0,
                        format!(
                            "the program is {} bytes, past the bound of {MAX_SOURCE_BYTES}",
                            program.len()
                        ),
                    ));
                }
                let eval = Request::new(Verb::Source, Iri::parse(LISP_EVAL).expect("an IRI"))
                    .with_arg("in", ArgRef::Inline(program.to_vec()));
                let result = inv.issue(eval).await.map_err(|e| match e {
                    Error::InvalidArgument { name, detail } => at(
                        &screen,
                        0,
                        format!("the program is refused (`{name}`): {detail}"),
                    ),
                    other => other,
                })?;
                // The screen is no fresher than the eval: live unless the program said
                // `(cacheable …)`, and then no fresher than what it sourced.
                let mut rep = compiled(inv, &screen, &result.bytes).await?;
                rep.expiry = rep.expiry.most_restrictive(result.expiry);
                Ok(rep)
            })
        },
    )
    .with_description(compiler_description(
        "ui-lisp-compile",
        "The UI Lisp screen compiler",
        "A screen that is a Lisp program (text/x-ikigai-ui-lisp), run by urn:lisp:eval under \
         the caller's capability and ikigai-lisp's budgets; its result, a screen s-expression, \
         is read and compiled to the UI model. The program produces data, never code for a face.",
        LISP,
    ))
}
