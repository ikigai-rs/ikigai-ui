//! Target descriptors, the support matrix and the target corridor.
mod common;

#[test]
fn target_resources_exist() {
    let kernel = common::host(None);
    for iri in [
        "urn:iki:ui:target:name:html",
        "urn:iki:ui:target:name:gpui",
        "urn:iki:ui:support",
        "urn:iki:ui:face:target:html:screen:gallery",
        "urn:iki:ui:face:target:gpui:screen:gallery",
    ] {
        common::source(&kernel, iri).unwrap_or_else(|e| panic!("{iri}: {e}"));
    }
}

use std::sync::Arc;

use ikigai_core::{
    Capability, Description, EndpointSpace, Error, Exact, FnEndpoint, Iri, Kernel, ReprType,
    Representation, Request, Verb,
};
use ikigai_ui::{
    model::{self, Node, Value},
    target::{self, Descriptor},
};

fn face(target: &str, screen: &str) -> String {
    format!("urn:iki:ui:face:target:{target}:screen:{screen}")
}

fn descriptor(kernel: &Kernel, name: &str) -> Descriptor {
    let rep = common::source(kernel, &format!("{}{name}", target::PREFIX)).unwrap();
    assert_eq!(rep.repr_type.media_type, "application/json");
    Descriptor::parse(&rep.bytes, name).unwrap()
}

fn model_at(kernel: &Kernel, iri: &str) -> Node {
    model::parse(&common::source(kernel, iri).unwrap().bytes).unwrap()
}

// Independent expectation for the shipped descriptors: table -> list and nav -> list
// preserve their children; text fallbacks keep only the source kind's fallback property.
fn expected(node: &Node, descriptor: &Descriptor) -> Node {
    let Some(destination) = descriptor.fallback.get(&node.kind) else {
        let mut result = node.clone();
        result.children = node
            .children
            .iter()
            .map(|n| expected(n, descriptor))
            .collect();
        return result;
    };
    let text = node.properties[model::kind(&node.kind).unwrap().fallback].clone();
    match destination.as_str() {
        "text" => Node {
            kind: "text".into(),
            fallback: None,
            properties: [("content".into(), text)].into(),
            children: vec![],
        },
        "list" => Node {
            kind: "list".into(),
            fallback: None,
            properties: [("label".into(), text)].into(),
            children: node
                .children
                .iter()
                .map(|n| expected(n, descriptor))
                .collect(),
        },
        other => panic!("update the independent expectation for {other}"),
    }
}

#[test]
fn gallery_differs_exactly_at_the_descriptor_fallbacks() {
    let kernel = common::host(None);
    let gallery = model_at(&kernel, ikigai_ui::GALLERY);
    let html = model_at(&kernel, &face("html", "gallery"));
    assert_eq!(html, gallery);
    let gpui = descriptor(&kernel, "gpui");
    let actual = model_at(&kernel, &face("gpui", "gallery"));
    assert_ne!(actual, html);
    assert_eq!(actual, expected(&gallery, &gpui));
    let mut walk = vec![&actual];
    while let Some(node) = walk.pop() {
        assert!(gpui.renders.contains(&node.kind));
        walk.extend(&node.children);
    }
}

#[test]
fn support_has_every_decision_and_limit_in_json_and_html() {
    let kernel = common::host(None);
    let matrix: serde_json::Value =
        serde_json::from_slice(&common::source(&kernel, target::SUPPORT).unwrap().bytes).unwrap();
    assert_eq!(matrix.as_object().unwrap().len(), target::NAMES.len());
    for name in target::NAMES {
        let descriptor = descriptor(&kernel, name);
        assert_eq!(
            matrix[name]["kind"].as_object().unwrap().len(),
            model::KINDS.len()
        );
        for kind in model::KINDS {
            assert_eq!(
                matrix[name]["kind"][kind.name]["renders"],
                descriptor.renders.iter().any(|k| k == kind.name)
            );
            assert_eq!(
                matrix[name]["kind"][kind.name]["fallback"],
                serde_json::to_value(descriptor.fallback.get(kind.name)).unwrap()
            );
        }
        assert_eq!(
            matrix[name]["capabilities"],
            serde_json::to_value(&descriptor.capabilities).unwrap()
        );
        assert_eq!(
            matrix[name]["limits"],
            serde_json::to_value(&descriptor.limits).unwrap()
        );
    }
    let page = common::issue(
        &kernel,
        Verb::Source,
        target::SUPPORT,
        &[("as", b"text/html")],
    )
    .unwrap();
    assert!(page.repr_type.media_type.starts_with("text/html"));
    let page = String::from_utf8(page.bytes).unwrap();
    for name in target::NAMES {
        assert!(page.contains(&format!("<th scope=\"col\">{name}</th>")));
    }
    for kind in model::KINDS {
        assert!(page.contains(&format!("<th scope=\"row\">{}</th>", kind.name)));
    }
    assert!(page.contains("<h2>Capabilities</h2>"));
    for name in target::NAMES {
        for capability in descriptor(&kernel, name).capabilities {
            assert!(page.contains(&format!(
                "<li>{}: {}</li>",
                capability.name, capability.version
            )));
        }
    }
    assert!(page.contains("Fallback: list"));
    assert!(page.contains("1000 rows and 100 columns"));
}

