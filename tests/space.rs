//! A screen is a space (ledger #3361): settings answered by the space a screen runs in, regions
//! that narrow what their children reach, and write-only fields.
mod common;

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use futures::executor::block_on;
use ikigai_core::{
    Capability, Description, EndpointSpace, Error, Exact, FnEndpoint, Invocation, Iri, Kernel,
    ReprType, Representation, Request, Scope, Verb,
};
use ikigai_ui::{model, setting};

const SECRET: &str = "4111 1111 1111 1111";
const SCOPE: &str = "urn:cap:demo:card:read";

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

/// The card number, readable only under [`SCOPE`]; `reached` counts every entry.
fn card(reached: Arc<AtomicU32>) -> FnEndpoint {
    let bytes = serde_json::to_vec(&serde_json::json!({
        "kind": "text", "properties": {"content": SECRET}
    }))
    .unwrap();
    FnEndpoint::new("demo-card", move |inv: &Invocation<'_>| {
        reached.fetch_add(1, Ordering::SeqCst);
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

fn reference(iri: &str, fallback: &str) -> serde_json::Value {
    serde_json::json!({"kind": "reference", "properties": {"resource": iri, "fallback": fallback}})
}

/// `urn:demo:ui:element:name:peek`: an element whose expansion references the card.
fn peek() -> FnEndpoint {
    let bytes = serde_json::to_vec(&serde_json::json!({
        "arguments": [],
        "expansion": {"kind": "group", "properties": {"label": "Peek"}, "children": [
            reference("urn:demo:card", "Peek withheld")
        ]}
    }))
    .unwrap();
    FnEndpoint::new("demo-peek", move |_: &Invocation<'_>| {
        Ok(Representation::new(ReprType::new("application/json"), bytes.clone()).cacheable())
    })
    .with_description(
        Description::new("demo-peek")
            .title("A peeking element")
            .summary("An element whose expansion reads the card.")
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .output("application/json"),
    )
}

const PEEK: &str = "urn:demo:ui:element:name:peek";

fn host(screens: Vec<(&'static str, serde_json::Value)>, reached: Arc<AtomicU32>) -> Kernel {
    let mut space = EndpointSpace::new()
        .bind(Exact::new("urn:demo:card"), card(reached))
        .bind(Exact::new(PEEK), peek())
        .bind(
            Exact::new("urn:demo:greeting"),
            model_endpoint(
                "greeting",
                serde_json::json!({"kind": "text", "properties": {"content": "Hello"}}),
            ),
        );
    for (name, body) in screens {
        space = space.bind(
            Exact::new(format!("urn:iki:ui:screen:name:{name}")),
            model_endpoint("screen", body),
        );
    }
    common::host_with(None, vec![Arc::new(space)])
}

fn face(kernel: &Kernel, iri: &str, capability: &Capability) -> Result<Representation, Error> {
    block_on(kernel.issue(
        Request::new(Verb::Source, Iri::parse(iri).unwrap()),
        capability,
    ))
}

fn plain_screen() -> serde_json::Value {
    serde_json::json!({"kind": "group", "properties": {"label": "Plain"}, "children": [
        {"kind": "heading", "properties": {"content": "Plain", "level": 1}},
        {"kind": "table", "properties": {"caption": "T"}, "children": [
            {"kind": "group", "properties": {"label": "r"}, "children": [
                {"kind": "text", "properties": {"content": "cell"}}
            ]}
        ]}
    ]})
}

// --- settings ----------------------------------------------------------------------------

#[test]
fn settings_default_in_the_root_and_a_screen_space_answers_ahead_of_it() {
    let kernel = host(vec![("plain", plain_screen())], Arc::default());
    for (name, default) in setting::DEFAULTS {
        let rep = common::source(&kernel, &format!("{}{name}", setting::PREFIX)).unwrap();
        assert_eq!(rep.bytes, default.as_bytes(), "{name}");
    }
    assert!(matches!(
        common::source(&kernel, "urn:iki:ui:setting:name:colour"),
        Err(Error::NotFound(_))
    ));
    let scope = setting::scope("urn:demo:space:screen:dark", &[("theme", "dark")]).unwrap();
    let theme = Request::new(
        Verb::Source,
        Iri::parse("urn:iki:ui:setting:name:theme").unwrap(),
    );
    let rep = block_on(kernel.issue_in(theme, &Capability::root(), scope)).unwrap();
    assert_eq!(rep.bytes, b"dark");

    // A screen space is checked when the host builds it.
    assert!(setting::screen_space("urn:demo:s", &[("colour", "red")]).is_err());
    assert!(setting::screen_space("urn:demo:s", &[("theme", "a"), ("theme", "b")]).is_err());
    assert!(setting::screen_space("urn:demo:s", &[("theme", "")]).is_err());
    assert!(setting::screen_space("not an iri", &[]).is_err());
}

#[test]
fn the_screen_face_reads_its_target_from_the_screen_space_and_each_space_caches_apart() {
    let kernel = host(vec![("plain", plain_screen())], Arc::default());
    let root = Capability::root();
    let request = || {
        Request::new(
            Verb::Source,
            Iri::parse("urn:iki:ui:face:screen:plain").unwrap(),
        )
    };
    let html = setting::scope("urn:demo:space:screen:html", &[("target", "html")]).unwrap();
    let gpui = setting::scope("urn:demo:space:screen:gpui", &[("target", "gpui")]).unwrap();

    let in_html = block_on(kernel.issue_in(request(), &root, html.clone())).unwrap();
    let in_gpui = block_on(kernel.issue_in(request(), &root, gpui.clone())).unwrap();
    // Each is the face its target's name gives.
    let by_name = |t: &str| {
        common::source(&kernel, &format!("urn:iki:ui:face:target:{t}:screen:plain"))
            .unwrap()
            .bytes
    };
    assert_eq!(in_html.bytes, by_name("html"));
    assert_eq!(in_gpui.bytes, by_name("gpui"));
    assert_ne!(
        in_html.bytes, in_gpui.bytes,
        "the two targets project apart"
    );

    // Two screen spaces, two cache entries, neither served to the other or to the root.
    assert!(kernel.is_cached_in(&request(), &root, &html));
    assert!(kernel.is_cached_in(&request(), &root, &gpui));
    let again = block_on(kernel.issue_in(request(), &root, gpui.clone())).unwrap();
    assert_eq!(again.bytes, in_gpui.bytes);
    // With no screen space, the root's default target (html) answers, under its own key.
    let plain = block_on(kernel.issue(request(), &root)).unwrap();
    assert_eq!(plain.bytes, in_html.bytes);
    assert!(kernel.is_cached_in(&request(), &root, &Scope::empty()));
    // Each entry stays keyed on the capability's fingerprint too.
    let other = Capability::scoped(["urn:cap:demo:other"]);
    assert!(!kernel.is_cached_in(&request(), &other, &gpui));
}

#[test]
fn a_screen_space_naming_no_target_is_refused() {
    let kernel = host(vec![("plain", plain_screen())], Arc::default());
    let scope = setting::scope("urn:demo:space:screen:svg", &[("target", "svg")]).unwrap();
    let request = Request::new(
        Verb::Source,
        Iri::parse("urn:iki:ui:face:screen:plain").unwrap(),
    );
    let err = block_on(kernel.issue_in(request, &Capability::root(), scope)).unwrap_err();
    assert!(matches!(err, Error::NotFound(_)), "{err}");
}

// --- regions -----------------------------------------------------------------------------

fn region(
    allow: &str,
    scopes: Option<&str>,
    children: Vec<serde_json::Value>,
) -> serde_json::Value {
    let mut properties = serde_json::json!({"label": "Payment"});
    if !allow.is_empty() {
        properties["allow"] = allow.into();
    }
    if let Some(s) = scopes {
        properties["scopes"] = s.into();
    }
    serde_json::json!({"kind": "region", "properties": properties, "children": children})
}

fn screen(children: Vec<serde_json::Value>) -> serde_json::Value {
    serde_json::json!({"kind": "group", "properties": {"label": "Account"}, "children": children})
}

fn element(kind: &str) -> serde_json::Value {
    serde_json::json!({"kind": kind, "properties": {"fallback": "A peek"}})
}

#[test]
fn a_custom_element_inside_a_region_is_refused_when_it_reads_outside_it() {
    let reached = Arc::new(AtomicU32::new(0));
    let kernel = host(
        vec![
            // The element outside every region: it reads the card under the caller's grant.
            ("open", screen(vec![element(PEEK)])),
            // The same element inside a region that allows its definition but not the card.
            (
                "narrow",
                screen(vec![region(PEEK, None, vec![element(PEEK)])]),
            ),
            // A region that allows the card too.
            (
                "allowed",
                screen(vec![region(
                    &format!("{PEEK} urn:demo:card"),
                    None,
                    vec![element(PEEK)],
                )]),
            ),
        ],
        reached.clone(),
    );
    let wide = Capability::scoped([SCOPE]);

    let open = face(&kernel, "urn:iki:ui:face:target:html:screen:open", &wide).unwrap();
    assert!(String::from_utf8_lossy(&open.bytes).contains(SECRET));

    let before = reached.load(Ordering::SeqCst);
    // Even under ROOT authority: the card is not in the region's space, so it is unresolvable
    // there, and the card's endpoint is never entered.
    for capability in [&wide, &Capability::root()] {
        let err = face(
            &kernel,
            "urn:iki:ui:face:target:html:screen:narrow",
            capability,
        )
        .unwrap_err();
        assert!(
            matches!(&err, Error::Unresolved(t) if t.as_str() == "urn:demo:card"),
            "{err}"
        );
        // The html face composes the same way.
        let err = face(&kernel, "urn:iki:ui:html:screen:narrow", capability).unwrap_err();
        assert!(matches!(err, Error::Unresolved(_)), "{err}");
    }
    assert_eq!(
        reached.load(Ordering::SeqCst),
        before,
        "the card was never entered"
    );

    // Allowed, it resolves under the caller's grant, and the region becomes its group.
    let allowed = face(&kernel, "urn:iki:ui:face:target:html:screen:allowed", &wide).unwrap();
    let node = model::parse(&allowed.bytes).unwrap();
    assert_eq!(node.children[0].kind, "group");
    assert_eq!(
        node.children[0].properties["label"],
        model::Value::Text("Payment".into())
    );
    assert!(String::from_utf8_lossy(&allowed.bytes).contains(SECRET));
}

#[test]
fn an_element_definition_outside_a_region_is_unresolved_inside_it() {
    let kernel = host(
        vec![(
            "bare",
            screen(vec![region("urn:demo:greeting", None, vec![element(PEEK)])]),
        )],
        Arc::default(),
    );
    let err = face(
        &kernel,
        "urn:iki:ui:face:target:html:screen:bare",
        &Capability::root(),
    )
    .unwrap_err();
    assert!(
        matches!(&err, Error::Unresolved(t) if t.as_str() == PEEK),
        "{err}"
    );
}

#[test]
fn a_region_reads_under_its_attenuated_scopes_and_withholds_what_they_do_not_grant() {
    let kernel = host(
        vec![
            (
                "scoped",
                screen(vec![region(
                    "urn:demo:card urn:demo:greeting",
                    Some("urn:cap:demo:greeting:read"),
                    vec![
                        reference("urn:demo:greeting", "No greeting"),
                        reference("urn:demo:card", "Card number withheld"),
                    ],
                )]),
            ),
            (
                "granted",
                screen(vec![region(
                    "urn:demo:card",
                    Some(SCOPE),
                    vec![reference("urn:demo:card", "Card number withheld")],
                )]),
            ),
        ],
        Arc::default(),
    );
    // The caller holds the card's scope, but the region names only the greeting's: inside it
    // the card is withheld, and nothing of it is in the bytes.
    let wide = Capability::scoped([SCOPE, "urn:cap:demo:greeting:read"]);
    let scoped = face(&kernel, "urn:iki:ui:face:target:html:screen:scoped", &wide).unwrap();
    let text = String::from_utf8_lossy(&scoped.bytes);
    assert!(
        text.contains("Hello") && text.contains("Card number withheld"),
        "{text}"
    );
    assert!(!text.contains(SECRET), "{text}");
    let request = Request::new(
        Verb::Source,
        Iri::parse("urn:iki:ui:face:target:html:screen:scoped").unwrap(),
    );
    assert!(
        !kernel.is_cached(&request, &wide),
        "a face that withheld is not cached"
    );

    // A region whose scopes the caller holds reads under them.
    let granted = face(&kernel, "urn:iki:ui:face:target:html:screen:granted", &wide).unwrap();
    assert!(String::from_utf8_lossy(&granted.bytes).contains(SECRET));
    // Asking for a scope the caller lacks does not bring it.
    let narrow = Capability::scoped(["urn:cap:demo:other"]);
    let none = face(
        &kernel,
        "urn:iki:ui:face:target:html:screen:granted",
        &narrow,
    )
    .unwrap();
    assert!(!String::from_utf8_lossy(&none.bytes).contains(SECRET));
}

#[test]
fn a_region_inherits_the_screen_settings_and_nests_bounded() {
    // A screen that references the target setting's model would be odd; read the setting
    // through a reference-free route instead: a region allowing nothing still sees the
    // settings, because the face reads them for it.
    let mut inner = region(
        "",
        None,
        vec![serde_json::json!({"kind": "text", "properties": {"content": "deep"}})],
    );
    for _ in 0..ikigai_ui::region::MAX_DEPTH {
        inner = region("", None, vec![inner]);
    }
    let kernel = host(
        vec![
            (
                "one",
                screen(vec![region(
                    "",
                    None,
                    vec![serde_json::json!({"kind": "text", "properties": {"content": "inside"}})],
                )]),
            ),
            ("deep", screen(vec![inner])),
        ],
        Arc::default(),
    );
    let one = face(
        &kernel,
        "urn:iki:ui:face:target:html:screen:one",
        &Capability::root(),
    )
    .unwrap();
    assert!(String::from_utf8_lossy(&one.bytes).contains("inside"));
    // Composed twice, the same region is the same narrowed space: cached.
    let request = Request::new(
        Verb::Source,
        Iri::parse("urn:iki:ui:face:target:html:screen:one").unwrap(),
    );
    assert!(kernel.is_cached(&request, &Capability::root()));
    let err = face(
        &kernel,
        "urn:iki:ui:face:target:html:screen:deep",
        &Capability::root(),
    )
    .unwrap_err();
    assert!(matches!(err, Error::InvalidArgument { .. }), "{err}");
}

#[test]
fn regions_are_validated() {
    let bad = [
        serde_json::json!({"kind": "region", "properties": {"allow": "urn:a"}}),
        serde_json::json!({"kind": "region", "properties": {"label": "R", "allow": "not-an-iri"}}),
        serde_json::json!({"kind": "region", "properties": {"label": "R", "scopes": " "}}),
        serde_json::json!({"kind": "region", "properties": {"label": "R", "content": "x"}}),
        serde_json::json!({"kind": "region", "properties": {"label": "R"},
            "replacement": {"kind": "text", "properties": {"content": "x"}}}),
        serde_json::json!({"kind": "region", "properties": {"label": "R", "allow":
            (0..=ikigai_ui::region::MAX_ALLOW).map(|i| format!("urn:a:{i}")).collect::<Vec<_>>().join(" ")}}),
    ];
    for node in bad {
        let bytes = serde_json::to_vec(&node).unwrap();
        assert!(
            matches!(model::parse(&bytes), Err(Error::InvalidArgument { .. })),
            "{node}"
        );
    }
}

// --- write-only fields -------------------------------------------------------------------

#[test]
fn a_write_only_field_sinks_to_its_own_resource_and_holds_no_value() {
    let field = serde_json::json!({"kind": "field", "properties": {
        "label": "Card number", "name": "card", "resource": "urn:demo:card"
    }});
    let kernel = host(
        vec![
            ("pay", screen(vec![field.clone()])),
            (
                "nested",
                screen(vec![
                    serde_json::json!({"kind": "form", "properties": {"label": "F"}, "children": [field.clone()]}),
                ]),
            ),
        ],
        Arc::default(),
    );
    let wide = Capability::scoped([SCOPE]);
    let rep = face(&kernel, "urn:iki:ui:face:target:html:screen:pay", &wide).unwrap();
    let node = model::parse(&rep.bytes).unwrap();
    let form = &node.children[0];
    assert_eq!(form.kind, "form");
    assert_eq!(
        form.properties["resource"],
        model::Value::Text("urn:demo:card".into())
    );
    assert_eq!(form.properties["verb"], model::Value::Text("sink".into()));
    assert_eq!(form.children[0].kind, "field");
    assert_eq!(
        form.children[0].properties["input"],
        model::Value::Text("password".into())
    );
    assert!(!form.children[0].properties.contains_key("value"));
    assert_eq!(form.children[1].kind, "button");
    // Its resource is never read: the card's number is not in the screen, even under a grant
    // that may read it.
    assert!(!String::from_utf8_lossy(&rep.bytes).contains(SECRET));

    // In html: its own form, posting to the resource through the submit rule, no value.
    let html = face(&kernel, "urn:iki:ui:html:screen:pay", &wide).unwrap();
    let html = String::from_utf8(html.bytes).unwrap();
    assert!(
        html.contains("action=\"/resolution/iri/urn%3Ademo%3Acard\""),
        "{html}"
    );
    assert!(html.contains("type=\"password\""), "{html}");
    assert!(html.contains("value=\"\""), "{html}");
    assert!(!html.contains(SECRET), "{html}");

    // One inside another form is refused, and one holding a value is not a model.
    let err = face(&kernel, "urn:iki:ui:face:target:html:screen:nested", &wide).unwrap_err();
    assert!(matches!(err, Error::InvalidArgument { .. }), "{err}");
    let mut valued = field;
    valued["properties"]["value"] = "4111".into();
    assert!(model::parse(&serde_json::to_vec(&valued).unwrap()).is_err());
}

// --- structural shapes and the context ----------------------------------------------------

#[test]
fn the_structural_shapes_and_examples_are_served_and_the_context_names_them() {
    let kernel = host(Vec::new(), Arc::default());
    for structure in &model::STRUCTURES {
        let shape = common::source(
            &kernel,
            &format!("{}{}", model::STRUCTURE_PREFIX, structure.name),
        )
        .unwrap();
        assert_eq!(shape.bytes, structure.shape.as_bytes());
        let example = common::source(
            &kernel,
            &format!("urn:iki:ui:example:structure:{}", structure.name),
        )
        .unwrap();
        let node = model::parse(&example.bytes).unwrap();
        assert_eq!(node.kind, structure.name);
    }
    assert!(matches!(
        common::source(&kernel, "urn:iki:ui:structure:name:script"),
        Err(Error::NotFound(_))
    ));
    let context: serde_json::Value = serde_json::from_str(model::CONTEXT).unwrap();
    assert_eq!(
        context["@context"]["reference"],
        "urn:iki:ui:structure:name:reference"
    );
    assert_eq!(
        context["@context"]["region"],
        "urn:iki:ui:structure:name:region"
    );
    assert_eq!(
        context["@context"]["resource"],
        "urn:iki:ui:property:name:resource"
    );
    assert_eq!(
        context["@context"]["fallback"],
        "urn:iki:ui:property:name:fallback"
    );
}

#[test]
fn an_outline_writes_a_reference_and_a_region() {
    let source = "group label=Account\n  reference resource=urn:demo:card fallback=\"Card withheld\"\n  region label=Payment allow=urn:demo:greeting\n    text content=Inside\n";
    let request = Request::new(
        Verb::Source,
        Iri::parse(ikigai_ui::screen::COMPILER).unwrap(),
    )
    .with_arg(
        "content",
        ikigai_core::ArgRef::Inline(source.as_bytes().to_vec()),
    );
    let kernel = host(Vec::new(), Arc::default());
    let rep = block_on(kernel.issue(request, &Capability::root())).unwrap();
    let node = model::parse(&rep.bytes).unwrap();
    assert_eq!(node.children[0].kind, "reference");
    assert_eq!(node.children[1].kind, "region");
    assert_eq!(node.children[1].children[0].kind, "text");
}
