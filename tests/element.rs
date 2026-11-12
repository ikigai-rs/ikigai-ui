//! Custom elements and hints (ledger #3358): the ladder, the bounds, the faces and the thread.
mod common;

use std::collections::BTreeMap;

use ikigai_core::{Error, Verb};
use ikigai_ui::element::{self, Definition, MAX_DEPTH, MAX_NODES};
use ikigai_ui::model::{self, Node};
use ikigai_ui::target::{Capability, Descriptor};

fn html() -> Descriptor {
    Descriptor::parse(include_bytes!("../assets/target/html.json"), "html").unwrap()
}

fn node(json: serde_json::Value) -> Node {
    serde_json::from_value(json).unwrap()
}

fn definition(json: serde_json::Value) -> Definition {
    Definition::parse(&serde_json::to_vec(&json).unwrap(), "test").unwrap()
}

fn el(name: &str) -> String {
    format!("urn:test:ui:element:name:{name}")
}

fn use_of(name: &str) -> serde_json::Value {
    serde_json::json!({ "kind": el(name), "properties": { "fallback": format!("{name} text") } })
}

fn invalid(result: Result<Node, Error>) -> String {
    match result {
        Err(Error::InvalidArgument { detail, .. }) => detail,
        other => panic!("not InvalidArgument: {other:?}"),
    }
}

#[test]
fn element_names_live_in_their_author_s_namespace_never_under_the_kinds() {
    assert!(model::is_element("urn:iki:ui:element:name:rating"));
    assert!(model::is_element("urn:acme:shop:ui:element:name:card"));
    assert!(!model::is_element(
        "urn:iki:ui:kind:name:x:ui:element:name:card"
    ));
    assert!(!model::is_element("urn:iki:ui:element:name:"));
    assert!(!model::is_element("urn:Acme:ui:element:name:card"));
    assert!(!model::is_element("heading"));
    // An element node needs its fallback text.
    let bare = node(serde_json::json!({ "kind": el("x") }));
    assert!(model::validate(&bare).is_err());
}

#[test]
fn an_element_expands_with_its_arguments_spliced_and_its_children_in_its_slot() {
    let definitions: BTreeMap<_, _> = [(
        el("card"),
        definition(serde_json::json!({
            "arguments": [{ "name": "title", "required": true }, { "name": "level", "default": "3" }],
            "expansion": { "kind": "group", "properties": { "label": "{title}" }, "children": [
                { "kind": "heading", "properties": { "content": "{title}", "level": 3 } },
                "{children}"
            ] }
        })),
    )]
    .into();
    let screen = node(serde_json::json!({
        "kind": el("card"),
        "properties": { "fallback": "A card", "title": "Hello <b>" },
        "children": [ { "kind": "text", "properties": { "content": "inside" } },
                      { "kind": el("card"), "properties": { "fallback": "Inner", "title": "Inner" } } ]
    }));
    let out = element::expand_with(&screen, &html(), &definitions).unwrap();
    assert_eq!(out.kind, "group");
    assert_eq!(
        out.properties["label"],
        model::Value::Text("Hello <b>".into())
    );
    assert_eq!(
        out.children.len(),
        3,
        "heading, then the element's two children"
    );
    assert_eq!(out.children[1].kind, "text");
    // A card inside a card's children is not a cycle: children are the caller's.
    assert_eq!(out.children[2].kind, "group");
    // An argument the contract does not name is refused, and so is a missing required one.
    let extra = node(serde_json::json!({ "kind": el("card"),
        "properties": { "fallback": "x", "title": "t", "colour": "red" } }));
    assert!(invalid(element::expand_with(&extra, &html(), &definitions)).contains("colour"));
    let missing = node(use_of("card"));
    assert!(invalid(element::expand_with(&missing, &html(), &definitions)).contains("title"));
}

