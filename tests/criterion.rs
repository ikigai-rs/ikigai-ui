//! Accessibility acceptance criteria per kind, element and target, as data (ledgers #3541 and
//! #3542): the sheets and the closed shape agree, the criteria resolve by name in their Turtle
//! and text faces and are cached, each kind's descriptor links its own, every SEE check with a
//! probe on the html face is verified against the rendered HTML of every node of its kind or
//! element on the gallery and elements screens, and the support matrix counts them. A SEE check
//! with no probe is reported as not verified, never skipped; the gpui face's probes run in the
//! window tests (`gpui/src/native.rs`).

mod common;

use std::sync::Arc;

use futures::executor::block_on;
use ikigai_core::{ArgRef, Capability, Error, Iri, Kernel, Request, Verb};
use ikigai_ui::criterion::{self, Check, Criterion, Mode, TARGETS};
use ikigai_ui::model::{self, Node, Value, KINDS};
use ikigai_ui::testkit::render;

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
    let html = r#""target": "html""#;
    let json = include_str!("../assets/criterion/button.json");
    let cases: [(&str, String, String); 9] = [
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
            "a source that is not a GitHub page",
            valid.replacen(
                "<https://github.com/tmobile",
                "<https://example.com/tmobile",
                1,
            ),
            json.replacen(
                "https://github.com/tmobile",
                "https://example.com/tmobile",
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
    assert!(conforms(&valid));
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
        assert_ne!(turtle, valid, "{what}: the Turtle case changed something");
        assert_ne!(sheet, json, "{what}: the sheet case changed something");
        assert!(!conforms(&turtle), "the shape refuses {what}");
        assert!(criterion::parse(&sheet).is_err(), "Rust refuses {what}");
    }
    // What only Rust says: a HEAR check has no probe, a setting is SEE, the targets in order.
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
            for source in &criterion::criterion(kind, target).unwrap().source {
                // This repository's own ACCESSIBILITY.md (meter, scene) is no third party's.
                if source.starts_with("https://github.com/ikigai-rs/ikigai-ui/") {
                    continue;
                }
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

/// A start tag: its name, its attributes, and where it starts and ends in the HTML.
#[derive(Debug)]
struct Tag {
    name: String,
    attributes: Vec<(String, String)>,
    start: usize,
    end: usize,
}

impl Tag {
    fn get(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

/// The start tags of `html`, which a template writes: double-quoted values or bare names.
fn tags(html: &str) -> Vec<Tag> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(i) = html[at..].find('<') {
        let start = at + i;
        let rest = &html[start + 1..];
        if !rest.starts_with(|c: char| c.is_ascii_alphabetic()) {
            at = start + 1;
            continue;
        }
        let name_len = rest
            .find(|c: char| !c.is_ascii_alphanumeric())
            .unwrap_or(rest.len());
        let name = rest[..name_len].to_ascii_lowercase();
        let mut attributes = Vec::new();
        let mut j = start + 1 + name_len;
        loop {
            let s = &html[j..];
            let trimmed = s.trim_start();
            j += s.len() - trimmed.len();
            if trimmed.starts_with('>') || trimmed.is_empty() {
                break;
            }
            let n_len = trimmed
                .find(|c: char| c == '=' || c == '>' || c.is_whitespace())
                .unwrap_or(trimmed.len());
            let attr = trimmed[..n_len].to_ascii_lowercase();
            j += n_len;
            if html[j..].starts_with("=\"") {
                let close = html[j + 2..].find('"').expect("a closed attribute value");
                attributes.push((attr, unescape(&html[j + 2..j + 2 + close])));
                j += 2 + close + 1;
            } else {
                attributes.push((attr, String::new()));
            }
        }
        out.push(Tag {
            name,
            attributes,
            start,
            end: j + 1,
        });
        at = j + 1;
    }
    out
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&amp;", "&")
}

/// The text inside `tag`, up to its end tag, with any markup in it dropped.
fn text_of(html: &str, tag: &Tag) -> String {
    let close = format!("</{}>", tag.name);
    let inner = &html[tag.end..tag.end + html[tag.end..].find(&close).expect("an end tag")];
    let mut text = String::new();
    let mut in_tag = false;
    for c in inner.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => text.push(c),
            _ => {}
        }
    }
    unescape(text.trim())
}

/// What a `<div>` wrapper's label names: its control.
const LABELLED: [&str; 5] = ["input", "select", "textarea", "meter", "button"];

/// The node's subject: its HTML's outermost element, or the control in a `<div>` wrapper.
fn subject(tags: &[Tag]) -> Option<&Tag> {
    let root = tags.first()?;
    if root.name != "div" {
        return Some(root);
    }
    tags.iter()
        .find(|t| LABELLED.contains(&t.name.as_str()))
        .or(Some(root))
}

/// Whether `tag` is in the tab order natively.
fn natively_focusable(tag: &Tag) -> bool {
    match tag.name.as_str() {
        "a" => tag.get("href").is_some_and(|h| !h.is_empty()),
        "input" => tag.get("type") != Some("hidden"),
        "button" | "select" | "textarea" | "summary" => true,
        _ => false,
    }
}

/// `tag`, `tag[name]` or `tag[name=value]` matches `t`.
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

/// The CSS rules of `css`: (selector, body), comments dropped, an at-rule's rules included.
fn rules(css: &str) -> Vec<(String, String)> {
    let css = ikigai_ui::testkit::lint::blank_comments(css);
    let mut out = Vec::new();
    let mut open: Vec<String> = Vec::new();
    let mut pending = String::new();
    for c in css.chars() {
        match c {
            '{' => {
                let selector = pending.rsplit(';').next().unwrap_or_default().trim();
                open.push(selector.to_string());
                pending.clear();
            }
            '}' => {
                let selector = open.pop().unwrap_or_default();
                if !selector.starts_with('@') && pending.contains(':') {
                    out.push((selector, pending.trim().to_string()));
                }
                pending.clear();
            }
            c => pending.push(c),
        }
    }
    out
}

/// The value of `property` in a rule's body.
fn declaration<'a>(body: &'a str, property: &str) -> Option<&'a str> {
    body.split(';').find_map(|d| {
        let (p, v) = d.split_once(':')?;
        (p.trim() == property).then_some(v.trim())
    })
}