#[test]
fn unknown_targets_are_not_found_even_before_screen_resolution() {
    let kernel = common::host(None);
    for iri in [
        format!("{}unknown", target::PREFIX),
        face("unknown", "missing"),
    ] {
        assert!(
            matches!(common::source(&kernel, &iri), Err(Error::NotFound(_))),
            "{iri}"
        );
    }
}

fn assert_cached(kernel: &Kernel, iri: &str) {
    let first = common::source(kernel, iri).unwrap();
    let request = Request::new(Verb::Source, Iri::parse(iri).unwrap());
    assert!(
        kernel.is_cached(&request, &Capability::root()),
        "{iri} is stored in the cache"
    );
    let second = common::source(kernel, iri).unwrap();
    assert_eq!(first.bytes, second.bytes);
    let report = common::issue(
        kernel,
        Verb::Source,
        "urn:kernel:uncached",
        &[("target", iri.as_bytes())],
    )
    .unwrap();
    let report = String::from_utf8(report.bytes).unwrap();
    assert!(report.contains("not computed uncached"), "{iri}: {report}");
}

#[test]
fn faces_and_support_are_cached_and_descriptor_writes_recompute_dependents() {
    let dir =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-target-descriptor");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("target")).unwrap();
    let kernel = common::host(Some(&dir));
    let html_iri = face("html", "gallery");
    let gpui_iri = face("gpui", "gallery");
    for iri in [&html_iri, &gpui_iri, target::SUPPORT] {
        assert_cached(&kernel, iri);
    }
    let original = descriptor(&kernel, "gpui");
    let before = common::source(&kernel, &gpui_iri).unwrap().bytes;
    let support_before = common::source(&kernel, target::SUPPORT).unwrap().bytes;
    let html_before = common::source(&kernel, &html_iri).unwrap().bytes;
    let mut modified = original.clone();
    modified.renders.push("image".into());
    modified.fallback.remove("image");
    let bytes = serde_json::to_vec(&modified).unwrap();
    common::issue(
        &kernel,
        Verb::Sink,
        "urn:iki:ui:dir:target/gpui.json",
        &[("content", &bytes)],
    )
    .unwrap();
    assert_eq!(descriptor(&kernel, "gpui"), modified);
    assert_ne!(common::source(&kernel, &gpui_iri).unwrap().bytes, before);
    assert_eq!(
        model_at(&kernel, &gpui_iri),
        expected(&model_at(&kernel, ikigai_ui::GALLERY), &modified)
    );
    assert_ne!(
        common::source(&kernel, target::SUPPORT).unwrap().bytes,
        support_before
    );
    assert_eq!(
        common::source(&kernel, &html_iri).unwrap().bytes,
        html_before
    );
    for iri in [&html_iri, &gpui_iri, target::SUPPORT] {
        assert_cached(&kernel, iri);
    }
    // A capability-only edit still invalidates every reader of the descriptor,
    // even when no node yet declares a capability requirement.
    modified.capabilities[0].version = "2.0".into();
    let bytes = serde_json::to_vec(&modified).unwrap();
    common::issue(
        &kernel,
        Verb::Sink,
        "urn:iki:ui:dir:target/gpui.json",
        &[("content", &bytes)],
    )
    .unwrap();
    for iri in [&gpui_iri, target::SUPPORT] {
        let request = Request::new(Verb::Source, Iri::parse(iri).unwrap());
        assert!(
            !kernel.is_cached(&request, &Capability::root()),
            "{iri} was invalidated"
        );
        assert_cached(&kernel, iri);
    }
    let matrix: serde_json::Value =
        serde_json::from_slice(&common::source(&kernel, target::SUPPORT).unwrap().bytes).unwrap();
    assert_eq!(matrix["gpui"]["capabilities"][0]["version"], "2.0");
    common::issue(
        &kernel,
        Verb::Delete,
        "urn:iki:ui:dir:target/gpui.json",
        &[],
    )
    .unwrap();
    assert_eq!(common::source(&kernel, &gpui_iri).unwrap().bytes, before);
    assert_eq!(
        common::source(&kernel, target::SUPPORT).unwrap().bytes,
        support_before
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn json_ld_face_names_every_projected_node() {
    let kernel = common::host(None);
    let iri = face("gpui", "gallery");
    let rep = common::issue(
        &kernel,
        Verb::Source,
        &iri,
        &[("as", model::JSON_LD.as_bytes())],
    )
    .unwrap();
    assert_eq!(rep.repr_type.media_type, model::JSON_LD);
    let mut ld: serde_json::Value = serde_json::from_slice(&rep.bytes).unwrap();
    ld.as_object_mut().unwrap().remove("@context").unwrap();
    let mut stack = vec![&mut ld];
    while let Some(node) = stack.pop() {
        let map = node.as_object_mut().unwrap();
        let id = map.remove("@id").unwrap();
        assert!(id
            .as_str()
            .unwrap()
            .starts_with(&format!("{iri}:node:path:/")));
        if let Some(children) = map.get_mut("children") {
            stack.extend(children.as_array_mut().unwrap());
        }
    }
    assert_eq!(ld, serde_json::to_value(model_at(&kernel, &iri)).unwrap());
}

#[test]
fn descriptor_requires_complete_terminating_decisions() {
    let kernel = common::host(None);
    let good = descriptor(&kernel, "gpui");
    let mut bad = Vec::new();
    let mut d = good.clone();
    d.fallback.remove("image");
    bad.push(d);
    let mut d = good.clone();
    d.fallback.insert("text".into(), "image".into());
    bad.push(d);
    let mut d = good.clone();
    d.fallback.insert("image".into(), "unknown".into());
    bad.push(d);
    let mut d = good.clone();
    d.fallback.insert("image".into(), "visual".into());
    d.fallback.insert("visual".into(), "image".into());
    bad.push(d);
    let mut d = good.clone();
    d.renders.retain(|s| s != "text");
    d.fallback.insert("text".into(), "group".into());
    bad.push(d);
    let mut d = good.clone();
    d.renders.push("text".into());
    bad.push(d);
    let mut d = good.clone();
    d.name = "html".into();
    bad.push(d);
    let mut d = good.clone();
    d.renders.push("unknown".into());
    bad.push(d);
    for d in bad {
        assert!(
            matches!(
                Descriptor::parse(&serde_json::to_vec(&d).unwrap(), "gpui"),
                Err(Error::InvalidArgument { .. })
            ),
            "{d:?}"
        );
    }
}

#[test]
fn limits_and_unconstructible_fallbacks_use_the_original_text() {
    let kernel = common::host(None);
    let mut html = descriptor(&kernel, "html");
    let mut table: Node = serde_json::from_str(model::kind("table").unwrap().example).unwrap();
    let limit = html.limits.table.as_mut().unwrap();
    limit.max_rows = table.children.len();
    limit.max_columns = table
        .children
        .iter()
        .map(|n| n.children.len())
        .max()
        .unwrap();
    assert_eq!(html.apply(&table).unwrap(), table, "inclusive bounds");
    table.children.push(table.children[0].clone());
    let result = html.apply(&table).unwrap();
    assert_eq!(result.kind, "text");
    assert_eq!(result.properties["content"], table.properties["caption"]);
    table.children.pop();
    let cell = table.children[0].children[0].clone();
    table.children[0].children.push(cell);
    assert_eq!(html.apply(&table).unwrap().kind, "text", "column bound");

    let mut gpui = descriptor(&kernel, "gpui");
    gpui.fallback.insert("image".into(), "link".into());
    let image: Node = serde_json::from_str(model::kind("image").unwrap().example).unwrap();
    assert_eq!(
        gpui.apply(&image).unwrap().properties["content"],
        image.properties["alt"],
        "a link cannot be made without href"
    );
    // A native nav cannot retain text children, so the parent's own fallback wins.
    html.renders.retain(|s| s != "link");
    html.fallback.insert("link".into(), "text".into());
    let nav: Node = serde_json::from_str(model::kind("nav").unwrap().example).unwrap();
    let result = html.apply(&nav).unwrap();
    assert_eq!(result.kind, "text");
    assert_eq!(result.properties["content"], nav.properties["label"]);
}

#[test]
fn host_screens_are_read_in_the_callers_scope_without_a_target_argument() {
    let screen = Node {
        kind: "visual".into(),
        fallback: None,
        properties: [("label".into(), Value::Text("Host visual".into()))].into(),
        children: vec![],
    };
    let bytes = serde_json::to_vec(&screen).unwrap();
    let endpoint = FnEndpoint::new("host-screen", move |inv: &ikigai_core::Invocation<'_>| {
        assert!(inv.request.args.is_empty(), "no target argument propagated");
        Ok(Representation::new(ReprType::new(model::MEDIA_TYPE), bytes.clone()).cacheable())
    })
    .with_description(
        Description::new("host-screen")
            .title("Host screen")
            .summary("A host supplied screen.")
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .output(model::MEDIA_TYPE),
    );
    let extra = EndpointSpace::new().bind(Exact::new("urn:iki:ui:screen:name:host"), endpoint);
    let kernel = common::host_with(None, vec![Arc::new(extra)]);
    assert_eq!(model_at(&kernel, &face("html", "host")), screen);
    assert_eq!(
        model_at(&kernel, &face("gpui", "host")).properties["content"],
        Value::Text("Host visual".into())
    );
}

#[test]
fn descriptor_directory_denials_are_not_swallowed() {
    let refusing = FnEndpoint::new("refusing-target-dir", |_| {
        Err(Error::Denied("descriptor directory".into()))
    });
    let extra = EndpointSpace::new().bind(Exact::new("urn:iki:ui:dir:target/gpui.json"), refusing);
    let kernel = common::host_with(None, vec![Arc::new(extra)]);
    for iri in [
        format!("{}gpui", target::PREFIX),
        target::SUPPORT.into(),
        face("gpui", "gallery"),
    ] {
        assert!(
            matches!(common::source(&kernel, &iri), Err(Error::Denied(_))),
            "{iri}"
        );
    }
}

/// Evidence for the scope limitation: an endpoint's supported chain-changing API severs
/// the root, so it cannot inject target context while keeping host screen/directory doors.
#[test]
fn confinement_cannot_preserve_host_screen_and_directory_resolution() {
    let probe = ikigai_core::AsyncFnEndpoint::new("scope-probe", |inv| {
        Box::pin(async move {
            let narrowed = inv.confine(
                Iri::parse("urn:test:target-corridor").unwrap(),
                Arc::new(EndpointSpace::new()),
            );
            assert!(narrowed.scope().is_severed());
            for iri in [ikigai_ui::GALLERY, "urn:iki:ui:dir:target/html.json"] {
                assert!(matches!(
                    narrowed
                        .issue(Request::new(Verb::Source, Iri::parse(iri).unwrap()))
                        .await,
                    Err(Error::Unresolved(_))
                ));
            }
            Ok(Representation::new(
                ReprType::new("text/plain"),
                b"root severed".to_vec(),
            ))
        })
    });
    let extra = EndpointSpace::new()
        .bind(Exact::new("urn:test:scope-probe"), probe)
        .bind(
            Exact::new("urn:iki:ui:dir:target/html.json"),
            FnEndpoint::new("root-file", |_| {
                Ok(Representation::new(
                    ReprType::new("application/json"),
                    b"{}".to_vec(),
                ))
            }),
        );
    let kernel = common::host_with(None, vec![Arc::new(extra)]);
    common::source(&kernel, ikigai_ui::GALLERY).unwrap();
    common::source(&kernel, "urn:iki:ui:dir:target/html.json").unwrap();
    assert_eq!(
        common::source(&kernel, "urn:test:scope-probe")
            .unwrap()
            .bytes,
        b"root severed"
    );
}

#[test]
fn a_screen_change_recomputes_both_faces() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let reads = Arc::new(AtomicUsize::new(0));
    let value = Arc::new(std::sync::Mutex::new("Before"));
    let endpoint = {
        let value = Arc::clone(&value);
        let reads = Arc::clone(&reads);
        FnEndpoint::new("changing-screen", move |_| {
            reads.fetch_add(1, Ordering::SeqCst);
            let node = Node {
                kind: "visual".into(),
                fallback: None,
                properties: [(
                    "label".into(),
                    Value::Text(value.lock().unwrap().to_string()),
                )]
                .into(),
                children: vec![],
            };
            Ok(Representation::new(
                ReprType::new(model::MEDIA_TYPE),
                serde_json::to_vec(&node).unwrap(),
            )
            .cacheable())
        })
    };
    let screen_iri = "urn:iki:ui:screen:name:changing";
    let extra = EndpointSpace::new().bind(Exact::new(screen_iri), endpoint);
    let kernel = common::host_with(None, vec![Arc::new(extra)]);
    for target in target::NAMES {
        assert_cached(&kernel, &face(target, "changing"));
    }
    assert_eq!(reads.load(Ordering::SeqCst), 1);
    *value.lock().unwrap() = "After";
    kernel.cut(screen_iri);
    for (target, property) in [("html", "label"), ("gpui", "content")] {
        assert_eq!(
            model_at(&kernel, &face(target, "changing")).properties[property],
            Value::Text("After".into())
        );
    }
    assert_eq!(reads.load(Ordering::SeqCst), 2);
}