#[test]
fn the_ladder_is_native_then_expansion_then_declared_fallback_then_text() {
    let definitions: BTreeMap<_, _> = [(
        el("a"),
        definition(serde_json::json!({ "arguments": [],
            "requires": { "capability": "spatial", "version": "2.1" },
            "expansion": { "kind": "status", "properties": { "content": "{fallback}" } } })),
    )]
    .into();
    let mut screen = node(use_of("a"));
    screen.replacement = Some(Box::new(node(serde_json::json!(
        { "kind": "heading", "properties": { "content": "declared", "level": 2 } }
    ))));
    let mut face = html();
    // Native: claimed, kept as the element.
    face.elements.push(el("a"));
    face.capabilities.push(Capability {
        name: "spatial".into(),
        version: "2.3".into(),
    });
    assert_eq!(
        element::expand_with(&screen, &face, &definitions)
            .unwrap()
            .kind,
        el("a")
    );
    // Expansion: unclaimed, requires met.
    face.elements.clear();
    assert_eq!(
        element::expand_with(&screen, &face, &definitions)
            .unwrap()
            .kind,
        "status"
    );
    // Declared fallback: requires unmet (another major), so no expansion.
    face.capabilities.last_mut().unwrap().version = "3.0".into();
    assert_eq!(
        element::expand_with(&screen, &face, &definitions)
            .unwrap()
            .kind,
        "heading"
    );
    // Fallback text: no definition, no replacement.
    screen.replacement = None;
    let out = element::expand_with(&screen, &face, &BTreeMap::new()).unwrap();
    assert_eq!(out.kind, "text");
    assert_eq!(
        out.properties["content"],
        model::Value::Text("a text".into())
    );
    // Unexpanded through `apply`, an unclaimed element still reaches its fallback text.
    assert_eq!(face.apply(&screen).unwrap().kind, "text");
}

#[test]
fn a_deep_chain_refuses_past_the_depth_bound() {
    let chain = |n: usize| -> BTreeMap<String, Definition> {
        (0..n)
            .map(|i| {
                let expansion = if i + 1 == n {
                    serde_json::json!({ "kind": "text", "properties": { "content": "end" } })
                } else {
                    use_of(&format!("e{}", i + 1))
                };
                (
                    el(&format!("e{i}")),
                    definition(serde_json::json!({ "arguments": [], "expansion": expansion })),
                )
            })
            .collect()
    };
    let screen = node(use_of("e0"));
    let ok = element::expand_with(&screen, &html(), &chain(MAX_DEPTH)).unwrap();
    assert_eq!(ok.kind, "text");
    let detail = invalid(element::expand_with(
        &screen,
        &html(),
        &chain(MAX_DEPTH + 1),
    ));
    assert!(detail.contains("nested past"), "{detail}");
}

#[test]
fn a_wide_fan_out_refuses_past_the_node_bound() {
    // Each level is ten uses of the next: a small input, 10^5 leaves.
    let mut definitions = BTreeMap::new();
    for i in 0..5 {
        let next = if i == 4 {
            serde_json::json!({ "kind": "text", "properties": { "content": "leaf" } })
        } else {
            use_of(&format!("f{}", i + 1))
        };
        let children: Vec<_> = (0..10).map(|_| next.clone()).collect();
        definitions.insert(
            el(&format!("f{i}")),
            definition(serde_json::json!({ "arguments": [],
                "expansion": { "kind": "list", "properties": { "label": "fan" }, "children": children } })),
        );
    }
    let detail = invalid(element::expand_with(
        &node(use_of("f0")),
        &html(),
        &definitions,
    ));
    assert!(detail.contains(&MAX_NODES.to_string()), "{detail}");
    // Three levels (1 + 10 + 100 lists, 1000 leaves) is within it.
    let ok = element::expand_with(&node(use_of("f2")), &html(), &definitions).unwrap();
    assert_eq!(ok.children.len(), 10);
}

#[test]
fn a_cycle_refuses() {
    let definitions: BTreeMap<_, _> = [
        (
            el("p"),
            definition(serde_json::json!({ "arguments": [], "expansion": use_of("q") })),
        ),
        (
            el("q"),
            definition(serde_json::json!({ "arguments": [], "expansion": use_of("p") })),
        ),
    ]
    .into();
    let detail = invalid(element::expand_with(
        &node(use_of("p")),
        &html(),
        &definitions,
    ));
    assert!(detail.contains("cycle"), "{detail}");
}

