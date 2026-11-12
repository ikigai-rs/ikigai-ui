//! Accessibility acceptance criteria per kind and target, as data (ledger #3541): the sheets and
//! the closed shape agree, the criteria resolve by name in their Turtle and text faces and are
//! cached, each kind's descriptor links its own, and every SEE check of button, link and field
//! on the html face is verified against the gallery's rendered HTML. A SEE check with no probe
//! is reported as not verified, never skipped.

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
    assert_eq!(seen, 6, "button, link and field, on html and gpui");
    // The shape's closed set of probes is the Rust one.
    let probes = format!("\"^({})$\"", criterion::PROBES.join("|"));
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
            assert_eq!(
                iri,
                format!("urn:iki:ui:criterion:kind:{kind}:target:{target}")
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
    // A kind with no criteria yet, a target that is not one, a name that is not a kind.
    for iri in [
        criterion::iri("heading", "html"),
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

const CONTROLS: [&str; 5] = ["button", "a", "input", "select", "textarea"];

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
    let control = tags
        .iter()
        .find(|t| CONTROLS.contains(&t.name.as_str()))
        .ok_or("no control element")?;
    let label = property(node, "label").unwrap_or_default();
    match probe {
        "element" => {
            let want = argument.ok_or("element names no element")?;
            if control.name != want {
                return Err(format!("the control is <{}>, not <{want}>", control.name));
            }
        }
        "attribute" => {
            let (name, value) = argument
                .and_then(|a| a.split_once('='))
                .ok_or("attribute is name=value")?;
            if control.get(name) != Some(value) {
                return Err(format!("{name} is {:?}, not {value:?}", control.get(name)));
            }
        }
        "focusable" => {
            let native = match control.name.as_str() {
                "a" => control.get("href").is_some_and(|h| !h.is_empty()),
                "input" => control.get("type") != Some("hidden"),
                _ => true,
            };
            if !native {
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
            let text = text_of(html, control);
            if text != label {
                return Err(format!("its text is {text:?}, its label {label:?}"));
            }
            for a in ["aria-label", "aria-labelledby"] {
                if control.get(a).is_some() {
                    return Err(format!("{a} overrides its text"));
                }
            }
        }
        "name-from-label" => {
            let id = control.get("id").ok_or("the control has no id")?;
            let tag = tags
                .iter()
                .find(|t| t.name == "label" && t.get("for") == Some(id))
                .ok_or(format!("no <label for=\"{id}\">"))?;
            let text = text_of(html, tag);
            if text != label {
                return Err(format!("the label says {text:?}, the field {label:?}"));
            }
            if control.get("placeholder").is_some_and(|p| p == label) {
                return Err("a placeholder as the label".into());
            }
            for a in ["aria-label", "aria-labelledby"] {
                if control.get(a).is_some() {
                    return Err(format!("{a} overrides its <label>"));
                }
            }
        }
        "href" => {
            let want = property(node, "href").ok_or("the node has no href")?;
            if control.get("href") != Some(want.as_str()) {
                return Err(format!("href is {:?}, not {want:?}", control.get("href")));
            }
        }
        "described-by" => {
            let ids: Vec<&str> = control
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
                if tag.start < control.start {
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
                control.get("aria-required") == Some("true") || control.get("required").is_some();
            if said != required {
                return Err(format!("required is {required}, the control says {said}"));
            }
        }
        "invalid" => {
            let invalid = property(node, "error").is_some();
            let said = control.get("aria-invalid") == Some("true");
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

/// Every node of `kinds` in `node`, with its html face's node path (`n`, `n.1`, `n.1.2`).
fn nodes<'a>(node: &'a Node, path: String, kinds: &[&str], out: &mut Vec<(String, &'a Node)>) {
    if kinds.contains(&node.kind.as_str()) {
        out.push((path.clone(), node));
    }
    for (i, child) in node.children.iter().enumerate() {
        nodes(child, format!("{path}.{i}"), kinds, out);
    }
}

fn gallery_sheets(kernel: &Kernel) -> Stylesheets {
    Stylesheets {
        css: [ikigai_ui::TOKENS_CSS, "urn:iki:ui:base.css"]
            .iter()
            .map(|iri| (iri.to_string(), render::source(kernel, iri, &[]).unwrap()))
            .collect(),
    }
}

/// Checks the html SEE checks of every button, link and field in the gallery, as its html
/// face renders them. A SEE check with no probe is reported, never skipped; a probe that fails
/// fails the test. The gpui face's checks are reported too: this test reads HTML only.
#[test]
fn every_see_check_holds_on_the_gallery_html() {
    let kernel = common::host(None);
    let page = render::source(&kernel, "urn:iki:ui:html:screen:gallery", &[]).unwrap();
    let face = render::source(&kernel, "urn:iki:ui:face:target:html:screen:gallery", &[]).unwrap();
    let face = model::parse(face.as_bytes()).unwrap();
    let sheets = gallery_sheets(&kernel);
    let kinds: Vec<&str> = criterion::kinds().collect();
    let mut found = Vec::new();
    nodes(&face, "n".into(), &kinds, &mut found);

    let mut verified = 0;
    let mut failures = Vec::new();
    let mut report = Vec::new();
    for kind in &kinds {
        let at: Vec<&(String, &Node)> = found.iter().filter(|(_, n)| n.kind == *kind).collect();
        assert!(
            !at.is_empty(),
            "the gallery has no {kind}, so nothing verifies its criteria"
        );
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
            for (path, node) in &at {
                let html = render::source(
                    &kernel,
                    &format!("urn:iki:ui:html:screen:gallery:node:{path}"),
                    &[],
                )
                .unwrap();
                assert!(
                    page.contains(&html),
                    "{path}'s HTML is the gallery's own: {html}"
                );
                match verify(probe, check.argument.as_deref(), node, &html, &sheets) {
                    Ok(()) => {
                        verified += 1;
                        report.push(format!("ok {kind} html {under} {n} ({probe}) at {path}"));
                    }
                    Err(e) => failures.push(format!(
                        "FAILED {kind} html {under} {n} ({probe}) at {path}: {e}: {}",
                        check.statement
                    )),
                }
            }
        }
        let see = |c: &Check| c.mode == Mode::See;
        let gpui = criterion::criterion(kind, "gpui").unwrap();
        let unread = gpui.checks().filter(|(_, _, c)| see(c)).count();
        report.push(format!(
            "NOT VERIFIED {kind} gpui: {unread} SEE checks, which this test cannot read from HTML"
        ));
    }
    println!("{}", report.join("\n"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    // Two buttons, three links and three fields in the gallery today, every probe on each.
    assert!(
        verified >= 30,
        "{verified} probes verified:\n{}",
        report.join("\n")
    );
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
