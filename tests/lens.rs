//! Lenses (ledger #3518) through a real `urn:sparql:select` (ikigai-sparql), over the demo's
//! contact graph: one lensed view per renderer, the bound respected and said, a withheld graph
//! said in words, and an update refused before anything runs.

mod common;

use std::sync::Arc;

use futures_executor::block_on;
use ikigai_core::{
    Capability, Description, EndpointSpace, Error, Exact, FnEndpoint, Invocation, Iri, ReprType,
    Representation, Request, Space, Verb,
};
use ikigai_ui::model::{Node, Value};
use ikigai_ui::scene::EXAMPLE_GRAPH;

fn view_iri(lens: &str, iri: &str) -> String {
    format!("urn:iki:ui:view:lens:{lens}:resource:{iri}")
}

fn view(
    kernel: &ikigai_core::Kernel,
    lens: &str,
    iri: &str,
    cap: &Capability,
) -> Result<Node, Error> {
    let r = block_on(kernel.issue(
        Request::new(Verb::Source, Iri::parse(view_iri(lens, iri)).unwrap()),
        cap,
    ))?;
    Ok(serde_json::from_slice(&r.bytes).expect("a model node"))
}

fn all<'a>(n: &'a Node, kind: &str, out: &mut Vec<&'a Node>) {
    if n.kind == kind {
        out.push(n);
    }
    for c in &n.children {
        all(c, kind, out);
    }
}

fn of<'a>(n: &'a Node, kind: &str) -> Vec<&'a Node> {
    let mut out = Vec::new();
    all(n, kind, &mut out);
    out
}

fn text(v: &Value) -> String {
    match v {
        Value::Text(s) => s.clone(),
        other => format!("{other:?}"),
    }
}

fn said(n: &Node) -> String {
    of(n, "text")
        .iter()
        .map(|t| text(&t.properties["content"]))
        .collect::<Vec<_>>()
        .join(" | ")
}

fn fixed(id: &str, media: &'static str, bytes: String) -> FnEndpoint {
    FnEndpoint::new(id, move |_: &Invocation<'_>| {
        Ok(Representation::new(
            ReprType::new(media),
            bytes.clone().into_bytes(),
        ))
    })
    .with_description(
        Description::new(id)
            .title(id)
            .summary(id)
            .verb(Verb::Source)
            .output(media),
    )
}

/// A host's lens `name`, placed ahead of ikigai-ui's.
fn lens(space: EndpointSpace, name: &str, lens: &str, query: &str) -> EndpointSpace {
    space
        .bind(
            Exact::new(format!("urn:iki:ui:lens:name:{name}")),
            fixed(
                &format!("host-lens-{name}"),
                "application/json",
                lens.into(),
            ),
        )
        .bind(
            Exact::new(format!("urn:iki:ui:lens:name:{name}:query")),
            fixed(
                &format!("host-lens-{name}-query"),
                "application/sparql-query",
                query.into(),
            ),
        )
}

const SECRET: &str = "urn:host:graph:secret";
const SECRET_READ: &str = "urn:cap:host:secret:read";

fn host_lenses() -> EndpointSpace {
    let space = lens(
        EndpointSpace::new(),
        "two",
        r#"{"renderer": "graph", "bound": 2, "legend": "Two nodes", "graphs": ["urn:iki:ui:example:graph:contact"]}"#,
        "PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>\nSELECT ?focus ?node ?label WHERE { GRAPH ?focus { ?node rdfs:label ?label } } ORDER BY ?label",
    );
    let space = lens(
        space,
        "secret",
        r#"{"renderer": "table", "bound": 5, "legend": "The secret", "graphs": ["{focus}"]}"#,
        "SELECT ?focus ?s WHERE { GRAPH ?focus { ?s ?p ?o } }",
    );
    let space = lens(
        space,
        "writer",
        r#"{"renderer": "table", "bound": 5, "legend": "A writer"}"#,
        "DELETE WHERE { ?s ?p ?o }",
    );
    space.bind(
        Exact::new(SECRET),
        FnEndpoint::new("host-secret", |_: &Invocation<'_>| {
            Ok(Representation::new(
                ReprType::new("text/turtle"),
                b"<urn:a> <urn:b> <urn:c> .".to_vec(),
            ))
        })
        .with_description(
            Description::new("host-secret")
                .title("A secret graph")
                .summary("A graph only a holder of its capability reads.")
                .verb(Verb::Source)
                .requires(SECRET_READ)
                .output("text/turtle"),
        ),
    )
}

