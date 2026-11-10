//! The node kinds and the UI model (ledger #3352): the Turtle shapes and the Rust validator
//! agree on every example, on the gallery and on a set of invalid models; the gallery resolves in
//! both faces and is cached on its second read; a deep model is refused, in a child process on
//! a 2 MiB thread, rather than aborting the host (`ikigai-jsonld/tests/json_depth.rs`'s pattern).

use std::process::Command;
use std::sync::Arc;

use futures::executor::block_on;
use ikigai_core::{ArgRef, Capability, Error, Iri, Kernel, Request, Verb};
use ikigai_ui::model::{self, Node, Value, KINDS};

fn kernel() -> Kernel {
    Kernel::new(Arc::new(ikigai_ui::space()))
}

fn read(kernel: &Kernel, iri: &str, as_type: Option<&str>) -> Result<(String, Vec<u8>), Error> {
    let mut request = Request::new(Verb::Source, Iri::parse(iri).unwrap());
    if let Some(t) = as_type {
        request = request.with_arg("as", ArgRef::Inline(t.as_bytes().to_vec()));
    }
    let rep = block_on(kernel.issue(request, &Capability::root()))?;
    Ok((rep.repr_type.media_type.clone(), rep.bytes))
}

/// The shapes graph: the kind list's node shape and every kind's shape.
fn shapes() -> String {
    let mut all = model::KIND_LIST.to_string();
    for k in &KINDS {
        all.push('\n');
        all.push_str(k.shape);
    }
    all
}

/// A model as RDF, for the SHACL side: a node is `<urn:test:node:{path}>`, typed by its kind's
/// IRI, with its properties and its children by `urn:iki:ui:property:name:child`.
fn turtle(root: &Node) -> String {
    let mut out = String::new();
    let mut stack = vec![("root".to_string(), root)];
    while let Some((id, node)) = stack.pop() {
        out.push_str(&format!(
            "<urn:test:node:{id}> a <{}{}> .\n",
            model::KIND_PREFIX,
            node.kind
        ));
        for (p, v) in &node.properties {
            let object = match v {
                Value::Number(n) => {
                    format!("\"{n}\"^^<http://www.w3.org/2001/XMLSchema#integer>")
                }
                Value::Bool(b) => b.to_string(),
                Value::Text(s) => serde_json::to_string(s).unwrap(),
            };
            out.push_str(&format!(
                "<urn:test:node:{id}> <{}{p}> {object} .\n",
                model::PROPERTY_PREFIX
            ));
        }
        if let Some(fallback) = &node.fallback {
            let fid = format!("{id}.fallback");
            out.push_str(&format!(
                "<urn:test:node:{id}> <{}fallback> <urn:test:node:{fid}> .\n",
                model::PROPERTY_PREFIX
            ));
            stack.push((fid, fallback));
        }
        for (i, child) in node.children.iter().enumerate() {
            let cid = format!("{id}.{i}");
            out.push_str(&format!(
                "<urn:test:node:{id}> <{}child> <urn:test:node:{cid}> .\n",
                model::PROPERTY_PREFIX
            ));
            stack.push((cid, child));
        }
    }
    out
}

fn shacl_conforms(node: &Node) -> bool {
    // SHACL reads `rdfs:subClassOf` from the DATA graph, so the data carries the vocabulary
    // (each kind is a subclass of `urn:iki:ui:node`, as its shape says) beside the model.
    let mut data = turtle(node);
    for k in &KINDS {
        assert!(k.shape.contains("rdfs:subClassOf <urn:iki:ui:node>"));
        data.push_str(&format!(
            "<{}{}> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <urn:iki:ui:node> .\n",
            model::KIND_PREFIX,
            k.name
        ));
    }
    let report =
        ikigai_shacl::validate_report(&data, &shapes()).expect("the shapes are in the subset");
    report.conforms
}

fn node(json: &str) -> Node {
    serde_json::from_str(json).unwrap()
}