#[test]
fn hints_are_bounded_namespaced_and_kept_only_by_a_face_that_reads_them() {
    let gpui = Descriptor::parse(include_bytes!("../assets/target/gpui.json"), "gpui").unwrap();
    let screen = node(serde_json::json!({
        "kind": "text", "properties": { "content": "hi" },
        "hint": {
            "urn:iki:ui:hint:gpui:padding": 8,
            "urn:iki:ui:hint:gpui:depth": { "is": 2, "capability": "spatial", "version": "1.0" },
            "urn:iki:ui:hint:spatial:anchor": "floor"
        }
    }));
    model::validate(&screen).unwrap();
    let kept = gpui.apply(&screen).unwrap();
    assert_eq!(
        kept.hint.keys().collect::<Vec<_>>(),
        ["urn:iki:ui:hint:gpui:padding"]
    );
    assert!(
        html().apply(&screen).unwrap().hint.is_empty(),
        "html reads no hints"
    );
    // What the screen means does not change: everything but the hints is the same.
    let mut bare = kept.clone();
    bare.hint.clear();
    assert_eq!(bare, html().apply(&screen).unwrap());
    // Never markup, never a boolean, never an unnamespaced key, never too many.
    for bad in [
        serde_json::json!({ "urn:iki:ui:hint:gpui:x": "<b>" }),
        serde_json::json!({ "urn:iki:ui:hint:gpui:x": true }),
        serde_json::json!({ "padding": 8 }),
        serde_json::json!({ "urn:iki:ui:hint:gpui:": 8 }),
        serde_json::Value::Object(
            (0..17)
                .map(|i| (format!("urn:iki:ui:hint:gpui:h{i}"), 1.into()))
                .collect(),
        ),
    ] {
        let n = node(
            serde_json::json!({ "kind": "text", "properties": { "content": "hi" }, "hint": bad }),
        );
        assert!(model::validate(&n).is_err(), "{bad}");
    }
    // The JSON-LD face names every hint object: no blank node.
    let ld = model::json_ld(&screen, "urn:t");
    assert_eq!(
        ld["hint"]["urn:iki:ui:hint:gpui:depth"]["@id"],
        "urn:t:node:path:/:hint:urn:iki:ui:hint:gpui:depth"
    );
}

#[test]
fn the_rating_is_expanded_on_html_and_claimed_natively_by_no_face() {
    let kernel = common::host(None);
    let screen = common::source(&kernel, element::SCREEN).unwrap();
    let screen = model::parse(&screen.bytes).unwrap();
    assert_eq!(screen.children[1].kind, "urn:iki:ui:element:name:rating");
    let html = common::text(&kernel, "urn:iki:ui:html:screen:elements", &[]);
    assert!(
        html.contains("<section class=\"ui-group\" aria-label=\"Service\">"),
        "{html}"
    );
    assert!(html.contains("Service: 4 out of 5"), "{html}");
    for target in ["html", "gpui"] {
        let face = common::source(
            &kernel,
            &format!("urn:iki:ui:face:target:{target}:screen:elements"),
        )
        .unwrap();
        let face = model::parse(&face.bytes).unwrap();
        assert_eq!(
            face.children[1].kind, "group",
            "{target} expands the rating"
        );
    }
    let support: serde_json::Value =
        serde_json::from_slice(&common::source(&kernel, "urn:iki:ui:support").unwrap().bytes)
            .unwrap();
    for target in ["html", "gpui"] {
        assert_eq!(
            support[target]["element"]["urn:iki:ui:element:name:rating"],
            false
        );
    }
    let page = common::text(&kernel, "urn:iki:ui:support", &[("as", "text/html")]);
    assert!(
        page.contains("urn:iki:ui:element:name:rating")
            && page.contains("Expanded from its definition")
    );
}

