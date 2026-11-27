//! What the demo's workspace page costs to render (ledger #4483), counted, not timed: a tracer
//! on the kernel sees every request. The page reads the workspace's bare name once (to learn
//! the version), composes the workspace screen once, over that saved version, and reads that
//! screen once for its node models; the kernel's cache answers each node's re-read.
use std::sync::{Arc, Mutex};

use futures::executor::block_on;
use ikigai_core::{Capability, Iri, Kernel, Request, Scope, TraceEvent, Tracer, Verb};

#[derive(Default)]
struct Seen(Mutex<Vec<TraceEvent>>);

impl Tracer for Seen {
    fn record(&self, event: TraceEvent) {
        self.0.lock().unwrap().push(event);
    }
}

/// One render of the workspace's html face: its bytes, and how many times it read the bare
/// name, composed a workspace screen (not from the cache) and read the screen for its models.
fn render(kernel: &Kernel, scope: &Scope) -> (Vec<u8>, usize, usize, usize) {
    let iri = format!(
        "urn:iki:ui:html:screen:workspace:resource:{}",
        ikigai_ui_demo::WORKSPACE
    );
    let seen = Arc::new(Seen::default());
    kernel.set_tracer(seen.clone());
    let rep = block_on(kernel.issue_in(
        Request::new(Verb::Source, Iri::parse(iri.as_str()).unwrap()),
        &Capability::root(),
        scope.clone(),
    ))
    .unwrap();
    kernel.clear_tracer();
    let seen = seen.0.lock().unwrap();
    let bare = seen
        .iter()
        .filter(|e| e.target == ikigai_ui_demo::WORKSPACE)
        .count();
    let composed = seen
        .iter()
        .filter(|e| e.target.starts_with(ikigai_ui::workspace::SCREEN) && !e.cache_hit)
        .count();
    let read = seen
        .iter()
        .filter(|e| e.target.starts_with(ikigai_ui::workspace::FACE))
        .count();
    (rep.bytes, bare, composed, read)
}

/// The demo's page reads its bare name once and its screen once; the next render reads the
/// bare name again (never cached) and the cache answers the rest, the same bytes.
#[test]
fn the_workspace_page_reads_its_screen_once() {
    let kernel = ikigai_ui_demo::kernel(ikigai_ui_demo::root(Arc::default()));
    // The door's scope, made once, as the door makes it once.
    let scope = block_on(ikigai_ui_demo::prefixes(&kernel)).unwrap();
    let (first, bare, composed, read) = render(&kernel, &scope);
    eprintln!(
        "first render: {bare} bare-name reads, {composed} screens composed, {read} screen reads"
    );
    assert_eq!((bare, composed, read), (1, 1, 1));
    let (again, bare, composed, _) = render(&kernel, &scope);
    eprintln!("second render: {bare} bare-name reads, {composed} screens composed");
    assert_eq!(again, first);
    assert_eq!((bare, composed), (1, 0));
}