#[test]
fn host_injected_scope_is_inherited_and_its_cache_is_separate() {
    let kernel = common::host(None);
    let iri = face("html", "gallery");
    let root = common::source(&kernel, &iri).unwrap();
    let mut contract = descriptor(&kernel, "html");
    contract.renders.retain(|k| k != "visual");
    contract.fallback.insert("visual".into(), "text".into());
    let descriptor_bytes = serde_json::to_vec(&contract).unwrap();
    let screen_bytes = br#"{"kind":"visual","properties":{"label":"Scoped visual"}}"#;
    let corridor = EndpointSpace::new()
        .bind(
            Exact::new("urn:iki:ui:target:name:html"),
            FnEndpoint::new("scoped-descriptor", move |_| {
                Ok(
                    Representation::new(
                        ReprType::new("application/json"),
                        descriptor_bytes.clone(),
                    )
                    .cacheable(),
                )
            }),
        )
        .bind(
            Exact::new(ikigai_ui::GALLERY),
            FnEndpoint::new("scoped-screen", move |_| {
                Ok(
                    Representation::new(ReprType::new(model::MEDIA_TYPE), screen_bytes.to_vec())
                        .cacheable(),
                )
            }),
        );
    let scope = ikigai_core::Scope::empty().with_named(
        Iri::parse("urn:test:injected-target").unwrap(),
        Arc::new(corridor),
    );
    let request = Request::new(Verb::Source, Iri::parse(&iri).unwrap());
    let scoped = futures::executor::block_on(kernel.issue_in(
        request.clone(),
        &Capability::root(),
        scope.clone(),
    ))
    .unwrap();
    let projected = model::parse(&scoped.bytes).unwrap();
    assert_eq!(projected.kind, "text");
    assert_eq!(
        projected.properties["content"],
        Value::Text("Scoped visual".into())
    );
    assert!(kernel.is_cached_in(&request, &Capability::root(), &scope));
    assert_eq!(common::source(&kernel, &iri).unwrap().bytes, root.bytes);
}

