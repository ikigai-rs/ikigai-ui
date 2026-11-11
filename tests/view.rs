//! Views generated from a resource's own model, recursively, with curated views as overrides
//! (ledger #3366): a resource with a contract, a type with a shape and a nested resource; a
//! caller with and without a Sink grant; a curated type view; a cycle; the depth bound; a
//! nested resource the caller may not read, withheld.
mod common;

use std::sync::Arc;

use futures::executor::block_on;
use ikigai_core::{
    ActionSpec, ArgSpec, Capability, Description, EndpointSpace, Error, Exact, FnEndpoint,
    Invocation, Iri, Kernel, ReprType, Representation, Request, Verb,
};
use ikigai_ui::model::{self, Node, Value};

const WRITE: &str = "urn:cap:demo:thing:write";
const CARD: &str = "urn:cap:demo:card:read";
const SECRET: &str = "4111 1111 1111 1111";

fn json(id: &'static str, body: serde_json::Value, sink: bool) -> FnEndpoint {
    let bytes = serde_json::to_vec(&body).unwrap();
    let mut d = Description::new(id)
        .title(format!("Thing {id}"))
        .summary("A demo thing.")
        .verb(Verb::Source)
        .verb(Verb::Meta)
        .output("application/json");
    if sink {
        d = d.verb(Verb::Sink).action(
            ActionSpec::new(Verb::Sink)
                .summary("Leave a note.")
                .requires(WRITE)
                .input(ArgSpec::new("note").optional())
                .output("text/plain"),
        );
    }
    FnEndpoint::new(id, move |inv: &Invocation<'_>| {
        if id == "card" && !inv.capability.allows(CARD) {
            return Err(Error::Denied(format!("{CARD} is needed")));
        }
        Ok(Representation::new(ReprType::new("application/json"), bytes.clone()).cacheable())
    })
    .with_description(d)
}

fn fixed(id: &'static str, media: &'static str, body: serde_json::Value) -> FnEndpoint {
    let bytes = serde_json::to_vec(&body).unwrap();
    FnEndpoint::new(id, move |_: &Invocation<'_>| {
        Ok(Representation::new(ReprType::new(media), bytes.clone()).cacheable())
    })
    .with_description(
        Description::new(id)
            .title(id)
            .summary("Fixed.")
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .output(media),
    )
}

const JSON_LD: &str = "application/ld+json";

/// The Thing shape as Turtle, the way a SHACL author writes it.
const TURTLE_SHAPE: &str = r#"@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix demo: <urn:demo:> .
demo:ThingShape a sh:NodeShape ;
    sh:targetClass demo:Thing ;
    rdfs:label "Thing" ;
    sh:property [ sh:path demo:friend ; sh:name "Friend" ; sh:order 2 ] ,
        [ sh:path demo:name ; sh:name "Name" ; sh:order 1 ] ,
        [ sh:path demo:card ; sh:name "Card" ; sh:order 3 ] .
"#;

/// The same shape as compact JSON-LD in the view's own terms.
const COMPACT_SHAPE: &str = r#"{
  "@context": {
    "sh": "http://www.w3.org/ns/shacl#",
    "rdfs": "http://www.w3.org/2000/01/rdf-schema#",
    "targetClass": {"@id": "sh:targetClass", "@type": "@id"},
    "label": "rdfs:label",
    "property": {"@id": "sh:property", "@container": "@set"},
    "path": {"@id": "sh:path", "@type": "@id"},
    "name": "sh:name",
    "order": "sh:order"
  },
  "@id": "urn:demo:ThingShape",
  "@type": "sh:NodeShape",
  "targetClass": "urn:demo:Thing",
  "label": "Thing",
  "property": [
    {"path": "urn:demo:friend", "name": "Friend", "order": 2},
    {"path": "urn:demo:name", "name": "Name", "order": 1},
    {"path": "urn:demo:card", "name": "Card", "order": 3}
  ]
}"#;

