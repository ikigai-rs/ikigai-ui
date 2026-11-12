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
        [
            "contact-graph",
            "contact-table",
            "contact-list",
            "contact-dashboard",
            "shacl-violations",
            "graph"
        ]
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
        [
            "contact-graph",
            "contact-table",
            "contact-list",
            "contact-dashboard",
            "shacl-violations",
            "graph"
        ]
    );
    let form = of(&n, "form")
        .into_iter()
        .find(|f| text(&f.properties["label"]) == "Lenses")
        .unwrap();
    // It opens the lens as a page in the shell (ledger #3538), the IRI one segment.
    assert_eq!(
        text(&form.properties["action"]),
        "/lens/name/contact-graph/resource/urn%3Aiki%3Aui%3Aexample%3Agraph%3Acontact"
    );
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
            ("contact-dashboard".to_string(), false),
            ("shacl-violations".to_string(), false),
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
            ("contact-dashboard".to_string(), false),
            ("shacl-violations".to_string(), false),
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

fn page(
    kernel: &ikigai_core::Kernel,
    iri: &str,
    args: &[(&str, &str)],
) -> Result<String, ikigai_core::Error> {
    let mut req = Request::new(Verb::Source, Iri::parse(iri).unwrap());
    for (k, v) in args {
        req = req.with_arg(*k, ikigai_core::ArgRef::Inline(v.as_bytes().to_vec()));
    }
    let r = block_on(kernel.issue(req, &Capability::root()))?;
    Ok(String::from_utf8(r.bytes).unwrap())
}

/// A lensed view is a full page in the shell (ledger #3538): title, theme chooser, status
/// region, tokens.css, shell.js and scene-renderer.js, so the graph lens's canvas draws.
#[test]
fn a_lensed_view_is_a_page_in_the_shell() {
    let host = common::host(None);
    // As the door gives it: the IRI percent-encoded as one segment.
    let encoded = ikigai_ui::lens::html_href("contact-graph", EXAMPLE_GRAPH, &Default::default());
    let rest = encoded
        .strip_prefix("/lens/name/contact-graph/resource/")
        .unwrap();
    let iri = format!("urn:demo:page:lens:contact-graph:resource:{rest}");
    let p = page(&host, &iri, &[("theme", "dark")]).unwrap();
    for shell in [
        "<!doctype html>",
        "data-theme=\"dark\"",
        "<link rel=\"stylesheet\" href=\"/ui/tokens.css\">",
        "<script src=\"/ui/shell.js\"></script>",
        "<script src=\"/ui/scene-renderer.js\" defer></script>",
        "<header>",
        "<div id=\"status\" role=\"status\" aria-live=\"polite\"></div>",
        "<main id=\"main\" tabindex=\"-1\">",
        "name=\"theme\"",
    ] {
        assert!(p.contains(shell), "{shell} not in {p}");
    }
    let title = p
        .split("<title>")
        .nth(1)
        .unwrap()
        .split("</title>")
        .next()
        .unwrap();
    assert!(title.ends_with(&format!(": {EXAMPLE_GRAPH}")), "{title}");
    assert!(
        !title.starts_with("contact-graph"),
        "the legend, not the name: {title}"
    );
    // The lensed view itself, its scene among it.
    let main = p
        .split("<main id=\"main\" tabindex=\"-1\">")
        .nth(1)
        .unwrap();
    assert!(main.contains("<canvas"), "{main}");
    assert!(main.contains("name=\"lens\""), "{main}");
    // Unencoded, the same page.
    let plain = page(
        &host,
        &format!("urn:demo:page:lens:contact-graph:resource:{EXAMPLE_GRAPH}"),
        &[("theme", "dark")],
    )
    .unwrap();
    assert_eq!(plain, p);
    // The view's own form (no action) submits to the page: lens= and facets pick another.
    let list = page(
        &host,
        &iri,
        &[("lens", "contact-list"), ("type", ENUMERATION)],
    )
    .unwrap();
    assert!(list.contains("<main id=\"main\""), "{list}");
    assert!(list.contains(" checked"), "{list}");
    assert!(list.contains("Showing only"), "{list}");
    // A resource that is no single IRI, or a lens that is no name, is refused.
    for bad in [
        "urn:demo:page:lens:contact-graph:resource:urn%3Ax%3Fy",
        "urn:demo:page:lens:contact-graph:resource:urn%3Ax%23y",
        "urn:demo:page:lens:contact-graph:resource:not%20an%20iri",
    ] {
        assert!(
            matches!(
                page(&host, bad, &[]),
                Err(ikigai_core::Error::InvalidArgument { .. })
            ),
            "{bad}"
        );
    }
    // A family that did not turn lenses on serves no lensed page.
    let plain_family = ikigai_core::Kernel::new(Arc::new(ikigai_core::Fallback::new(vec![
        Arc::new(ikigai_ui::space()),
        Arc::new(ikigai_ui::Pages::new("x", "urn:x:").build().unwrap()),
    ])));
    assert!(page(
        &plain_family,
        &format!("urn:x:page:lens:graph:resource:{EXAMPLE_GRAPH}"),
        &[]
    )
    .is_err());
}

