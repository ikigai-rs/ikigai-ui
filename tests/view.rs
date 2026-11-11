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
        )
        .bind(
            Exact::new("urn:iki:ui:shape:type:urn:demo:Thing"),
            fixed(
                "thing-shape",
                "application/ld+json",
                serde_json::json!({
                    "rdfs:label": "Thing",
                    "sh:property": [
                        {"sh:path": "friend", "sh:name": "Friend", "sh:order": 2},
                        {"sh:path": "name", "sh:name": "Name", "sh:order": 1},
                        {"sh:path": "card", "sh:name": "Card", "sh:order": 3}
                    ]
                }),
            ),
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
    let mut s = space();
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
    common::host_with(None, vec![Arc::new(s)])
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