#[test]
fn every_example_validates_in_both() {
    for k in &KINDS {
        let n = node(k.example);
        assert_eq!(n.kind, k.name, "the {} example is a {}", k.name, k.name);
        model::validate(&n).unwrap_or_else(|e| panic!("{}: {e}", k.name));
        assert!(
            shacl_conforms(&n),
            "the shapes accept the {} example",
            k.name
        );
    }
}

#[test]
fn every_kind_is_served_by_name_and_sealed() {
    let kernel = kernel();
    let (t, list) = read(&kernel, ikigai_ui::KIND, None).unwrap();
    assert_eq!(t, "text/turtle");
    let list = String::from_utf8(list).unwrap();
    for k in &KINDS {
        let iri = format!("{}{}", model::KIND_PREFIX, k.name);
        assert!(list.contains(&format!("<{iri}>")), "the list names {iri}");
        let (t, shape) = read(&kernel, &iri, None).unwrap();
        assert_eq!(t, "text/turtle");
        assert_eq!(shape, k.shape.as_bytes());
        // The fallback text is required by the shape.
        assert!(k.shape.contains(&format!(
            "<urn:iki:ui:property:name:fallback> <{}{}>",
            model::PROPERTY_PREFIX,
            k.fallback
        )));
    }
    // A name that is not a kind is refused HERE, so no later module answers it.
    for bogus in ["html", "script", "raw"] {
        let iri = format!("{}{bogus}", model::KIND_PREFIX);
        assert!(
            matches!(read(&kernel, &iri, None), Err(Error::NotFound(_))),
            "{iri} is not a kind"
        );
    }
}