fn host() -> ikigai_core::Kernel {
    common::host_with(None, vec![Arc::new(host_lenses()) as Arc<dyn Space>])
}

#[test]
fn the_table_lens_shows_each_field_of_the_contact_graph() {
    let n = view(
        &common::host(None),
        "contact-table",
        EXAMPLE_GRAPH,
        &Capability::root(),
    )
    .unwrap();
    let tables = of(&n, "table");
    assert_eq!(tables.len(), 1, "{n:?}");
    // A header and one row per field: email, message, name, topic.
    let rows = of(tables[0], "group");
    assert_eq!(rows.len(), 1 + 4, "{n:?}");
    let header: Vec<String> = of(rows[0], "text")
        .iter()
        .map(|t| text(&t.properties["content"]))
        .collect();
    assert_eq!(
        header,
        ["field", "name", "required"],
        "the focus is not a column"
    );
    // Every rendered node keeps its IRI: a field cell links through the home mapping.
    let links: Vec<String> = of(tables[0], "link")
        .iter()
        .map(|l| text(&l.properties["href"]))
        .collect();
    assert!(
        links
            .iter()
            .any(|h| h.contains("example%3Agraph%3Acontact%3Aemail")
                || h.contains("example:graph:contact:email")),
        "{links:?}"
    );
    // The chooser offers the graph's own lenses, then `graph`.
    let options: Vec<String> = of(&n, "option")
        .iter()
        .map(|o| text(&o.properties["value"]))
        .collect();
    assert_eq!(
        options,
        ["contact-graph", "contact-table", "contact-list", "graph"]
    );
}

#[test]
fn the_list_lens_groups_by_type() {
    let n = view(
        &common::host(None),
        "contact-list",
        EXAMPLE_GRAPH,
        &Capability::root(),
    )
    .unwrap();
    let labels: Vec<String> = of(&n, "list")
        .iter()
        .map(|l| text(&l.properties["label"]))
        .collect();
    assert_eq!(labels[1..], ["Fields", "Topics"], "{labels:?}");
    assert_eq!(
        of(&n, "link").len(),
        1 + 4 + 3,
        "the own view, four fields, three topics"
    );
}

#[test]
fn the_graph_lens_is_a_scene_with_the_selection_as_links() {
    for name in ["contact-graph", "graph"] {
        let n = view(
            &common::host(None),
            name,
            EXAMPLE_GRAPH,
            &Capability::root(),
        )
        .unwrap();
        let scene = of(&n, "scene");
        assert_eq!(scene.len(), 1, "{name}");
        assert!(
            scene[0].replacement.is_some(),
            "{name}: a scene says itself in words"
        );
        let selection = of(&n, "list");
        assert_eq!(
            of(selection[0], "link").len(),
            8,
            "{name}: the graph's eight labeled nodes"
        );
    }
}

#[test]
fn past_its_bound_a_lens_shows_the_first_rows_and_says_so() {
    let n = view(&host(), "two", EXAMPLE_GRAPH, &Capability::root()).unwrap();
    assert_eq!(of(of(&n, "list")[0], "link").len(), 2);
    let words = said(&n);
    assert!(words.contains("first 2"), "{words}");
}

#[test]
fn a_withheld_graph_is_said_in_words() {
    let host = host();
    let cap = Capability::scoped(["urn:cap:nothing"]);
    let n = view(&host, "secret", SECRET, &cap).unwrap();
    let words = said(&n);
    assert!(words.contains("withheld"), "{words}");
    assert!(of(&n, "table").is_empty(), "never an empty renderer");
    // The holder of the capability reads it.
    let n = view(&host, "secret", SECRET, &Capability::root()).unwrap();
    assert_eq!(of(&n, "table").len(), 1, "{n:?}");
}

#[test]
fn an_update_is_refused_before_it_runs() {
    let e = view(&host(), "writer", EXAMPLE_GRAPH, &Capability::root()).unwrap_err();
    assert!(matches!(e, Error::InvalidArgument { .. }), "{e:?}");
}
