//! The native face's worker inside a secure region (ledger #3536, audit #3525 follow-up). A
//! region's boundary is gone from the snapshot (it composes to a `group`), so what the window
//! reads afterwards on its own (a `scene`'s datum, through `Worker::scene`) must already have
//! been held to the region where the face was composed.
//! The rest the window reads (screens, references, element definitions) is read by the face's
//! composition, held to the region there (ui PRs 49, 50); its palette is the host's theme, read
//! by no node. Each test here reads through the worker the window uses.
#![cfg(not(target_arch = "wasm32"))]
use std::{
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use ikigai_core::{
    Capability, Description, EndpointSpace, Error, Exact, Fallback, FnEndpoint, Invocation, Kernel,
    ReprType, Representation, Scope, Verb,
};
use ikigai_ui::model::{self, Node};
use ikigai_ui_gpui::{
    descriptor_space,
    worker::{Pending, Worker},
};

const GRAPH: &str = "urn:demo:graph";
const DATUM: &str = "urn:iki:ui:scene:graph:urn:demo:graph";
const SCOPE: &str = "urn:cap:demo:graph:read";
const SECRET: &str = "Vault 7";

fn wait<T>(pending: &Pending<T>) -> Result<T, Error> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(result) = pending.poll() {
            return result;
        }
        assert!(Instant::now() < deadline, "worker timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}

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

/// The scene datum, readable only under [`SCOPE`]; `reached` counts every entry.
fn datum(reached: Arc<AtomicU32>) -> FnEndpoint {
    let bytes = serde_json::to_vec(&serde_json::json!({
        "graph": GRAPH,
        "nodes": [{"id": "urn:demo:vault", "label": SECRET, "position": [0.0, 0.0, 0.0]}],
        "edges": [],
        "camera": {"position": [0.0, 0.0, 3.0], "target": [0.0, 0.0, 0.0], "fov": 45.0}
    }))
    .unwrap();
    FnEndpoint::new("demo-scene", move |inv: &Invocation<'_>| {
        reached.fetch_add(1, Ordering::SeqCst);
        if !inv.capability.allows(SCOPE) {
            return Err(Error::Denied(format!("{SCOPE} is needed")));
        }
        Ok(Representation::new(
            ReprType::new(ikigai_ui::scene::SCENE_MEDIA_TYPE),
            bytes.clone(),
        )
        .cacheable())
    })
    .with_description(
        Description::new("demo-scene")
            .title("A sensitive graph's scene")
            .summary("Sensitive.")
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .output(ikigai_ui::scene::SCENE_MEDIA_TYPE),
    )
}

const FIELD: &str = "urn:demo:card-field";
const CARD: &str = "4111 1111 1111 1111";

/// A field holding the card number as its value, readable only under [`SCOPE`].
fn card_field(reached: Arc<AtomicU32>) -> FnEndpoint {
    let bytes = serde_json::to_vec(&serde_json::json!({
        "kind": "field", "properties": {"label": "Card", "name": "card", "value": CARD}
    }))
    .unwrap();
    FnEndpoint::new("demo-card-field", move |inv: &Invocation<'_>| {
        reached.fetch_add(1, Ordering::SeqCst);
        if !inv.capability.allows(SCOPE) {
            return Err(Error::Denied(format!("{SCOPE} is needed")));
        }
        Ok(Representation::new(ReprType::new(model::MEDIA_TYPE), bytes.clone()).cacheable())
    })
    .with_description(
        Description::new("demo-card-field")
            .title("A card field")
            .summary("Sensitive.")
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .output(model::MEDIA_TYPE),
    )
}

fn scene() -> serde_json::Value {
    serde_json::json!({"kind": "scene", "properties": {"label": "Vaults", "datum": DATUM},
        "replacement": {"kind": "text", "properties": {"content": "The vault graph, withheld"}}})
}

fn region(allow: &str, scopes: Option<&str>) -> serde_json::Value {
    let mut properties = serde_json::json!({"label": "Payment"});
    if !allow.is_empty() {
        properties["allow"] = allow.into();
    }
    if let Some(s) = scopes {
        properties["scopes"] = s.into();
    }
    serde_json::json!({"kind": "region", "properties": properties, "children": [scene()]})
}

