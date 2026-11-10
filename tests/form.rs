//! A form generated from a model (ledger #3354): from a resource's contract and from an
//! OpenAPI 3.1 document, the same form; only the Sink's inputs; through both authoring forms;
//! malformed and missing models refused; the form cached.

mod common;

use std::sync::Arc;

use futures::executor::block_on;
use ikigai_core::{ArgRef, Capability, Error, Iri, Kernel, Request, Verb};
use ikigai_ui::model::{self, Node, Value};

fn kernel() -> Kernel {
    common::host_with(None, vec![Arc::new(common::form_models())])
}

fn form(kernel: &Kernel, iri: &str) -> Result<Node, Error> {
    let rep = common::source(kernel, iri)?;
    assert_eq!(rep.repr_type.media_type, model::MEDIA_TYPE);
    model::parse(&rep.bytes)
}

fn resource(iri: &str) -> String {
    format!("{}{iri}", ikigai_ui::FORM_RESOURCE)
}

fn openapi(doc: &str, op: &str) -> String {
    format!("{}{doc}:operation:{op}", ikigai_ui::FORM_OPENAPI)
}

fn prop<'a>(n: &'a Node, p: &str) -> Option<&'a Value> {
    n.properties.get(p)
}

fn text(s: &str) -> Value {
    Value::Text(s.to_string())
}

#[test]
fn a_resource_gives_a_form_of_its_sink_inputs() {
    let kernel = kernel();
    let f = form(&kernel, &resource(common::CONTACT)).unwrap();
    assert_eq!(f.kind, "form");
    assert_eq!(prop(&f, "label"), Some(&text("Contact")));
    assert_eq!(prop(&f, "resource"), Some(&text(common::CONTACT)));
    assert_eq!(prop(&f, "action"), Some(&text(common::CONTACT)));
    assert_eq!(prop(&f, "verb"), Some(&text("sink")));
    assert_eq!(prop(&f, "method"), Some(&text("post")));
    let Some(Value::Text(d)) = prop(&f, "description") else {
        panic!("a description")
    };
    assert!(
        d.starts_with("Send us a message. Generated from a model"),
        "{d}"
    );
    // Only the Sink's inputs, in order: never the Source's `q`. Then the submit button.
    let names: Vec<_> = f
        .children
        .iter()
        .filter_map(|c| prop(c, "name").cloned())
        .collect();
    let want = [
        "name",
        "email",
        "message",
        "topic",
        "copies",
        "subscribe",
        "on",
    ];
    assert_eq!(names, want.map(text));
    assert_eq!(f.children.last().unwrap().kind, "button");
    let by = |n: &str| {
        f.children
            .iter()
            .find(|c| prop(c, "name") == Some(&text(n)))
            .unwrap()
    };
    // Required, with help text, typed.
    let name = by("name");
    assert_eq!(name.kind, "field");
    assert_eq!(prop(name, "required"), Some(&Value::Bool(true)));
    assert_eq!(prop(name, "description"), Some(&text("Who is writing.")));
    assert_eq!(prop(name, "input"), Some(&text("text")));
    assert_eq!(prop(by("email"), "input"), Some(&text("url")));
    assert_eq!(prop(by("message"), "input"), Some(&text("textarea")));
    assert_eq!(prop(by("on"), "input"), Some(&text("date")));
    // Optional.
    let subscribe = by("subscribe");
    assert_eq!(prop(subscribe, "required"), Some(&Value::Bool(false)));
    assert_eq!(prop(subscribe, "input"), Some(&text("checkbox")));
    // Defaulted.
    let copies = by("copies");
    assert_eq!(prop(copies, "input"), Some(&text("number")));
    assert_eq!(prop(copies, "value"), Some(&text("1")));
    // Enumerated: a choice of options, each with a value, the default its value.
    let topic = by("topic");
    assert_eq!(topic.kind, "choice");
    assert_eq!(prop(topic, "value"), Some(&text("support")));
    let options: Vec<_> = topic
        .children
        .iter()
        .map(|o| (o.kind.as_str(), prop(o, "value").cloned()))
        .collect();
    assert_eq!(
        options,
        [
            ("option", Some(text("sales"))),
            ("option", Some(text("support")))
        ]
    );
}

