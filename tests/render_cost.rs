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

// A workspace page (ledger #4483, the decision of 2026-11-27): the bare name answers the head
// and is never cached; each saved version, `…:version:{n}`, is never rewritten and answers
// cacheably. The html face reads the bare name once to learn `n`, then composes the page over
// the version, so the kernel's cache answers every node's re-read of the screen.

const WORKSPACE: &str = "urn:iki:ui:workspace:person:urn:person:cost:name:cost";

/// A workspace of `k` panes in a row, each its own lensed screen; `label` names it.
fn atom(k: usize, label: &str) -> Vec<u8> {
    let share = 1000 / k;
    let children: Vec<_> = (0..k)
        .map(|i| {
            let share = if i == 0 {
                1000 - share * (k - 1)
            } else {
                share
            };
            serde_json::json!({"share": share, "pane": format!("p{i}")})
        })
        .collect();
    let panes: serde_json::Map<String, serde_json::Value> = (0..k)
        .map(|i| {
            (
                format!("p{i}"),
                serde_json::json!({
                    "label": format!("Pane {i}"),
                    "content": format!("lens:p{i}:resource:urn:x:cost"),
                }),
            )
        })
        .collect();
    serde_json::to_vec(&serde_json::json!({
        "label": label,
        "layout": {"split": "row", "children": children},
        "panes": panes,
    }))
    .unwrap()
}

/// Every saved version, oldest first: the store's whole state.
type Versions = Arc<Mutex<Vec<Vec<u8>>>>;

/// A host storing `versions` the way a store answers them: the bare name the latest,
/// uncacheable, its type saying `version=n`; `…:version:{n}` that version, cacheable. Each
/// pane's screen is a cacheable paragraph; the addable list, when `addable`, is cacheable too.
/// With `saved` false, the store serves no `…:version:{n}`.
fn workspace_host(versions: Versions, addable: bool, saved: bool) -> Kernel {
    let latest = versions.clone();
    let mut space = EndpointSpace::new()
        .bind(
            ikigai_core::Exact::new(WORKSPACE),
            FnEndpoint::new("cost-atom", move |_: &Invocation<'_>| {
                let held = latest.lock().unwrap();
                Ok(Representation::new(
                    ReprType::new("application/json").with_param("version", held.len().to_string()),
                    held.last().unwrap().clone(),
                ))
            }),
        )
        .bind(
            UriTemplate::parse(format!("{WORKSPACE}:version:{{n}}")).unwrap(),
            FnEndpoint::new("cost-version", move |inv: &Invocation<'_>| {
                if !saved {
                    return Err(ikigai_core::Error::NotFound("no saved versions".into()));
                }
                let held = versions.lock().unwrap();
                let n: usize = inv.bindings.get("n").unwrap_or_default().parse().unwrap();
                let bytes = held
                    .get(n.wrapping_sub(1))
                    .ok_or_else(|| ikigai_core::Error::NotFound(format!("no version {n}")))?;
                Ok(Representation::new(
                    ReprType::new("application/json").with_param("version", n.to_string()),
                    bytes.clone(),
                )
                .cacheable())
            }),
        )
        .bind(
            UriTemplate::parse("urn:iki:ui:screen:name:lens:{rest}").unwrap(),
            FnEndpoint::new("cost-pane", |inv: &Invocation<'_>| {
                let rest = inv.bindings.get("rest").unwrap_or_default();
                let pane = serde_json::json!({
                    "kind": "text", "properties": {"content": format!("The content of {rest}.")}
                });
                Ok(Representation::new(
                    ReprType::new(model::MEDIA_TYPE),
                    serde_json::to_vec(&pane).unwrap(),
                )
                .cacheable())
            }),
        );
    if addable {
        space = space.bind(
            ikigai_core::Exact::new(format!("{WORKSPACE}:addable")),
            FnEndpoint::new("cost-addable", |_: &Invocation<'_>| {
                let list = br#"[{"label": "Notes", "content": "lens:notes:resource:urn:x:notes"}]"#;
                Ok(
                    Representation::new(ReprType::new("application/json"), list.to_vec())
                        .cacheable(),
                )
            }),
        );
    }
    let root = Arc::new(ikigai_core::Fallback::new(vec![
        Arc::new(space) as Arc<dyn ikigai_core::Space>,
        common::root(None, Vec::new()),
    ]));
    common::kernel(root)
}

/// What one workspace page render cost.
#[derive(Debug, PartialEq)]
struct Cost {
    /// Reads of the bare name (each one the moving head, never cached).
    bare: usize,
    /// Workspace screens composed (`urn:iki:ui:screen:workspace:…`), not served from the cache.
    screens: usize,
    /// Screens the html face read for its models (`urn:iki:ui:screen:name:workspace:…`).
    screen_reads: usize,
    /// Node models computed, not served from the cache.
    models: usize,
    /// Distinct node models read.
    nodes: usize,
}