/// Invalid models and the path each is refused at.
const INVALID: [(&str, &str); 15] = [
    (
        r#"{"kind":"choice","properties":{"label":"C","name":"c"},"children":[{"kind":"text","properties":{"content":"A"}}]}"#,
        "/children/0",
    ),
    (r#"{"kind":"option","properties":{"label":"A"}}"#, "/"),
    (
        r#"{"kind":"field","properties":{"label":"F","name":"f","required":"yes"}}"#,
        "/",
    ),
    (
        r#"{"kind":"form","properties":{"label":"F","verb":"patch"}}"#,
        "/",
    ),
    // `target` names a face (`urn:iki:ui:target:name:{target}`), not a form's resource.
    (
        r#"{"kind":"form","properties":{"label":"F","target":"urn:iki:demo:contact"}}"#,
        "/",
    ),
    (
        r#"{"kind":"html","properties":{"content":"<b>x</b>"}}"#,
        "/",
    ),
    (r#"{"kind":"heading","properties":{"level":2}}"#, "/"),
    (
        r#"{"kind":"heading","properties":{"content":"A","level":9}}"#,
        "/",
    ),
    (
        r#"{"kind":"text","properties":{"content":"A","html":"<b>"}}"#,
        "/",
    ),
    (r#"{"kind":"text","properties":{"content":""}}"#, "/"),
    (
        r#"{"kind":"text","properties":{"content":"A"},"children":[{"kind":"text","properties":{"content":"B"}}]}"#,
        "/",
    ),
    (
        r#"{"kind":"nav","properties":{"label":"Site"},"children":[{"kind":"text","properties":{"content":"B"}}]}"#,
        "/children/0",
    ),
    (
        r#"{"kind":"group","properties":{"label":"G"},"children":[{"kind":"text","properties":{"content":"A"}},{"kind":"list","properties":{"label":"L"},"children":[{"kind":"script","properties":{"content":"x"}}]}]}"#,
        "/children/1/children/0",
    ),
    (
        r#"{"kind":"form","properties":{"label":"F","method":"put"}}"#,
        "/",
    ),
    (
        r#"{"kind":"group","properties":{"label":"G"},"children":[{"kind":"image","properties":{"alt":"A"}}]}"#,
        "/children/0",
    ),
];

#[test]
fn the_invalid_set_refuses_with_the_path_in_both() {
    for (json, path) in INVALID {
        let n = node(json);
        match model::validate(&n) {
            Err(Error::InvalidArgument { detail, .. }) => assert!(
                detail.starts_with(&format!("{path}: ")),
                "{json}: refused at `{path}`, said `{detail}`"
            ),
            other => panic!("{json}: expected InvalidArgument, got {other:?}"),
        }
        assert!(!shacl_conforms(&n), "the shapes refuse {json}");
        assert!(matches!(
            model::parse(json.as_bytes()),
            Err(Error::InvalidArgument { .. })
        ));
    }
}

#[test]
fn the_gallery_resolves_in_both_faces() {
    let kernel = kernel();
    let (t, bytes) = read(&kernel, ikigai_ui::GALLERY, None).unwrap();
    assert_eq!(t, model::MEDIA_TYPE);
    let gallery = model::parse(&bytes).unwrap();
    assert!(shacl_conforms(&gallery));
    let mut seen: Vec<String> = Vec::new();
    for section in &gallery.children {
        assert_eq!(section.kind, "group");
        for n in &section.children[1..] {
            seen.push(n.kind.clone());
        }
    }
    seen.sort();
    let mut all: Vec<String> = KINDS.iter().map(|k| k.name.to_string()).collect();
    all.sort();
    assert_eq!(seen, all, "the gallery shows every kind once");

    let (t, ld) = read(&kernel, ikigai_ui::GALLERY, Some(model::JSON_LD)).unwrap();
    assert_eq!(t, model::JSON_LD);
    let mut ld: serde_json::Value = serde_json::from_slice(&ld).unwrap();
    let context = ld.as_object_mut().unwrap().remove("@context").unwrap();
    let served: serde_json::Value =
        serde_json::from_slice(&read(&kernel, model::CONTEXT_IRI, None).unwrap().1).unwrap();
    assert_eq!(context, served["@context"], "one context");
    assert_eq!(context["heading"], "urn:iki:ui:kind:name:heading");
    assert_eq!(context["label"], "urn:iki:ui:property:name:label");
    assert_eq!(ld["@id"], format!("{}:node:path:/", ikigai_ui::GALLERY));
    let mut stack = vec![&mut ld];
    while let Some(serde_json::Value::Object(map)) = stack.pop() {
        assert!(map.remove("@id").is_some(), "every node is named");
        if let Some(serde_json::Value::Array(children)) = map.get_mut("children") {
            stack.extend(children.iter_mut());
        }
    }
    assert_eq!(
        ld,
        serde_json::to_value(&gallery).unwrap(),
        "the same source"
    );

    for k in &KINDS {
        let iri = format!("urn:iki:ui:example:kind:{}", k.name);
        let (t, bytes) = read(&kernel, &iri, None).unwrap();
        assert_eq!(t, model::MEDIA_TYPE);
        model::parse(&bytes).unwrap();
        let (t, _) = read(&kernel, &iri, Some(model::JSON_LD)).unwrap();
        assert_eq!(t, model::JSON_LD);
    }
}

#[test]
fn the_gallery_is_cached_on_the_second_read() {
    let kernel = kernel();
    let first = read(&kernel, ikigai_ui::GALLERY, None).unwrap();
    let second = read(&kernel, ikigai_ui::GALLERY, None).unwrap();
    assert_eq!(first, second);
    let request = Request::new(Verb::Source, Iri::parse("urn:kernel:uncached").unwrap()).with_arg(
        "target",
        ArgRef::Inline(ikigai_ui::GALLERY.as_bytes().to_vec()),
    );
    let report = block_on(kernel.issue(request, &Capability::root())).unwrap();
    let report = String::from_utf8(report.bytes).unwrap();
    assert!(
        report.contains("not computed uncached"),
        "the gallery was never computed uncached: {report}"
    );
}

const PROBE: &str = "IKIGAI_UI_MODEL_DEPTH_PROBE";

/// A model `n` node levels deep: two JSON levels a level.
fn deep(n: usize) -> String {
    format!(
        "{}{{\"kind\":\"text\",\"properties\":{{\"content\":\"x\"}}}}{}",
        "{\"kind\":\"group\",\"properties\":{\"label\":\"g\"},\"children\":[".repeat(n),
        "]}".repeat(n)
    )
}

/// The child's side: parse a deep model on a 2 MiB thread.
#[test]
fn depth_probe() {
    let Ok(n) = std::env::var(PROBE) else { return };
    let n: usize = n.parse().unwrap();
    let doc = deep(n);
    let answer = std::thread::Builder::new()
        .stack_size(2 << 20)
        .spawn(move || model::parse(doc.as_bytes()).map(|_| ()))
        .unwrap()
        .join()
        .unwrap();
    match answer {
        Err(Error::InvalidArgument { detail, .. }) if n > model::MAX_DEPTH / 2 => {
            assert!(detail.contains("nested past"), "{detail}")
        }
        Ok(()) if n < model::MAX_DEPTH / 2 => {}
        other => panic!("{n} levels: {other:?}"),
    }
}

fn probe(n: usize) {
    let out = Command::new(std::env::current_exe().unwrap())
        .args(["depth_probe", "--exact", "--nocapture", "--test-threads=1"])
        .env(PROBE, n.to_string())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{n} levels: the child {:?}\n{}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn a_deep_model_is_refused_in_the_child_rather_than_aborting() {
    probe(10_000);
    probe(model::MAX_DEPTH);
    probe(model::MAX_DEPTH / 2 - 1);
}

#[test]
fn a_model_past_the_size_bound_is_refused() {
    let big = format!(
        "{{\"kind\":\"text\",\"properties\":{{\"content\":\"{}\"}}}}",
        "x".repeat(model::MAX_BYTES)
    );
    assert!(matches!(
        model::parse(big.as_bytes()),
        Err(Error::InvalidArgument { .. })
    ));
}

#[test]
fn fallback_subtrees_validate_in_rust_and_shapes_and_have_named_json_ld_nodes() {
    let valid = node(
        r#"{
        "kind":"image", "properties":{"alt":"Image", "src":"/image.png"},
        "fallback":{"kind":"group", "properties":{"label":"Replacement"},
            "children":[{"kind":"text", "properties":{"content":"Description"}}],
            "fallback":{"kind":"text", "properties":{"content":"Last replacement"}}
        }
    }"#,
    );
    model::validate(&valid).unwrap();
    assert!(shacl_conforms(&valid));
    let ld = model::json_ld(&valid, "urn:test:screen");
    for (value, path) in [
        (&ld, "/"),
        (&ld["fallback"], "/fallback"),
        (&ld["fallback"]["children"][0], "/fallback/children/0"),
        (&ld["fallback"]["fallback"], "/fallback/fallback"),
    ] {
        assert_eq!(value["@id"], format!("urn:test:screen:node:path:{path}"));
    }
    for bad in [
        r#"{"kind":"image","properties":{"alt":"Image","src":"/image.png"},"fallback":{"kind":"heading","properties":{"content":"Missing level"}}}"#,
        r#"{"kind":"text","properties":{"content":"Text"},"fallback":{"kind":"text","properties":{"content":"Replacement","level":2}}}"#,
    ] {
        let invalid = node(bad);
        assert!(!shacl_conforms(&invalid));
        assert!(
            matches!(model::parse(bad.as_bytes()), Err(Error::InvalidArgument { detail, .. }) if detail.starts_with("/fallback:"))
        );
    }
}

#[test]
fn fallback_subtrees_obey_the_same_input_depth_bound() {
    let doc = format!(
        "{}{{\"kind\":\"text\",\"properties\":{{\"content\":\"End\"}}}}{}",
        "{\"kind\":\"text\",\"properties\":{\"content\":\"Text\"},\"fallback\":".repeat(10_000),
        "}".repeat(10_000)
    );
    assert!(
        matches!(model::parse(doc.as_bytes()), Err(Error::InvalidArgument { detail, .. }) if detail.contains("nested past"))
    );
}

#[test]
fn every_kind_shape_accepts_a_valid_replacement_subtree() {
    for kind in KINDS {
        let mut example = model::parse(kind.example.as_bytes()).unwrap();
        example.fallback = Some(Box::new(node(
            r#"{"kind":"text","properties":{"content":"Replacement"}}"#,
        )));
        model::validate(&example).unwrap();
        assert!(shacl_conforms(&example), "{} accepts fallback", kind.name);
    }
}