#[test]
fn fallback_chains_terminate_and_directly_edited_contracts_are_checked() {
    let kernel = common::host(None);
    let mut contract = descriptor(&kernel, "gpui");
    // Neither table nor nav is rendered, but their chain ends in rendered list.
    contract.fallback.insert("table".into(), "nav".into());
    let contract = Descriptor::parse(&serde_json::to_vec(&contract).unwrap(), "gpui").unwrap();
    let table: Node = serde_json::from_str(model::kind("table").unwrap().example).unwrap();
    let result = contract.apply(&table).unwrap();
    assert_eq!(result.kind, "list");
    assert_eq!(result.properties["label"], table.properties["caption"]);
    assert_eq!(result.children, table.children);

    // Callers can also edit/deserialize the public type without going through parse.
    // apply must refuse those invalid contracts rather than loop or panic.
    let mut invalid = contract;
    invalid.fallback.insert("nav".into(), "table".into());
    assert!(matches!(
        invalid.apply(&table),
        Err(Error::InvalidArgument { .. })
    ));
}

#[test]
fn versioned_capabilities_are_accepted_and_reported() {
    let kernel = common::host(None);
    let rep = common::source(&kernel, "urn:iki:ui:target:name:html").unwrap();
    let mut json: serde_json::Value = serde_json::from_slice(&rep.bytes).unwrap();
    json["capabilities"] = serde_json::json!([{"name": "hover", "version": "1.0"}]);
    Descriptor::parse(&serde_json::to_vec(&json).unwrap(), "html").unwrap();
    let matrix: serde_json::Value =
        serde_json::from_slice(&common::source(&kernel, target::SUPPORT).unwrap().bytes).unwrap();
    for name in target::NAMES {
        assert!(
            matrix[name]["capabilities"].is_array(),
            "{name} capabilities missing"
        );
    }
}

