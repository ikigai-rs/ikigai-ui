//! The embedded 3D scene (ledger #3514): the layout is deterministic and cached (a second read
//! is a cache hit, an edit to the graph cuts it), the scene datum validates and a malformed one
//! is refused, and the html face's native and replacement-only paths hold their snapshots.

mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use common::*;
use ikigai_core::{
    EndpointSpace, Error, Fallback, Invocation, Request, Scope, Space, UriTemplate, Verb,
};
use ikigai_ui::model::{self, Node};
use ikigai_ui::scene::{self, Scene};
use ikigai_ui::testkit::snapshot::assert_snapshots;

/// A space whose every invocation is counted: the graph's reads.
struct Counted {
    inner: Arc<dyn Space>,
    reads: Arc<AtomicUsize>,
}

impl Space for Counted {
    fn resolve(&self, request: &Request, scope: &Scope) -> ikigai_core::Resolution {
        let reads = self.reads.clone();
        self.inner.resolve(request, scope).map_endpoint(|inner| {
            let description = inner.describe();
            Arc::new(
                ikigai_core::AsyncFnEndpoint::new(
                    "counted",
                    move |inv: &Invocation<'_>| -> ikigai_core::InvokeFuture<'_> {
                        reads.fetch_add(1, Ordering::SeqCst);
                        let inner = inner.clone();
                        Box::pin(async move { inner.invoke(inv).await })
                    },
                )
                .with_description(description),
            )
        })
    }
}

const GRAPH: &str = "urn:iki:ui:dir:graph.jsonld";

fn graph(extra: &str) -> String {
    format!(
        r#"{{"@graph": [
  {{"@id": "urn:t:a", "@type": "urn:t:Thing", "rdfs:label": "a", "urn:t:knows": {{"@id": "urn:t:b"}}}},
  {{"@id": "urn:t:b", "@type": "urn:t:Thing", "rdfs:label": "b", "urn:t:knows": {{"@id": "urn:t:c"}}}},
  {{"@id": "urn:t:c", "rdfs:label": "c"}}{extra}
]}}"#
    )
}

fn positions(kernel: &ikigai_core::Kernel) -> serde_json::Value {
    let rep = source(kernel, &format!("{}{GRAPH}", scene::LAYOUT_PREFIX)).unwrap();
    serde_json::from_slice::<serde_json::Value>(&rep.bytes).unwrap()["positions"].clone()
}

#[test]
fn the_layout_is_deterministic() {
    let g = scene::read_graph(scene::EXAMPLE_GRAPH_JSON_LD.as_bytes()).unwrap();
    let first = scene::layout(&g);
    assert_eq!(first.len(), g.nodes.len(), "a position per node");
    assert_eq!(scene::layout(&g), first, "same graph, same positions");
    // Through two kernels, byte for byte.
    let iri = format!("{}{}", scene::LAYOUT_PREFIX, scene::EXAMPLE_GRAPH);
    assert_eq!(
        source(&host(None), &iri).unwrap().bytes,
        source(&host(None), &iri).unwrap().bytes
    );
    // Every position on the unit sphere's inside, and no two nodes at one place.
    let points: Vec<_> = first.values().collect();
    for p in &points {
        assert!(p.iter().map(|x| x * x).sum::<f64>() <= 1.0 + 1e-2, "{p:?}");
    }
    for (i, p) in points.iter().enumerate() {
        assert!(points[i + 1..].iter().all(|q| q != p), "two nodes at {p:?}");
    }
}