/// The same shape as JSON-LD with other prefixes, an `@graph` and its property nodes apart.
const PREFIXED_SHAPE: &str = r#"{
  "@context": {"shacl": "http://www.w3.org/ns/shacl#", "s": "http://www.w3.org/2000/01/rdf-schema#"},
  "@graph": [
    {"@id": "urn:demo:OtherShape", "shacl:targetClass": {"@id": "urn:demo:Other"},
     "shacl:property": {"@id": "_:x"}},
    {"@id": "_:x", "shacl:path": {"@id": "urn:demo:hidden"}, "shacl:name": "Hidden"},
    {"@id": "_:f", "shacl:path": {"@id": "urn:demo:friend"}, "shacl:name": "Friend", "shacl:order": 2},
    {"@id": "urn:demo:ThingShape", "@type": "shacl:NodeShape",
     "shacl:targetClass": {"@id": "urn:demo:Thing"}, "s:label": {"@value": "Thing", "@language": "en"},
     "shacl:property": [{"@id": "_:f"},
       {"shacl:path": {"@id": "urn:demo:name"}, "shacl:name": "Name", "shacl:order": 1},
       {"shacl:path": {"@id": "urn:demo:card"}, "shacl:name": "Card", "shacl:order": 3}]}
  ]
}"#;

/// The same shape expanded: every key a full IRI, no context.
const EXPANDED_SHAPE: &str = r#"[{
  "@id": "urn:demo:ThingShape",
  "@type": ["http://www.w3.org/ns/shacl#NodeShape"],
  "http://www.w3.org/ns/shacl#targetClass": [{"@id": "urn:demo:Thing"}],
  "http://www.w3.org/2000/01/rdf-schema#label": [{"@value": "Thing"}],
  "http://www.w3.org/ns/shacl#property": [
    {"http://www.w3.org/ns/shacl#path": [{"@id": "urn:demo:card"}],
     "http://www.w3.org/ns/shacl#name": [{"@value": "Card"}],
     "http://www.w3.org/ns/shacl#order": [{"@value": 3}]},
    {"http://www.w3.org/ns/shacl#path": [{"@id": "urn:demo:name"}],
     "http://www.w3.org/ns/shacl#name": [{"@value": "Name"}],
     "http://www.w3.org/ns/shacl#order": [{"@value": 1}]},
    {"http://www.w3.org/ns/shacl#path": [{"@id": "urn:demo:friend"}],
     "http://www.w3.org/ns/shacl#name": [{"@value": "Friend"}],
     "http://www.w3.org/ns/shacl#order": [{"@value": 2}]}
  ]
}]"#;

/// A shape served as `media`, whatever face it is asked for (as a file is).
fn shape_at(id: &'static str, media: &'static str, body: String) -> FnEndpoint {
    FnEndpoint::new(id, move |_: &Invocation<'_>| {
        Ok(Representation::new(ReprType::new(media), body.clone().into_bytes()).cacheable())
    })
    .with_description(
        Description::new(id)
            .title(id)
            .summary("A shape.")
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .output(media),
    )
}

fn space() -> EndpointSpace {
    let mut s = EndpointSpace::new()
        .bind(
            Exact::new("urn:demo:a"),
            json(
                "a",
                serde_json::json!({
                    "@type": "urn:demo:Thing",
                    "name": "Alpha",
                    "hidden": "not in the shape",
                    "friend": {"@id": "urn:demo:b"},
                    "card": {"@id": "urn:demo:card"}
                }),
                true,
            ),
        )
        .bind(
            Exact::new("urn:demo:b"),
            json(
                "b",
                serde_json::json!({
                    "@type": "urn:demo:Pal",
                    "nick": "Bravo",
                    "back": {"@id": "urn:demo:a"}
                }),
                false,
            ),
        )
        .bind(
            Exact::new("urn:demo:card"),
            json("card", serde_json::json!({"number": SECRET}), false),
        );
    // A chain c0 → c1 → … → c9, deeper than the bound.
    const CHAIN: [&str; 10] = [
        "urn:demo:c0",
        "urn:demo:c1",
        "urn:demo:c2",
        "urn:demo:c3",
        "urn:demo:c4",
        "urn:demo:c5",
        "urn:demo:c6",
        "urn:demo:c7",
        "urn:demo:c8",
        "urn:demo:c9",
    ];
    const IDS: [&str; 10] = ["c0", "c1", "c2", "c3", "c4", "c5", "c6", "c7", "c8", "c9"];
    for i in 0..10 {
        let body = match CHAIN.get(i + 1) {
            Some(next) => serde_json::json!({"step": i, "next": {"@id": next}}),
            None => serde_json::json!({"step": i}),
        };
        s = s.bind(Exact::new(CHAIN[i]), json(IDS[i], body, false));
    }
    s
}

