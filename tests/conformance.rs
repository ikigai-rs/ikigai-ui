//! The module recipe as one test: ikigai-conformance walks every endpoint the test host binds
//! (ikigai-ui's, a page family, and the demo app's fixtures) and reports every violation at
//! once.
//!
//! - The artifacts, the palettes, the theme chooser, the header chooser and the token stylesheet are cacheable: the artifacts mark
//!   their answers cacheable and take the directory read's threads, and the stylesheet and the
//!   pages are compositions, cacheable while every part is.
//! - `space()` is self-named `urn:iki:space:ui`; `pages(family, base)` is anonymous, because
//!   its doors are the host's names, and the host names it.
//! - BODIES (conformance 0.12.0) compares each `Source`'s `invoke` and `invoke_body` in the
//!   module space, so the suite gets the same root `Arc` the kernel was built from, and the
//!   report must leave nothing unprobed, so that wiring cannot silently fall off. No argument
//!   here is declared `reads_body`, so BODY-INPUTS has nothing to probe and no content store
//!   is needed.

mod common;

use std::sync::Arc;

use ikigai_conformance::{Check, Expected, Fixture, Suite};
use ikigai_core::Verb;

#[test]
fn conforms() {
    let root = common::root(None, vec![Arc::new(common::form_models())]);
    let kernel = common::kernel(root.clone());
    let pages = Arc::new(common::demo_pages());
    let mut suite = Suite::new();
    // The lenses' data is compiled in: pure, cacheable constants (ledger #3518).
    for id in [
        "ui-lens-graph",
        "ui-lens-graph-query",
        "ui-lens-contact-graph",
        "ui-lens-contact-graph-query",
        "ui-lens-contact-table",
        "ui-lens-contact-table-query",
        "ui-lens-contact-list",
        "ui-lens-contact-list-query",
        "ui-lens-contact-dashboard",
        "ui-lens-contact-dashboard-query",
        "ui-lens-instance-contact",
        "ui-lens-demo-thread",
        "ui-lens-demo-thread-query",
        "ui-lens-demo-board",
        "ui-lens-demo-board-query",
        "ui-lens-demo-tree",
        "ui-lens-demo-tree-query",
        "ui-lens-demo-timeline",
        "ui-lens-demo-timeline-query",
        "ui-lens-demo-graph",
        "ui-lens-demo-instance",
    ] {
        suite = suite.pure(id);
    }
    // ikigai-sparql's endpoints, bound so the lenses run on a real store: a query each over the
    // contact graph, so the answer's golden thread hangs on its source.
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
    let report = suite
        .fixture(Fixture::new("ui-palette", Verb::Source).binding("name", "light"))
        .fixture(Fixture::new("ui-renderer", Verb::Source).binding("name", "table"))
        .fixture(Fixture::new("ui-home", Verb::Source).binding("iri", common::CONTACT))
        .fixture(Fixture::new("ui-lens-resource", Verb::Source).binding("iri", common::CONTACT))
        // Lenses (ledger #3518): the example lenses run on a real urn:sparql:select over the
        // contact graph (ikigai-sparql, bound in tests/common).
        .fixture(
            Fixture::new("ui-lens-result", Verb::Source)
                .binding("lens", "contact-table")
                .binding("iri", ikigai_ui::scene::EXAMPLE_GRAPH),
        )
        .fixture(
            Fixture::new("ui-screen-lens", Verb::Source)
                .binding("lens", "contact-list")
                .binding("iri", ikigai_ui::scene::EXAMPLE_GRAPH),
        )
        .fixture(
            Fixture::new("ui-view-lens", Verb::Source)
                .binding("lens", "demo-board")
                .binding("iri", ikigai_ui::lens::DEMO_GRAPH),
        )
        // A lens query as a resource (ledger #3530): the preparer is pure over its `content`,
        // and a lens's prepared form is cached, threaded from its query's source.
        .fixture(Fixture::new("ui-lens-prepare", Verb::Source).arg(
            "content",
            "SELECT ?focus ?s WHERE { GRAPH ?focus { ?s ?p ?o } }",
        ))
        .pure("ui-lens-prepare")
        .cacheable("ui-lens-prepare")
        .fixture(Fixture::new("ui-lens-prepared", Verb::Source).binding("lens", "contact-table"))
        .cacheable("ui-lens-prepared")
        .fixture(Fixture::new("ui-theme-chooser", Verb::Source).binding("theme", "dark"))
        .fixture(
            Fixture::new("ui-chooser", Verb::Source)
                .binding("family", "demo")
                .binding("theme", "dark"),
        )
        .fixture(Fixture::new("ui-page-demo", Verb::Source).binding("name", "home"))
        // A lensed view as a page in the shell (ledger #3538), and its title.
        .fixture(
            Fixture::new("ui-page-lens-demo", Verb::Source)
                .binding("lens", "contact-list")
                .binding("iri", ikigai_ui::scene::EXAMPLE_GRAPH),
        )
        .fixture(
            Fixture::new("ui-lens-title", Verb::Source)
                .binding("lens", "contact-list")
                .binding("iri", ikigai_ui::scene::EXAMPLE_GRAPH),
        )
        .fixture(Fixture::new("demo-title", Verb::Source).binding("name", "home"))
        .fixture(Fixture::new("demo-fragment", Verb::Source).binding("name", "home"))
        .fixture(Fixture::new("ui-kind-name", Verb::Source).binding("kind", "heading"))
        .fixture(Fixture::new("ui-example-kind", Verb::Source).binding("kind", "nav"))
        .fixture(Fixture::new("ui-form-resource", Verb::Source).binding("iri", common::CONTACT))
        .fixture(
            Fixture::new("ui-form-openapi", Verb::Source)
                .binding("doc", common::OPENAPI)
                .binding("op", "contact"),
        )
        .cacheable("ui-form-resource")
        .cacheable("ui-form-openapi")
        .pure("demo-openapi")
        .pure("ui-renderer")
        .pure("demo-contact")
        .cacheable("demo-contact")
        .cacheable("demo-openapi")
        .fixture(Fixture::new("ui-target-name", Verb::Source).binding("name", "html"))
        .fixture(
            Fixture::new("ui-face-target-screen", Verb::Source)
                .binding("target", "gpui")
                .binding("screen", "gallery"),
        )
        // Settings and structural nodes (ledger #3361): the screen-space face reads its target
        // from the setting; with no screen space injected, the root's default answers.
        .fixture(Fixture::new("ui-face-screen", Verb::Source).binding("screen", "gallery"))
        .fixture(Fixture::new("ui-setting-name", Verb::Source).binding("name", "target"))
        .fixture(Fixture::new("ui-structure-name", Verb::Source).binding("name", "reference"))
        .fixture(Fixture::new("ui-example-structure", Verb::Source).binding("name", "region"))
        .pure("ui-face-screen")
        .cacheable("ui-face-screen")
        .pure("ui-setting-name")
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
        .cacheable("ui-setting-name")
        .pure("ui-structure-name")
        .cacheable("ui-structure-name")
        .pure("ui-example-structure")
        .cacheable("ui-example-structure")
        // The html target (ledger #3355): every view reads the screen through the kernel.
        .fixture(Fixture::new("ui-html-screen", Verb::Source).binding("screen", "gallery"))
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
        .fixture(Fixture::new("ui-template-kind", Verb::Source).binding("kind", "table"))
        // The outline compiler (ledger #3365): an outline of one kind and one reference.
        .fixture(Fixture::new("ui-outline-compile", Verb::Source).arg(
            "content",
            "group label=Probe\n  urn:iki:ui:example:kind:heading content=Probed",
        ))
        // The s-expression front end (ledger #3367): the same screen as a datum.
        .fixture(Fixture::new("ui-sexpr-compile", Verb::Source).arg(
            "content",
            "(group :label \"Probe\" (urn:iki:ui:example:kind:heading :content \"Probed\"))",
        ))
        .cacheable("ui-sexpr-compile")
        .pure("ui-html-empty")
        .cacheable("ui-html-empty")
        .cacheable("ui-template-kind")
        .cacheable("ui-html-screen")
        .cacheable("ui-html-node")
        .cacheable("ui-html-child")
        .cacheable("ui-html-property")
        // Descriptors are file-backed artifacts; support and faces are pure projections.
        .pure("ui-support")
        .pure("ui-face-target-screen")
        .cacheable("ui-target-name")
        .cacheable("ui-support")
        .cacheable("ui-face-target-screen")
        .cacheable("ui-kind")
        .pure("ui-kind")
        .cacheable("ui-kind-name")
        .pure("ui-kind-name")
        .cacheable("ui-example-kind")
        .pure("ui-example-kind")
        .cacheable("ui-context")
        .pure("ui-context")
        .cacheable("ui-screen-gallery")
        .cacheable("ui-outline-compile")
        .cacheable("ui-prefixes")
        .pure("ui-prefixes")
        // Custom elements (ledger #3358): the definitions are file-backed artifacts and the
        // elements screen is compiled in; html and the face expand them through the kernel.
        .fixture(Fixture::new("ui-element-name", Verb::Source).binding("name", "rating"))
        .cacheable("ui-element-name")
        .pure("ui-screen-elements")
        .cacheable("ui-screen-elements")
        .cacheable("ui-htmx")
        .cacheable("ui-shell-js")
        .cacheable("ui-template-shell")
        .cacheable("ui-template-tokens")
        .cacheable("ui-tokens-css")
        .pure("ui-palette")
        .cacheable("ui-palette")
        .pure("ui-theme-chooser")
        .cacheable("ui-theme-chooser")
        .cacheable("ui-chooser")
        .cacheable("ui-page-demo")
        .pure("ui-head-demo")
        .cacheable("ui-head-demo")
        .pure("demo-header")
        .pure("demo-title")
        .pure("demo-fragment")
        .namespace("urn:iki:ui:token:")
        // The node kinds and their properties are this crate's own,
        // documented in `ikigai_ui::model` and shipped as the kinds' shapes (ledger #3352).
        .namespace("urn:iki:ui:kind:")
        .namespace("urn:iki:ui:property:")
        // This crate's own custom elements (ledger #3358), urn:iki:ui:element:name:{name}.
        .namespace("urn:iki:ui:element:")
        // The structural nodes, reference and region (ledger #3361): the shapes are this crate's.
        .namespace("urn:iki:ui:structure:")
        // The choice presentation hints (ledger #3537), this crate's own, documented at
        // `ikigai_ui::model::CHOICE_HINTS` and `RADIO`.
        .namespace("urn:iki:ui:hint:choice:")
        .namespace("urn:iki:ui:hint:group:")
        // Demo-only discussion/task vocabulary documented in assets/lens/demo.ttl.
        .namespace("urn:iki:ui:example:lenses:")
        .self_named_space("ui", ikigai_ui::space)
        .host_named_space("ikigai_ui::pages(family, base)", pages)
        // Since core 0.2.5 a root exact alias is a view of its own (ledger #2513): listed, and
        // described by its target's contract under the VIEW'S NAME, so its id is the IRI
        // core gives it. `pages` cannot choose that id, and the header door is an IRI on
        // purpose (UI conventions D8: the shell composes `urn:iki:ui:header:{family}`).
        .opt_out_check(
            "urn:iki:ui:header:demo",
            Check::Names,
            "a root view's id is its IRI, set by ikigai-core 0.2.5+ (ledger #2513), not by \
             this module; the door is the shell's header name (D8)",
        )
        // The seal answers nothing on purpose (ledger #3354): every name it holds is NotFound,
        // so no other module answers under `urn:iki:ui:kind:`.
        .opt_out_check(
            "ui-kind-seal",
            Check::Outputs,
            "the seal serves nothing by design: every name under urn:iki:ui:kind: that is not \
             a kind's is NotFound here, so no later module answers it",
        )
        .opt_out_check(
            "ui-kind-seal",
            Check::Resolves,
            "the seal resolves nothing by design: NotFound is its answer (ledger #3354)",
        )
        // The form fixtures' resources are models, read by Meta; their Sink is never fired.
        .opt_out_check(
            "demo-contact",
            Check::Outputs,
            "a test model: the form reads its contract (Meta); its Sink is never fired",
        )
        .opt_out_check(
            "demo-contact",
            Check::Pipeline,
            "a test model of a form's resource: a form posts its fields as named arguments, \
             not one `content`; it is never a pipeline stage",
        )
        .opt_out_check(
            "demo-contact-flat",
            Check::Outputs,
            "a test model: the form reads its contract (Meta); its Sink is never fired",
        )
        .opt_out_check(
            "demo-contact-flat",
            Check::Pipeline,
            "a test model of a form's resource: a form posts its fields as named arguments, \
             not one `content`; it is never a pipeline stage",
        )
        .opt_out_check(
            "demo-contact-urn",
            Check::Outputs,
            "a test model: the form reads its contract (Meta); tests/door.rs fires its Sink \
             through a door, which records the name it was sent",
        )
        .opt_out_check(
            "demo-contact-urn",
            Check::Pipeline,
            "a test model of a form's resource: a form posts its fields as named arguments, \
             not one `content`; it is never a pipeline stage",
        )
        // A screen program runs on the host's `urn:lisp:eval` (ikigai-lisp), which this module
        // issues and never binds; tests/sexpr.rs runs it against the real eval.
        .opt_out_check(
            "ui-lisp-compile",
            Check::Resolves,
            "its program runs on urn:lisp:eval, the host's ikigai-lisp, which this module does \
             not bind; tests/sexpr.rs resolves it against the real eval",
        )
        .opt_out_check(
            "ui-lisp-compile",
            Check::Required,
            "its missing-input probe needs urn:lisp:eval, which the host binds \
             (tests/sexpr.rs)",
        )
        .opt_out_check(
            "ui-lisp-compile",
            Check::Outputs,
            "its output is probed only through urn:lisp:eval, which the host binds \
             (tests/sexpr.rs)",
        )
        .body_space(root)
        .run_blocking(&kernel);
    println!("{report}");
    report.assert_clean_with(&Expected::new().nothing_unprobed());
    assert_eq!(ikigai_core::space_iri("ui").as_str(), ikigai_ui::SPACE_ID);
    for id in [
        "ui-htmx",
        "ui-shell-js",
        "ui-template-shell",
        "ui-template-tokens",
        "ui-tokens-css",
        "ui-palette",
        "ui-theme-chooser",
        "ui-chooser",
        "ui-page-demo",
        "ui-head-demo",
        "ui-kind",
        "ui-kind-name",
        "ui-example-kind",
        "ui-context",
        "ui-screen-gallery",
        "ui-outline-compile",
        "ui-sexpr-compile",
        "ui-lisp-compile",
        "ui-prefixes",
        "ui-screen-gallery-source",
        "ui-form-resource",
        "ui-form-openapi",
        "ui-kind-seal",
        "ui-target-name",
        "ui-support",
        "ui-face-target-screen",
        "ui-face-screen",
        "ui-setting-name",
        "ui-structure-name",
        "ui-example-structure",
        "ui-element-name",
        "ui-screen-elements",
    ] {
        assert!(
            report.to_string().contains(id),
            "the walk saw `{id}`: {report}"
        );
    }
}