#[test]
fn stage_two_named_renderers_are_available_over_demo_queries() {
    let kernel = common::host(None);
    for name in ["thread", "board", "tree", "timeline"] {
        let n = view(
            &kernel,
            &format!("demo-{name}"),
            ikigai_ui::lens::DEMO_GRAPH,
            &Capability::root(),
        )
        .unwrap_or_else(|e| panic!("{name} lens must render: {e}"));
        assert!(
            !of(&n, "link").is_empty(),
            "{name} retains linked selections"
        );
    }
}

fn labeled<'a>(n: &'a Node, label: &str) -> &'a Node {
    fn find<'a>(n: &'a Node, label: &str) -> Option<&'a Node> {
        if n.properties.get("label").is_some_and(|v| text(v) == label) {
            return Some(n);
        }
        n.children.iter().find_map(|c| find(c, label))
    }
    find(n, label).unwrap_or_else(|| panic!("missing {label}: {n:?}"))
}

fn labels(n: &Node) -> Vec<String> {
    of(n, "link")
        .iter()
        .map(|n| text(&n.properties["label"]))
        .collect()
}

fn demo_view(kernel: &ikigai_core::Kernel, name: &str, args: &[(&str, &str)]) -> Node {
    let mut req = Request::new(
        Verb::Source,
        Iri::parse(view_iri(name, ikigai_ui::lens::DEMO_GRAPH)).unwrap(),
    );
    for (k, v) in args {
        req = req.with_arg(*k, ikigai_core::ArgRef::Inline(v.as_bytes().to_vec()));
    }
    let rep = block_on(kernel.issue(req, &Capability::root())).unwrap();
    serde_json::from_slice(&rep.bytes).unwrap()
}

#[test]
fn a_long_thread_keeps_its_root_first_and_last_replies_and_participants() {
    let n = demo_view(&common::host(None), "demo-thread", &[]);
    let threads = labeled(&n, "Threads");
    let links = labels(threads);
    assert!(links.contains(&"How should we render results?".into()));
    assert!(links.contains(&"Use named templates".into()));
    assert!(links.contains(&"Ready for review".into()));
    assert!(!links.contains(&"Keep the links".into()));
    assert!(said(threads).contains("4 replies"));
    let mut participants = labels(labeled(&n, "Participants"));
    participants.sort();
    assert_eq!(participants, ["Ada", "Brian"]);
    assert_eq!(
        labels(labeled(&n, "Selection: resources in this answer")).len(),
        6
    );
}

