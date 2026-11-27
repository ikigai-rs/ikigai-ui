//! What one html render costs (ledger #4483): the html face reads the screen's model through
//! the kernel, and a render must read and compose it a bounded number of times, not once per
//! node. Counted, not timed: a tracer on the kernel sees every request the render issues, and
//! each read of `urn:iki:ui:screen:name:{name}` the html face makes is one whole-screen parse
//! and composition, so a count that grows with the tree makes the render grow with its square.
mod common;

use std::sync::{Arc, Mutex};

use futures::executor::block_on;
use ikigai_core::{
    Capability, EndpointSpace, FnEndpoint, Invocation, Iri, Kernel, ReprType, Representation,
    Request, TraceEvent, Tracer, UriTemplate, Verb,
};
use ikigai_ui::model;

const SCREEN: &str = "urn:iki:ui:screen:name:cost-";

/// A screen of `n` labeled groups, each with a heading and two paragraphs: `4n + 1` nodes.
fn tree(n: usize) -> Vec<u8> {
    let groups: Vec<_> = (0..n)
        .map(|i| {
            serde_json::json!({
                "kind": "group",
                "properties": {"label": format!("Group {i}")},
                "children": [
                    {"kind": "heading", "properties": {"content": format!("Heading {i}"), "level": 2}},
                    {"kind": "text", "properties": {"content": format!("First {i}.")}},
                    {"kind": "text", "properties": {"content": format!("Second {i}.")}},
                ],
            })
        })
        .collect();
    serde_json::to_vec(&serde_json::json!({
        "kind": "group",
        "properties": {"label": "Cost"},
        "children": groups,
    }))
    .unwrap()
}

#[derive(Default)]
struct Seen(Mutex<Vec<TraceEvent>>);

impl Tracer for Seen {
    fn record(&self, event: TraceEvent) {
        self.0.lock().unwrap().push(event);
    }
}

fn host() -> Kernel {
    let screens = EndpointSpace::new().bind(
        UriTemplate::parse(format!("{SCREEN}{{n}}")).unwrap(),
        FnEndpoint::new("cost-screen", |inv: &Invocation<'_>| {
            let n: usize = inv.bindings.get("n").unwrap_or_default().parse().unwrap();
            Ok(Representation::new(ReprType::new(model::MEDIA_TYPE), tree(n)).cacheable())
        }),
    );
    let root = Arc::new(ikigai_core::Fallback::new(vec![
        Arc::new(screens) as Arc<dyn ikigai_core::Space>,
        common::root(None, Vec::new()),
    ]));
    common::kernel(root)
}

/// One render of the `n`-group screen on a fresh kernel: its HTML, how many times the html face
/// read the screen's model, and how many node models it computed (not served from the cache).
fn render(n: usize) -> (String, usize, usize) {
    let kernel = host();
    let seen = Arc::new(Seen::default());
    kernel.set_tracer(seen.clone());
    let iri = format!("urn:iki:ui:html:screen:cost-{n}");
    let rep = block_on(kernel.issue(
        Request::new(Verb::Source, Iri::parse(iri.as_str()).unwrap()),
        &Capability::root(),
    ))
    .unwrap();
    kernel.clear_tracer();
    let seen = seen.0.lock().unwrap();
    let reads = seen
        .iter()
        .filter(|e| e.target == format!("{SCREEN}{n}"))
        .count();
    let model = format!("urn:iki:ui:html:screen:cost-{n}:model:");
    let computed = seen
        .iter()
        .filter(|e| e.target.starts_with(&model) && !e.cache_hit)
        .count();
    (String::from_utf8(rep.bytes).unwrap(), reads, computed)
}

/// A render reads the screen's model once, whatever the size of the tree: 1 group or 32.
#[test]
fn a_render_reads_the_screen_once_whatever_its_size() {
    let mut counts = Vec::new();
    for n in [1, 4, 8, 32] {
        let (html, reads, computed) = render(n);
        assert!(html.contains(&format!("Second {}.", n - 1)), "{html}");
        counts.push((n, 4 * n + 1, reads, computed));
    }
    for (groups, nodes, reads, computed) in &counts {
        eprintln!("{groups} groups, {nodes} nodes: {reads} screen reads, {computed} node models");
    }
    assert!(
        counts.iter().all(|&(_, _, reads, _)| reads == 1),
        "the screen is read once per render (groups, nodes, reads, models): {counts:?}"
    );
    // Each node's composed model is computed once, from its parent's: the rest are the cache's.
    assert!(
        counts
            .iter()
            .all(|&(_, nodes, _, computed)| computed == nodes),
        "one node model per node (groups, nodes, reads, models): {counts:?}"
    );
}