#[test]
fn native_rendering_precedes_declared_fallback_and_limits_try_it() {
    let kernel = common::host(None);
    let mut html = descriptor(&kernel, "html");
    html.fallback.insert("table".into(), "list".into());
    let table: Node = serde_json::from_str(model::kind("table").unwrap().example).unwrap();
    assert_eq!(
        html.apply(&table).unwrap(),
        table,
        "native wins before fallback"
    );
    html.limits.table.as_mut().unwrap().max_rows = 0;
    let projected = html.apply(&table).unwrap();
    assert_eq!(
        projected.kind, "list",
        "failed native rendering tries declared fallback"
    );
    assert_eq!(projected.properties["label"], table.properties["caption"]);
}

#[test]
fn node_fallback_is_projected_before_the_descriptor_fallback() {
    let kernel = common::host(None);
    let gpui = descriptor(&kernel, "gpui");
    let source = model::parse(
        br#"{
        "kind":"image", "properties":{"alt":"Original image", "src":"/image.png"},
        "fallback":{"kind":"status", "properties":{"content":"Node replacement"}}
    }"#,
    )
    .unwrap();
    let result = gpui.apply(&source).unwrap();
    assert_eq!(result.kind, "status");
    assert_eq!(
        result.properties["content"],
        Value::Text("Node replacement".into())
    );
    assert_eq!(
        descriptor(&kernel, "html").apply(&source).unwrap().kind,
        "image"
    );
    let bytes = serde_json::to_vec(&source).unwrap();
    let extra = EndpointSpace::new().bind(
        Exact::new("urn:iki:ui:screen:name:replacement"),
        FnEndpoint::new("replacement-screen", move |_| {
            Ok(Representation::new(ReprType::new(model::MEDIA_TYPE), bytes.clone()).cacheable())
        }),
    );
    let kernel = common::host_with(None, vec![Arc::new(extra)]);
    assert_eq!(model_at(&kernel, &face("gpui", "replacement")), result);
    let html = model_at(&kernel, &face("html", "replacement"));
    assert_eq!(html.kind, "image");
    assert!(
        html.fallback.is_none(),
        "resolved faces omit replacement metadata"
    );
}

