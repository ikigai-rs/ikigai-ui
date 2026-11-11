//! Reference nodes (ledger #3361): a screen names a resource and the face resolves it under the
//! CALLER's capability. A reference the caller may not read is withheld: its fallback text
//! renders and the value never enters the face's bytes or a cache entry the caller can read.
mod common;

use std::sync::Arc;

use futures::executor::block_on;
use ikigai_core::{
    Capability, Description, EndpointSpace, Error, Exact, FnEndpoint, Invocation, Iri, Kernel,
    ReprType, Representation, Request, Verb,
};
use ikigai_ui::{model, reference};

const SECRET: &str = "4111 1111 1111 1111";
const SCOPE: &str = "urn:cap:demo:card:read";
const FACE: &str = "urn:iki:ui:face:target:html:screen:account";

fn model_endpoint(id: &'static str, body: serde_json::Value) -> FnEndpoint {
    let bytes = serde_json::to_vec(&body).unwrap();
    FnEndpoint::new(id, move |_: &Invocation<'_>| {
        Ok(Representation::new(ReprType::new(model::MEDIA_TYPE), bytes.clone()).cacheable())
    })
    .with_description(
        Description::new(id)
            .title(id)
            .summary("A test model.")
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .output(model::MEDIA_TYPE),
    )
}

/// The card number, readable only under [`SCOPE`].
fn card() -> FnEndpoint {
    let bytes = serde_json::to_vec(&serde_json::json!({
        "kind": "text", "properties": {"content": SECRET}
    }))
    .unwrap();
    FnEndpoint::new("demo-card", move |inv: &Invocation<'_>| {
        if !inv.capability.allows(SCOPE) {
            return Err(Error::Denied(format!("{SCOPE} is needed")));
        }
        Ok(Representation::new(ReprType::new(model::MEDIA_TYPE), bytes.clone()).cacheable())
    })
    .with_description(
        Description::new("demo-card")
            .title("A card number")
            .summary("Sensitive.")
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .output(model::MEDIA_TYPE),
    )
}

fn reference_node(iri: &str, fallback: &str) -> serde_json::Value {
    serde_json::json!({
        "kind": "reference",
        "properties": {"resource": iri, "fallback": fallback}
    })
}

fn host(extra: EndpointSpace) -> Kernel {
    let space = extra
        .bind(
            Exact::new("urn:iki:ui:screen:name:account"),
            model_endpoint(
                "account",
                serde_json::json!({
                    "kind": "group",
                    "properties": {"label": "Account"},
                    "children": [
                        {"kind": "heading", "properties": {"content": "Account", "level": 1}},
                        reference_node("urn:demo:card", "Card number withheld")
                    ]
                }),
            ),
        )
        .bind(Exact::new("urn:demo:card"), card());
    common::host_with(None, vec![Arc::new(space)])
}

fn face(kernel: &Kernel, capability: &Capability) -> Result<Representation, Error> {
    let request = Request::new(Verb::Source, Iri::parse(FACE).unwrap());
    block_on(kernel.issue(request, capability))
}

#[test]
fn a_reference_is_visible_under_the_grant_and_withheld_without_it() {
    let kernel = host(EndpointSpace::new());
    let wide = Capability::scoped([SCOPE]);
    let narrow = Capability::scoped(["urn:cap:demo:other"]);

    let shown = face(&kernel, &wide).unwrap();
    let shown = String::from_utf8(shown.bytes).unwrap();
    assert!(shown.contains(SECRET), "{shown}");
    assert!(!shown.contains("withheld"), "{shown}");

    // Under the narrow grant, after the wide one filled whatever cache it fills.
    let withheld = face(&kernel, &narrow).unwrap();
    let node = model::parse(&withheld.bytes).unwrap();
    assert_eq!(node.children[1].kind, "text");
    assert_eq!(
        node.children[1].properties["content"],
        model::Value::Text("Card number withheld".into())
    );
    assert!(!String::from_utf8_lossy(&withheld.bytes).contains(SECRET));
    for scope in [&narrow, &Capability::scoped(Vec::<String>::new())] {
        let again = face(&kernel, scope).unwrap();
        assert!(!String::from_utf8_lossy(&again.bytes).contains(SECRET));
    }

    // No cache entry readable under the narrow grant holds the value: the face that swallowed
    // the denial is not cached, and neither is the card under that grant.
    let request = Request::new(Verb::Source, Iri::parse(FACE).unwrap());
    assert!(!kernel.is_cached(&request, &narrow));
    let card = Request::new(Verb::Source, Iri::parse("urn:demo:card").unwrap());
    assert!(!kernel.is_cached(&card, &narrow));
    // The wide grant's answer is keyed on its own fingerprint.
    assert!(kernel.is_cached(&request, &wide));
}