/// One render of the workspace's html face on `kernel`: its HTML and its [`Cost`].
fn render_workspace(kernel: &Kernel) -> (String, Cost) {
    let seen = Arc::new(Seen::default());
    kernel.set_tracer(seen.clone());
    let face = ikigai_ui::workspace::face_name(WORKSPACE, &Default::default());
    let iri = format!("urn:iki:ui:html:screen:{face}");
    let rep = block_on(kernel.issue(
        Request::new(Verb::Source, Iri::parse(iri.as_str()).unwrap()),
        &Capability::root(),
    ))
    .unwrap();
    kernel.clear_tracer();
    let seen = seen.0.lock().unwrap();
    let count = |f: &dyn Fn(&TraceEvent) -> bool| seen.iter().filter(|e| f(e)).count();
    let models: Vec<&str> = seen
        .iter()
        .filter(|e| e.target.contains(":model:") && e.target.starts_with("urn:iki:ui:html:"))
        .map(|e| e.target.as_str())
        .collect();
    let cost = Cost {
        bare: count(&|e| e.target == WORKSPACE),
        screens: count(&|e| e.target.starts_with(ikigai_ui::workspace::SCREEN) && !e.cache_hit),
        screen_reads: count(&|e| e.target.starts_with(ikigai_ui::workspace::FACE)),
        models: count(&|e| {
            e.target.contains(":model:") && e.target.starts_with("urn:iki:ui:html:") && !e.cache_hit
        }),
        nodes: models
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
    };
    (String::from_utf8(rep.bytes).unwrap(), cost)
}

/// A workspace page reads its bare name once and composes its screen once, whatever its size:
/// 2 panes or 8, with or without an addable list. Each node model is computed once. (Far bigger
/// pages outgrow the kernel's default cache bound, 4096 entries, and recompute what it evicts.)
#[test]
fn a_workspace_page_reads_its_screen_once_whatever_its_size() {
    for addable in [false, true] {
        for k in [2, 4, 8] {
            let versions: Versions = Arc::new(Mutex::new(vec![atom(k, "Cost")]));
            let kernel = workspace_host(versions, addable, true);
            let (html, cost) = render_workspace(&kernel);
            assert!(html.contains(&format!("The content of p{}:resource:urn:x:cost.", k - 1)));
            eprintln!("{k} panes, addable {addable}: {cost:?}");
            assert_eq!(cost.bare, 1, "{k} panes, addable {addable}: {cost:?}");
            assert_eq!(cost.screens, 1, "{k} panes, addable {addable}: {cost:?}");
            assert_eq!(
                cost.screen_reads, 1,
                "{k} panes, addable {addable}: {cost:?}"
            );
            assert_eq!(
                cost.models, cost.nodes,
                "{k} panes, addable {addable}: {cost:?}"
            );
        }
    }
}

/// An edit shows at once: the page after it is the new version's, and the old version's
/// cached page is never served for the bare name. With no edit, the next render reads the bare
/// name again (it is never cached) and the cache answers the rest.
#[test]
fn an_edit_shows_at_once_and_the_old_page_is_never_served() {
    let versions: Versions = Arc::new(Mutex::new(vec![atom(3, "Before the edit")]));
    let kernel = workspace_host(versions.clone(), true, true);
    let (first, _) = render_workspace(&kernel);
    assert!(first.contains("Before the edit"), "{first}");
    assert!(first.contains(r#"name="if-version" value="1""#), "{first}");

    let (again, cost) = render_workspace(&kernel);
    assert_eq!(again, first);
    assert_eq!(cost.bare, 1, "the bare name is read again: {cost:?}");
    assert_eq!(
        cost.screens, 0,
        "the version's screen is the cache's: {cost:?}"
    );
    assert_eq!(cost.models, 0, "every node model is the cache's: {cost:?}");

    // The edit: the store saves version 2, a pane more and a new label.
    versions.lock().unwrap().push(atom(4, "After the edit"));
    let (after, cost) = render_workspace(&kernel);
    assert!(after.contains("After the edit"), "{after}");
    assert!(!after.contains("Before the edit"), "{after}");
    assert!(after.contains(r#"name="if-version" value="2""#), "{after}");
    assert!(!after.contains(r#"name="if-version" value="1""#), "{after}");
    assert!(
        after.contains("The content of p3:resource:urn:x:cost."),
        "{after}"
    );
    assert_eq!((cost.bare, cost.screens), (1, 1), "{cost:?}");
}

/// A store that says a version but serves none at `…:version:{n}` renders the same page, from
/// the bare name, as it did before pages were pinned.
#[test]
fn a_store_without_saved_versions_renders_from_the_bare_name() {
    let versions: Versions = Arc::new(Mutex::new(vec![atom(3, "Cost"), atom(3, "Cost")]));
    let (pinned, _) = render_workspace(&workspace_host(versions.clone(), true, true));
    let (unpinned, cost) = render_workspace(&workspace_host(versions, true, false));
    assert_eq!(unpinned, pinned);
    assert!(
        pinned.contains(r#"name="if-version" value="2""#),
        "{pinned}"
    );
    assert!(
        cost.bare > 1,
        "with no saved version, the bare name is read: {cost:?}"
    );
}

/// A pinned face name spells its version one way: `version:03:` is refused, not a second name
/// for version 3.
#[test]
fn a_pinned_name_spells_its_version_one_way() {
    let versions: Versions = Arc::new(Mutex::new(vec![atom(2, "Cost"); 3]));
    let kernel = workspace_host(versions, false, true);
    let read = |n: &str| {
        let iri = format!(
            "{}version:{n}:resource:{WORKSPACE}",
            ikigai_ui::workspace::FACE
        );
        block_on(kernel.issue(
            Request::new(Verb::Source, Iri::parse(iri.as_str()).unwrap()),
            &Capability::root(),
        ))
    };
    assert!(read("3").is_ok());
    for bad in ["03", "x", "-1"] {
        let refused = read(bad).unwrap_err();
        assert!(
            matches!(refused, ikigai_core::Error::InvalidArgument { .. }),
            "{bad}: {refused:?}"
        );
    }
}