fn host(curated: bool) -> Kernel {
    let mut s = space().bind(
        Exact::new("urn:iki:ui:shape:type:urn:demo:Thing"),
        shape_at("thing-shape", JSON_LD, COMPACT_SHAPE.to_string()),
    );
    if curated {
        s = s.bind(
            Exact::new("urn:iki:ui:view:type:urn:demo:Pal"),
            fixed(
                "pal-view",
                model::MEDIA_TYPE,
                serde_json::json!({
                    "kind": "group",
                    "properties": {"label": "A pal"},
                    "children": [
                        {"kind": "text", "properties": {"content": "Curated view of {iri}"}},
                        {"kind": "reference", "properties": {
                            "resource": "urn:iki:ui:view:generated:{iri}",
                            "fallback": "Pal withheld"
                        }}
                    ]
                }),
            ),
        );
    }
    common::host_with(None, vec![Arc::new(s), json_ld()])
}

/// The host's RDF and JSON-LD modules, which read a shape for the view.
fn json_ld() -> Arc<dyn ikigai_core::Space> {
    Arc::new(ikigai_core::Fallback::new(vec![
        Arc::new(ikigai_rdf::space()) as Arc<dyn ikigai_core::Space>,
        Arc::new(ikigai_jsonld::space()),
    ]))
}

/// A host whose Thing shape is `shape` (and, unless `modules` is false, the JSON-LD modules).
fn shaped(shape: FnEndpoint, modules: bool) -> Kernel {
    let s = space().bind(Exact::new("urn:iki:ui:shape:type:urn:demo:Thing"), shape);
    let mut extra: Vec<Arc<dyn ikigai_core::Space>> = vec![Arc::new(s)];
    if modules {
        extra.push(json_ld());
    }
    common::host_with(None, extra)
}

fn view(kernel: &Kernel, iri: &str, cap: &Capability) -> Node {
    let at = Iri::parse(format!("urn:iki:ui:view:resource:{iri}")).unwrap();
    let rep = block_on(kernel.issue(Request::new(Verb::Source, at), cap)).unwrap();
    model::parse(&rep.bytes).unwrap()
}

fn all(n: &Node, out: &mut Vec<Node>) {
    out.push(n.clone());
    for c in &n.children {
        all(c, out);
    }
}

fn texts(n: &Node) -> Vec<String> {
    let mut nodes = Vec::new();
    all(n, &mut nodes);
    nodes
        .iter()
        .flat_map(|n| n.properties.values())
        .filter_map(|v| match v {
            Value::Text(s) => Some(s.clone()),
            _ => None,
        })
        .collect()
}

fn kinds(n: &Node, kind: &str) -> Vec<Node> {
    let mut nodes = Vec::new();
    all(n, &mut nodes);
    nodes.into_iter().filter(|n| n.kind == kind).collect()
}

#[test]
fn a_view_is_generated_from_contract_shape_and_data_with_nested_resources() {
    let kernel = host(false);
    let wide = Capability::scoped([WRITE, CARD]);
    let v = view(&kernel, "urn:demo:a", &wide);
    let t = texts(&v);
    // The contract: title, summary, faces.
    let heading = &kinds(&v, "heading")[0];
    assert_eq!(heading.properties["content"], Value::Text("Thing a".into()));
    assert!(t.iter().any(|s| s == "A demo thing."), "{t:?}");
    assert!(
        t.iter().any(|s| s.starts_with("Faces: application/json")),
        "{t:?}"
    );
    // The shape: only its properties, its labels, its order.
    let props = v
        .children
        .iter()
        .find(|c| c.properties.get("label") == Some(&Value::Text("Properties".into())))
        .expect("a properties group");
    assert_eq!(props.children[0].kind, "text");
    assert_eq!(
        props.children[0].properties["content"],
        Value::Text("Name: Alpha".into())
    );
    assert_eq!(
        props.children[1].properties["label"],
        Value::Text("Friend".into())
    );
    assert!(!t.iter().any(|s| s.contains("not in the shape")), "{t:?}");
    // The nested resource renders through its own (generated) view.
    assert!(t.iter().any(|s| s == "Thing b"), "{t:?}");
    assert!(t.iter().any(|s| s == "nick: Bravo"), "{t:?}");
    // Readable under the grant.
    assert!(t.iter().any(|s| s.contains(SECRET)), "{t:?}");
}

