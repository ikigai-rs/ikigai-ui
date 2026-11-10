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
