//! The module recipe as one test: ikigai-conformance walks every endpoint the test host binds
//! (ikigai-ui's, a page family, and the demo app's fixtures) and reports every violation at
//! once.
//!
//! - The artifacts, the palettes and the token stylesheet are cacheable: the artifacts mark
//!   their answers cacheable and take the directory read's threads, and the stylesheet and the
//!   pages are compositions, cacheable while every part is.
//! - `space()` is self-named `urn:iki:space:ui`; `pages(family, base)` is anonymous, because
//!   its doors are the host's names, and the host names it.

mod common;

use std::sync::Arc;

use ikigai_conformance::{Fixture, Suite};
use ikigai_core::Verb;

#[test]
fn conforms() {
    let kernel = common::host(None);
    let pages = Arc::new(ikigai_ui::pages("demo", "urn:demo:").unwrap());
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
        .pure("demo-header")
        .pure("demo-title")
        .pure("demo-fragment")
        .namespace("urn:iki:ui:token:")
        .self_named_space("ui", ikigai_ui::space)
        .host_named_space("ikigai_ui::pages(family, base)", pages)
        .run_blocking(&kernel);
    println!("{report}");
    assert!(report.is_clean(), "{report}");
    assert_eq!(ikigai_core::space_iri("ui").as_str(), ikigai_ui::SPACE_ID);
    for id in [
        "ui-htmx",
        "ui-shell-js",
        "ui-template-shell",
        "ui-template-tokens",
        "ui-tokens-css",
        "ui-palette",
        "ui-page-demo",
    ] {
        assert!(
            report.to_string().contains(id),
            "the walk saw `{id}`: {report}"
        );
    }
}