#[test]
fn a_form_for_each_action_the_caller_may_take_and_none_for_the_others() {
    let kernel = host(false);
    let with = view(&kernel, "urn:demo:a", &Capability::scoped([WRITE]));
    let forms = kinds(&with, "form");
    assert_eq!(forms.len(), 1, "{with:?}");
    assert_eq!(
        forms[0].properties["resource"],
        Value::Text("urn:demo:a".into())
    );
    // b has no Sink: no form for b, even under the grant (a's, nested in b's view, is there).
    let b = kinds(
        &view(&kernel, "urn:demo:b", &Capability::scoped([WRITE])),
        "form",
    );
    assert!(b
        .iter()
        .all(|f| f.properties["resource"] == Value::Text("urn:demo:a".into())));

    let without = view(
        &host(false),
        "urn:demo:a",
        &Capability::scoped(["urn:cap:demo:other"]),
    );
    assert!(kinds(&without, "form").is_empty(), "fresh: {without:?}");
    // After the grant's answer filled the cache, the narrow caller still gets none.
    let without = view(
        &kernel,
        "urn:demo:a",
        &Capability::scoped(["urn:cap:demo:other"]),
    );
    assert!(kinds(&without, "form").is_empty(), "{without:?}");
}

#[test]
fn a_property_the_caller_may_not_read_is_withheld() {
    let kernel = host(false);
    let narrow = Capability::scoped([WRITE]);
    let at = Iri::parse("urn:iki:ui:view:resource:urn:demo:a").unwrap();
    let rep = block_on(kernel.issue(Request::new(Verb::Source, at.clone()), &narrow)).unwrap();
    let body = String::from_utf8_lossy(&rep.bytes).into_owned();
    assert!(!body.contains(SECRET), "{body}");
    let v = model::parse(&rep.bytes).unwrap();
    assert!(texts(&v).iter().any(|s| s == "Card: withheld"), "{body}");
    // The answer that swallowed a denial is not cached under the narrow grant.
    assert!(!kernel.is_cached(&Request::new(Verb::Source, at), &narrow));
}

#[test]
fn a_cycle_becomes_a_link_at_the_repeat() {
    let kernel = host(false);
    let v = view(&kernel, "urn:demo:a", &Capability::scoped([WRITE]));
    let links = kinds(&v, "link");
    let back: Vec<_> = links
        .iter()
        .filter(|l| l.properties["href"] == Value::Text("/ui/view/resource/urn:demo:a".into()))
        .collect();
    assert_eq!(back.len(), 1, "{links:?}");
    assert_eq!(back[0].properties["label"], Value::Text("back".into()));
    // From b, the cycle closes at b.
    let v = view(&kernel, "urn:demo:b", &Capability::scoped([WRITE]));
    assert!(kinds(&v, "link")
        .iter()
        .any(|l| l.properties["href"] == Value::Text("/ui/view/resource/urn:demo:b".into())));
}

#[test]
fn past_the_depth_bound_a_nested_resource_is_a_link() {
    let kernel = host(false);
    let v = view(&kernel, "urn:demo:c0", &Capability::scoped([WRITE]));
    let headings: Vec<String> = kinds(&v, "heading")
        .iter()
        .map(|h| format!("{:?}", h.properties["content"]))
        .collect();
    assert_eq!(
        headings.len(),
        ikigai_ui::view::MAX_DEPTH + 1,
        "{headings:?}"
    );
    let links = kinds(&v, "link");
    assert_eq!(links.len(), 1, "{links:?}");
    let expected = format!(
        "/ui/view/resource/urn:demo:c{}",
        ikigai_ui::view::MAX_DEPTH + 1
    );
    assert_eq!(links[0].properties["href"], Value::Text(expected));
}