#[test]
fn board_columns_page_and_a_facet_is_bound_as_a_term() {
    let host = common::host(None);
    let first = demo_view(&host, "demo-board", &[]);
    let second = demo_view(&host, "demo-board", &[("page", "2")]);
    let first_todo = labels(labeled(&first, "Todo"));
    let second_todo = labels(labeled(&second, "Todo"));
    assert!(first_todo.contains(&"Thread renderer".into()));
    assert!(!first_todo.contains(&"Timeline renderer".into()));
    assert!(second_todo.contains(&"Timeline renderer".into()));
    assert!(!second_todo.contains(&"Thread renderer".into()));
    assert!(said(labeled(&second, "Todo")).contains("Page 2 of 2; 4 cards"));
    assert!(labels(labeled(&second, "Doing")).is_empty());
    let filtered = demo_view(
        &host,
        "demo-board",
        &[("onlyStatus", "Todo"), ("page", "2")],
    );
    assert_eq!(
        labels(labeled(&filtered, "Selection: resources in this answer")).len(),
        4
    );
    assert!(!format!("{filtered:?}").contains("Review accessibility"));
    let screen = format!("lens:demo-board:resource:{}", ikigai_ui::lens::DEMO_GRAPH);
    let page = html(
        &host,
        &screen,
        &[
            ("lens", "demo-board"),
            ("page", "2"),
            ("onlyStatus", "Todo"),
        ],
    );
    assert!(page.contains("Page 2 of 2"));
    assert!(checked_box(&page));
    assert!(page.contains("name=\"page\""));
    assert!(
        !page.contains("hx-get="),
        "lens GET controls navigate the page"
    );
    assert!(page.contains("ui-columns"));
    assert!(page.contains("Timeline renderer"));
}

#[test]
fn a_tree_counts_all_descendants_and_discloses_its_display_bound() {
    let n = demo_view(&common::host(None), "demo-tree", &[]);
    let tree = labeled(&n, "Hierarchy");
    assert!(said(tree).contains("5 descendants in this answer"));
    assert!(said(tree).contains("3 descendants in this answer"));
    assert!(said(tree).contains("Hierarchy bound reached"));
    assert_eq!(
        labels(labeled(&n, "Selection: resources in this answer")).len(),
        6
    );
}

#[test]
fn timeline_days_and_rows_follow_time() {
    let n = demo_view(&common::host(None), "demo-timeline", &[]);
    assert_eq!(
        labels(labeled(&n, "2026-11-10")),
        [
            "How should we render results?",
            "Use named templates",
            "Keep the links"
        ]
    );
    assert_eq!(
        labels(labeled(&n, "2026-11-11")),
        [
            "Group the work by status",
            "Ready for review",
            "A second discussion"
        ]
    );
}

#[test]
fn every_structured_renderer_says_when_its_graph_is_withheld() {
    let mut space = host_lenses();
    for renderer in ["thread", "board", "tree", "timeline"] {
        space = lens(
            space,
            &format!("secret-{renderer}"),
            &format!(
                r#"{{"renderer":"{renderer}","bound":3,"legend":"Private","graphs":["{SECRET}"]}}"#
            ),
            "SELECT ?focus ?item WHERE { GRAPH ?focus { ?item ?p ?o } }",
        );
    }
    let kernel = common::host_with(None, vec![Arc::new(space)]);
    for renderer in ["thread", "board", "tree", "timeline"] {
        let n = view(
            &kernel,
            &format!("secret-{renderer}"),
            SECRET,
            &Capability::scoped(std::iter::empty::<&str>()),
        )
        .unwrap();
        assert!(said(&n).contains("withheld"), "{renderer}: {n:?}");
    }
}

#[test]
fn lensed_page_links_round_trip_for_native_navigation() {
    use ikigai_ui::lens::{html_href, screen_from_href, screen_name, Selection};
    let chosen = Selection::from([
        ("page".into(), "2".into()),
        ("onlyStatus".into(), "Todo".into()),
    ]);
    let href = html_href("demo-board", ikigai_ui::lens::DEMO_GRAPH, &chosen);
    assert_eq!(
        screen_from_href(&href).unwrap(),
        screen_name("demo-board", ikigai_ui::lens::DEMO_GRAPH, &chosen)
    );
    for bad in [
        "https://evil.example",
        "/lens/name/../resource/urn:x",
        "/lens/name/tree/resource/%zz",
        "/lens/name/tree/resource/urn:x%3Fy",
    ] {
        assert!(screen_from_href(bad).is_err(), "{bad}");
    }
}

