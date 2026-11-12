#![cfg(not(target_arch = "wasm32"))]
use std::{
    collections::BTreeMap,
    sync::{mpsc, Arc},
    time::{Duration, Instant},
};

use ikigai_core::{
    Capability, Description, EndpointSpace, Error, Exact, Fallback, FnEndpoint, Invocation, Kernel,
    ReprType, Representation, Scope, Verb,
};
use ikigai_ui::{
    model::{self, Node},
    target::Descriptor,
};
use ikigai_ui_gpui::{
    descriptor_space, text,
    worker::{Feedback, Pending, Worker},
    Snapshot, DESCRIPTOR,
};

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
fn host() -> (Worker, ikigai_ui_demo::Store) {
    let store = ikigai_ui_demo::Store::default();
    let root = Arc::new(Fallback::new(vec![
        Arc::new(descriptor_space()),
        ikigai_ui_demo::root(store.clone()),
    ]));
    (
        Worker::new(
            Arc::new(ikigai_ui_demo::kernel(root)),
            Capability::scoped([ikigai_ui_demo::CONTACT_WRITE]),
            Scope::empty(),
        ),
        store,
    )
}
fn find<'a>(node: &'a Node, kind: &str) -> Option<&'a Node> {
    if node.kind == kind {
        return Some(node);
    }
    node.children.iter().find_map(|child| find(child, kind))
}
fn valid_values() -> BTreeMap<String, String> {
    [
        ("name", "Ada"),
        ("email", "ada@example.org"),
        ("topic", "feedback"),
        ("message", "Native and HTML share a Sink."),
    ]
    .into_iter()
    .map(|(k, v)| (k.into(), v.into()))
    .collect()
}

#[test]
fn gallery_face_covers_the_descriptor_and_keeps_generated_forms() {
    let (worker, _) = host();
    let gallery = wait(&worker.load("gallery")).unwrap();
    let descriptor = Descriptor::parse(DESCRIPTOR, "gpui").unwrap();
    for kind in descriptor.renders {
        assert!(
            find(gallery.root(), &kind).is_some(),
            "gallery did not retain {kind}"
        );
    }
    let contact = wait(&worker.load("contact")).unwrap();
    let form = find(contact.root(), "form").unwrap();
    assert_eq!(text(form, "resource"), Some(ikigai_ui_demo::CONTACT));
    assert!(find(form, "field").is_some());
    assert!(find(form, "choice").is_some());
}

#[test]
fn native_submission_uses_the_same_sink_and_returns_field_refusals() {
    let (worker, store) = host();
    let contact = wait(&worker.load("contact")).unwrap();
    let form = find(contact.root(), "form").unwrap();
    let mut values = valid_values();
    values.insert("email".into(), "invalid".into());
    let mut feedback = Feedback::default();
    feedback.complete(wait(&worker.submit(form, values)));
    assert_eq!(feedback.fields["email"], "an address like ada@example.org");
    assert!(store.lock().unwrap().is_empty());
    feedback.complete(wait(&worker.submit(form, valid_values())));
    assert!(feedback.fields.is_empty());
    assert_eq!(store.lock().unwrap()[0].name, "Ada");
}

#[test]
fn submissions_cannot_gain_authority_or_follow_html_actions() {
    let (worker, store) = host();
    let contact = wait(&worker.load("contact")).unwrap();
    let mut form = find(contact.root(), "form").unwrap().clone();
    form.properties.remove("resource");
    assert!(wait(&worker.submit(&form, valid_values())).is_err());
    assert!(store.lock().unwrap().is_empty());
    let root = Arc::new(Fallback::new(vec![
        Arc::new(descriptor_space()),
        ikigai_ui_demo::root(store.clone()),
    ]));
    let denied = Worker::new(
        Arc::new(ikigai_ui_demo::kernel(root)),
        Capability::scoped(Vec::<String>::new()),
        Scope::empty(),
    );
    let form = find(contact.root(), "form").unwrap();
    assert!(wait(&denied.submit(form, valid_values())).is_err());
    assert!(store.lock().unwrap().is_empty());
}

#[test]
fn snapshot_rejects_unresolved_or_executable_vocabulary() {
    for model in [
        serde_json::json!({"kind":"raw", "properties":{"content":"<script>run()</script>"}}),
        serde_json::json!({"kind":"urn:example:ui:element:name:unknown", "properties":{"fallback":"unresolved"}}),
    ] {
        assert!(Snapshot::from_face(&serde_json::to_vec(&model).unwrap()).is_err());
    }
    let node = serde_json::json!({"kind":"text", "properties":{"content":"<script>literal text</script>"}});
    let snapshot = Snapshot::from_face(&serde_json::to_vec(&node).unwrap()).unwrap();
    assert_eq!(
        text(snapshot.root(), "content"),
        Some("<script>literal text</script>")
    );
}