#[test]
fn a_curated_type_view_overrides_the_generated_one_and_can_start_from_it() {
    let kernel = host(true);
    let cap = Capability::scoped([WRITE]);
    // Nested: b is a Pal, so a shows b through the curated view.
    let v = view(&kernel, "urn:demo:a", &cap);
    let t = texts(&v);
    assert!(t.iter().any(|s| s == "Curated view of urn:demo:b"), "{t:?}");
    assert!(
        t.iter()
            .any(|s| s == "urn:iki:ui:view:generated:urn:demo:b"),
        "{t:?}"
    );
    // At the top too.
    let v = view(&kernel, "urn:demo:b", &cap);
    assert_eq!(v.properties["label"], Value::Text("A pal".into()));
    // The generated view is still there under its own name, which the curated view references.
    let at = Iri::parse("urn:iki:ui:view:generated:urn:demo:b").unwrap();
    let rep = block_on(kernel.issue(Request::new(Verb::Source, at), &cap)).unwrap();
    let g = model::parse(&rep.bytes).unwrap();
    assert!(texts(&g).iter().any(|s| s == "nick: Bravo"));
    // Without a curated view, the same name is the generated one.
    let plain = view(&host(false), "urn:demo:b", &cap);
    assert_eq!(plain.properties["label"], Value::Text("Thing b".into()));
}

#[test]
fn curated_placeholders_are_filled_in_replacement_subtrees() {
    let s = space().bind(
        Exact::new("urn:iki:ui:view:type:urn:demo:Pal"),
        fixed(
            "pal-replacement",
            model::MEDIA_TYPE,
            serde_json::json!({
                "kind": "visual", "properties": {"label": "View of {iri}"},
                "replacement": {
                    "kind": "group", "properties": {"label": "Replacement of {iri}"},
                    "children": [{"kind": "text", "properties": {"content": "Child of {iri}"}}],
                    "replacement": {"kind": "text", "properties": {"content": "Nested {iri}"}}
                }
            }),
        ),
    );
    let kernel = common::host_with(None, vec![Arc::new(s)]);
    let v = view(&kernel, "urn:demo:b", &Capability::root());
    let bytes = serde_json::to_string(&v).unwrap();
    assert!(!bytes.contains("{iri}"), "unfilled placeholder: {bytes}");
    assert!(bytes.contains("Replacement of urn:demo:b"));
    assert!(bytes.contains("Child of urn:demo:b"));
    assert!(bytes.contains("Nested urn:demo:b"));
}

#[test]
fn nested_curated_views_that_cross_the_node_budget_become_links() {
    for replacements in [true, false] {
        // Both variants have more than MAX_NODES nodes. The second hides half of its
        // nodes in replacements, which must count just like ordinary children.
        let children: Vec<_> = (0..if replacements { 140 } else { 280 })
            .map(|_| {
                let mut n = serde_json::json!({"kind": "text", "properties": {"content": "Expanded"}});
                if replacements {
                    n["replacement"] = serde_json::json!({"kind": "text", "properties": {"content": "Replacement"}});
                }
                n
            })
            .collect();
        let s = EndpointSpace::new()
            .bind(Exact::new("urn:demo:collection"), json("collection", serde_json::json!({
                "rows": [{"@id": "urn:demo:large"}, {"@id": "urn:demo:large"}]
            }), false))
            .bind(Exact::new("urn:demo:large"), json("large", serde_json::json!({"@type": "urn:demo:Large"}), false))
            .bind(Exact::new("urn:iki:ui:view:type:urn:demo:Large"), fixed("large-view", model::MEDIA_TYPE,
                serde_json::json!({"kind": "group", "properties": {"label": "Large"}, "children": children})));
        let kernel = common::host_with(None, vec![Arc::new(s)]);
        let v = view(&kernel, "urn:demo:collection", &Capability::root());
        assert!(
            !texts(&v).iter().any(|s| s == "Expanded"),
            "replacements={replacements}"
        );
        assert_eq!(kinds(&v, "link").len(), 2, "{v:?}");
    }
}

#[test]
fn empty_and_null_array_values_produce_valid_views() {
    let s = EndpointSpace::new().bind(
        Exact::new("urn:demo:empty-values"),
        json(
            "empty-values",
            serde_json::json!({"values": ["", null, "present"]}),
            false,
        ),
    );
    let kernel = common::host_with(None, vec![Arc::new(s)]);
    let v = view(&kernel, "urn:demo:empty-values", &Capability::root());
    let lists = kinds(&v, "list");
    let values = lists
        .iter()
        .find(|n| n.properties["label"] == Value::Text("values".into()))
        .unwrap();
    assert_eq!(values.children.len(), 3);
    assert_eq!(
        values.children[0].properties["content"],
        Value::Text("-".into())
    );
    assert_eq!(
        values.children[1].properties["content"],
        Value::Text("-".into())
    );
    assert_eq!(
        values.children[2].properties["content"],
        Value::Text("present".into())
    );
}