#[test]
fn descriptor_unknown_fields_and_invalid_capabilities_have_clear_errors() {
    let kernel = common::host(None);
    let good = serde_json::to_value(descriptor(&kernel, "html")).unwrap();
    for path in ["", "/capabilities/0", "/limits", "/limits/table"] {
        let mut bad = good.clone();
        bad.pointer_mut(path).unwrap()["future-field"] = true.into();
        assert!(matches!(
            Descriptor::parse(&serde_json::to_vec(&bad).unwrap(), "html"),
            Err(Error::InvalidArgument { detail, .. }) if detail.contains("unknown field `future-field`")
        ));
    }
    for capabilities in [
        serde_json::json!([{"name":"hover"}]),
        serde_json::json!([{"version":"1.0"}]),
        serde_json::json!([{"name":"", "version":"1.0"}]),
        serde_json::json!([{"name":"hover", "version":"  "}]),
        serde_json::json!([{"name":"hover", "version":1}]),
        serde_json::json!([{"name":"hover", "version":"1.0"}, {"name":"hover", "version":"2.0"}]),
    ] {
        let mut bad = good.clone();
        bad["capabilities"] = capabilities;
        assert!(matches!(
            Descriptor::parse(&serde_json::to_vec(&bad).unwrap(), "html"),
            Err(Error::InvalidArgument { .. })
        ));
    }
    let mut empty = good;
    empty["capabilities"] = serde_json::json!([]);
    Descriptor::parse(&serde_json::to_vec(&empty).unwrap(), "html").unwrap();
}