#[test]
fn both_authoring_forms_and_the_openapi_document_give_the_same_form() {
    let kernel = kernel();
    let per_verb = form(&kernel, &resource(common::CONTACT)).unwrap();
    let mut flat = form(&kernel, &resource(common::CONTACT_FLAT)).unwrap();
    // The flat resource is a different resource: its resource differs, nothing else.
    for p in ["resource", "action"] {
        flat.properties.insert(p.into(), text(common::CONTACT));
    }
    assert_eq!(per_verb, flat);
    let from_openapi = form(&kernel, &openapi(common::OPENAPI, "contact")).unwrap();
    assert_eq!(
        per_verb, from_openapi,
        "the contract and its OpenAPI give one form"
    );
}

#[test]
fn the_json_ld_face_is_served_too() {
    let kernel = kernel();
    let rep = common::issue(
        &kernel,
        Verb::Source,
        &resource(common::CONTACT),
        &[("as", model::JSON_LD.as_bytes())],
    )
    .unwrap();
    assert_eq!(rep.repr_type.media_type, model::JSON_LD);
}

#[test]
fn a_missing_operation_is_not_found_and_a_malformed_model_is_invalid() {
    let kernel = kernel();
    assert!(matches!(
        form(&kernel, &openapi(common::OPENAPI, "nope")),
        Err(Error::NotFound(_))
    ));
    // The demo header is no OpenAPI document, and the OpenAPI document declares no Sink.
    for iri in [
        openapi("urn:demo:header", "contact"),
        resource(common::OPENAPI),
    ] {
        match form(&kernel, &iri) {
            Err(Error::InvalidArgument { detail, .. }) => {
                assert!(detail.contains("urn:demo:"), "{iri}: names what: {detail}")
            }
            other => panic!("{iri}: {other:?}"),
        }
    }
    for (doc, what) in [
        (r#"{"openapi":"3.0.3","paths":{}}"#, "is not 3.1"),
        (
            r#"{"openapi":"3.1.0","paths":{"/x":{"post":{"operationId":"x","requestBody":{"content":{"application/json":{"schema":{"$ref":"other.json#/X"}}}}}}}}"#,
            "only a local",
        ),
        (
            r#"{"openapi":"3.1.0","paths":{"/x":{"post":{"operationId":"x","requestBody":{"content":{"application/json":{"schema":{"properties":{"a":{"enum":[{}]}}}}}}}}}}"#,
            "not a scalar",
        ),
    ] {
        match ikigai_ui::form::from_openapi("urn:t", doc.as_bytes(), "x") {
            Err(Error::InvalidArgument { detail, .. }) => {
                assert!(detail.contains(what), "{detail}")
            }
            other => panic!("{doc}: {other:?}"),
        }
    }
}

#[test]
fn the_form_is_cached_and_hangs_from_the_contract() {
    let kernel = kernel();
    let iri = resource(common::CONTACT);
    let first = form(&kernel, &iri).unwrap();
    assert_eq!(first, form(&kernel, &iri).unwrap());
    let request = Request::new(Verb::Source, Iri::parse("urn:kernel:uncached").unwrap())
        .with_arg("target", ArgRef::Inline(iri.as_bytes().to_vec()));
    let report = block_on(kernel.issue(request, &Capability::root())).unwrap();
    let report = String::from_utf8(report.bytes).unwrap();
    assert!(report.contains("not computed uncached"), "{report}");
}

#[test]
fn every_name_under_the_kind_prefix_is_sealed() {
    let kernel = kernel();
    for iri in [
        "urn:iki:ui:kind:other",
        "urn:iki:ui:kind:name:field:property:name:label",
        "urn:iki:ui:kind:name:a:b",
        "urn:iki:ui:kind:x:y:z",
    ] {
        assert!(
            matches!(common::source(&kernel, iri), Err(Error::NotFound(_))),
            "{iri} is refused here"
        );
    }
}