#[test]
fn table_layout_does_not_replace_other_types_or_curated_views() {
    for (case, items) in [
        (
            "curated",
            serde_json::json!([{"@id": "urn:demo:curated-row"}]),
        ),
        (
            "mixed",
            serde_json::json!([{"@id": "urn:demo:row"}, {"@id": "urn:demo:b"}]),
        ),
    ] {
        let s = space()
            .bind(
                Exact::new("urn:demo:collection"),
                json("collection", serde_json::json!({"rows": items}), false),
            )
            .bind(
                Exact::new("urn:demo:row"),
                json(
                    "row",
                    serde_json::json!({"@type": "urn:demo:Row", "n": 1}),
                    false,
                ),
            )
            .bind(
                Exact::new("urn:demo:curated-row"),
                json(
                    "curated-row",
                    serde_json::json!({"@type": "urn:demo:CuratedRow", "n": 2}),
                    false,
                ),
            );
        let mut s = s;
        for class in ["Row", "CuratedRow"] {
            s = s.bind(
                Exact::new(format!("urn:iki:ui:shape:type:urn:demo:{class}")),
                shape_at(
                    "table-shape",
                    JSON_LD,
                    serde_json::json!({
                        "@context": {"sh": "http://www.w3.org/ns/shacl#"},
                        "@id": format!("urn:demo:{class}Shape"),
                        "sh:targetClass": {"@id": format!("urn:demo:{class}")},
                        "urn:iki:ui:shape:layout": "table",
                        "sh:property": [{"sh:path": {"@id": "urn:demo:n"}, "sh:name": "N"}]
                    })
                    .to_string(),
                ),
            );
        }
        s = s.bind(
            Exact::new("urn:iki:ui:view:type:urn:demo:CuratedRow"),
            fixed(
                "curated-row-view",
                model::MEDIA_TYPE,
                serde_json::json!({
                    "kind": "text", "properties": {"content": "Curated {iri}"}
                }),
            ),
        );
        let kernel = common::host_with(None, vec![Arc::new(s), json_ld()]);
        let v = view(&kernel, "urn:demo:collection", &Capability::root());
        assert!(kinds(&v, "table").is_empty(), "{case}: {v:?}");
        let t = texts(&v);
        let expected = if case == "mixed" {
            "nick: Bravo"
        } else {
            "Curated urn:demo:curated-row"
        };
        assert!(t.iter().any(|s| s == expected), "{case}: {t:?}");
    }
}

/// The Properties group's (kind, label or content) pairs, in order.
fn properties(v: &Node) -> Vec<String> {
    let props = v
        .children
        .iter()
        .find(|c| c.properties.get("label") == Some(&Value::Text("Properties".into())))
        .expect("a properties group");
    props
        .children
        .iter()
        .map(|c| {
            let p = c.properties.get("label").or(c.properties.get("content"));
            format!("{} {p:?}", c.kind)
        })
        .collect()
}

#[test]
fn a_shape_reads_the_same_as_turtle_compact_json_ld_other_prefixes_or_expanded() {
    let cap = Capability::scoped([WRITE, CARD]);
    let forms = [
        ("turtle", "text/turtle", TURTLE_SHAPE),
        ("compact", JSON_LD, COMPACT_SHAPE),
        ("prefixed", JSON_LD, PREFIXED_SHAPE),
        ("expanded", JSON_LD, EXPANDED_SHAPE),
    ];
    let views: Vec<(&str, Node)> = forms
        .iter()
        .map(|(id, media, body)| {
            let kernel = shaped(shape_at(id, media, body.to_string()), true);
            (*id, view(&kernel, "urn:demo:a", &cap))
        })
        .collect();
    for (id, v) in &views {
        let t = texts(v);
        assert_eq!(
            properties(v),
            [
                "text Some(Text(\"Name: Alpha\"))",
                "group Some(Text(\"Friend\"))",
                "group Some(Text(\"Card\"))",
            ],
            "{id}: {t:?}"
        );
        assert!(
            !t.iter().any(|s| s.contains("not in the shape")),
            "{id}: {t:?}"
        );
        assert!(
            !t.iter().any(|s| s.contains("could not be read")),
            "{id}: {t:?}"
        );
    }
    let first = serde_json::to_value(&views[0].1).unwrap();
    for (id, v) in &views[1..] {
        assert_eq!(serde_json::to_value(v).unwrap(), first, "{id}");
    }
}