#[test]
fn five_second_composition_never_runs_on_the_polling_thread() {
    let ui_thread = std::thread::current().id();
    let (started, signal) = mpsc::channel();
    let slow = EndpointSpace::new().bind(
        Exact::new("urn:iki:ui:screen:name:slow"),
        FnEndpoint::new("slow-composition", move |_: &Invocation<'_>| {
            assert_ne!(std::thread::current().id(), ui_thread);
            started.send(()).unwrap();
            std::thread::sleep(Duration::from_secs(5));
            Ok(Representation::new(
                ReprType::new(model::MEDIA_TYPE),
                br#"{"kind":"text","properties":{"content":"Ready"}}"#.to_vec(),
            ))
        })
        .with_description(
            Description::new("slow-composition")
                .title("Slow composition")
                .summary("Blocks for five seconds.")
                .verb(Verb::Source)
                .output(model::MEDIA_TYPE),
        ),
    );
    let root = Arc::new(Fallback::new(vec![
        Arc::new(slow),
        Arc::new(descriptor_space()),
        Arc::new(ikigai_ui::space()),
        Arc::new(ikigai_ui::screen::screens()),
    ]));
    let worker = Worker::new(
        Arc::new(Kernel::new(root)),
        Capability::root(),
        Scope::empty(),
    );
    let before = Instant::now();
    let pending = worker.load("slow");
    assert!(before.elapsed() < Duration::from_millis(100));
    signal.recv_timeout(Duration::from_secs(2)).unwrap();
    let mut events = 0;
    let deadline = Instant::now() + Duration::from_millis(250);
    while Instant::now() < deadline {
        assert!(pending.poll().is_none());
        events += 1;
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(events > 20, "the event loop must keep making progress");
    assert_eq!(
        text(wait(&pending).unwrap().root(), "content"),
        Some("Ready")
    );
    assert!(before.elapsed() >= Duration::from_secs(5));
}

#[test]
fn snapshot_validation_bounds_the_native_walkers_inputs() {
    for invalid in [
        serde_json::json!({"kind":"field", "properties":{"label":"Missing name"}}),
        serde_json::json!({"kind":"heading", "properties":{"content":"Bad level", "level":i64::MAX}}),
        serde_json::json!({"kind":"choice", "properties":{"label":"Bad option", "name":"topic"},
            "children":[{"kind":"text", "properties":{"content":"Not an option"}}]}),
        serde_json::json!({"kind":"link", "properties":{"label":"Code", "href":"javascript:alert(1)"}}),
        serde_json::json!({"kind":"table", "properties":{"caption":"Bad row"},
            "children":[{"kind":"field", "properties":{"label":"Not a row", "name":"row"}}]}),
    ] {
        assert!(Snapshot::from_face(&serde_json::to_vec(&invalid).unwrap()).is_err());
    }
    let empty_table = br#"{"kind":"table","properties":{"caption":"No rows"}}"#;
    assert!(Snapshot::from_face(empty_table).is_ok());
    let mut deep = r#"{"kind":"text","properties":{"content":"Leaf"}}"#.to_string();
    for _ in 0..model::MAX_DEPTH {
        deep =
            format!(r#"{{"kind":"group","properties":{{"label":"Nested"}},"children":[{deep}]}}"#);
    }
    assert!(Snapshot::from_face(deep.as_bytes()).is_err());
    let huge =
        serde_json::json!({"kind":"text", "properties":{"content":"x".repeat(model::MAX_BYTES)}});
    assert!(Snapshot::from_face(&serde_json::to_vec(&huge).unwrap()).is_err());
}

#[test]
fn submission_preserves_each_named_value_as_utf8_bytes() {
    let (sent, received) = mpsc::channel();
    let sink = EndpointSpace::new().bind(
        Exact::new("urn:test:native-values"),
        FnEndpoint::new("native-values", move |inv: &Invocation<'_>| {
            let values: BTreeMap<String, String> = ["z-last", "a-first", "topic", "message"]
                .into_iter()
                .map(|name| Ok((name.to_string(), inv.inline_str(name)?.to_string())))
                .collect::<Result<_, Error>>()?;
            sent.send(values).unwrap();
            Ok(Representation::new(ReprType::new("text/plain"), Vec::new()))
        }),
    );
    let worker = Worker::new(
        Arc::new(Kernel::new(Arc::new(sink))),
        Capability::root(),
        Scope::empty(),
    );
    let form = model::parse(br#"{"kind":"form","properties":{"label":"Values","resource":"urn:test:native-values","verb":"sink"}}"#).unwrap();
    let values = BTreeMap::from([
        ("z-last".into(), "🦀 e\u{301}".into()),
        ("a-first".into(), "".into()),
        ("topic".into(), "stored-value-not-the-label".into()),
        ("message".into(), "line one\n日本語\n".repeat(10000)),
    ]);
    wait(&worker.submit(&form, values.clone())).unwrap();
    assert_eq!(
        received.recv_timeout(Duration::from_secs(2)).unwrap(),
        values
    );
}

#[test]
fn custom_elements_reach_the_native_face_expanded_into_kinds_it_draws() {
    // card, key-value and tag-list (ledger #3511): GPUI claims none, so each arrives expanded,
    // and only as kinds the native renderer draws (its key-value is a table, drawn as rows).
    let (worker, _) = host();
    let elements = wait(&worker.load("elements")).unwrap();
    let root = elements.root();
    let descriptor = Descriptor::parse(DESCRIPTOR, "gpui").unwrap();
    fn every<'a>(node: &'a Node, out: &mut Vec<&'a Node>) {
        out.push(node);
        for child in &node.children {
            every(child, out);
        }
    }
    let mut nodes = Vec::new();
    every(root, &mut nodes);
    for node in &nodes {
        assert!(
            descriptor.renders.contains(&node.kind),
            "{} is not drawn natively",
            node.kind
        );
    }
    let card = &root.children[2];
    assert_eq!(card.kind, "group");
    assert_eq!(text(card, "label"), Some("Field notes"));
    assert_eq!(card.children[0].kind, "heading");
    assert_eq!(
        text(&card.children[1], "content").map(|s| s.starts_with("The weekly")),
        Some(true)
    );
    let key_value = &root.children[3];
    assert_eq!(key_value.kind, "table");
    assert_eq!(text(key_value, "caption"), Some("Gauge 14"));
    assert_eq!(key_value.children.len(), 3);
    let tags = &root.children[4];
    assert_eq!(tags.kind, "list");
    assert_eq!(text(tags, "label"), Some("Topics"));
    assert_eq!(text(&tags.children[0], "href"), Some("/topic/hydrology"));
}