/// Verify one SEE probe of `node`'s html face: `html` is its rendered HTML.
fn verify(
    probe: &str,
    argument: Option<&str>,
    node: &Node,
    html: &str,
    sheets: &Stylesheets,
) -> Result<(), String> {
    let tags = tags(html);
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
            let ring = sheets
                .css
                .iter()
                .flat_map(|(name, css)| rules(css).into_iter().map(move |r| (name, r)))
                .find(|(_, (selector, body))| {
                    selector.split(',').any(|s| s.trim() == ":focus-visible")
                        && declaration(body, "outline").is_some()
                })
                .ok_or("no :focus-visible outline")?;
            let outline = declaration(&ring.1 .1, "outline").unwrap();
            let width: f64 = outline
                .split_whitespace()
                .find_map(|w| w.strip_suffix("px")?.parse().ok())
                .ok_or(format!("outline {outline} has no px width"))?;
            if width < 2.0 || !outline.contains("var(--focus)") {
                return Err(format!("outline {outline} is not 2px or more in --focus"));
            }
            // Nothing takes the ring away from the control.
            for (name, css) in &sheets.css {
                for (selector, body) in rules(css) {
                    let off =
                        declaration(&body, "outline").is_some_and(|v| v == "none" || v == "0");
                    let hits = selector.split(',').any(|s| {
                        let s = s.trim();
                        s.starts_with(&control.name)
                            && s[control.name.len()..]
                                .chars()
                                .next()
                                .is_none_or(|c| !c.is_ascii_alphanumeric())
                            || s.starts_with(":focus")
                    });
                    if off && hits {
                        return Err(format!("{name}: {selector} turns the outline off"));
                    }
                }
            }
        }
        "name-from-content" => {
            let want = property(node, "label")
                .or_else(|| property(node, "content"))
                .unwrap_or_default();
            let text = text_of(html, control);
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
            let text = text_of(html, tag);
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
            let text = text_of(html, tag);
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
            let text = text_of(html, subject);
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
            let text = text_of(html, caption);
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
                let text = text_of(html, tag);
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
            for (name, css) in &sheets.css {
                for (selector, body) in rules(css) {
                    for p in ["font-size", "font"] {
                        if let Some(v) = declaration(&body, p) {
                            if v.contains("px") {
                                return Err(format!("{name}: {selector} sets {p}: {v}"));
                            }
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
        report.push(format!(
            "NOT VERIFIED HERE {kind} gpui: {unread} SEE checks, which this test cannot read from HTML"
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
            let tally = &support[target]["a11y"][kind];
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
        ("heading", "<td>Renders</td><td>SEE: 1 of 4 verified by a test; HEAR: 1 untested</td><td>Renders</td><td>SEE: 4 of 5 verified by a test; HEAR: 3 untested</td>"),
        ("urn:iki:ui:element:name:meter", "<td>Native</td><td>SEE: 1 of 4 verified by a test; HEAR: 1 untested</td><td>Native</td><td>SEE: 5 of 6 verified by a test; HEAR: 2 untested</td>"),
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
    let cases: [(&str, Option<&str>, &Node, &str, String); 20] = [
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
    // A stylesheet that sets text in px, or takes the ring away, fails its probe.
    for (css, probe) in [
        ("p { font-size: 14px; }", "text-scales"),
        ("button:focus { outline: none; }", "focus-visible"),
    ] {
        let mut broken = gallery_sheets(&kernel);
        broken.css.push(("broken.css".into(), css.into()));
        assert!(
            verify(probe, None, &button, "<button>Save</button>", &broken).is_err(),
            "{probe} refuses {css}"
        );
    }
}