#[test]
fn a_shape_that_cannot_be_read_as_json_ld_is_a_note_not_silence() {
    let cap = Capability::scoped([WRITE, CARD]);
    let cases = [
        // No transreptor reads it as JSON-LD.
        (
            shaped(shape_at("plain", "text/plain", "a shape?".into()), true),
            "nothing here reads as JSON-LD",
        ),
        // JSON-LD that does not compact.
        (
            shaped(
                shape_at("broken", JSON_LD, "{\"@context\": 7}".into()),
                true,
            ),
            "does not flatten",
        ),
        // A host with no JSON-LD compaction (nor a Turtle transreptor).
        (
            shaped(shape_at("ttl", "text/turtle", TURTLE_SHAPE.into()), false),
            "could not be read",
        ),
        // The old informal keys, with no context: `sh:` is not SHACL there.
        (
            shaped(
                shape_at(
                    "informal",
                    JSON_LD,
                    r#"{"sh:property": [{"sh:path": "name", "sh:name": "Name"}]}"#.into(),
                ),
                true,
            ),
            "IRI confused with prefix",
        ),
    ];
    for (kernel, why) in cases {
        let v = view(&kernel, "urn:demo:a", &cap);
        let t = texts(&v);
        let note = t
            .iter()
            .find(|s| s.starts_with("The shape for urn:demo:Thing could not be read: "))
            .unwrap_or_else(|| panic!("no note ({why}): {t:?}"));
        assert!(note.contains(why), "{note}");
        // The view shows as if there were no shape: every property, under its key.
        assert!(t.iter().any(|s| s == "hidden: not in the shape"), "{t:?}");
    }
}

#[test]
fn a_table_of_resources_is_bounded_like_any_other_nested_read() {
    // A list of resources whose type lays out as a table: past the node bound, no row is read.
    const ROWS: usize = 2 * ikigai_ui::view::MAX_NODES;
    let reads = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = reads.clone();
    let row = FnEndpoint::new("row", move |inv: &Invocation<'_>| {
        if inv.request.verb == Verb::Source {
            counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
        let n = inv.bindings.get("n").unwrap_or_default();
        let body = serde_json::json!({"@type": "urn:demo:Row", "n": n});
        Ok(Representation::new(
            ReprType::new("application/json"),
            serde_json::to_vec(&body).unwrap(),
        )
        .cacheable())
    })
    .with_description(
        Description::new("row")
            .title("A row")
            .summary("A row.")
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .output("application/json"),
    );
    let rows: Vec<serde_json::Value> = (0..ROWS)
        .map(|i| serde_json::json!({"@id": format!("urn:demo:row:{i}")}))
        .collect();
    let s = EndpointSpace::new()
        .bind(
            Exact::new("urn:demo:many"),
            json("many", serde_json::json!({"rows": rows}), false),
        )
        .bind(
            Exact::new("urn:demo:few"),
            json("few", serde_json::json!({"rows": &rows[..3]}), false),
        )
        .bind(
            ikigai_core::UriTemplate::parse("urn:demo:row:{n}").unwrap(),
            row,
        )
        .bind(
            Exact::new("urn:iki:ui:shape:type:urn:demo:Row"),
            shape_at(
                "row-shape",
                JSON_LD,
                serde_json::json!({
                    "@context": {"sh": "http://www.w3.org/ns/shacl#"},
                    "@id": "urn:demo:RowShape",
                    "sh:targetClass": {"@id": "urn:demo:Row"},
                    "urn:iki:ui:shape:layout": "table",
                    "sh:property": [{"sh:path": {"@id": "urn:demo:n"}, "sh:name": "N"}]
                })
                .to_string(),
            ),
        );
    let kernel = common::host_with(None, vec![Arc::new(s), json_ld()]);
    let v = view(&kernel, "urn:demo:many", &Capability::root());
    let read = reads.load(std::sync::atomic::Ordering::SeqCst);
    assert!(
        read <= ikigai_ui::view::MAX_NODES,
        "{read} rows read past the node bound of {}",
        ikigai_ui::view::MAX_NODES
    );
    // Past the bound, the rows that were not read are links to their views.
    assert!(!kinds(&v, "link").is_empty());
    // A table that fits is still a table.
    let few = view(&kernel, "urn:demo:few", &Capability::root());
    assert_eq!(kinds(&few, "table").len(), 1);
}
