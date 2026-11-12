//! The module recipe over the demo's own endpoints (ledger #3356): ArgSpecs, declared
//! capabilities enforced, no blank nodes. ikigai-ui's own are the root crate's
//! `tests/conformance.rs`; here they are the fixtures the demo's fragments read.

use std::sync::Arc;

use ikigai_conformance::{Check, Fixture, Suite};
use ikigai_core::Verb;

#[test]
fn conforms() {
    let store: ikigai_ui_demo::Store = Arc::default();
    store.lock().unwrap().push(ikigai_ui_demo::Submission {
        id: 1,
        ..ikigai_ui_demo::validate(&[
            ("name".into(), "Ada".into()),
            ("email".into(), "ada@example.org".into()),
            ("topic".into(), "question".into()),
            ("message".into(), "Hi".into()),
        ])
        .unwrap()
    });
    let root = ikigai_ui_demo::root(store);
    let kernel = ikigai_ui_demo::kernel(root.clone());
    let mut suite = Suite::new();
    // Lenses (ledger #3518): their data is compiled-in constants, and the example lenses run
    // on the real urn:sparql:* the demo binds over the contact graph.
    for id in [
        "ui-renderer",
        "ui-lens-graph",
        "ui-lens-graph-query",
        "ui-lens-contact-graph",
        "ui-lens-contact-graph-query",
        "ui-lens-contact-table",
        "ui-lens-contact-table-query",
        "ui-lens-contact-list",
        "ui-lens-contact-list-query",
        "ui-lens-instance-contact",
    ] {
        suite = suite.pure(id);
    }
    for (id, query) in [
        ("sparql-select", "SELECT * WHERE { ?s ?p ?o } LIMIT 1"),
        ("sparql-ask", "ASK { ?s ?p ?o }"),
        (
            "sparql-construct",
            "CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o } LIMIT 1",
        ),
        ("sparql-describe", "DESCRIBE <urn:iki:uidemo:contact>"),
    ] {
        suite = suite.fixture(
            Fixture::new(id, Verb::Source)
                .arg("query", query)
                .arg("graph", ikigai_ui::scene::EXAMPLE_GRAPH),
        );
    }
    for id in ["ui-lens-result", "ui-view-lens"] {
        suite = suite.fixture(
            Fixture::new(id, Verb::Source)
                .binding("lens", "contact-table")
                .binding("iri", ikigai_ui::scene::EXAMPLE_GRAPH),
        );
    }
    let report = suite
        .fixture(Fixture::new("ui-renderer", Verb::Source).binding("name", "table"))
        .fixture(Fixture::new("uidemo-title", Verb::Source).binding("name", "gallery"))
        .fixture(Fixture::new("uidemo-fragment", Verb::Source).binding("name", "contact"))
        .fixture(Fixture::new("uidemo-submission-id", Verb::Source).binding("id", "1"))
        .fixture(Fixture::new("uidemo-contact", Verb::Sink).arg(
            "content",
            "name=Ada&email=ada%40example.org&topic=question&message=Hi",
        ))
        .fixture(Fixture::new("ui-page-uidemo", Verb::Source).binding("name", "gallery"))
        .pure("uidemo-header")
        .pure("uidemo-title")
        // ikigai-ui's own endpoints, with the fixtures and markings its tests/conformance.rs
        // gives them.
        .fixture(
            Fixture::new("ui-form-resource", Verb::Source).binding("iri", ikigai_ui_demo::CONTACT),
        )
        .fixture(Fixture::new("ui-html-screen", Verb::Source).binding("screen", "gallery"))
        .fixture(Fixture::new("ui-screen-name", Verb::Source).binding("screen", "contact"))
        .pure("uidemo-example-rating")
        .fixture(Fixture::new("ui-outline-compile", Verb::Source).arg(
            "content",
            "group label=Probe\n  urn:iki:ui:example:kind:heading content=Probed",
        ))
        .fixture(Fixture::new("ui-sexpr-compile", Verb::Source).arg(
            "content",
            "(group :label \"Probe\" (urn:iki:ui:example:kind:heading :content \"Probed\"))",
        ))
        .fixture(
            Fixture::new("ui-html-node", Verb::Source)
                .binding("screen", "gallery")
                .binding("node", "n.1"),
        )
        .fixture(
            Fixture::new("ui-html-child", Verb::Source)
                .binding("screen", "gallery")
                .binding("node", "n.3"),
        )
        .fixture(
            Fixture::new("ui-html-property", Verb::Source)
                .binding("screen", "gallery")
                .binding("node", "n.0")
                .binding("property", "label"),
        )
        .fixture(
            Fixture::new("ui-face-target-screen", Verb::Source)
                .binding("target", "gpui")
                .binding("screen", "gallery"),
        )
        .opt_out_check(
            "ui-lisp-compile",
            Check::Resolves,
            "this host binds no urn:lisp:eval (ikigai-lisp); the root crate's tests/sexpr.rs \
             resolves it against the real eval",
        )
        .opt_out_check(
            "ui-form-openapi",
            Check::Resolves,
            "this host binds no OpenAPI document; the root crate's conformance test resolves it",
        )
        .opt_out_check(
            "ui-kind-seal",
            Check::Resolves,
            "the seal resolves nothing by design: NotFound is its answer (ledger #3354)",
        )
        .opt_out_check(
            "ui-kind-seal",
            Check::Outputs,
            "the seal serves nothing by design",
        )
        .opt_out_check(
            "urn:iki:ui:header:uidemo",
            Check::Names,
            "a root view's id is its IRI, set by ikigai-core (ledger #2513)",
        )
        .pure("ui-palette")
        .pure("ui-theme-chooser")
        .pure("ui-kind")
        .pure("ui-kind-name")
        .pure("ui-example-kind")
        .pure("ui-context")
        .pure("ui-prefixes")
        .pure("ui-screen-elements")
        .pure("ui-html-empty")
        .pure("ui-head-uidemo")
        .pure("ui-support")
        .pure("ui-face-target-screen")
        .fixture(Fixture::new("ui-face-screen", Verb::Source).binding("screen", "gallery"))
        .pure("ui-face-screen")
        .pure("ui-setting-name")
        .pure("ui-structure-name")
        .pure("ui-example-structure")
        // The embedded 3D scene (ledger #3514), as ikigai-ui's tests/conformance.rs registers it.
        .fixture(
            Fixture::new("ui-layout-graph", Verb::Source)
                .binding("graph", ikigai_ui::scene::EXAMPLE_GRAPH),
        )
        .fixture(
            Fixture::new("ui-scene-graph", Verb::Source)
                .binding("graph", ikigai_ui::scene::EXAMPLE_GRAPH),
        )
        .pure("ui-shape-scene")
        .pure("ui-example-graph")
        // ikigai-ui's own vocabularies, as its tests/conformance.rs registers them.
        .namespace("urn:iki:ui:token:")
        .namespace("urn:iki:ui:kind:")
        .namespace("urn:iki:ui:property:")
        .namespace("urn:iki:ui:element:")
        // The structural nodes, reference and region (ledger #3361): the shapes are this crate's.
        .namespace("urn:iki:ui:structure:")
        .body_space(root)
        .run_blocking(&kernel);
    println!("{report}");
    report.assert_clean();
}
