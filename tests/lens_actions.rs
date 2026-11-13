//! Gestures and the operations lens use the caller's manifold and ordinary model nodes.
mod common;

use ikigai_core::{ArgRef, Capability, Iri, Request, Verb};
use ikigai_ui::model::Node;

#[test]
fn operations_is_an_ordinary_view_even_before_any_values_are_dropped() {
    let kernel = common::host(None);
    let rep = futures::executor::block_on(kernel.issue(
        Request::new(
            Verb::Source,
            Iri::parse("urn:iki:ui:view:lens:operations:resource:urn:viewer:drop").unwrap(),
        ),
        &Capability::root(),
    ))
    .expect("the operations lens exists");
    let model: Node = serde_json::from_slice(&rep.bytes).unwrap();
    ikigai_ui::model::validate(&model).unwrap();
    assert!(serde_json::to_string(&model)
        .unwrap()
        .contains("Drop resources or literals"));
}

#[test]
fn lens_gestures_have_generated_sink_forms() {
    let kernel = common::host(None);
    let rep = futures::executor::block_on(
        kernel.issue(
            Request::new(
                Verb::Source,
                Iri::parse("urn:iki:ui:form:resource:urn:iki:ui:lens:name:graph:action:connect")
                    .unwrap(),
            )
            .with_arg("as", ArgRef::Inline(b"application/json".to_vec())),
            &Capability::root(),
        ),
    )
    .expect("a lens gesture has a Sink contract");
    let model: Node = serde_json::from_slice(&rep.bytes).unwrap();
    assert_eq!(model.kind, "form");
}

use common::lens_actions::*;
use ikigai_core::Error;
use ikigai_ui::lens::{self, actions};
use std::collections::BTreeMap;
#[test]
fn gestures_are_offered_only_for_the_bound_graph_and_selection_links_home() {
    let kernel = vocabulary(None);
    let read = Capability::scoped([format!("urn:cap:store:read:graph:{GRAPH}")]);
    let wrong = Capability::scoped([
        format!("urn:cap:store:read:graph:{GRAPH}"),
        "urn:cap:store:write:graph:urn:other".into(),
    ]);
    for cap in [&read, &wrong] {
        let view = vocabulary_view(&kernel, cap);
        assert!(!nodes(&view, "form")
            .iter()
            .any(|f| string(f, "resource").contains(":action:")));
        assert!(issue(
            &kernel,
            cap,
            Verb::Sink,
            &action("connect"),
            &[
                ("selected", "urn:test:Parent"),
                ("target", "urn:other:Term")
            ]
        )
        .is_err());
    }
    let view = vocabulary_view(&kernel, &writer());
    let forms: Vec<_> = nodes(&view, "form")
        .into_iter()
        .filter(|f| string(f, "resource").contains(":action:"))
        .collect();
    assert_eq!(forms.len(), 2);
    for form in forms {
        assert!(nodes(form, "field")
            .iter()
            .any(|f| string(f, "name") == "selected" && string(f, "value") == "urn:test:Parent"));
    }
    assert!(nodes(&view, "link")
        .iter()
        .any(|l| string(l, "href") == "/terms/Parent"));
}

#[test]
fn extending_and_connecting_write_the_right_direction_and_refresh_lenses() {
    let kernel = vocabulary(None);
    let before = vocabulary_view(&kernel, &writer());
    assert!(!serde_json::to_string(&before)
        .unwrap()
        .contains("urn:test:Child"));
    issue(
        &kernel,
        &writer(),
        Verb::Sink,
        &action("extend"),
        &[
            ("selected", "urn:test:Parent"),
            ("target", "urn:test:Child"),
        ],
    )
    .unwrap();
    assert!(ask(
        &kernel,
        &format!("<urn:test:Child> <{SUBCLASS}> <urn:test:Parent>")
    ));
    assert!(!ask(
        &kernel,
        &format!("<urn:test:Parent> <{SUBCLASS}> <urn:test:Child>")
    ));
    let after = vocabulary_view(&kernel, &writer());
    assert!(
        serde_json::to_string(&after)
            .unwrap()
            .contains("urn:test:Child"),
        "the store's write cuts the lens result's thread: {after:?}"
    );
    for relation in [
        SUBCLASS,
        "http://www.w3.org/2002/07/owl#equivalentClass",
        "http://www.w3.org/2004/02/skos/core#exactMatch",
        "http://www.w3.org/2004/02/skos/core#closeMatch",
    ] {
        issue(
            &kernel,
            &writer(),
            Verb::Sink,
            &action("connect"),
            &[
                ("selected", "urn:test:Parent"),
                ("target", "urn:other:Term"),
                ("relation", relation),
            ],
        )
        .unwrap();
        assert!(ask(
            &kernel,
            &format!("<urn:test:Parent> <{relation}> <urn:other:Term>")
        ));
    }
}