#[test]
fn the_layout_is_cached_and_an_edit_to_the_graph_cuts_it() {
    let dir = scratch("scene-layout");
    std::fs::write(dir.join("graph.jsonld"), graph("")).unwrap();
    let reads = Arc::new(AtomicUsize::new(0));
    let files: Arc<dyn Space> = Arc::new(EndpointSpace::new().bind(
        UriTemplate::parse("urn:iki:ui:dir:{path}").unwrap(),
        ikigai_fs::FileEndpoint::new(dir.clone()).cacheable(),
    ));
    let kernel = kernel(Arc::new(Fallback::new(vec![
        Arc::new(ikigai_ui::space()),
        Arc::new(Counted {
            inner: files,
            reads: reads.clone(),
        }),
    ])));
    let first = positions(&kernel);
    assert_eq!(first.as_object().unwrap().len(), 3);
    let after_first = reads.load(Ordering::SeqCst);
    assert!(after_first >= 1, "the graph was read");
    assert_eq!(positions(&kernel), first);
    assert_eq!(
        reads.load(Ordering::SeqCst),
        after_first,
        "a second read of an unchanged graph's layout is a cache hit"
    );
    // The scene datum reads the layout through the kernel: no second layout either.
    let datum = source(&kernel, &format!("{}{GRAPH}", scene::SCENE_PREFIX)).unwrap();
    let datum: Scene = serde_json::from_slice(&datum.bytes).unwrap();
    scene::validate(&datum).unwrap();
    assert_eq!(datum.nodes.len(), 3);
    assert_eq!(datum.edges.len(), 2);

    // An edit to the graph cuts the layout: the next read lays out the new graph.
    let edited =
        graph(r#", {"@id": "urn:t:d", "rdfs:label": "d", "urn:t:knows": {"@id": "urn:t:a"}}"#);
    issue(
        &kernel,
        Verb::Sink,
        GRAPH,
        &[("content", edited.as_bytes())],
    )
    .unwrap();
    let before_edit = reads.load(Ordering::SeqCst);
    let second = positions(&kernel);
    assert!(
        reads.load(Ordering::SeqCst) > before_edit,
        "the edit cut the cached layout"
    );
    assert_eq!(second.as_object().unwrap().len(), 4);
    assert!(second.get("urn:t:d").is_some());
    std::fs::remove_dir_all(&dir).unwrap();
}

fn example() -> Scene {
    let rep = source(
        &host(None),
        &format!("{}{}", scene::SCENE_PREFIX, scene::EXAMPLE_GRAPH),
    )
    .unwrap();
    assert_eq!(rep.repr_type.media_type, scene::SCENE_MEDIA_TYPE);
    serde_json::from_slice(&rep.bytes).unwrap()
}

#[test]
fn the_scene_datum_validates_and_a_malformed_one_is_refused() {
    let good = example();
    scene::validate(&good).unwrap();
    assert_eq!(good.nodes.len(), 8, "every node of the contact graph");
    assert_eq!(good.edges.len(), 7, "every edge of the contact graph");

    let refused = |s: &Scene| match scene::validate(s) {
        Err(Error::InvalidArgument { detail, .. }) => detail,
        other => panic!("not refused: {other:?}"),
    };
    let mut dangling = good.clone();
    dangling.edges[0].to = "urn:nowhere".into();
    assert!(refused(&dangling).starts_with("/edges/0"));
    let mut unlabelled = good.clone();
    unlabelled.nodes[1].label.clear();
    assert!(refused(&unlabelled).starts_with("/nodes/1"));
    let mut twice = good.clone();
    twice.nodes[1].id = twice.nodes[0].id.clone();
    assert!(refused(&twice).contains("twice"));
    let mut lost = good.clone();
    lost.nodes[0].position[2] = f64::NAN;
    assert!(refused(&lost).starts_with("/nodes/0"));
    let mut wide = good.clone();
    wide.camera.fov = 180.0;
    assert!(refused(&wide).starts_with("/camera"));
    // The datum is closed: a field it does not name (a shader, a script) does not parse.
    let mut json = serde_json::to_value(&good).unwrap();
    json["shader"] = "@fragment fn fs() {}".into();
    assert!(serde_json::from_value::<Scene>(json).is_err());
    let mut json = serde_json::to_value(&good).unwrap();
    json["nodes"][0]["script"] = "alert(1)".into();
    assert!(serde_json::from_value::<Scene>(json).is_err());
}

#[test]
fn the_galleries_replacement_names_every_edge_of_the_graph() {
    let datum = example();
    let model: Node = serde_json::from_str(include_str!("../assets/example/scene.json")).unwrap();
    model::validate(&model).unwrap();
    let rows: Vec<Vec<String>> = model
        .replacement
        .as_ref()
        .expect("a replacement")
        .children
        .iter()
        .skip(1)
        .map(|row| {
            row.children
                .iter()
                .map(|c| match &c.properties["content"] {
                    model::Value::Text(t) => t.clone(),
                    other => panic!("{other:?}"),
                })
                .collect()
        })
        .collect();
    let label = |id: &str| {
        datum
            .nodes
            .iter()
            .find(|n| n.id == id)
            .map(|n| n.label.clone())
            .unwrap()
    };
    let mut edges: Vec<Vec<String>> = datum
        .edges
        .iter()
        .map(|e| vec![label(&e.from), e.label.clone(), label(&e.to)])
        .collect();
    let mut rows = rows;
    edges.sort();
    rows.sort();
    assert_eq!(rows, edges);
}

#[test]
fn a_scene_with_no_replacement_is_refused() {
    let bare: Node = serde_json::from_str(
        r#"{"kind":"scene","properties":{"label":"x","datum":"urn:iki:ui:scene:graph:urn:t:g"}}"#,
    )
    .unwrap();
    match model::validate(&bare) {
        Err(Error::InvalidArgument { detail, .. }) => assert!(detail.contains("replacement")),
        other => panic!("not refused: {other:?}"),
    }
}

/// The scene through the html face: native (its figure, the canvas hidden until drawn and the
/// replacement beside it), and replacement-only, from a host whose html descriptor does not
/// claim `scene` (as a face with no 3D does): the ladder puts the replacement in its place.
#[test]
fn the_scenes_html_paths_match_their_snapshots() {
    let figure = |html: &str| -> Option<String> {
        let at = html.find("<figure class=\"ui-scene\"")?;
        let end = at + html[at..].find("</figure>")? + "</figure>".len();
        Some(format!("{}\n", &html[at..end]))
    };
    let native = figure(&text(&host(None), "urn:iki:ui:html:screen:gallery", &[]))
        .expect("the scene's figure");
    assert!(native.contains("<canvas class=\"ui-scene-canvas\""));
    assert!(native.contains(" hidden></canvas>"), "hidden until drawn");
    assert!(native.contains("data-ui-scene=\"/resolution/iri/urn%3Aiki%3Aui%3Ascene%3Agraph%3A"));
    assert!(
        native.contains("href=\"#ui-n.4.3-text\""),
        "the scene links its text form"
    );
    assert!(native.contains("id=\"ui-n.4.3-text\"><table"));

    let dir = scratch("scene-no-3d");
    std::fs::create_dir_all(dir.join("target")).unwrap();
    let html = include_str!("../assets/target/html.json")
        .replace(",\n    \"scene\"\n", "\n")
        .replace("\"fallback\": {}", "\"fallback\": {\"scene\": \"text\"}");
    assert!(!html.contains("\"scene\"\n"));
    std::fs::write(dir.join("target/html.json"), html).unwrap();
    let page = text(&host(Some(&dir)), "urn:iki:ui:html:screen:gallery", &[]);
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(
        figure(&page).is_none(),
        "no scene figure on a face with no scene"
    );
    assert!(!page.contains("<canvas"));
    let at = page
        .find("<table class=\"ui-table\"><caption>The contact contract as text")
        .expect("the replacement in the scene's place");
    let end = at + page[at..].find("</table>").unwrap() + "</table>".len();
    let replacement_only = format!("{}\n", &page[at..end]);
    assert!(
        native.contains(replacement_only.trim_end()),
        "the same replacement as the native path shows beside its canvas"
    );
    assert_snapshots(
        &snapshots(),
        vec![
            ("scene-native.html".to_string(), native),
            ("scene-replacement.html".to_string(), replacement_only),
        ],
    );
}

#[test]
fn a_typed_property_is_a_type_and_a_result_set_is_its_nodes() {
    use ikigai_ui::scene::read_graph;
    // A store's JSON-LD writes `rdf:type` as a property: the node's type, never an edge.
    let g = read_graph(
        br#"[{"@id": "urn:a", "http://www.w3.org/1999/02/22-rdf-syntax-ns#type": [{"@id": "urn:T"}],
              "urn:p": [{"@id": "urn:b"}]}]"#,
    )
    .unwrap();
    assert_eq!(g.nodes["urn:a"].1.as_deref(), Some("urn:T"));
    assert_eq!(g.edges, [("urn:a".into(), "urn:p".into(), "urn:b".into())]);
    assert!(!g.nodes.contains_key("urn:T"));
    // A SELECT result is its rows' `?node`s that are IRIs, labeled by `?label`.
    let g = read_graph(
        br#"{"head": {"vars": ["node", "label"]}, "results": {"bindings": [
              {"node": {"type": "uri", "value": "urn:a"}, "label": {"type": "literal", "value": "A"}},
              {"node": {"type": "literal", "value": "not a node"}}]}}"#,
    )
    .unwrap();
    assert_eq!(g.nodes.len(), 1);
    assert_eq!(g.nodes["urn:a"].0, "A");
    assert!(g.edges.is_empty());
}