#[test]
fn every_structured_selection_and_related_iri_uses_the_home_mapping() {
    let homes = EndpointSpace::new().bind(
        Exact::new(ikigai_ui::lens::HOME_TABLE),
        fixed(
            "demo-homes",
            "application/json",
            r#"[{"prefix":"urn:iki:ui:example:lenses:","href":"https://example.com/resources/"}]"#
                .into(),
        ),
    );
    let kernel = common::host_with(None, vec![Arc::new(homes)]);
    for name in ["demo-thread", "demo-board", "demo-tree", "demo-timeline"] {
        let n = demo_view(&kernel, name, &[]);
        let selection = labeled(&n, "Selection: resources in this answer");
        assert!(of(selection, "link")
            .iter()
            .all(|n| text(&n.properties["href"]).starts_with("https://example.com/resources/")));
        for link in of(&n, "link").into_iter().filter(|l| {
            text(&l.properties["label"]).starts_with("Assignee:")
                || text(&l.properties["label"]).starts_with("Related:")
        }) {
            assert!(text(&link.properties["href"]).starts_with("https://example.com/resources/"));
        }
    }
}

// Lenses 3 (ledger #3520): a lens whose renderer is `construct` builds the UI model itself.

/// The rendered view's body: the node after the lensed view's heading, link and form.
fn body(n: &Node) -> &Node {
    n.children.last().expect("the renderer's node")
}

#[test]
fn a_construct_lens_builds_the_dashboard_as_the_model() {
    let n = view(
        &common::host(None),
        "contact-dashboard",
        EXAMPLE_GRAPH,
        &Capability::root(),
    )
    .unwrap();
    let board = body(&n);
    assert_eq!(board.kind, "group", "{board:?}");
    assert_eq!(text(&board.properties["label"]), "At a glance");
    // Children in their stated order: the table, then the line under it.
    let kinds: Vec<&str> = board.children.iter().map(|c| c.kind.as_str()).collect();
    assert_eq!(kinds, ["table", "text"]);
    let table = &board.children[0];
    let rows: Vec<Vec<String>> = table
        .children
        .iter()
        .map(|g| {
            g.children
                .iter()
                .map(|c| {
                    text(
                        c.properties
                            .get("content")
                            .unwrap_or_else(|| &c.properties["label"]),
                    )
                })
                .collect()
        })
        .collect();
    assert_eq!(
        rows,
        [
            ["Type", "Count"],
            ["Enumeration", "3"],
            ["PropertyValueSpecification", "4"],
            ["Service", "1"],
        ],
        "the header first, then the rows by IRI: {table:?}"
    );
    assert_eq!(text(&table.children[0].properties["header"]), "column");
    // Every rendered type keeps its IRI and links through the home mapping.
    let hrefs: Vec<String> = of(table, "link")
        .iter()
        .map(|l| text(&l.properties["href"]))
        .collect();
    assert_eq!(
        hrefs,
        [
            ikigai_ui::view::href("http://schema.org/Enumeration"),
            ikigai_ui::view::href("http://schema.org/PropertyValueSpecification"),
            ikigai_ui::view::href("http://schema.org/Service"),
        ]
    );
    assert_eq!(
        text(&board.children[1].properties["content"]),
        "3 of 4 fields are required."
    );
}

/// A host whose lens `name` is a `construct` over `query`, bound `bound`.
fn construct_host(name: &str, bound: usize, query: &str) -> ikigai_core::Kernel {
    let mut space = lens(
        EndpointSpace::new(),
        name,
        &format!(
            r#"{{"renderer": "construct", "bound": {bound}, "legend": "Built", "graphs": ["{{focus}}"]}}"#
        ),
        query,
    );
    space = space.bind(
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
    );
    common::host_with(None, vec![Arc::new(space) as Arc<dyn Space>])
}

const PREFIXES: &str =
    "PREFIX kind: <urn:iki:ui:kind:name:>\nPREFIX ui: <urn:iki:ui:property:name:>\n";
/// The store refuses a binding its query never names, so each names the focus.
const FOCUS: &str = "OPTIONAL { GRAPH ?focus { ?focus ?seen ?it } }";