#[test]
fn shape_refusals_and_injection_never_write() {
    let shapes = format!("@prefix sh: <http://www.w3.org/ns/shacl#> . <urn:shape> a sh:NodeShape; sh:targetClass <{CLASS}>; sh:property [sh:path <{SUBCLASS}>; sh:maxCount 0] .");
    let kernel = vocabulary(Some(&shapes));
    let e = issue(
        &kernel,
        &writer(),
        Verb::Sink,
        &action("extend"),
        &[
            ("selected", "urn:test:Parent"),
            ("target", "urn:test:Child"),
        ],
    )
    .unwrap_err();
    assert!(
        matches!(e,Error::InvalidArgument {ref name,..} if name == "target"),
        "{e}"
    );
    assert!(!ask(&kernel, "<urn:test:Child> ?p ?o"));
    for args in [
        vec![
            ("selected", "urn:test:Parent"),
            ("target", "urn:test:Child> . } }; DROP ALL; #"),
        ],
        vec![
            ("selected", "urn:test:Parent"),
            ("target", "urn:test:Child"),
            ("relation", "urn:evil"),
        ],
        vec![
            ("selected", "urn:test:Parent"),
            ("target", "urn:test:Child"),
            ("graph", "urn:evil"),
        ],
    ] {
        assert!(issue(&kernel, &writer(), Verb::Sink, &action("connect"), &args).is_err());
    }
    assert!(!ask(&kernel, "<urn:test:Child> ?p ?o"));
    let valid = vocabulary(Some(&format!("@prefix sh: <http://www.w3.org/ns/shacl#> . <urn:shape> a sh:NodeShape; sh:targetClass <{CLASS}>; sh:property [sh:path <{SUBCLASS}>; sh:maxCount 1] .")));
    issue(
        &valid,
        &writer(),
        Verb::Sink,
        &action("extend"),
        &[
            ("selected", "urn:test:Parent"),
            ("target", "urn:test:Child"),
        ],
    )
    .unwrap();
    assert!(ask(
        &valid,
        &format!("<urn:test:Child> <{SUBCLASS}> <urn:test:Parent>")
    ));
}