#[test]
fn editing_a_definition_recomputes_the_screens_that_use_it() {
    let dir = common::scratch("element-thread");
    let kernel = common::host(Some(&dir));
    let gallery = common::text(&kernel, "urn:iki:ui:html:screen:gallery", &[]);
    let before = common::text(&kernel, "urn:iki:ui:html:screen:elements", &[]);
    assert!(before.contains("Service: 4 out of 5"));
    let mine = String::from_utf8(element::ELEMENTS[0].bytes.to_vec())
        .unwrap()
        .replace(
            "{label}: {score} out of {out-of}",
            "{score}/{out-of} for {label}",
        );
    common::issue(
        &kernel,
        Verb::Sink,
        "urn:iki:ui:dir:element/rating.json",
        &[("content", mine.as_bytes())],
    )
    .unwrap();
    let after = common::text(&kernel, "urn:iki:ui:html:screen:elements", &[]);
    assert!(after.contains("4/5 for Service"), "{after}");
    let face = common::source(&kernel, "urn:iki:ui:face:target:gpui:screen:elements").unwrap();
    assert!(String::from_utf8(face.bytes)
        .unwrap()
        .contains("4/5 for Service"));
    assert_eq!(
        common::text(&kernel, "urn:iki:ui:html:screen:gallery", &[]),
        gallery
    );
    common::issue(
        &kernel,
        Verb::Delete,
        "urn:iki:ui:dir:element/rating.json",
        &[],
    )
    .unwrap();
    assert_eq!(
        common::text(&kernel, "urn:iki:ui:html:screen:elements", &[]),
        before
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

// --- card, key-value and tag-list (ledger #3511): how each translates -------------------------

const NEW: [&str; 3] = ["card", "key-value", "tag-list"];

/// The elements screen in a host that has none of `NEW`'s definitions (each NotFound), so each
/// takes the ladder's last rung: its fallback text.
fn host_without_definitions() -> ikigai_core::Kernel {
    use ikigai_core::{EndpointSpace, Exact, Fallback, FnEndpoint, Invocation};
    use std::sync::Arc;
    let mut withheld = EndpointSpace::new();
    for name in NEW {
        withheld = withheld.bind(
            Exact::new(format!("{}{name}", element::PREFIX)),
            FnEndpoint::new("test-no-definition", |_: &Invocation<'_>| {
                Err(Error::NotFound("no definition here".into()))
            }),
        );
    }
    common::kernel(Arc::new(Fallback::new(vec![
        Arc::new(withheld),
        common::root(None, Vec::new()),
    ])))
}

fn example(name: &str) -> Node {
    let i = element::ELEMENTS
        .iter()
        .position(|a| a.iri == format!("{}{name}", element::PREFIX))
        .unwrap();
    model::parse(element::EXAMPLES[i].as_bytes()).unwrap()
}

#[test]
fn card_key_value_and_tag_list_are_compositions_of_core_kinds_on_the_elements_screen() {
    let kernel = common::host(None);
    let screen = common::source(&kernel, element::SCREEN).unwrap();
    let screen = model::parse(&screen.bytes).unwrap();
    let kinds: Vec<&str> = screen.children.iter().map(|c| c.kind.as_str()).collect();
    for name in NEW {
        let iri = format!("{}{name}", element::PREFIX);
        assert!(
            kinds.contains(&iri.as_str()),
            "{iri} on the elements screen"
        );
        // Its definition reads, and its expansion names only core kinds: no element, no markup.
        let def = common::source(&kernel, &iri).unwrap();
        let def = Definition::parse(&def.bytes, &iri).unwrap();
        let text = def.expansion.to_string();
        assert!(!text.contains("urn:") && !text.contains('<'), "{text}");
    }
}

#[test]
fn the_new_elements_expand_into_core_kinds_on_both_faces() {
    let kernel = common::host(None);
    for target in ["html", "gpui"] {
        let face = common::source(
            &kernel,
            &format!("urn:iki:ui:face:target:{target}:screen:elements"),
        )
        .unwrap();
        let face = model::parse(&face.bytes).unwrap();
        // [heading, rating, card, key-value, tag-list]
        let card = &face.children[2];
        assert_eq!(card.kind, "group", "{target}: a card is a named group");
        assert_eq!(card.children[0].kind, "heading");
        assert_eq!(
            card.children[0].properties["level"],
            model::Value::Number(3)
        );
        assert_eq!(card.children[1].kind, "text");
        assert_eq!(
            card.children[2].kind, "list",
            "{target}: its links follow the body"
        );
        let kv = &face.children[3];
        // HTML keeps the table; GPUI's descriptor takes a table to a list.
        let expected = if target == "html" { "table" } else { "list" };
        assert_eq!(kv.kind, expected, "{target}: a key-value list");
        assert_eq!(kv.children.len(), 3);
        let tags = &face.children[4];
        assert_eq!(tags.kind, "list", "{target}: a tag list");
        assert_eq!(tags.children[0].kind, "link");
        assert_eq!(tags.children[1].kind, "text");
    }
    let support: serde_json::Value =
        serde_json::from_slice(&common::source(&kernel, "urn:iki:ui:support").unwrap().bytes)
            .unwrap();
    let page = common::text(&kernel, "urn:iki:ui:support", &[("as", "text/html")]);
    for name in NEW {
        let iri = format!("{}{name}", element::PREFIX);
        for target in ["html", "gpui"] {
            assert_eq!(support[target]["element"][&iri], false, "{target} {iri}");
        }
        assert!(
            page.contains(&format!(
                "<tr><th scope=\"row\">{iri}</th><td>Expanded from its definition</td><td>Expanded from its definition</td></tr>"
            )),
            "{page}"
        );
    }
}

#[test]
fn a_face_that_claims_one_keeps_it_whole_for_its_native_renderer() {
    for name in NEW {
        let iri = format!("{}{name}", element::PREFIX);
        let node = example(name);
        let mut face = html();
        face.elements.push(iri.clone());
        let kept = element::expand_with(&node, &face, &BTreeMap::new()).unwrap();
        assert_eq!(kept, node, "{iri} kept, its children too");
    }
}

#[test]
fn the_elements_html_face_matches_its_snapshots_expanded_and_as_fallback_text() {
    let expanded = common::text(&common::host(None), "urn:iki:ui:html:screen:elements", &[]);
    let fallback = common::text(
        &host_without_definitions(),
        "urn:iki:ui:html:screen:elements",
        &[],
    );
    for text in [
        "<p>Field notes: weekly survey of the river gauges.</p>",
        "<p>Gauge 14: Mill Lane, 2.31 m, read 2026-11-10.</p>",
        "<p>Topics: hydrology, flooding, open data.</p>",
    ] {
        assert!(fallback.contains(text), "{fallback}");
        assert!(!expanded.contains(text), "{expanded}");
    }
    ikigai_ui::testkit::snapshot::assert_snapshots(
        &common::snapshots(),
        vec![
            ("elements-expanded.html".to_string(), expanded),
            ("elements-fallback.html".to_string(), fallback),
        ],
    );
}

// --- disclosure (ledger #3512): native on both faces, expanded where a face does not claim it --

const DISCLOSURE: &str = "urn:iki:ui:element:name:disclosure";

/// The html descriptor with no element claimed: the native renderer turned off, from the UI
/// directory the way a host overrides it.
fn html_without_native(dir: &std::path::Path) -> ikigai_core::Kernel {
    let kernel = common::host(Some(dir));
    let mut descriptor: serde_json::Value =
        serde_json::from_slice(include_bytes!("../assets/target/html.json")).unwrap();
    descriptor.as_object_mut().unwrap().remove("elements");
    common::issue(
        &kernel,
        Verb::Sink,
        "urn:iki:ui:dir:target/html.json",
        &[("content", &serde_json::to_vec(&descriptor).unwrap())],
    )
    .unwrap();
    kernel
}

/// A screen holding only `node`, at `urn:iki:ui:screen:name:disclosure-test`.
fn host_with_screen(node: serde_json::Value) -> ikigai_core::Kernel {
    use ikigai_core::{EndpointSpace, Exact, FnEndpoint, Invocation, ReprType, Representation};
    use std::sync::Arc;
    let bytes = serde_json::to_vec(&node).unwrap();
    common::host_with(
        None,
        vec![Arc::new(EndpointSpace::new().bind(
            Exact::new("urn:iki:ui:screen:name:disclosure-test"),
            FnEndpoint::new("test-screen", move |_: &Invocation<'_>| {
                Ok(
                    Representation::new(ReprType::new(model::MEDIA_TYPE), bytes.clone())
                        .cacheable(),
                )
            }),
        ))],
    )
}