#[test]
fn a_constructed_model_is_validated_as_any_screen_is() {
    // A heading with no level, a kind that is not one, a property the context does not name,
    // a link whose href is script, a table whose child is not a group, two roots, a cycle: each
    // is a screen no screen could be, refused, never shown.
    for (what, template) in [
        ("a missing required property", "<urn:x:h> a kind:heading ; ui:content \"Hi\" ."),
        ("not a kind", "<urn:x:h> a <urn:x:widget> ; ui:content \"Hi\" ."),
        ("an unnamed predicate", "<urn:x:h> a kind:text ; ui:content \"Hi\" ; <urn:x:p> \"x\" ."),
        ("an unsafe href", "<urn:x:h> a kind:link ; ui:label \"Go\" ; ui:href \"javascript:alert(1)\" ."),
        ("a wrong child", "<urn:x:t> a kind:table ; ui:caption \"T\" ; ui:child <urn:x:c> . <urn:x:c> a kind:text ; ui:content \"c\" ."),
        ("two roots", "<urn:x:a> a kind:text ; ui:content \"a\" . <urn:x:b> a kind:text ; ui:content \"b\" ."),
        ("a cycle", "<urn:x:r> a kind:list ; ui:label \"r\" . <urn:x:a> a kind:list ; ui:label \"a\" ; ui:child <urn:x:b> . <urn:x:b> a kind:list ; ui:label \"b\" ; ui:child <urn:x:a> ."),
        ("a shared child", "<urn:x:r> a kind:list ; ui:label \"r\" ; ui:child <urn:x:a>, <urn:x:b> . <urn:x:a> a kind:list ; ui:label \"a\" ; ui:child <urn:x:c> . <urn:x:b> a kind:list ; ui:label \"b\" ; ui:child <urn:x:c> . <urn:x:c> a kind:text ; ui:content \"c\" ."),
        ("a child not constructed", "<urn:x:r> a kind:list ; ui:label \"r\" ; ui:child <urn:x:gone> ."),
    ] {
        let q = format!("{PREFIXES}CONSTRUCT {{ {template} }} WHERE {{ {FOCUS} }}");
        let e = view(&construct_host("bad", 20, &q), "bad", EXAMPLE_GRAPH, &Capability::root())
            .unwrap_err();
        assert!(
            matches!(&e, Error::InvalidArgument { name, .. } if name == "construct" || name == "model"),
            "{what}: {e:?}"
        );
    }
    // The same vocabulary, well formed, is shown: a heading's level is its integer.
    let q = format!("{PREFIXES}CONSTRUCT {{ <urn:x:h> a kind:heading ; ui:content \"Hi\" ; ui:level 3 . }} WHERE {{ {FOCUS} }}");
    let n = view(
        &construct_host("ok", 20, &q),
        "ok",
        EXAMPLE_GRAPH,
        &Capability::root(),
    )
    .unwrap();
    assert_eq!(body(&n).kind, "heading");
    assert_eq!(body(&n).properties["level"], Value::Number(3));
}