#[test]
fn operations_are_capability_scoped_prefilled_and_personal() {
    let (kernel, calls) = operations_host();
    let no_grant = Capability::scoped(std::iter::empty::<&str>());
    let cap = Capability::scoped([INVOKE]);
    assert!(
        !nodes(&operations(&kernel, &no_grant, &dropped("2")), "form")
            .iter()
            .any(|f| string(f, "resource").ends_with(CALL))
    );
    let n = operations(&kernel, &cap, &dropped("2"));
    let f = nodes(&n, "form")
        .into_iter()
        .find(|f| string(f, "resource").ends_with(CALL))
        .unwrap();
    assert_eq!(
        string(f, "resource"),
        format!("{}{CALL}", actions::OPERATION)
    );
    for (name, value) in [("person", "urn:test:Alice"), ("times", "2")] {
        assert!(nodes(f, "field")
            .iter()
            .any(|f| string(f, "name") == name && string(f, "value") == value));
    }
    assert!(
        !serde_json::to_string(&operations(&kernel, &cap, "[]"))
            .unwrap()
            .contains("urn:test:Alice"),
        "a subsequent viewer has no shared dropped state"
    );
    let hidden = operations(&kernel, &cap, r#"[{"iri":"urn:test:Hidden"}]"#);
    assert!(serde_json::to_string(&hidden).unwrap().contains("withheld"));
    assert!(
        calls.lock().unwrap().is_empty(),
        "dropping never invokes anything"
    );
}
#[test]
fn every_operation_submission_is_validated_before_invoke() {
    let (kernel, calls) = operations_host();
    let cap = Capability::scoped([INVOKE]);
    let wrapper = format!("{}{CALL}", actions::OPERATION);
    assert!(issue(
        &kernel,
        &cap,
        Verb::Sink,
        &wrapper,
        &[("person", "urn:test:Alice"), ("times", "not an integer")]
    )
    .is_err());
    assert!(issue(
        &kernel,
        &cap,
        Verb::Sink,
        &wrapper,
        &[("person", "urn:test:Alice")]
    )
    .is_err());
    assert!(calls.lock().unwrap().is_empty());
    issue(
        &kernel,
        &cap,
        Verb::Sink,
        &wrapper,
        &[("person", "urn:test:Alice"), ("times", "3")],
    )
    .unwrap();
    assert_eq!(calls.lock().unwrap()[0]["times"], "3");
    let denied = Capability::scoped(std::iter::empty::<&str>());
    assert!(issue(
        &kernel,
        &denied,
        Verb::Sink,
        &wrapper,
        &[("person", "urn:test:Alice"), ("times", "3")]
    )
    .is_err());
    assert_eq!(calls.lock().unwrap().len(), 1);
}
#[test]
fn operations_work_through_encoded_native_and_html_screen_routes() {
    let (kernel, _) = operations_host();
    let chosen = BTreeMap::from([("dropped".into(), dropped("4"))]);
    let screen = lens::screen_name("operations", "urn:viewer:personal", &chosen);
    let rep = issue(
        &kernel,
        &Capability::scoped([INVOKE]),
        Verb::Source,
        &format!("urn:iki:ui:screen:name:{screen}"),
        &[],
    )
    .unwrap();
    assert!(String::from_utf8(rep.bytes)
        .unwrap()
        .contains("urn:test:Alice"));
    let html = issue(
        &kernel,
        &Capability::scoped([INVOKE]),
        Verb::Source,
        &format!("urn:iki:ui:html:screen:{screen}"),
        &[],
    )
    .unwrap();
    let html = String::from_utf8(html.bytes).unwrap();
    assert!(html.contains("Repeat a greeting"));
    assert!(html.contains("urn:test:Alice"));
}

#[test]
fn vocabulary_can_open_before_a_selection_and_extend_requires_a_class() {
    let kernel = vocabulary(None);
    let n = model(
        issue(
            &kernel,
            &writer(),
            Verb::Source,
            &format!("{}vocabulary:resource:{GRAPH}", lens::VIEW),
            &[],
        )
        .unwrap(),
    );
    ikigai_ui::model::validate(&n).unwrap();
    assert!(nodes(&n, "link")
        .iter()
        .any(|l| string(l, "label") == "Select as first term"));
    let err = issue(
        &kernel,
        &writer(),
        Verb::Sink,
        &action("extend"),
        &[
            ("selected", "urn:not:a:class"),
            ("target", "urn:test:Child"),
        ],
    )
    .unwrap_err();
    assert!(matches!(err,Error::InvalidArgument {ref name,..} if name == "selected"));
    assert!(!ask(&kernel, "<urn:test:Child> ?p ?o"));
}

#[test]
fn rdf_and_contextual_json_ld_resources_infer_their_real_types() {
    let (kernel, _) = operations_host();
    for resource in ["urn:test:Turtle", "urn:test:Context"] {
        let raw =
            serde_json::json!([{"iri":resource},{"value":"2","datatype":INTEGER}]).to_string();
        let n = operations(&kernel, &Capability::scoped([INVOKE]), &raw);
        let f = nodes(&n, "form")
            .into_iter()
            .find(|f| string(f, "resource").ends_with(CALL))
            .unwrap();
        assert!(nodes(f, "field")
            .iter()
            .any(|f| string(f, "name") == "person" && string(f, "value") == resource));
    }
    assert!(issue(
        &kernel,
        &Capability::root(),
        Verb::Source,
        "urn:iki:ui:view:lens:operations:resource:urn:personal",
        &[(
            "dropped",
            r#"[{"iri":"urn:test:Alice","types":["urn:forged:Type"]}]"#
        )]
    )
    .is_err());
}

#[test]
fn personal_selection_and_operation_bounds_are_explicit() {
    use ikigai_core::{
        ArgSpec, Description, EndpointSpace, Exact, FnEndpoint, Invocation, ReprType,
        Representation,
    };
    use std::sync::Arc;
    let mut space = EndpointSpace::new();
    for i in 0..33 {
        space = space.bind(
            Exact::new(format!("urn:bound:op:{i:02}")),
            FnEndpoint::new(format!("bound-op-{i}"), |_: &Invocation<'_>| {
                Ok(Representation::new(ReprType::new("text/plain"), Vec::new()))
            })
            .with_description(
                Description::new(format!("bound-op-{i}"))
                    .title("Bounded operation")
                    .summary("A typed operation.")
                    .verb(Verb::Sink)
                    .input(ArgSpec::new("value").class(INTEGER)),
            ),
        );
    }
    let kernel = common::host_with(None, vec![Arc::new(space)]);
    let n = operations(
        &kernel,
        &Capability::root(),
        &serde_json::json!([{"value":"2","datatype":INTEGER}]).to_string(),
    );
    assert_eq!(
        nodes(&n, "form")
            .iter()
            .filter(|f| string(f, "resource").contains("urn:bound:op:"))
            .count(),
        32
    );
    assert!(serde_json::to_string(&n)
        .unwrap()
        .contains("Only the first 32"));
    let raw = serde_json::to_string(&vec![serde_json::json!({"value":"x"}); 33]).unwrap();
    let e = issue(
        &kernel,
        &Capability::root(),
        Verb::Source,
        "urn:iki:ui:view:lens:operations:resource:urn:personal",
        &[("dropped", &raw)],
    )
    .unwrap_err();
    assert!(e.to_string().contains("At most 32"));
}
