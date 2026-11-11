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
