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

/// The scene a lensed view's `scene` node names, read as the page would read it.
fn scene_of(kernel: &ikigai_core::Kernel, n: &Node) -> ikigai_ui::scene::Scene {
    let scene = of(n, "scene")[0];
    let datum = text(&scene.properties["datum"]);
    let r = block_on(kernel.issue(
        Request::new(Verb::Source, Iri::parse(datum).unwrap()),
        &Capability::root(),
    ))
    .unwrap();
    let s: ikigai_ui::scene::Scene = serde_json::from_slice(&r.bytes).unwrap();
    ikigai_ui::scene::validate(&s).unwrap();
    s
}

const TOPICS: &str = "PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
PREFIX schema: <http://schema.org/>
CONSTRUCT { ?field schema:valueReference ?topic . ?field rdfs:label ?f . ?topic rdfs:label ?t }
WHERE { GRAPH ?focus { ?field schema:valueReference ?topic . ?field rdfs:label ?f . ?topic rdfs:label ?t } }";

#[test]
fn a_construct_lens_draws_exactly_its_result() {
    let host = common::host_with(
        None,
        vec![Arc::new(lens(
            EndpointSpace::new(),
            "topics",
            r#"{"renderer": "graph", "bound": 10, "legend": "The topics", "graphs": ["urn:iki:ui:example:graph:contact"]}"#,
            TOPICS,
        )) as Arc<dyn Space>],
    );
    let n = view(&host, "topics", EXAMPLE_GRAPH, &Capability::root()).unwrap();
    let scene = scene_of(&host, &n);
    // The scene is the lens's result: the topic field and its three topics, no more.
    let ids: Vec<&str> = scene.nodes.iter().map(|n| n.id.as_str()).collect();
    let c = |s: &str| format!("urn:iki:ui:example:graph:contact:{s}");
    assert_eq!(
        ids,
        [c("bug"), c("other"), c("question"), c("topic")],
        "{scene:?}"
    );
    let labels: Vec<&str> = scene.nodes.iter().map(|n| n.label.as_str()).collect();
    assert_eq!(labels, ["bug", "other", "question", "topic"]);
    assert_eq!(scene.edges.len(), 3, "{:?}", scene.edges);
    assert!(scene
        .edges
        .iter()
        .all(|e| e.from == c("topic") && e.label == "valueReference"));
    // The selection is the same nodes, as links.
    let selection = of(&n, "list");
    let links: Vec<String> = of(selection[0], "link")
        .iter()
        .map(|l| text(&l.properties["label"]))
        .collect();
    assert_eq!(links, ["bug", "other", "question", "topic"]);
    // The replacement names every edge of it: from, edge, to.
    let replacement = of(&n, "scene")[0].replacement.as_deref().unwrap();
    let rows: Vec<Vec<String>> = of(replacement, "group")
        .iter()
        .filter(|g| g.children.len() == 3)
        .map(|g| {
            of(g, "text")
                .iter()
                .map(|t| text(&t.properties["content"]))
                .collect()
        })
        .collect();
    assert_eq!(
        rows,
        [
            ["From", "Edge", "To"],
            ["topic", "valueReference", "bug"],
            ["topic", "valueReference", "other"],
            ["topic", "valueReference", "question"],
        ],
        "{replacement:?}"
    );
}

fn view_with(
    kernel: &ikigai_core::Kernel,
    lens: &str,
    iri: &str,
    args: &[(&str, &str)],
) -> Result<Node, Error> {
    let mut req = Request::new(Verb::Source, Iri::parse(view_iri(lens, iri)).unwrap());
    for (k, v) in args {
        req = req.with_arg(*k, ikigai_core::ArgRef::Inline(v.as_bytes().to_vec()));
    }
    let r = block_on(kernel.issue(req, &Capability::root()))?;
    Ok(serde_json::from_slice(&r.bytes).expect("a model node"))
}

const ENUMERATION: &str = "http://schema.org/Enumeration";