#[test]
fn past_its_bound_a_construct_lens_shows_nothing_and_says_so() {
    let q = format!(
        "{PREFIXES}CONSTRUCT {{ <urn:x:r> a kind:list ; ui:label \"r\" ; ui:child ?i . ?i a kind:text ; ui:content ?o }}
         WHERE {{ {FOCUS} VALUES ?o {{ \"a\" \"b\" \"c\" \"d\" }} BIND(IRI(CONCAT(\"urn:x:i:\", ?o)) AS ?i) }}"
    );
    let n = view(
        &construct_host("many", 3, &q),
        "many",
        EXAMPLE_GRAPH,
        &Capability::root(),
    )
    .unwrap();
    let words = said(&n);
    assert!(
        words.contains("constructed 5 nodes") && words.contains("bound of 3"),
        "{words}"
    );
    assert!(of(&n, "list").is_empty(), "whole or not at all: {n:?}");
    // Within its bound, the whole model.
    let n = view(
        &construct_host("many", 5, &q),
        "many",
        EXAMPLE_GRAPH,
        &Capability::root(),
    )
    .unwrap();
    let list = body(&n);
    let items: Vec<String> = list
        .children
        .iter()
        .map(|c| text(&c.properties["content"]))
        .collect();
    assert_eq!(items, ["a", "b", "c", "d"]);
}

#[test]
fn a_construct_lens_over_a_withheld_graph_says_so_in_words() {
    let q = format!("{PREFIXES}CONSTRUCT {{ ?s a kind:text ; ui:content \"seen\" }} WHERE {{ GRAPH ?focus {{ ?s ?p ?o }} }}");
    let host = construct_host("peek", 10, &q);
    let n = view(
        &host,
        "peek",
        SECRET,
        &Capability::scoped(["urn:cap:nothing"]),
    )
    .unwrap();
    assert!(said(&n).contains("withheld"), "{}", said(&n));
    let n = view(&host, "peek", SECRET, &Capability::root()).unwrap();
    assert_eq!(text(&body(&n).properties["content"]), "seen");
}

#[test]
fn a_construct_lens_is_a_construct_and_read_only() {
    for q in [
        "SELECT ?s WHERE { ?s ?p ?o }",
        "DELETE WHERE { ?s ?p ?o }",
        "PREFIX kind: <urn:iki:ui:kind:name:>\nCONSTRUCT { ?s a kind:text } WHERE { SERVICE <http://evil.example/sparql> { ?s ?p ?o } }",
    ] {
        let e = view(&construct_host("odd", 10, q), "odd", EXAMPLE_GRAPH, &Capability::root())
            .unwrap_err();
        assert!(matches!(e, Error::InvalidArgument { .. }), "{q}: {e:?}");
    }
}

// Lenses 4 (ledger #3521): SHACL violations, and asserted beside derived.

const ROLE: &str = ikigai_ui::model::ROLE;

fn role(n: &Node) -> Option<&'static str> {
    ikigai_ui::model::role(n)
}

fn label(n: &Node) -> String {
    text(&n.properties["label"])
}

/// A table's rows past its header, each as its cells' words.
fn rows(table: &Node) -> Vec<(&Node, Vec<String>)> {
    table
        .children
        .iter()
        .skip(1)
        .map(|row| {
            let cells = row
                .children
                .iter()
                .map(|c| match c.kind.as_str() {
                    "link" => label(c),
                    _ => text(&c.properties["content"]),
                })
                .collect();
            (row, cells)
        })
        .collect()
}

#[test]
fn the_shacl_lens_marks_each_failing_node_by_the_danger_role_and_in_words() {
    let n = view(
        &common::host(None),
        "shacl-violations",
        EXAMPLE_GRAPH,
        &Capability::root(),
    )
    .unwrap();
    // The table form: one row per failure, each in the danger role AND saying what failed.
    let tables = of(&n, "table");
    assert_eq!(tables.len(), 1, "{n:?}");
    let rows = rows(tables[0]);
    let mut nodes: Vec<&str> = rows.iter().map(|(_, c)| c[0].as_str()).collect();
    nodes.sort();
    // `topic` says nothing of being required; its three topics have no description. The
    // fields that conform (name, email, message) are not here.
    assert_eq!(nodes, ["bug", "other", "question", "topic"], "{rows:?}");
    for (row, cells) in &rows {
        assert_eq!(role(row), Some("danger"), "{row:?}");
        assert_eq!(cells[1], "Violation", "the severity, in words: {cells:?}");
    }
    let topic = rows.iter().find(|(_, c)| c[0] == "topic").unwrap();
    assert_eq!(topic.1[2], "The field does not say whether it is required.");
    assert_eq!(
        topic.1[3], "A field says whether it is required",
        "the shape it failed"
    );
    assert_eq!(topic.1[4], "http://schema.org/valueRequired");
    // Every rendered node keeps its IRI: the node and the shape link through the home mapping.
    let links: Vec<String> = of(tables[0], "link")
        .iter()
        .map(|l| text(&l.properties["href"]))
        .collect();
    assert!(
        links
            .iter()
            .any(|h| h.contains("contact%3Atopic") || h.contains("contact:topic")),
        "{links:?}"
    );
    assert!(
        links.iter().any(|h| h.contains("TopicDescribed")),
        "the shape links too: {links:?}"
    );
    // The graph form: a scene of the failing nodes beside the same nodes as links, each in the
    // danger role, saying its severity, its message and its shape.
    assert_eq!(of(&n, "scene").len(), 1, "{n:?}");
    let list = of(&n, "list")
        .into_iter()
        .find(|l| label(l) == "Failing nodes")
        .expect("the failing nodes as a list");
    assert_eq!(list.children.len(), 4);
    for item in &list.children {
        assert_eq!(item.kind, "link");
        assert_eq!(role(item), Some("danger"));
        let words = label(item);
        assert!(words.starts_with("Violation: "), "{words}");
        assert!(words.contains("Shape: "), "{words}");
    }
    // The scene draws exactly those nodes, each labeled with its severity.
    let scene = scene_of(&common::host(None), &n);
    assert_eq!(scene.nodes.len(), 4, "{scene:?}");
    assert!(scene
        .nodes
        .iter()
        .all(|s| s.label.starts_with("Violation: ")));
    // Each severity is a checkbox, bound into the query as a term.
    let severities: Vec<String> = of(&n, "field")
        .iter()
        .filter(|f| text(&f.properties["name"]) == "severity")
        .map(|f| text(&f.properties["value"]))
        .collect();
    assert_eq!(
        severities,
        [
            "http://www.w3.org/ns/shacl#Violation",
            "http://www.w3.org/ns/shacl#Warning",
            "http://www.w3.org/ns/shacl#Info"
        ]
    );
}

#[test]
fn a_severity_facet_keeps_only_that_severity() {
    let host = common::host(None);
    let violations = view_with(
        &host,
        "shacl-violations",
        EXAMPLE_GRAPH,
        &[("severity", "http://www.w3.org/ns/shacl#Violation")],
    )
    .unwrap();
    assert_eq!(rows(of(&violations, "table")[0]).len(), 4);
    // The subset validator reports every failure as a violation: there are no warnings, and the
    // lens says so in words rather than drawing an empty table.
    let warnings = view_with(
        &host,
        "shacl-violations",
        EXAMPLE_GRAPH,
        &[("severity", "http://www.w3.org/ns/shacl#Warning")],
    )
    .unwrap();
    assert!(of(&warnings, "table").is_empty(), "{warnings:?}");
    assert!(
        said(&warnings).contains("found nothing"),
        "{}",
        said(&warnings)
    );
}

#[test]
fn a_withheld_graph_is_said_in_words_by_the_shacl_lens_too() {
    let shapes = EndpointSpace::new().bind(
        Exact::new(format!("urn:iki:ui:lens:shapes:{SECRET}")),
        fixed(
            "host-secret-shapes",
            "text/turtle",
            "@prefix sh: <http://www.w3.org/ns/shacl#> . <urn:s> a sh:NodeShape ; sh:targetNode <urn:a> ; sh:property [ sh:path <urn:z> ; sh:minCount 1 ] .".into(),
        ),
    );
    let host = common::host_with(
        None,
        vec![
            Arc::new(host_lenses()) as Arc<dyn Space>,
            Arc::new(shapes) as Arc<dyn Space>,
        ],
    );
    let n = view(
        &host,
        "shacl-violations",
        SECRET,
        &Capability::scoped(["urn:cap:nothing"]),
    )
    .unwrap();
    assert!(said(&n).contains("withheld"), "{}", said(&n));
    assert!(of(&n, "table").is_empty(), "never an empty renderer");
    // Its holder sees the one failure.
    let n = view(&host, "shacl-violations", SECRET, &Capability::root()).unwrap();
    assert_eq!(rows(of(&n, "table")[0]).len(), 1, "{n:?}");
}

#[test]
fn the_asserted_vs_derived_lens_draws_asserted_solid_and_derived_dashed_and_says_which() {
    let log = ikigai_ui::lens::EXAMPLE_LOG;
    let n = view(
        &common::host(None),
        "asserted-vs-derived",
        log,
        &Capability::root(),
    )
    .unwrap();
    // The legend says, in words, what each line means.
    let legend = of(&n, "list")
        .into_iter()
        .find(|l| label(l) == "Legend")
        .expect("a legend");
    let marks: Vec<_> = legend.children.iter().map(role).collect();
    assert_eq!(marks, [Some("asserted"), Some("derived")]);
    assert!(said(legend).contains("Solid rule: asserted"));
    assert!(said(legend).contains("Dashed rule: derived"));
    let rows = rows(of(&n, "table")[0]);
    // Every row has a role, and its last cell says the same split in words.
    for (row, cells) in &rows {
        let how = &cells[3];
        match role(row) {
            Some("asserted") => assert_eq!(how, "asserted"),
            Some("derived") => assert!(how.starts_with("derived: "), "{how}"),
            other => panic!("a row with no role: {other:?} {cells:?}"),
        }
    }
    let find = |subject: &str, property: &str, value: &str| {
        rows.iter()
            .find(|(_, c)| c[0] == subject && c[1] == property && c[2] == value)
            .map(|(r, c)| (role(r), c[3].clone()))
    };
    // The entries are asserted.
    assert_eq!(
        find("entry 1, the contact form opened", "type", "Entry"),
        Some((Some("asserted"), "asserted".into()))
    );
    // OWL 2 RL derives each entry a prov:Entity, from the asserted axiom: derived only.
    assert_eq!(
        find("entry 1, the contact form opened", "type", "Entity"),
        Some((Some("derived"), "derived: OWL 2 RL entails it".into())),
        "{rows:?}"
    );
    // PROV-O records the summary as derived from the entries, each named.
    let summary: Vec<_> = rows
        .iter()
        .filter(|(_, c)| c[0] == "summary, one message today" && c[1] == "label")
        .map(|(r, c)| (role(r), c[3].as_str()))
        .collect();
    assert_eq!(summary.len(), 1, "one row per triple: {summary:?}");
    assert_eq!(summary[0].0, Some("derived"));
    for entry in ["entry-1", "entry-2"] {
        assert!(
            summary[0]
                .1
                .contains(&format!("urn:iki:ui:example:graph:log:{entry}")),
            "{summary:?}"
        );
    }
    // OWL RL's own axioms about its vocabulary are not shown as the graph's.
    assert!(rows
        .iter()
        .all(|(_, c)| !c[0].starts_with("http://www.w3.org/2002/07/owl#")));
    // Nothing the graph asserts is drawn as derived by reasoning.
    assert!(find("entry 1, the contact form opened", "type", "Entry")
        .is_some_and(|(r, _)| r == Some("asserted")));
}

#[test]
fn a_role_the_query_leaves_empty_is_no_hint() {
    // `tidy` drops a hint that fills to nothing: only the query marks a row.
    let template = serde_json::json!({"kind": "list", "properties": {"label": "L"}, "children": [
        {"repeat": "rows", "each": {"kind": "text", "hint": {ROLE: "{?role}"}, "properties": {"content": "{?x}"}}}
    ]});
    let rows = ikigai_ui::lens::Rows {
        vars: vec!["x".into(), "role".into()],
        rows: vec![
            [
                ("x".to_string(), "a".to_string()),
                ("role".to_string(), "danger".to_string()),
            ]
            .into(),
            [("x".to_string(), "b".to_string())].into(),
        ],
        past: false,
    };
    let fill = ikigai_ui::lens::Fill {
        lens: "l",
        legend: "L",
        focus: "urn:x",
        bound: 5,
        page: 1,
        rows: &rows,
        edges: &Default::default(),
        home: &Default::default(),
    };
    let n = ikigai_ui::lens::expand(&template, &fill).unwrap();
    assert_eq!(role(&n.children[0]), Some("danger"));
    assert!(n.children[1].hint.is_empty(), "{:?}", n.children[1]);
}

#[test]
fn the_html_face_draws_each_role_as_a_class_beside_its_words() {
    let host = common::host(None);
    let page = html(
        &host,
        &format!("lens:shacl-violations:resource:{EXAMPLE_GRAPH}"),
        &[],
    );
    // Four failures, each a danger row in the table and a danger item in the list.
    assert_eq!(
        page.matches("<tr class=\"ui-role-danger\">").count(),
        4,
        "{page}"
    );
    assert_eq!(
        page.matches("<li class=\"ui-role-danger\">").count(),
        4,
        "{page}"
    );
    assert!(page.contains(">Violation: topic. The field does not say whether it is required. Shape: A field says whether it is required.</a>"), "{page}");
    let page = html(
        &host,
        &format!(
            "lens:asserted-vs-derived:resource:{}",
            ikigai_ui::lens::EXAMPLE_LOG
        ),
        &[],
    );
    assert!(
        page.contains("<li class=\"ui-role-asserted\"><p>Solid rule: asserted"),
        "{page}"
    );
    assert!(
        page.contains("<li class=\"ui-role-derived\"><p>Dashed rule: derived"),
        "{page}"
    );
    assert!(page.contains("<tr class=\"ui-role-derived\">"), "{page}");
    assert!(page.contains("<tr class=\"ui-role-asserted\">"), "{page}");
    // The header row plays no role.
    assert!(
        page.contains("<tr><th scope=\"col\"><p>Subject</p></th>"),
        "{page}"
    );
}