fn disclosure_html(html: &str) -> &str {
    let start = html
        .find("<details")
        .or_else(|| html.find("<section class=\"ui-group\" aria-label=\"How often"))
        .expect(html);
    &html[start..]
}

#[test]
fn a_disclosure_is_a_definition_of_core_kinds_and_both_faces_claim_it() {
    let kernel = common::host(None);
    let def = common::source(&kernel, DISCLOSURE).unwrap();
    let def = Definition::parse(&def.bytes, DISCLOSURE).unwrap();
    let text = def.expansion.to_string();
    assert!(!text.contains("urn:") && !text.contains('<'), "{text}");
    // The model carries only the initial state: a boolean argument, closed by default.
    let open = def.arguments.iter().find(|a| a.name == "open").unwrap();
    assert_eq!(
        (open.required, open.default.as_deref()),
        (false, Some("false"))
    );
    let screen = model::parse(&common::source(&kernel, element::SCREEN).unwrap().bytes).unwrap();
    assert_eq!(screen.children[5].kind, DISCLOSURE);
    for target in ["html", "gpui"] {
        let face = common::source(
            &kernel,
            &format!("urn:iki:ui:face:target:{target}:screen:elements"),
        )
        .unwrap();
        let face = model::parse(&face.bytes).unwrap();
        // Kept whole for the native renderer, the same pure value the screen holds.
        assert_eq!(face.children[5], screen.children[5], "{target}");
    }
    let support: serde_json::Value =
        serde_json::from_slice(&common::source(&kernel, "urn:iki:ui:support").unwrap().bytes)
            .unwrap();
    for target in ["html", "gpui"] {
        assert_eq!(support[target]["element"][DISCLOSURE], true, "{target}");
    }
    let page = common::text(&kernel, "urn:iki:ui:support", &[("as", "text/html")]);
    assert!(
        page.contains(&format!(
            "<tr><th scope=\"row\">{DISCLOSURE}</th><td>Native</td><td>Native</td></tr>"
        )),
        "{page}"
    );
}