#[test]
fn the_screen_holds_the_name_never_the_value() {
    let kernel = host(EndpointSpace::new());
    let rep = common::source(&kernel, "urn:iki:ui:screen:name:account").unwrap();
    let text = String::from_utf8(rep.bytes).unwrap();
    assert!(
        text.contains("urn:demo:card") && !text.contains(SECRET),
        "{text}"
    );
    model::parse(text.as_bytes()).unwrap();
}

#[test]
fn references_are_validated() {
    let bad = [
        serde_json::json!({"kind": "reference", "properties": {"resource": "urn:demo:card"}}),
        serde_json::json!({"kind": "reference", "properties": {"fallback": "x"}}),
        serde_json::json!({"kind": "reference", "properties": {"resource": "not an iri", "fallback": "x"}}),
        serde_json::json!({"kind": "reference", "properties": {"resource": "urn:a", "fallback": "x", "label": "y"}}),
        serde_json::json!({"kind": "reference", "properties": {"resource": "urn:a", "fallback": "x"},
            "children": [{"kind": "text", "properties": {"content": "c"}}]}),
    ];
    for node in bad {
        let bytes = serde_json::to_vec(&node).unwrap();
        assert!(
            matches!(model::parse(&bytes), Err(Error::InvalidArgument { .. })),
            "{node}"
        );
    }
}

#[test]
fn nested_references_resolve_and_are_bounded() {
    // urn:demo:chain:{n} references urn:demo:chain:{n+1}; the last is text.
    let mut extra = EndpointSpace::new();
    let links = reference::MAX_DEPTH + 2;
    for n in 0..links {
        let iri = format!("urn:demo:chain:{n}");
        let body = if n + 1 == links {
            serde_json::json!({"kind": "text", "properties": {"content": "end"}})
        } else {
            reference_node(&format!("urn:demo:chain:{}", n + 1), "link")
        };
        extra = extra.bind(Exact::new(iri), model_endpoint("chain", body));
    }
    let screen = |at: usize| {
        model_endpoint(
            "chain-screen",
            reference_node(&format!("urn:demo:chain:{at}"), "chain"),
        )
    };
    extra = extra
        .bind(Exact::new("urn:iki:ui:screen:name:deep"), screen(0))
        .bind(
            Exact::new("urn:iki:ui:screen:name:shallow"),
            screen(links - 2),
        );
    let kernel = host(extra);
    let shallow = common::source(&kernel, "urn:iki:ui:face:target:html:screen:shallow").unwrap();
    let node = model::parse(&shallow.bytes).unwrap();
    assert_eq!(node.properties["content"], model::Value::Text("end".into()));
    let deep = common::source(&kernel, "urn:iki:ui:face:target:html:screen:deep").unwrap_err();
    assert!(matches!(deep, Error::InvalidArgument { .. }), "{deep}");

    // Many references in one screen are bounded too.
    let many: Vec<_> = (0..=reference::MAX_REFERENCES)
        .map(|_| reference_node("urn:demo:leaf", "x"))
        .collect();
    let leaf = serde_json::json!({"kind": "text", "properties": {"content": "leaf"}});
    let extra = EndpointSpace::new().bind(Exact::new("urn:demo:leaf"), model_endpoint("leaf", leaf)).bind(
        Exact::new("urn:iki:ui:screen:name:many"),
        model_endpoint(
            "many",
            serde_json::json!({"kind": "list", "properties": {"label": "Many"}, "children": many}),
        ),
    );
    let kernel = host(extra);
    let err = common::source(&kernel, "urn:iki:ui:face:target:html:screen:many").unwrap_err();
    assert!(matches!(err, Error::InvalidArgument { .. }), "{err}");
}
