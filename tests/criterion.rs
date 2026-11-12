//! Accessibility acceptance criteria per kind, element and target, as data (ledgers #3541 and
//! #3542): the sheets and the closed shape agree, the criteria resolve by name in their Turtle
//! and text faces and are cached, each kind's descriptor links its own, every SEE check with a
//! probe on the html face is verified against the rendered HTML of every node of its kind or
//! element on the gallery and elements screens, and the support matrix counts them. A SEE check
//! with no probe is reported as not verified, never skipped; the gpui face's probes run in the
//! window tests (`gpui/src/native.rs`). The probes read the HTML with scraper (html5ever) and
//! the stylesheets with cssparser, never with a reader of this test's own (ledger #3560).

mod common;

use std::sync::Arc;

use cssparser::{
    parse_important, AtRuleParser, CowRcStr, DeclarationParser, Delimiter, ParseError, Parser,
    ParserState, QualifiedRuleParser, RuleBodyItemParser, RuleBodyParser, SourceLocation,
    StyleSheetParser, Token,
};
use futures::executor::block_on;
use ikigai_core::{ArgRef, Capability, Error, Iri, Kernel, Request, Verb};
use ikigai_ui::criterion::{self, Check, Criterion, Mode, TARGETS};
use ikigai_ui::model::{self, Node, Value, KINDS};
use ikigai_ui::testkit::render;
use scraper::{node::Element, ElementRef, Html};

fn kernel() -> Kernel {
    Kernel::new(Arc::new(ikigai_ui::space()))
}

fn read(kernel: &Kernel, iri: &str, as_type: Option<&str>) -> Result<(String, String), Error> {
    let mut request = Request::new(Verb::Source, Iri::parse(iri).unwrap());
    if let Some(t) = as_type {
        request = request.with_arg("as", ArgRef::Inline(t.as_bytes().to_vec()));
    }
    let rep = block_on(kernel.issue(request, &Capability::root()))?;
    Ok((
        rep.repr_type.media_type.clone(),
        String::from_utf8(rep.bytes).unwrap(),
    ))
}

fn conforms(data: &str) -> bool {
    ikigai_shacl::validate_report(data, criterion::SHAPE)
        .expect("the criterion shape is in the subset")
        .conforms
}

fn button_html() -> &'static Criterion {
    criterion::criterion("button", "html").unwrap()
}

#[test]
fn every_criterion_conforms_to_the_closed_shape() {
    let mut seen = 0;
    for kind in criterion::kinds() {
        for target in TARGETS {
            let c = criterion::criterion(kind, target).expect("a criterion per target");
            let data = criterion::turtle(kind, c);
            let report = ikigai_shacl::validate_report(&data, criterion::SHAPE).unwrap();
            assert!(report.conforms, "{kind} on {target}: {report:?}\n{data}");
            // SEE and HEAR, settings and keys: every criterion has each.
            assert!(c.checks().any(|(_, _, k)| k.mode == Mode::See));
            assert!(c.checks().any(|(_, _, k)| k.mode == Mode::Hear));
            seen += 1;
        }
    }
    assert_eq!(
        seen,
        2 * (KINDS.len() + criterion::ELEMENTS.len()),
        "every kind and element"
    );
    // The shape's closed set of probes is the Rust ones, html's then gpui's own.
    let mut all: Vec<&str> = criterion::PROBES.to_vec();
    all.extend(
        criterion::GPUI_PROBES
            .iter()
            .filter(|p| !criterion::PROBES.contains(p)),
    );
    let probes = format!("\"^({})$\"", all.join("|"));
    assert!(
        criterion::SHAPE.contains(&probes),
        "the shape's probes are {probes}"
    );
}