#[test]
fn capabilities_in_support_html_are_escaped_and_versions_preserved() {
    let kernel = common::host(None);
    let mut contract = descriptor(&kernel, "html");
    contract.capabilities = vec![target::Capability {
        name: "<feature>&".into(),
        version: "<version>&".into(),
    }];
    contract.fallback.insert("table".into(), "list".into());
    let bytes = serde_json::to_vec(&contract).unwrap();
    let extra = EndpointSpace::new().bind(
        Exact::new("urn:iki:ui:dir:target/html.json"),
        FnEndpoint::new("html-contract", move |_| {
            Ok(Representation::new(ReprType::new("application/json"), bytes.clone()).cacheable())
        }),
    );
    let kernel = common::host_with(None, vec![Arc::new(extra)]);
    let matrix: serde_json::Value =
        serde_json::from_slice(&common::source(&kernel, target::SUPPORT).unwrap().bytes).unwrap();
    assert_eq!(
        matrix["html"]["capabilities"],
        serde_json::to_value(&contract.capabilities).unwrap()
    );
    let page = common::issue(
        &kernel,
        Verb::Source,
        target::SUPPORT,
        &[("as", b"text/html")],
    )
    .unwrap();
    let page = String::from_utf8(page.bytes).unwrap();
    assert!(page.contains("&lt;feature&gt;&amp;: &lt;version&gt;&amp;"));
    assert!(!page.contains("<feature>"));
    assert!(page.contains("Renders; fallback: list"));
    assert_eq!(matrix["html"]["kind"]["table"]["renders"], true);
    assert_eq!(matrix["html"]["kind"]["table"]["fallback"], "list");
}

#[test]
fn the_ladder_applies_recursively_and_always_reaches_renderable_text() {
    let kernel = common::host(None);
    let mut gpui = descriptor(&kernel, "gpui");
    // A native fallback destination can fail construction and have its own replacement.
    gpui.fallback.insert("image".into(), "link".into());
    gpui.fallback.insert("link".into(), "status".into());
    let source = model::parse(
        br#"{"kind":"image","properties":{"alt":"Original image","src":"/image.png"}}"#,
    )
    .unwrap();
    assert_eq!(gpui.apply(&source).unwrap().kind, "status");
    let source = model::parse(
        br#"{
        "kind":"group", "properties":{"label":"Parent"}, "children":[{
            "kind":"image", "properties":{"alt":"Image", "src":"/image.png"},
            "fallback":{"kind":"visual", "properties":{"label":"Replacement description"}}
        }]
    }"#,
    )
    .unwrap();
    let projected = gpui.apply(&source).unwrap();
    assert_eq!(projected.kind, "group");
    assert_eq!(projected.children[0].kind, "text");
    assert_eq!(
        projected.children[0].properties["content"],
        Value::Text("Replacement description".into())
    );
    assert!(projected.children[0].fallback.is_none());
    // Native claims must not hide a cycle that would be entered after a limit failure.
    gpui.fallback.insert("status".into(), "image".into());
    assert!(gpui.validate().is_err());
}

#[test]
fn option_and_generated_forms_follow_each_target_contract() {
    let kernel = common::host_with(None, vec![Arc::new(common::form_models())]);
    let html = descriptor(&kernel, "html");
    let gpui = descriptor(&kernel, "gpui");
    let option = model::parse(model::kind("option").unwrap().example.as_bytes()).unwrap();
    assert!(html.renders.iter().any(|kind| kind == "option"));
    assert_eq!(html.apply(&option).unwrap(), option);
    assert_eq!(gpui.fallback["option"], "text");
    assert_eq!(
        gpui.apply(&option).unwrap(),
        model::parse(br#"{"kind":"text","properties":{"content":"Small"}}"#).unwrap()
    );

    let form = model_at(
        &kernel,
        &format!("{}{}", ikigai_ui::FORM_RESOURCE, common::CONTACT),
    );
    assert!(form.children.iter().any(|child| child.kind == "choice"));
    assert_eq!(html.apply(&form).unwrap(), form);
    assert_eq!(gpui.apply(&form).unwrap().kind, "text");
}
