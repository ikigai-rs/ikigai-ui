//! The module recipe as one test: ikigai-conformance walks every endpoint the test host binds
//! (ikigai-ui's, a page family, and the demo app's fixtures) and reports every violation at
//! once.
//!
//! - The artifacts, the palettes and the token stylesheet are cacheable: the artifacts mark
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
use ikigai_core::{Kernel, Verb};

#[test]
fn conforms() {
    let root = common::root(None, Vec::new());
    let kernel = Kernel::new(root.clone());
    let pages = Arc::new(common::demo_pages());
    let report = Suite::new()
        .fixture(Fixture::new("ui-palette", Verb::Source).binding("name", "light"))
        .fixture(Fixture::new("ui-page-demo", Verb::Source).binding("name", "home"))
        .fixture(Fixture::new("demo-title", Verb::Source).binding("name", "home"))
        .fixture(Fixture::new("demo-fragment", Verb::Source).binding("name", "home"))
        .cacheable("ui-htmx")
        .cacheable("ui-shell-js")
        .cacheable("ui-template-shell")
        .cacheable("ui-template-tokens")
        .cacheable("ui-tokens-css")
        .pure("ui-palette")
        .cacheable("ui-palette")
        .cacheable("ui-page-demo")
        .pure("ui-head-demo")
        .cacheable("ui-head-demo")
        .pure("demo-header")
        .pure("demo-title")
        .pure("demo-fragment")
        .namespace("urn:iki:ui:token:")
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
        "ui-page-demo",
        "ui-head-demo",
    ] {
        assert!(
            report.to_string().contains(id),
            "the walk saw `{id}`: {report}"
        );
    }
}