/// A change the shape refuses, as the Turtle face, and the same change to the sheet, which the
/// Rust validator refuses.
#[test]
fn the_shape_and_the_rust_validator_refuse_the_same() {
    let valid = criterion::turtle("button", button_html());
    let gpui_valid = criterion::turtle("button", criterion::criterion("button", "gpui").unwrap());
    let html = r#""target": "html""#;
    let json = include_str!("../assets/criterion/button.json");
    let cases: [(&str, String, String); 12] = [
        (
            "a property the shape does not name",
            valid.replacen("  c:semantics ", "  c:color \"red\" ;\n  c:semantics ", 1),
            json.replacen(html, r#""target": "html", "color": "red""#, 1),
        ),
        (
            "a check with a property the shape does not name",
            valid.replacen(
                "  c:mode \"see\" ;",
                "  c:mode \"see\" ;\n  c:color \"red\" ;",
                1,
            ),
            json.replacen(r#""mode": "see","#, r#""mode": "see", "color": "red","#, 1),
        ),
        (
            "a mode that is neither see nor hear",
            valid.replacen("c:mode \"see\"", "c:mode \"smell\"", 1),
            json.replacen(r#""mode": "see""#, r#""mode": "smell""#, 1),
        ),
        (
            "no role",
            valid.replacen("  c:role \"Button.\" ;\n", "", 1),
            json.replacen(r#""role": "Button.","#, "", 1),
        ),
        (
            "a probe that is not one",
            valid.replacen("c:probe \"element\"", "c:probe \"vibes\"", 1),
            json.replacen(r#""probe": "element""#, r#""probe": "vibes""#, 1),
        ),
        (
            "a target that is not one",
            valid.replacen("target:name:html>", "target:name:tty>", 1),
            json.replacen(html, r#""target": "tty""#, 1),
        ),
        (
            "an empty statement",
            valid.replacen(
                "c:statement \"The face emits a native <button> element.\"",
                "c:statement \"\"",
                1,
            ),
            json.replacen(
                r#""statement": "The face emits a native <button> element.""#,
                r#""statement": """#,
                1,
            ),
        ),
        (
            "a source that is not an https page",
            valid.replacen(
                "<https://github.com/tmobile",
                "<http://github.com/tmobile",
                1,
            ),
            json.replacen("https://github.com/tmobile", "http://github.com/tmobile", 1),
        ),
        (
            "a blocker that is not one",
            gpui_valid.replacen("c:blocked \"toolkit\"", "c:blocked \"mood\"", 1),
            json.replacen(r#""blocked": "toolkit""#, r#""blocked": "mood""#, 1),
        ),
        (
            "an announcement on a platform that is not one",
            gpui_valid.replacen("c:platform \"linux\"", "c:platform \"beos\"", 1),
            json.replacen(r#""platform": "linux""#, r#""platform": "beos""#, 1),
        ),
        (
            "an announcement with no words",
            gpui_valid.replacen(
                "  c:says \"\\\"Send, button\\\"",
                "  c:said \"\\\"Send, button\\\"",
                1,
            ),
            json.replacen(
                r#""says": "\"Send, button\"#,
                r#""said": "\"Send, button\"#,
                1,
            ),
        ),
        (
            "a kind that is not a kind's name",
            valid.replacen(
                "c:kind <urn:iki:ui:kind:name:button>",
                "c:kind <urn:iki:ui:widget:button>",
                1,
            ),
            json.replacen(r#""kind": "button""#, r#""kind": "widget""#, 1),
        ),
    ];
    assert!(conforms(&valid) && conforms(&gpui_valid));
    assert!(criterion::parse(json).is_ok());
    // An element's criterion names its element as its kind, and only one.
    let meter = criterion::turtle("meter", criterion::criterion("meter", "html").unwrap());
    assert!(conforms(&meter), "{meter}");
    assert!(meter.contains("c:kind <urn:iki:ui:element:name:meter>"));
    for broken in [
        meter.replacen(
            "  c:kind <urn:iki:ui:element:name:meter> ;\n",
            "  c:kind <urn:iki:ui:element:name:meter> ;\n  c:kind <urn:iki:ui:kind:name:text> ;\n",
            1,
        ),
        meter.replacen("  c:kind <urn:iki:ui:element:name:meter> ;\n", "", 1),
        meter.replacen("element:name:meter>", "widget:name:meter>", 1),
    ] {
        assert_ne!(broken, meter);
        assert!(!conforms(&broken), "the shape refuses {broken}");
    }
    for (what, turtle, sheet) in cases {
        assert!(
            turtle != valid && turtle != gpui_valid,
            "{what}: the Turtle case changed something"
        );
        assert_ne!(sheet, json, "{what}: the sheet case changed something");
        assert!(!conforms(&turtle), "the shape refuses {what}");
        assert!(criterion::parse(&sheet).is_err(), "Rust refuses {what}");
    }
    // What only Rust says: a HEAR check has no probe, a setting is SEE, the targets in order, a
    // blocked check has no probe, an announcement per platform on gpui and none on html.
    for (what, sheet) in [
        (
            "a probe on a HEAR check",
            json.replacen(
                r#""mode": "hear", "statement": "WHEN I Tab"#,
                r#""mode": "hear", "probe": "focusable", "statement": "WHEN I Tab"#,
                1,
            ),
        ),
        (
            "a HEAR setting",
            json.replacen(
                r#""mode": "see", "statement": "WHEN I set my text size"#,
                r#""mode": "hear", "statement": "WHEN I set my text size"#,
                1,
            ),
        ),
        (
            "gpui before html",
            json.replacen(html, r#""target": "gpui""#, 1).replacen(
                r#""target": "gpui""#,
                r#""target": "html""#,
                2,
            ),
        ),
        (
            "a probe on a blocked check",
            json.replacen(
                r#""mode": "hear", "statement": "WHEN I Tab to the button with VoiceOver"#,
                r#""mode": "see", "probe": "focusable", "statement": "WHEN I Tab to the button with VoiceOver"#,
                1,
            ),
        ),
        (
            "an announcement on the html face",
            json.replacen(
                r#""name": "The button's label, the same words as its visible text; its purpose"#,
                r#""announce": [{"platform": "macos", "reader": "VoiceOver", "api": "AXButton", "says": "Send, button"}], "name": "The button's label, the same words as its visible text; its purpose"#,
                1,
            ),
        ),
        (
            "a gpui face missing a platform's announcement",
            json.replacen(r#"{"platform": "windows""#, r#"{"platform": "linux""#, 1),
        ),
        (
            "an element probe with no element",
            json.replacen(r#", "argument": "button""#, "", 1),
        ),
        (
            "a gpui probe on the html face",
            json.replacen(r#""probe": "focusable""#, r#""probe": "toggles""#, 1),
        ),
        (
            "an html probe on the gpui face",
            json.replacen(r#""probe": "tab-stop""#, r#""probe": "focusable""#, 1),
        ),
        (
            "an element that is not one of this crate's",
            include_str!("../assets/criterion/meter.json").replacen(
                r#""kind": "meter""#,
                r#""kind": "rating""#,
                1,
            ),
        ),
    ] {
        assert_ne!(sheet, json, "{what}: the case changed something");
        assert!(criterion::parse(&sheet).is_err(), "Rust refuses {what}");
    }
}

#[test]
fn criteria_resolve_by_name_in_turtle_and_text_and_are_cached() {
    let kernel = kernel();
    for kind in criterion::kinds() {
        for target in TARGETS {
            let c = criterion::criterion(kind, target).unwrap();
            let iri = criterion::iri(kind, target);
            let what = if criterion::is_element(kind) {
                "element"
            } else {
                "kind"
            };
            assert_eq!(
                iri,
                format!("urn:iki:ui:criterion:{what}:{kind}:target:{target}")
            );
            let (t, body) = read(&kernel, &iri, None).unwrap();
            assert_eq!(t, "text/turtle");
            assert_eq!(body, criterion::turtle(kind, c));
            assert!(body.contains(&format!("<{iri}> a c:Criterion")));
            let (t, words) = read(&kernel, &iri, Some("text/plain")).unwrap();
            assert_eq!(t, "text/plain");
            assert_eq!(words, criterion::text(kind, c));
            for line in [
                format!("{kind} on {target}: accessibility acceptance criteria"),
                format!("  Name: {}", c.name),
                format!("  Role: {}", c.role),
            ] {
                assert!(words.contains(&line), "{iri}: {line} in\n{words}");
            }
            for (_, _, check) in c.checks() {
                assert!(words.contains(&check.statement));
            }
        }
    }
    // Cached: a second read is the first's, and the kernel never computed it uncached.
    let iri = criterion::iri("field", "html");
    assert_eq!(read(&kernel, &iri, None), read(&kernel, &iri, None));
    let request = Request::new(Verb::Source, Iri::parse("urn:kernel:uncached").unwrap())
        .with_arg("target", ArgRef::Inline(iri.as_bytes().to_vec()));
    let report = block_on(kernel.issue(request, &Capability::root())).unwrap();
    let report = String::from_utf8(report.bytes).unwrap();
    assert!(report.contains("not computed uncached"), "{report}");
    // The vocabulary and shape by name.
    let (t, shape) = read(&kernel, criterion::SHAPE_IRI, None).unwrap();
    assert_eq!(
        (t.as_str(), shape.as_str()),
        ("text/turtle", criterion::SHAPE)
    );
    // Every kind has criteria now, each element under its own name: an element as a kind or a
    // kind as an element, the example element with none, a target that is not one, a name
    // that is not a kind.
    assert_eq!(
        criterion::iri("key-value", "gpui"),
        "urn:iki:ui:criterion:element:key-value:target:gpui"
    );
    for kind in KINDS.iter().map(|k| k.name) {
        assert!(criterion::sheet(kind).is_some(), "{kind} has criteria");
    }
    for iri in [
        "urn:iki:ui:criterion:kind:meter:target:html".to_string(),
        "urn:iki:ui:criterion:element:button:target:html".to_string(),
        "urn:iki:ui:criterion:element:rating:target:html".to_string(),
        criterion::iri("button", "tty"),
        criterion::iri("blink", "html"),
    ] {
        assert!(
            matches!(read(&kernel, &iri, None), Err(Error::NotFound(_))),
            "{iri} is NotFound"
        );
    }
}

#[test]
fn each_kind_descriptor_links_its_own_criteria() {
    let link = "<urn:iki:ui:criterion:term:criterion>";
    for k in &KINDS {
        let has = criterion::sheet(k.name).is_some();
        assert_eq!(k.shape.contains(link), has, "{}", k.name);
        if has {
            let iris: Vec<String> = TARGETS
                .iter()
                .map(|t| format!("<{}>", criterion::iri(k.name, t)))
                .collect();
            assert!(
                k.shape.contains(&format!(
                    "<{}{}> {link} {}",
                    model::KIND_PREFIX,
                    k.name,
                    iris.join(", ")
                )),
                "{}",
                k.name
            );
        }
    }
}

#[test]
fn every_source_is_attributed_in_the_notice() {
    let notice = include_str!("../NOTICE");
    for kind in criterion::kinds() {
        for target in TARGETS {
            let sources = &criterion::criterion(kind, target).unwrap().source;
            // Wording seeded from a GitHub repository is attributed; a platform's guidance
            // (the gpui face's sources) is drawn on, not copied.
            for source in sources
                .iter()
                .filter(|s| s.starts_with("https://github.com/"))
                // This repository's own ACCESSIBILITY.md (meter, scene) is no third party's.
                .filter(|s| !s.starts_with("https://github.com/ikigai-rs/ikigai-ui/"))
            {
                let repo = source.split('/').take(5).collect::<Vec<_>>().join("/");
                assert!(notice.contains(&repo), "{repo} in NOTICE");
                let commit = source.split('/').nth(6).unwrap();
                assert!(notice.contains(commit), "{commit} in NOTICE");
            }
        }
    }
    assert!(notice.contains("Apache License, Version 2.0"));
}

// --- the SEE checks against the gallery's rendered HTML ---------------------------------------

/// An element of a node's HTML, as html5ever parsed it: its name, its attributes, and its place
/// in document order.
#[derive(Clone, Copy)]
struct Tag<'a> {
    element: ElementRef<'a>,
    name: &'a str,
    start: usize,
}

impl<'a> Tag<'a> {
    fn get(&self, name: &str) -> Option<&'a str> {
        self.element.attr(name)
    }
}

/// The elements of `html`, a node's HTML parsed as a fragment the way a browser parses one
/// (scraper, html5ever underneath), in document order.
fn tags(html: &Html) -> Vec<Tag<'_>> {
    html.root_element()
        .descendants()
        .filter_map(ElementRef::wrap)
        // The fragment's <html> root is the parser's, not the node's.
        .skip(1)
        .enumerate()
        .map(|(start, element)| {
            let value: &Element = element.value();
            Tag {
                element,
                name: value.name(),
                start,
            }
        })
        .collect()
}

/// The text inside `tag`, its references decoded and any markup in it dropped.
fn text_of(tag: &Tag) -> String {
    tag.element.text().collect::<String>().trim().to_string()
}

/// What a `<div>` wrapper's label names: its control.
const LABELLED: [&str; 5] = ["input", "select", "textarea", "meter", "button"];

/// The node's subject: its HTML's outermost element, or the control in a `<div>` wrapper.
fn subject<'a, 'b>(tags: &'b [Tag<'a>]) -> Option<&'b Tag<'a>> {
    let root = tags.first()?;
    if root.name != "div" {
        return Some(root);
    }
    tags.iter()
        .find(|t| LABELLED.contains(&t.name))
        .or(Some(root))
}

/// Whether `tag` is in the tab order natively.
fn natively_focusable(tag: &Tag) -> bool {
    match tag.name {
        "a" => tag.get("href").is_some_and(|h| !h.is_empty()),
        "input" => tag.get("type") != Some("hidden"),
        "button" | "select" | "textarea" | "summary" => true,
        _ => false,
    }
}

/// `tag`, `tag[name]` or `tag[name=value]` matches `t`: a probe's argument, not CSS (`0` is no
/// CSS attribute value).
fn matches(selector: &str, t: &Tag) -> bool {
    let (name, attribute) = match selector.split_once('[') {
        Some((name, rest)) => (name, rest.strip_suffix(']')),
        None => (selector, None),
    };
    t.name == name
        && attribute.is_none_or(|a| match a.split_once('=') {
            Some((n, v)) => t.get(n) == Some(v),
            None => t.get(a).is_some(),
        })
}

fn property(node: &Node, name: &str) -> Option<String> {
    node.properties.get(name).map(|v| match v {
        Value::Text(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
    })
}

/// The stylesheets a gallery page loads from this crate: the tokens and the opt-in base.
struct Stylesheets {
    css: Vec<(String, String)>,
}

/// A style rule as cssparser reads it: each selector of its list, nesting resolved, and its
/// declarations, in order.
struct Rule {
    selectors: Vec<String>,
    declarations: Vec<Declaration>,
}

impl Rule {
    /// The declaration of `property` that wins in this rule: its last.
    fn declaration(&self, property: &str) -> Option<&Declaration> {
        self.declarations
            .iter()
            .rev()
            .find(|d| d.property == property)
    }
}

struct Declaration {
    /// Lowercase, as CSS property names are ASCII case-insensitive.
    property: String,
    /// The value's source, without `!important`.
    value: String,
    /// The value's component values, a function's arguments after it.
    atoms: Vec<Atom>,
}

/// A component value the probes read; units are lowercase.
#[derive(Debug, PartialEq)]
enum Atom {
    Ident(String),
    Number(f32),
    Dimension(f32, String),
    Function(String),
}

impl Declaration {
    fn has_ident(&self, word: &str) -> bool {
        self.atoms
            .iter()
            .any(|a| matches!(a, Atom::Ident(i) if i.eq_ignore_ascii_case(word)))
    }

    fn px(&self) -> impl Iterator<Item = f32> + '_ {
        self.atoms.iter().filter_map(|a| match a {
            Atom::Dimension(v, unit) if unit == "px" => Some(*v),
            _ => None,
        })
    }
}

/// The component values of a declaration's value. A `{}` block is no value: it makes the
/// declaration fail, so a nested rule such as `a:hover { … }` is read as a rule.
fn atoms<'i>(input: &mut Parser<'i>) -> Result<Vec<Atom>, ParseError<()>> {
    let mut out = Vec::new();
    while let Ok(token) = input.next() {
        let nested = match token {
            Token::Ident(i) => {
                out.push(Atom::Ident(i.to_string()));
                false
            }
            Token::Number { value, .. } => {
                out.push(Atom::Number(*value));
                false
            }
            Token::Dimension { value, unit, .. } => {
                out.push(Atom::Dimension(*value, unit.to_ascii_lowercase()));
                false
            }
            Token::Function(name) => {
                out.push(Atom::Function(name.to_ascii_lowercase()));
                true
            }
            Token::ParenthesisBlock | Token::SquareBracketBlock => true,
            Token::CurlyBracketBlock => return Err(ParseError::custom(())),
            _ => false,
        };
        if nested {
            out.extend(input.parse_nested_block(atoms)?);
        }
    }
    Ok(out)
}

enum Item {
    Declaration(Declaration),
    Rule,
}

/// Reads a stylesheet into `rules`: style rules at any depth, inside at-rules or nested in
/// another style rule (`parents`, whose selectors a nested `&` stands for).
struct Reader<'a> {
    parents: Vec<String>,
    rules: &'a mut Vec<Rule>,
    errors: &'a mut Vec<String>,
}

impl Reader<'_> {
    /// A block's declarations, its nested rules read into `rules`.
    fn body(&mut self, input: &mut Parser) -> Vec<Declaration> {
        let mut declarations = Vec::new();
        let mut items = RuleBodyParser::new(input, self);
        let mut errors = Vec::new();
        for item in &mut items {
            match item {
                Ok(Item::Declaration(d)) => declarations.push(d),
                Ok(Item::Rule) => {}
                Err((_, source, at)) => errors.push(unread(source, at)),
            }
        }
        self.errors.extend(errors);
        declarations
    }
}

fn unread(source: &str, at: SourceLocation) -> String {
    format!("{}:{}: cannot read {source:?}", at.line + 1, at.column)
}

impl<'i> DeclarationParser<'i> for Reader<'_> {
    type Declaration = Item;
    type Error = ();

    fn parse_value(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i>,
        _: &ParserState,
    ) -> Result<Item, ParseError<()>> {
        let start = input.position();
        let atoms = input.parse_until_before(Delimiter::Bang, atoms)?;
        let value = input.slice_from(start).trim().to_string();
        // `!important` is no part of the value: `outline: none !important` is `none`.
        let _ = input.try_parse(parse_important);
        input.expect_exhausted()?;
        let property = if name.starts_with("--") {
            name.to_string()
        } else {
            name.to_ascii_lowercase()
        };
        Ok(Item::Declaration(Declaration {
            property,
            value,
            atoms,
        }))
    }
}

impl<'i> QualifiedRuleParser<'i> for Reader<'_> {
    type Prelude = Vec<String>;
    type QualifiedRule = Item;
    type Error = ();

    fn parse_prelude(&mut self, input: &mut Parser<'i>) -> Result<Vec<String>, ParseError<()>> {
        let selectors = input.parse_comma_separated(|input| {
            let start = input.position();
            while input.next().is_ok() {}
            Ok(input.slice_from(start).trim().to_string())
        })?;
        if self.parents.is_empty() {
            return Ok(selectors);
        }
        // CSS nesting: `&` is the parent, and a selector without one is its descendant.
        Ok(self
            .parents
            .iter()
            .flat_map(|parent| {
                selectors.iter().map(move |s| {
                    if s.contains('&') {
                        s.replace('&', parent)
                    } else {
                        format!("{parent} {s}")
                    }
                })
            })
            .collect())
    }

    fn parse_block(
        &mut self,
        selectors: Vec<String>,
        _: &ParserState,
        input: &mut Parser<'i>,
    ) -> Result<Item, ParseError<()>> {
        let declarations = Reader {
            parents: selectors.clone(),
            rules: &mut *self.rules,
            errors: &mut *self.errors,
        }
        .body(input);
        self.rules.push(Rule {
            selectors,
            declarations,
        });
        Ok(Item::Rule)
    }
}

impl<'i> AtRuleParser<'i> for Reader<'_> {
    type Prelude = String;
    type AtRule = Item;
    type Error = ();

    fn parse_prelude(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i>,
    ) -> Result<String, ParseError<()>> {
        while input.next().is_ok() {}
        Ok(format!("@{name}"))
    }

    fn rule_without_block(&mut self, _: String, _: &ParserState) -> Result<Item, ()> {
        Ok(Item::Rule)
    }

    /// An at-rule's block holds rules (`@media`), or declarations (`@font-face`, or a
    /// conditional rule nested in a style rule, which apply to its parent).
    fn parse_block(
        &mut self,
        name: String,
        _: &ParserState,
        input: &mut Parser<'i>,
    ) -> Result<Item, ParseError<()>> {
        let declarations = Reader {
            parents: self.parents.clone(),
            rules: &mut *self.rules,
            errors: &mut *self.errors,
        }
        .body(input);
        if !declarations.is_empty() && !self.parents.is_empty() {
            self.rules.push(Rule {
                selectors: self.parents.clone(),
                declarations,
            });
        } else if !declarations.is_empty() {
            self.rules.push(Rule {
                selectors: vec![name],
                declarations,
            });
        }
        Ok(Item::Rule)
    }
}

impl<'i> RuleBodyItemParser<'i, Item, ()> for Reader<'_> {
    fn parse_declarations(&self) -> bool {
        true
    }

    fn parse_qualified(&self) -> bool {
        true
    }
}

/// The style rules of `css`, read by cssparser (CSS Syntax Level 3): comments, strings, escapes,
/// `!important` and nesting as a browser reads them. Anything it cannot read is an error.
fn rules(css: &str) -> Result<Vec<Rule>, String> {
    let mut input = Parser::new(css);
    let mut rules = Vec::new();
    let mut errors = Vec::new();
    let mut reader = Reader {
        parents: Vec::new(),
        rules: &mut rules,
        errors: &mut errors,
    };
    let mut unreadable = Vec::new();
    for item in StyleSheetParser::new(&mut input, &mut reader) {
        if let Err((_, source, at)) = item {
            unreadable.push(unread(source, at));
        }
    }
    errors.extend(unreadable);
    if errors.is_empty() {
        Ok(rules)
    } else {
        Err(errors.join("; "))
    }
}

/// The rules of every sheet, each with its sheet's name.
fn sheet_rules(sheets: &Stylesheets) -> Result<Vec<(&str, Rule)>, String> {
    let mut out = Vec::new();
    for (name, css) in &sheets.css {
        let read = rules(css).map_err(|e| format!("{name}: {e}"))?;
        out.extend(read.into_iter().map(|r| (name.as_str(), r)));
    }
    Ok(out)
}

/// Verify one SEE probe of `node`'s html face: `html` is its rendered HTML.
fn verify(
    probe: &str,
    argument: Option<&str>,
    node: &Node,
    html: &str,
    sheets: &Stylesheets,
) -> Result<(), String> {
    let doc = Html::parse_fragment(html);
    let tags = tags(&doc);
    let subject = subject(&tags).ok_or("no element")?;
    let control = if subject.name == "details" {
        tags.iter()
            .find(|t| t.name == "summary")
            .ok_or("a <details> with no <summary>")?
    } else {
        subject
    };
    let label = property(node, "label").unwrap_or_default();
    match probe {
        "element" => {
            let want = argument.ok_or("element names no element")?;
            if subject.name != want {
                return Err(format!("the subject is <{}>, not <{want}>", subject.name));
            }
        }
        "attribute" => {
            let (name, value) = argument
                .and_then(|a| a.split_once('='))
                .ok_or("attribute is name=value")?;
            if subject.get(name) != Some(value) {
                return Err(format!("{name} is {:?}, not {value:?}", subject.get(name)));
            }
        }
        "contains" => {
            let selector = argument.ok_or("contains names no element")?;
            if !tags.iter().any(|t| matches(selector, t)) {
                return Err(format!("nothing matches {selector}"));
            }
        }
        "focusable" => {
            if !natively_focusable(control) {
                return Err(format!(
                    "<{}> is not in the tab order natively",
                    control.name
                ));
            }
            if control.get("disabled").is_some() {
                return Err("disabled".into());
            }
            if let Some(t) = control.get("tabindex") {
                if t.trim().starts_with('-') {
                    return Err(format!("tabindex={t}"));
                }
            }
        }
        "no-tab-stop" => {
            for t in &tags {
                if natively_focusable(t) {
                    return Err(format!("<{}> is in the tab order", t.name));
                }
                if let Some(i) = t.get("tabindex") {
                    if !i.trim().starts_with('-') {
                        return Err(format!("<{}> has tabindex={i}", t.name));
                    }
                }
            }
        }
        "focus-visible" => {
            let rules = sheet_rules(sheets)?;
            let ring = rules
                .iter()
                .find_map(|(_, rule)| {
                    rule.selectors
                        .iter()
                        .any(|s| s == ":focus-visible")
                        .then(|| rule.declaration("outline"))
                        .flatten()
                })
                .ok_or("no :focus-visible outline")?;
            let outline = &ring.value;
            let width = ring
                .px()
                .next()
                .ok_or(format!("outline {outline} has no px width"))?;
            let focus = ring.atoms.windows(2).any(|w| {
                w[0] == Atom::Function("var".into()) && w[1] == Atom::Ident("--focus".into())
            });
            if width < 2.0 || !focus {
                return Err(format!("outline {outline} is not 2px or more in --focus"));
            }
            // Nothing takes the ring away from the control: no outline, outline-style or
            // outline-width that is none, hidden or zero.
            for (name, rule) in &rules {
                let off = rule.declarations.iter().any(|d| {
                    ["outline", "outline-style", "outline-width"].contains(&d.property.as_str())
                        && (d.has_ident("none")
                            || d.has_ident("hidden")
                            || d.atoms.iter().any(|a| {
                                matches!(a, Atom::Number(v) | Atom::Dimension(v, _) if *v == 0.0)
                            }))
                });
                let hits = rule.selectors.iter().find(|s| {
                    s.starts_with(control.name)
                        && s[control.name.len()..]
                            .chars()
                            .next()
                            .is_none_or(|c| !c.is_ascii_alphanumeric())
                        || s.starts_with(":focus")
                });
                if let (true, Some(selector)) = (off, hits) {
                    return Err(format!("{name}: {selector} turns the outline off"));
                }
            }
        }
        "name-from-content" => {
            let want = property(node, "label")
                .or_else(|| property(node, "content"))
                .unwrap_or_default();
            let text = text_of(control);
            if text != want {
                return Err(format!("its text is {text:?}, its name {want:?}"));
            }
            for a in ["aria-label", "aria-labelledby"] {
                if control.get(a).is_some() {
                    return Err(format!("{a} overrides its text"));
                }
            }
        }
        "name-from-label" => {
            let id = subject.get("id").ok_or("the control has no id")?;
            let tag = tags
                .iter()
                .find(|t| t.name == "label" && t.get("for") == Some(id))
                .ok_or(format!("no <label for=\"{id}\">"))?;
            let text = text_of(tag);
            if text != label {
                return Err(format!("the label says {text:?}, the node {label:?}"));
            }
            if subject.get("placeholder").is_some_and(|p| p == label) {
                return Err("a placeholder as the label".into());
            }
            for a in ["aria-label", "aria-labelledby"] {
                if subject.get(a).is_some() {
                    return Err(format!("{a} overrides its <label>"));
                }
            }
        }
        "labelled-by" => {
            let id = subject.get("aria-labelledby").ok_or("no aria-labelledby")?;
            let tag = tags
                .iter()
                .find(|t| t.get("id") == Some(id))
                .ok_or(format!("no element {id}"))?;
            let text = text_of(tag);
            if text != label {
                return Err(format!("{id} says {text:?}, the node {label:?}"));
            }
        }
        "aria-label" => {
            let named = tags
                .iter()
                .find_map(|t| t.get("aria-label"))
                .ok_or("no aria-label")?;
            if label.is_empty() || (named != label && !named.starts_with(&format!("{label}:"))) {
                return Err(format!("aria-label is {named:?}, the label {label:?}"));
            }
        }
        "heading-level" => {
            let level = property(node, "level").ok_or("the node has no level")?;
            if subject.name != format!("h{level}") {
                return Err(format!("<{}> for level {level}", subject.name));
            }
        }
        "content" => {
            let want = property(node, "content").unwrap_or_default();
            let text = text_of(subject);
            if text != want {
                return Err(format!("its text is {text:?}, the content {want:?}"));
            }
        }
        "caption" => {
            let want = property(node, "caption").unwrap_or(label);
            let caption = tags
                .iter()
                .find(|t| t.name == "caption")
                .ok_or("no <caption>")?;
            let text = text_of(caption);
            if want.is_empty() || text != want {
                return Err(format!("the caption says {text:?}, the node {want:?}"));
            }
        }
        "header-scope" => {
            let headers: Vec<&Tag> = tags.iter().filter(|t| t.name == "th").collect();
            if headers.is_empty() {
                return Err("no header cell".into());
            }
            if let Some(th) = headers
                .iter()
                .find(|t| !matches!(t.get("scope"), Some("row" | "col")))
            {
                return Err(format!("a <th> with scope {:?}", th.get("scope")));
            }
        }
        "items" => {
            let items = tags.iter().filter(|t| t.name == "li").count();
            if items != node.children.len() {
                return Err(format!("{items} <li> for {} children", node.children.len()));
            }
        }
        "alt" => {
            let want = property(node, "alt").unwrap_or_default();
            if subject.name != "img" || want.is_empty() || subject.get("alt") != Some(&want) {
                return Err(format!(
                    "<{}> alt {:?}, the node's {want:?}",
                    subject.name,
                    subject.get("alt")
                ));
            }
        }
        "value" => {
            let want = property(node, "value").ok_or("the node has no value")?;
            if subject.get("value") != Some(want.as_str()) {
                return Err(format!("value is {:?}, not {want:?}", subject.get("value")));
            }
        }
        "quiet" => {
            for t in &tags {
                let live = t.get("aria-live").is_some()
                    || matches!(t.get("role"), Some("status" | "alert" | "log"));
                if live {
                    return Err(format!("<{}> is a live region", t.name));
                }
            }
        }
        "open-state" => {
            let details = tags
                .iter()
                .find(|t| t.name == "details")
                .ok_or("no <details>")?;
            let open = property(node, "open").as_deref() == Some("true");
            if details.get("open").is_some() != open {
                return Err(format!("open is {open}, the <details> says otherwise"));
            }
        }
        "range" => {
            let meter = tags
                .iter()
                .find(|t| t.name == "meter")
                .ok_or("no <meter>")?;
            for (name, default) in [("value", None), ("min", Some("0")), ("max", Some("1"))] {
                let want = property(node, name)
                    .or(default.map(String::from))
                    .and_then(|v| v.parse::<f64>().ok())
                    .ok_or(format!("the node has no {name}"))?;
                let said = meter.get(name).and_then(|v| v.parse::<f64>().ok());
                if said != Some(want) {
                    return Err(format!("{name} is {said:?}, the node's {want}"));
                }
            }
        }
        "replacement" => {
            let canvas = tags
                .iter()
                .find(|t| t.name == "canvas")
                .ok_or("no <canvas>")?;
            if canvas.get("hidden").is_none() {
                return Err("the canvas is not hidden before it draws".into());
            }
            let id = tags
                .iter()
                .find_map(|t| {
                    (t.name == "a")
                        .then(|| t.get("href")?.strip_prefix('#'))
                        .flatten()
                })
                .ok_or("no link to the replacement")?;
            let target = tags
                .iter()
                .find(|t| t.get("id") == Some(id))
                .ok_or(format!("the link points at {id}, which is not there"))?;
            if target.start < canvas.start {
                return Err(format!("{id} comes before the canvas"));
            }
        }
        "href" => {
            let want = property(node, "href").ok_or("the node has no href")?;
            if subject.get("href") != Some(want.as_str()) {
                return Err(format!("href is {:?}, not {want:?}", subject.get("href")));
            }
        }
        "described-by" => {
            let ids: Vec<&str> = subject
                .get("aria-describedby")
                .ok_or("no aria-describedby")?
                .split_whitespace()
                .collect();
            let [hint, error] = ids[..] else {
                return Err(format!(
                    "aria-describedby names {ids:?}, not a hint and an error"
                ));
            };
            for (id, class, want) in [
                (hint, "ui-hint", property(node, "description")),
                (error, "ui-note", property(node, "error")),
            ] {
                let tag = tags
                    .iter()
                    .find(|t| t.get("id") == Some(id))
                    .ok_or(format!("no element {id}"))?;
                if tag.get("class") != Some(class) {
                    return Err(format!("{id} is not the {class}"));
                }
                if tag.start < subject.start {
                    return Err(format!("{id} comes before the control"));
                }
                let text = text_of(tag);
                if text != want.clone().unwrap_or_default() {
                    return Err(format!("{id} says {text:?}, the node {want:?}"));
                }
            }
        }
        "required" => {
            let required = property(node, "required").as_deref() == Some("true");
            let said =
                subject.get("aria-required") == Some("true") || subject.get("required").is_some();
            if said != required {
                return Err(format!("required is {required}, the control says {said}"));
            }
        }
        "invalid" => {
            let invalid = property(node, "error").is_some();
            let said = subject.get("aria-invalid") == Some("true");
            if said != invalid {
                return Err(format!("an error: {invalid}; aria-invalid: {said}"));
            }
        }
        "text-scales" => {
            for (name, rule) in sheet_rules(sheets)? {
                for p in ["font-size", "font"] {
                    if let Some(d) = rule.declaration(p) {
                        if d.px().next().is_some() {
                            let selector = rule.selectors.join(", ");
                            return Err(format!("{name}: {selector} sets {p}: {}", d.value));
                        }
                    }
                }
            }
        }
        other => return Err(format!("{other}: no such probe")),
    }
    Ok(())
}

fn gallery_sheets(kernel: &Kernel) -> Stylesheets {
    Stylesheets {
        css: [ikigai_ui::TOKENS_CSS, "urn:iki:ui:base.css"]
            .iter()
            .map(|iri| (iri.to_string(), render::source(kernel, iri, &[]).unwrap()))
            .collect(),
    }
}

/// The screens the html probes read: every kind is on the gallery, every element on the
/// elements screen.
const SCREENS: [&str; 2] = ["gallery", "elements"];

/// Checks the html SEE checks of every kind and element on the gallery and elements screens,
/// each node as its html face renders it. A SEE check with no probe is reported, never
/// skipped; a probe that fails fails the test, and so does a criterion with a probe and no
/// node on either screen to check it on. The gpui face's checks are reported too: this test
/// reads HTML only, and the window tests check those with a probe.
#[test]
fn every_see_check_holds_on_the_gallery_html() {
    let kernel = common::host(None);
    let sheets = gallery_sheets(&kernel);
    let mut found: Vec<(String, String, &'static str, Node)> = Vec::new();
    let mut pages = Vec::new();
    for screen in SCREENS {
        let page =
            render::source(&kernel, &format!("urn:iki:ui:html:screen:{screen}"), &[]).unwrap();
        let source =
            render::source(&kernel, &format!("urn:iki:ui:screen:name:{screen}"), &[]).unwrap();
        let source = model::parse(source.as_bytes()).unwrap();
        let face = render::source(
            &kernel,
            &format!("urn:iki:ui:face:target:html:screen:{screen}"),
            &[],
        )
        .unwrap();
        let face = model::parse(face.as_bytes()).unwrap();
        for (at, name, node) in criterion::subjects(&source, &face) {
            let path: String = std::iter::once("n".to_string())
                .chain(at.iter().map(|i| i.to_string()))
                .collect::<Vec<_>>()
                .join(".");
            found.push((screen.to_string(), path, name, node.clone()));
        }
        pages.push((screen, page));
    }

    let mut verified = 0;
    let mut failures = Vec::new();
    let mut report = Vec::new();
    for kind in criterion::kinds() {
        let at: Vec<_> = found.iter().filter(|(_, _, n, _)| *n == kind).collect();
        let html_c = criterion::criterion(kind, "html").unwrap();
        for (under, n, check) in html_c.checks() {
            if check.mode != Mode::See {
                continue;
            }
            let Some(probe) = check.probe.as_deref() else {
                report.push(format!(
                    "NOT VERIFIED {kind} html {under} {n}: no probe can see it in HTML: {}",
                    check.statement
                ));
                continue;
            };
            assert!(
                !at.is_empty(),
                "no {kind} on {SCREENS:?}, so nothing verifies its {under} {n} ({probe})"
            );
            for (screen, path, _, node) in &at {
                let html = render::source(
                    &kernel,
                    &format!("urn:iki:ui:html:screen:{screen}:node:{path}"),
                    &[],
                )
                .unwrap();
                let page = &pages.iter().find(|(s, _)| s == screen).unwrap().1;
                assert!(
                    page.contains(&html),
                    "{screen} {path}'s HTML is the page's own: {html}"
                );
                match verify(probe, check.argument.as_deref(), node, &html, &sheets) {
                    Ok(()) => {
                        verified += 1;
                        report.push(format!(
                            "ok {kind} html {under} {n} ({probe}) at {screen} {path}"
                        ));
                    }
                    Err(e) => failures.push(format!(
                        "FAILED {kind} html {under} {n} ({probe}) at {screen} {path}: {e}: {}",
                        check.statement
                    )),
                }
            }
        }
        let see = |c: &Check| c.mode == Mode::See;
        let gpui = criterion::criterion(kind, "gpui").unwrap();
        let unread = gpui.checks().filter(|(_, _, c)| see(c)).count();
        let blocked = gpui
            .checks()
            .filter(|(_, _, c)| c.blocked.is_some())
            .count();
        report.push(format!(
            "NOT VERIFIED HERE {kind} gpui: {unread} SEE checks, which this test cannot read from HTML"
        ));
        report.push(format!(
            "BLOCKED {kind} gpui: {blocked} checks, by the toolkit (GPUI 0.2.2 has no accessibility tree)"
        ));
    }
    println!("{}", report.join("\n"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    // Every probe on every node it applies to: at least one check per probe and kind.
    let probes: usize = criterion::kinds()
        .map(|k| {
            criterion::criterion(k, "html")
                .unwrap()
                .checks()
                .filter(|(_, _, c)| c.probe.is_some())
                .count()
        })
        .sum();
    assert!(
        verified >= probes,
        "{verified} probes verified of {probes}:\n{}",
        report.join("\n")
    );
}

/// The support matrix's accessibility column is each criterion's tally: per target, each kind
/// and element, its SEE checks, those a test verifies, and its HEAR checks, in JSON and on the
/// support page.
#[test]
fn the_support_matrix_counts_verified_see_and_untested_hear_checks() {
    let kernel = kernel();
    let (_, json) = read(&kernel, "urn:iki:ui:support", None).unwrap();
    let support: serde_json::Value = serde_json::from_str(&json).unwrap();
    let (_, page) = read(&kernel, "urn:iki:ui:support", Some("text/html")).unwrap();
    for target in TARGETS {
        for kind in criterion::kinds() {
            let c = criterion::criterion(kind, target).unwrap();
            let see = c.checks().filter(|(_, _, k)| k.mode == Mode::See).count();
            let probed = c
                .checks()
                .filter(|(_, _, k)| k.mode == Mode::See && k.probe.is_some())
                .count();
            let hear = c.checks().filter(|(_, _, k)| k.mode == Mode::Hear).count();
            let blocked = c
                .checks()
                .filter(|(_, _, k)| k.mode == Mode::See && k.blocked.is_some())
                .count();
            let tally = &support[target]["a11y"][kind];
            assert_eq!(tally["see_blocked"], blocked, "{target} {kind}");
            assert_eq!(tally["see"], see, "{target} {kind}");
            assert_eq!(tally["see_verified"], probed, "{target} {kind}");
            assert_eq!(tally["hear_untested"], hear, "{target} {kind}");
            assert!(
                hear > 0,
                "{target} {kind}: every criterion has a HEAR check"
            );
        }
        // The html face's SEE checks: every one verified but the 400% zoom.
        let html = &support["html"]["a11y"]["heading"];
        assert_eq!(html["see_verified"], html["see"].as_u64().unwrap() - 1);
    }
    for (row, cells) in [
        // The targets in name order: gpui, then html.
        ("heading", "<td>Renders</td><td>SEE: 1 of 7 verified by a test, 2 blocked by the toolkit; HEAR: 1 untested</td><td>Renders</td><td>SEE: 4 of 5 verified by a test; HEAR: 3 untested</td>"),
        ("urn:iki:ui:element:name:meter", "<td>Native</td><td>SEE: 1 of 7 verified by a test, 2 blocked by the toolkit; HEAR: 1 untested</td><td>Native</td><td>SEE: 5 of 6 verified by a test; HEAR: 2 untested</td>"),
        ("urn:iki:ui:element:name:rating", "<td>Expanded from its definition</td><td>No criteria</td><td>Expanded from its definition</td><td>No criteria</td>"),
    ] {
        let row = format!("<tr><th scope=\"row\">{row}</th>");
        assert!(page.contains(&format!("{row}{cells}")), "{row}{cells} in\n{page}");
    }
    assert!(page.contains("<th scope=\"col\">gpui accessibility</th>"));
}

/// The probes are not vacuous: a face that drew a button as a `<div>`, a field whose label
/// names another input, or a link that goes elsewhere fails them.
#[test]
fn a_broken_face_fails_its_probes() {
    let kernel = kernel();
    let sheets = gallery_sheets(&kernel);
    let node = |json: &str| -> Node { serde_json::from_str(json).unwrap() };
    let button = node(r#"{"kind":"button","properties":{"label":"Save"}}"#);
    let link = node(r#"{"kind":"link","properties":{"label":"Help","href":"/help"}}"#);
    let field = node(
        r#"{"kind":"field","properties":{"label":"Email","name":"email","required":"true","error":"Has an @."}}"#,
    );
    let good_field = r#"<div class="ui-field"><label for="ui-n">Email</label> <input id="ui-n" name="email" type="email" aria-required="true" aria-invalid="true" aria-describedby="ui-n-description ui-n-error"><p class="ui-hint" id="ui-n-description"></p><p class="ui-note" id="ui-n-error">Has an @.</p></div>"#;
    for (probe, argument, node, html) in [
        (
            "element",
            Some("button"),
            &button,
            r##"<div tabindex="0"><a href="#">Save</a></div>"##.to_string(),
        ),
        (
            "attribute",
            Some("type=submit"),
            &button,
            r#"<button type="button">Save</button>"#.to_string(),
        ),
        (
            "focusable",
            None,
            &button,
            r#"<button tabindex="-1">Save</button>"#.to_string(),
        ),
        ("focusable", None, &link, r#"<a>Help</a>"#.to_string()),
        (
            "name-from-content",
            None,
            &button,
            r#"<button>Click here</button>"#.to_string(),
        ),
        (
            "name-from-content",
            None,
            &button,
            r#"<button aria-label="Go">Save</button>"#.to_string(),
        ),
        (
            "href",
            None,
            &link,
            r#"<a href="/elsewhere">Help</a>"#.to_string(),
        ),
        (
            "name-from-label",
            None,
            &field,
            good_field.replace("for=\"ui-n\"", "for=\"ui-m\""),
        ),
        (
            "name-from-label",
            None,
            &field,
            good_field.replace(">Email</label>", ">E-mail</label>"),
        ),
        (
            "described-by",
            None,
            &field,
            good_field.replace("ui-n-description ui-n-error", "ui-n-error"),
        ),
        (
            "described-by",
            None,
            &field,
            good_field.replace(">Has an @.</p>", "></p>"),
        ),
        (
            "required",
            None,
            &field,
            good_field.replace("aria-required=\"true\"", "aria-required=\"false\""),
        ),
        (
            "invalid",
            None,
            &field,
            good_field.replace("aria-invalid=\"true\"", "aria-invalid=\"false\""),
        ),
    ] {
        assert!(
            verify(probe, argument, node, &html, &sheets).is_err(),
            "{probe} refuses {html}"
        );
    }
    for probe in [
        "name-from-label",
        "described-by",
        "required",
        "invalid",
        "focusable",
    ] {
        verify(probe, None, &field, good_field, &sheets).unwrap();
    }
    // The probes of the other kinds and the elements, each against a face that breaks it, and
    // the same face whole, which passes.
    let heading = node(r#"{"kind":"heading","properties":{"content":"Totals","level":2}}"#);
    let text = node(r#"{"kind":"text","properties":{"content":"A <b> stays text."}}"#);
    let list = node(
        r#"{"kind":"list","properties":{"label":"Steps"},"children":[{"kind":"text","properties":{"content":"Read"}},{"kind":"text","properties":{"content":"Write"}}]}"#,
    );
    let table = node(r#"{"kind":"table","properties":{"caption":"Totals"}}"#);
    let form = node(r#"{"kind":"form","properties":{"label":"Sign up"}}"#);
    let image = node(r#"{"kind":"image","properties":{"src":"/a.jpg","alt":"A lighthouse"}}"#);
    let option = node(r#"{"kind":"option","properties":{"label":"Small","value":"s"}}"#);
    let disclosure = node(
        r#"{"kind":"urn:iki:ui:element:name:disclosure","properties":{"summary":"How often?","open":true}}"#,
    );
    let meter = node(
        r#"{"kind":"urn:iki:ui:element:name:meter","properties":{"label":"Capacity","value":82,"max":100}}"#,
    );
    let scene = node(r#"{"kind":"scene","properties":{"label":"The graph"}}"#);
    let good_list =
        r#"<ul class="ui-list" aria-label="Steps"><li><p>Read</p></li><li><p>Write</p></li></ul>"#;
    let good_table = r#"<table class="ui-table"><caption>Totals</caption><tbody><tr><th scope="col"><p>Fruit</p></th></tr><tr><th scope="row"><p>Apples</p></th><td><p>3</p></td></tr></tbody></table>"#;
    let good_form = r#"<form id="ui-n" class="ui-form" aria-labelledby="ui-n-label"><h3 id="ui-n-label">Sign up</h3></form>"#;
    let good_details = r#"<details class="ui-disclosure" open><summary>How often?</summary><p>Daily.</p></details>"#;
    let good_meter = r#"<div class="ui-meter"><label for="m">Capacity</label> <meter id="m" value="82" min="0" max="100">82</meter> <span>82 of 100</span></div>"#;
    let good_scene = r##"<figure class="ui-scene"><figcaption>The graph <a href="#t">The same graph as text</a></figcaption><canvas tabindex="0" role="application" aria-label="The graph: arrow keys orbit" hidden></canvas><div id="t"><table></table></div></figure>"##;
    let cases: [(&str, Option<&str>, &Node, &str, String); 23] = [
        (
            "heading-level",
            None,
            &heading,
            "<h2>Totals</h2>",
            "<h3>Totals</h3>".into(),
        ),
        (
            "name-from-content",
            None,
            &heading,
            "<h2>Totals</h2>",
            "<h2>Sums</h2>".into(),
        ),
        (
            "no-tab-stop",
            None,
            &heading,
            "<h2>Totals</h2>",
            r#"<h2 tabindex="0">Totals</h2>"#.into(),
        ),
        (
            "content",
            None,
            &text,
            "<p>A &lt;b&gt; stays text.</p>",
            "<p>A  stays text.</p>".into(),
        ),
        (
            "quiet",
            None,
            &text,
            "<p>A &lt;b&gt; stays text.</p>",
            r#"<p role="status">A &lt;b&gt; stays text.</p>"#.into(),
        ),
        (
            "aria-label",
            None,
            &list,
            good_list,
            good_list.replace("Steps", "Stairs"),
        ),
        (
            "items",
            None,
            &list,
            good_list,
            good_list.replace("<li><p>Write</p></li>", "<p>Write</p>"),
        ),
        (
            "caption",
            None,
            &table,
            good_table,
            good_table.replace("<caption>Totals</caption>", ""),
        ),
        (
            "header-scope",
            None,
            &table,
            good_table,
            good_table.replace(r#" scope="row""#, ""),
        ),
        (
            "labelled-by",
            None,
            &form,
            good_form,
            good_form.replace(">Sign up</h3>", ">Join</h3>"),
        ),
        (
            "contains",
            Some("h3"),
            &form,
            good_form,
            good_form.replace("h3", "p"),
        ),
        // Markup in a comment is no element (ledger #3560: the hand-written reader read it).
        (
            "contains",
            Some("h3"),
            &form,
            good_form,
            good_form
                .replace("<h3", "<!-- <h3")
                .replace("</h3>", "</h3> -->"),
        ),
        (
            "labelled-by",
            None,
            &form,
            good_form,
            good_form
                .replace("<h3", "<!-- <h3")
                .replace("</h3>", "</h3> -->"),
        ),
        // An unquoted value is an attribute value (the hand-written reader never returned).
        (
            "attribute",
            Some("type=submit"),
            &button,
            "<button type=submit>Save</button>",
            "<button type=button>Save</button>".into(),
        ),
        (
            "alt",
            None,
            &image,
            r#"<img src="/a.jpg" alt="A lighthouse">"#,
            r#"<img src="/a.jpg" alt="">"#.into(),
        ),
        (
            "value",
            None,
            &option,
            r#"<option value="s">Small</option>"#,
            r#"<option value="l">Small</option>"#.into(),
        ),
        (
            "element",
            Some("details"),
            &disclosure,
            good_details,
            good_details.replace("details", "div"),
        ),
        (
            "focusable",
            None,
            &disclosure,
            good_details,
            good_details.replace("<summary>", r#"<summary tabindex="-1">"#),
        ),
        (
            "open-state",
            None,
            &disclosure,
            good_details,
            good_details.replace(" open>", ">"),
        ),
        (
            "name-from-label",
            None,
            &meter,
            good_meter,
            good_meter.replace(r#"for="m""#, r#"for="x""#),
        ),
        (
            "range",
            None,
            &meter,
            good_meter,
            good_meter.replace(r#"max="100""#, r#"max="1""#),
        ),
        (
            "aria-label",
            None,
            &scene,
            good_scene,
            good_scene.replace("The graph:", "A graph:"),
        ),
        (
            "replacement",
            None,
            &scene,
            good_scene,
            good_scene.replace(" hidden>", ">"),
        ),
    ];
    for (probe, argument, node, good, broken) in cases {
        verify(probe, argument, node, good, &sheets)
            .unwrap_or_else(|e| panic!("{probe} holds on {good}: {e}"));
        assert_ne!(good, broken, "{probe}: the case changed something");
        assert!(
            verify(probe, argument, node, &broken, &sheets).is_err(),
            "{probe} refuses {broken}"
        );
    }
    // A probe the sheets name is one this test knows; and so is every probe.
    for probe in criterion::PROBES {
        assert!(
            !verify(probe, Some("p"), &text, "<p>x</p>", &sheets)
                .is_err_and(|e| e.ends_with("no such probe")),
            "{probe} is implemented"
        );
    }
    // A stylesheet that sets text in px, or takes the ring away, fails its probe, however the
    // CSS spells it (ledger #3560: `!important`, an uppercase unit and nesting got past the
    // hand-written reader).
    for (css, probe) in [
        ("p { font-size: 14px; }", "text-scales"),
        ("p { font-size: 14PX; }", "text-scales"),
        ("p { font: italic 12px/1.5 serif; }", "text-scales"),
        (
            "@media (min-width: 1px) { p { font-size: 14px } }",
            "text-scales",
        ),
        ("button:focus { outline: none; }", "focus-visible"),
        (
            "button:focus { outline: none !important; }",
            "focus-visible",
        ),
        ("button:focus { outline: 0; }", "focus-visible"),
        ("button:focus { outline-style: none; }", "focus-visible"),
        ("button { &:focus { outline: none; } }", "focus-visible"),
        (":focus { outline: none; }", "focus-visible"),
        // A sheet the CSS parser cannot read is no proof of anything.
        ("p { color: red; } }{ x", "text-scales"),
    ] {
        let mut broken = gallery_sheets(&kernel);
        broken.css.push(("broken.css".into(), css.into()));
        assert!(
            verify(probe, None, &button, "<button>Save</button>", &broken).is_err(),
            "{probe} refuses {css}"
        );
    }
}

// --- the gpui face: desktop criteria (ledger #3546) -------------------------------------------

/// The gpui face's criteria are desktop-native ones, written from each platform's own guidance:
/// magentaA11y and atomica11y-skills cover the web and iOS and Android, not macOS, Windows or
/// Linux, so neither is a gpui source. Each says what each platform's screen reader should
/// announce once there is a tree (macOS VoiceOver over NSAccessibility, Windows Narrator over
/// UI Automation, Linux Orca over AT-SPI), and because GPUI 0.2.2 exposes no accessibility tree,
/// every HEAR check is blocked by the toolkit, and so are the device settings GPUI does not
/// report (the system text size, the platform's high contrast). The html face has no blocker.
#[test]
fn the_gpui_criteria_are_desktop_criteria_and_their_hear_checks_are_blocked_by_the_toolkit() {
    for kind in criterion::kinds() {
        let gpui = criterion::criterion(kind, "gpui").unwrap();
        for s in &gpui.source {
            assert!(
                !s.contains("/magentaA11y/") && !s.contains("/atomica11y-skills/"),
                "{kind} on gpui: {s} covers the web and mobile, not the desktop"
            );
        }
        for host in ["developer.apple.com", "learn.microsoft.com", "docs.gtk.org"] {
            assert!(
                gpui.source.iter().any(|s| s.contains(host)),
                "{kind} on gpui: a source on {host}"
            );
        }
        let platforms: Vec<&str> = gpui.announce.iter().map(|a| a.platform.as_str()).collect();
        assert_eq!(platforms, criterion::PLATFORMS, "{kind} on gpui");
        for (a, reader) in gpui.announce.iter().zip(["VoiceOver", "Narrator", "Orca"]) {
            assert!(
                a.reader.contains(reader),
                "{kind}: {} by {reader}",
                a.platform
            );
        }
        for (under, n, check) in gpui.checks() {
            if check.mode == Mode::Hear {
                assert_eq!(
                    check.blocked.as_deref(),
                    Some("toolkit"),
                    "{kind} on gpui, {under} {n}: a HEAR check is blocked by the toolkit"
                );
            }
        }
        let blocked_setting = |words: &str| {
            gpui.setting
                .iter()
                .any(|c| c.statement.contains(words) && c.blocked.as_deref() == Some("toolkit"))
        };
        for words in ["text size", "high contrast"] {
            assert!(blocked_setting(words), "{kind} on gpui: {words}, blocked");
        }
        let html = criterion::criterion(kind, "html").unwrap();
        assert!(html.checks().all(|(_, _, c)| c.blocked.is_none()));
        // Blocked in words, in both faces.
        let words = criterion::text(kind, gpui);
        assert!(words.contains("[HEAR, blocked by the toolkit]"), "{words}");
        assert!(words.contains("macOS (VoiceOver"), "{words}");
        let data = criterion::turtle(kind, gpui);
        assert!(data.contains("c:blocked \"toolkit\""), "{data}");
        assert!(
            data.contains(":announce:windows> a c:Announcement"),
            "{data}"
        );
    }
}