/// Ledger #3531: a facet's value is bound into the query as a term, and changes the result.
#[test]
fn a_facet_changes_the_result_and_is_its_own_cached_result() {
    let host = common::host(None);
    let all = view_with(&host, "contact-list", EXAMPLE_GRAPH, &[]).unwrap();
    let topics = view_with(
        &host,
        "contact-list",
        EXAMPLE_GRAPH,
        &[("type", ENUMERATION)],
    )
    .unwrap();
    let lists = |n: &Node| -> Vec<String> {
        of(n, "list")
            .iter()
            .map(|l| text(&l.properties["label"]))
            .collect()
    };
    assert_eq!(lists(&all)[1..], ["Fields", "Topics"]);
    assert_eq!(lists(&topics)[1..], ["Topics"], "{topics:?}");
    // The facets are checkboxes, the chosen one checked, in a GET form.
    let boxes = of(&topics, "field");
    assert_eq!(boxes.len(), 2);
    assert!(boxes
        .iter()
        .all(|f| text(&f.properties["input"]) == "checkbox"));
    let checked: Vec<String> = boxes
        .iter()
        .filter(|f| f.hint.contains_key(ikigai_ui::lens::CHECKED))
        .map(|f| text(&f.properties["value"]))
        .collect();
    assert_eq!(checked, [ENUMERATION]);
    assert!(said(&topics).contains("Showing only"), "{}", said(&topics));
    // Each combination is its own result resource, cacheable, and answers its own rows.
    let result = |args: &[(&str, &str)]| {
        let mut req = Request::new(
            Verb::Source,
            Iri::parse(format!(
                "urn:iki:ui:lens:name:contact-list:focus:{EXAMPLE_GRAPH}"
            ))
            .unwrap(),
        );
        for (k, v) in args {
            req = req.with_arg(*k, ikigai_core::ArgRef::Inline(v.as_bytes().to_vec()));
        }
        block_on(host.issue(req, &Capability::root())).unwrap()
    };
    let (a, b) = (result(&[]), result(&[("type", ENUMERATION)]));
    assert!(a.expiry == ikigai_core::Expiry::Never && b.expiry == ikigai_core::Expiry::Never);
    assert_ne!(a.bytes, b.bytes);
    assert_eq!(b.bytes, result(&[("type", ENUMERATION)]).bytes);
}

#[test]
fn a_facet_value_outside_its_allowed_set_is_refused() {
    let host = common::host(None);
    for args in [
        &[("type", "http://schema.org/Person")][..],
        &[("type", "} ; DROP ALL")][..],
        &[("colour", "red")][..],
    ] {
        let e = view_with(&host, "contact-list", EXAMPLE_GRAPH, args).unwrap_err();
        assert!(
            matches!(e, Error::InvalidArgument { .. }),
            "{args:?}: {e:?}"
        );
    }
}

fn resource_view(kernel: &ikigai_core::Kernel, iri: &str) -> Node {
    let r = block_on(kernel.issue(
        Request::new(
            Verb::Source,
            Iri::parse(format!("urn:iki:ui:view:resource:{iri}")).unwrap(),
        ),
        &Capability::root(),
    ))
    .unwrap();
    serde_json::from_slice(&r.bytes).unwrap()
}

#[test]
fn a_resource_with_lenses_offers_them_and_one_without_does_not() {
    let host = common::host(None);
    let n = resource_view(&host, EXAMPLE_GRAPH);
    let choices = of(&n, "choice");
    let lenses: Vec<&&Node> = choices
        .iter()
        .filter(|c| text(&c.properties["name"]) == "lens")
        .collect();
    assert_eq!(lenses.len(), 1, "{n:?}");
    let options: Vec<String> = of(lenses[0], "option")
        .iter()
        .map(|o| text(&o.properties["value"]))
        .collect();
    assert_eq!(
        options,
        ["contact-graph", "contact-table", "contact-list", "graph"]
    );
    let form = of(&n, "form")
        .into_iter()
        .find(|f| text(&f.properties["label"]) == "Lenses")
        .unwrap();
    assert!(text(&form.properties["action"])
        .starts_with("/ui/html:screen:lens:contact-graph:resource:"));
    // A resource that declares no lenses is offered none.
    let other = resource_view(&host, "urn:iki:ui:example:graph:contact:email");
    assert!(
        of(&other, "choice")
            .iter()
            .all(|c| text(&c.properties["name"]) != "lens"),
        "{other:?}"
    );
}

fn html(kernel: &ikigai_core::Kernel, screen: &str, args: &[(&str, &str)]) -> String {
    let mut req = Request::new(
        Verb::Source,
        Iri::parse(format!("urn:iki:ui:html:screen:{screen}")).unwrap(),
    );
    for (k, v) in args {
        req = req.with_arg(*k, ikigai_core::ArgRef::Inline(v.as_bytes().to_vec()));
    }
    let r = block_on(kernel.issue(req, &Capability::root())).unwrap();
    String::from_utf8(r.bytes).unwrap()
}

/// Whether a checkbox in `page` starts checked (the lens chooser's radios are not checkboxes).
fn checked_box(page: &str) -> bool {
    page.split("<input ")
        .skip(1)
        .map(|tag| &tag[..tag.find('>').unwrap()])
        .any(|tag| tag.contains("type=\"checkbox\"") && tag.contains(" checked"))
}

/// The html face opens a lensed view, and its form's parameters pick another lens and facets.
#[test]
fn a_lensed_view_has_an_html_route() {
    let host = common::host(None);
    let screen = format!("lens:contact-list:resource:{EXAMPLE_GRAPH}");
    let page = html(&host, &screen, &[]);
    assert!(page.contains("type=\"checkbox\""), "{page}");
    assert!(page.contains("name=\"lens\""), "{page}");
    assert!(
        page.contains(">Topics<") || page.contains("Topics"),
        "{page}"
    );
    assert!(!checked_box(&page), "{page}");
    let page = html(
        &host,
        &screen,
        &[("lens", "contact-list"), ("type", ENUMERATION)],
    );
    assert!(checked_box(&page), "{page}");
    assert!(page.contains("Showing only"), "{page}");
    let page = html(&host, &screen, &[("lens", "contact-table")]);
    assert!(
        page.contains("The contact service&#x27;s fields") || page.contains("fields"),
        "{page}"
    );
    assert!(!page.contains("type=\"checkbox\""), "{page}");
}