fn worker(reached: Arc<AtomicU32>, capability: Capability) -> Worker {
    let mut space = EndpointSpace::new()
        .bind(Exact::new(DATUM), datum(reached.clone()))
        .bind(Exact::new(FIELD), card_field(reached));
    for (name, body) in [
        ("open", scene()),
        // A region that does not allow the datum.
        ("narrow", region("urn:demo:greeting", None)),
        // A region that allows it, under scopes that do not grant it.
        ("scoped", region(DATUM, Some("urn:cap:demo:other"))),
        // A region that allows it under scopes that grant it.
        ("granted", region(DATUM, Some(SCOPE))),
        // A field whose value is a reference, inside a region that allows it under scopes
        // that do not grant it.
        (
            "field",
            serde_json::json!({"kind": "region", "properties": {"label": "Card",
            "allow": FIELD, "scopes": "urn:cap:demo:other"},
            "children": [{"kind": "form", "properties": {"label": "Card", "resource": "urn:demo:sink", "verb": "sink"},
                "children": [{"kind": "reference", "properties": {"resource": FIELD, "fallback": "Card withheld"}}]}]}),
        ),
        // The same reference inside a region that does not allow it.
        (
            "unallowed",
            serde_json::json!({"kind": "region", "properties": {"label": "Card",
            "allow": "urn:demo:greeting"},
            "children": [{"kind": "reference", "properties": {"resource": FIELD, "fallback": "Card withheld"}}]}),
        ),
    ] {
        let body = serde_json::json!({"kind": "group", "properties": {"label": "Account"}, "children": [body]});
        space = space.bind(
            Exact::new(format!("urn:iki:ui:screen:name:{name}")),
            model_endpoint("screen", body),
        );
    }
    let root = Arc::new(Fallback::new(vec![
        Arc::new(space),
        Arc::new(descriptor_space()),
        Arc::new(ikigai_ui::space()),
        Arc::new(ikigai_ui::screen::screens()),
    ]));
    Worker::new(Arc::new(Kernel::new(root)), capability, Scope::empty())
}

fn find<'a>(node: &'a Node, kind: &str) -> Option<&'a Node> {
    if node.kind == kind {
        return Some(node);
    }
    node.children.iter().find_map(|child| find(child, kind))
}

#[cfg(feature = "scene")]
/// What the window does with a snapshot (`Screen::install`): it reads every scene's datum.
fn window_reads(worker: &Worker, root: &Node) -> Option<Result<ikigai_ui::scene::Scene, Error>> {
    find(root, "scene").map(|s| wait(&worker.scene(ikigai_ui_gpui::text(s, "datum").unwrap())))
}

#[test]
fn a_reference_inside_a_region_reaches_no_snapshot_unless_the_region_grants_it() {
    let reached = Arc::new(AtomicU32::new(0));
    let worker = worker(reached.clone(), Capability::scoped([SCOPE]));
    // Allowed under scopes that do not grant it: withheld, so no field (and no value the
    // window's retained form state would be seeded from) reaches the snapshot.
    let snapshot = wait(&worker.load("field")).unwrap();
    let bytes = serde_json::to_string(snapshot.root()).unwrap();
    assert!(!bytes.contains(CARD), "{bytes}");
    assert!(find(snapshot.root(), "field").is_none(), "{bytes}");
    assert!(bytes.contains("Card withheld"), "{bytes}");
    // Not allowed: refused before the field is entered, even under the root grant.
    let before = reached.load(Ordering::SeqCst);
    let root = self::worker(reached.clone(), Capability::root());
    match wait(&root.load("unallowed")) {
        Err(Error::Unresolved(t)) => assert_eq!(t.as_str(), FIELD),
        other => panic!(
            "{:?}",
            other.map(|s| serde_json::to_string(s.root()).unwrap())
        ),
    }
    assert_eq!(
        reached.load(Ordering::SeqCst),
        before,
        "the field was entered"
    );
}

#[cfg(feature = "scene")]
#[test]
fn outside_a_region_the_window_reads_the_datum_under_the_host_grant() {
    let worker = worker(Arc::default(), Capability::scoped([SCOPE]));
    let snapshot = wait(&worker.load("open")).unwrap();
    let read = window_reads(&worker, snapshot.root())
        .expect("a scene")
        .unwrap();
    assert_eq!(read.nodes[0].label, SECRET);
}

#[cfg(feature = "scene")]
#[test]
fn a_scene_inside_a_region_that_does_not_allow_its_datum_is_never_read() {
    let reached = Arc::new(AtomicU32::new(0));
    for capability in [Capability::scoped([SCOPE]), Capability::root()] {
        let worker = worker(reached.clone(), capability);
        // The face refuses, as it refuses a reference the region does not allow, so no
        // snapshot names the datum and the window never reads it.
        match wait(&worker.load("narrow")) {
            Err(Error::Unresolved(t)) => assert_eq!(t.as_str(), DATUM),
            Err(e) => panic!("{e}"),
            Ok(snapshot) => {
                let read = window_reads(&worker, snapshot.root());
                panic!("the snapshot named the datum; the window read {read:?}");
            }
        }
    }
    assert_eq!(reached.load(Ordering::SeqCst), 0, "the datum was entered");
}

#[cfg(feature = "scene")]
#[test]
fn a_scene_inside_a_region_whose_scopes_do_not_grant_its_datum_is_withheld() {
    let worker = worker(Arc::default(), Capability::scoped([SCOPE]));
    let snapshot = wait(&worker.load("scoped")).unwrap();
    // Withheld: the scene is its replacement, so the window holds no datum to read.
    if let Some(read) = window_reads(&worker, snapshot.root()) {
        panic!("the window read the datum under the host's grant: {read:?}");
    }
    let bytes = serde_json::to_string(snapshot.root()).unwrap();
    assert!(!bytes.contains(DATUM), "{bytes}");
    assert!(bytes.contains("The vault graph, withheld"), "{bytes}");

    // Under scopes that grant it, the window reads it.
    let snapshot = wait(&worker.load("granted")).unwrap();
    let read = window_reads(&worker, snapshot.root())
        .expect("a scene")
        .unwrap();
    assert_eq!(read.nodes[0].label, SECRET);
}