#[test]
fn the_html_face_renders_a_disclosure_natively_as_details_and_summary() {
    let html = common::text(&common::host(None), "urn:iki:ui:html:screen:elements", &[]);
    let details = disclosure_html(&html);
    // Closed (the example's `open` is false): no `open` attribute; no script, no ARIA needed.
    assert!(
        details.starts_with(
            "<details class=\"ui-disclosure\"><summary>How often is the gauge read?</summary>\
             <p>Gauge 14 is read every morning"
        ),
        "{details}"
    );
    // The body's link is a real <a> inside the <details>: in the tab order when it is open.
    assert!(
        details.contains("<a href=\"/page/contact\">Ask the hydrology desk</a></details>"),
        "{details}"
    );
    assert!(!details.contains("<script") && !details.contains("aria-"));
    // Initially open when the model says so; text stays escaped.
    let kernel = host_with_screen(serde_json::json!({
        "kind": DISCLOSURE,
        "properties": { "fallback": "Notes", "summary": "Notes <b>", "level": 2,
                        "body": "Read & kept.", "open": true }
    }));
    let open = common::text(&kernel, "urn:iki:ui:html:screen:disclosure-test", &[]);
    assert_eq!(
        open,
        "<details class=\"ui-disclosure\" open><summary>Notes &lt;b&gt;</summary><p>Read &amp; kept.</p></details>"
    );
}

#[test]
fn with_the_native_renderer_off_a_disclosure_expands_to_a_heading_and_its_body_always_shown() {
    let dir = common::scratch("disclosure-expanded");
    let kernel = html_without_native(&dir);
    let html = common::text(&kernel, "urn:iki:ui:html:screen:elements", &[]);
    assert!(!html.contains("<details"), "{html}");
    let section = disclosure_html(&html);
    assert!(
        section.starts_with(
            "<section class=\"ui-group\" aria-label=\"How often is the gauge read?\">\
             <h3>How often is the gauge read?</h3><p>Gauge 14 is read every morning"
        ),
        "{section}"
    );
    // Nothing is hidden: the body's link is always in the page.
    assert!(section.contains("<a href=\"/page/contact\">Ask the hydrology desk</a></section>"));
    let face = common::source(&kernel, "urn:iki:ui:face:target:html:screen:elements").unwrap();
    let face = model::parse(&face.bytes).unwrap();
    assert_eq!(face.children[5].kind, "group");
    assert_eq!(face.children[5].children[0].kind, "heading");
    assert_eq!(
        face.children[5].children[0].properties["level"],
        model::Value::Number(3)
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_disclosure_html_matches_its_snapshots_native_and_expanded() {
    let native = common::text(
        &common::host(None),
        "urn:iki:ui:html:screen:elements:node:n.5",
        &[],
    );
    let dir = common::scratch("disclosure-snapshot");
    let expanded = common::text(
        &html_without_native(&dir),
        "urn:iki:ui:html:screen:elements:node:n.5",
        &[],
    );
    std::fs::remove_dir_all(&dir).unwrap();
    ikigai_ui::testkit::snapshot::assert_snapshots(
        &common::snapshots(),
        vec![
            ("disclosure-native.html".to_string(), native),
            ("disclosure-expanded.html".to_string(), expanded),
        ],
    );
}