/// The radio buttons of the choice named `name` in `page`, as (value, checked).
fn radios(page: &str, name: &str) -> Vec<(String, bool)> {
    page.split("<input ")
        .skip(1)
        .map(|tag| &tag[..tag.find('>').unwrap()])
        .filter(|tag| tag.contains("type=\"radio\"") && tag.contains(&format!("name=\"{name}\"")))
        .map(|tag| {
            let value = tag.split("value=\"").nth(1).unwrap();
            let value = value[..value.find('"').unwrap()].to_string();
            (value, tag.contains(" checked"))
        })
        .collect()
}

/// The lens chooser is one radio group (ledger #3537): a fieldset with its legend, a radio per
/// lens, the shown lens checked, and no select. Native radios move with the arrow keys.
#[test]
fn a_lensed_views_chooser_is_radio_buttons_in_a_fieldset() {
    let host = common::host(None);
    let screen = format!("lens:contact-list:resource:{EXAMPLE_GRAPH}");
    let page = html(&host, &screen, &[]);
    assert!(page.contains("<fieldset"), "{page}");
    assert!(page.contains(">Lens</legend>"), "{page}");
    assert!(!page.contains("<select"), "{page}");
    let lenses = radios(&page, "lens");
    assert_eq!(
        lenses,
        [
            ("contact-graph".to_string(), false),
            ("contact-table".to_string(), false),
            ("contact-list".to_string(), true),
            ("graph".to_string(), false),
        ],
        "{page}"
    );
    // Each radio has its own label, tied by id.
    for (value, _) in &lenses {
        assert!(page.contains(&format!(">{value}</label>")), "{page}");
    }
    let page = html(&host, &screen, &[("lens", "contact-table")]);
    let checked: Vec<String> = radios(&page, "lens")
        .into_iter()
        .filter(|(_, c)| *c)
        .map(|(v, _)| v)
        .collect();
    assert_eq!(checked, ["contact-table"], "{page}");
}

/// A resource's own view offers its lenses the same way: the chooser carries the hint, and
/// the html face draws it as radios.
#[test]
fn a_resource_views_lens_chooser_is_radio_buttons() {
    let host = common::host(None);
    let n = resource_view(&host, EXAMPLE_GRAPH);
    let form = of(&n, "form")
        .into_iter()
        .find(|f| text(&f.properties["label"]) == "Lenses")
        .unwrap()
        .clone();
    let chooser = of(&form, "choice")[0];
    assert_eq!(
        chooser.hint.get(ikigai_ui::model::RADIO),
        Some(&ikigai_ui::model::Hint::Plain(Value::Text("radio".into()))),
        "{chooser:?}"
    );
    let bytes = serde_json::to_vec(&form).unwrap();
    let screen = EndpointSpace::new().bind(
        Exact::new("urn:iki:ui:screen:name:resource-lenses"),
        fixed(
            "resource-lenses",
            "application/json",
            String::from_utf8(bytes).unwrap(),
        ),
    );
    let host = common::host_with(None, vec![Arc::new(screen) as Arc<dyn Space>]);
    let page = html(&host, "resource-lenses", &[]);
    assert!(page.contains(">Lens</legend>"), "{page}");
    assert!(!page.contains("<select"), "{page}");
    assert_eq!(
        radios(&page, "lens"),
        [
            ("contact-graph".to_string(), true),
            ("contact-table".to_string(), false),
            ("contact-list".to_string(), false),
            ("graph".to_string(), false),
        ],
        "{page}"
    );
}

/// An html descriptor that does not claim the choice hints keeps the select.
#[test]
fn a_face_that_does_not_claim_the_hint_keeps_the_select() {
    let dir = common::scratch("choice-unclaimed");
    std::fs::create_dir_all(dir.join("target")).unwrap();
    let mut descriptor: serde_json::Value =
        serde_json::from_slice(include_bytes!("../assets/target/html.json")).unwrap();
    descriptor.as_object_mut().unwrap().remove("hints");
    std::fs::write(
        dir.join("target/html.json"),
        serde_json::to_vec(&descriptor).unwrap(),
    )
    .unwrap();
    let host = common::host(Some(&dir));
    let screen = format!("lens:contact-list:resource:{EXAMPLE_GRAPH}");
    let page = html(&host, &screen, &[]);
    assert!(page.contains("<select"), "{page}");
    assert!(
        page.contains("<option value=\"contact-list\" selected>"),
        "{page}"
    );
    assert!(radios(&page, "lens").is_empty(), "{page}");
    assert!(!page.contains("<fieldset"), "{page}");
}
