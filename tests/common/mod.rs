//! A test host: ikigai-ui's space, one page family (`demo`, under `urn:demo:`, with its own
//! stylesheet and the push stream, D9) with a header, titles and fragments, and optionally a directory bound at `urn:iki:ui:dir:{path}` through
//! ikigai-fs, the way a host overrides an artifact (D17's Delivery A).
#![allow(dead_code)] // each test binary uses its own part of this module

use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures::executor::block_on;
use ikigai_core::{
    ArgRef, ArgSpec, Capability, Description, EndpointSpace, Error, Exact, Fallback, FnEndpoint,
    Invocation, Iri, Kernel, ReprType, Representation, Request, Result, Space, UriTemplate, Verb,
};

pub const HTML: &str = "text/html; charset=utf-8";
const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

/// The demo family's header: a named nav and a theme chooser that works with no script.
pub const HEADER: &str = "<nav aria-label=\"Site\"><a href=\"/demo/page/home\">Demo</a> \
<a href=\"/demo/page/about\">About</a></nav>\n\
<form class=\"chooser\" method=\"get\"><label for=\"theme\">Theme</label> \
<select id=\"theme\" name=\"theme\" data-ui-theme><option value=\"auto\">Match system</option>\
<option value=\"light\">Light</option><option value=\"dark\">Dark</option>\
<option value=\"high-contrast\">High contrast</option></select> \
<button type=\"submit\">Apply</button></form>";

/// The demo family's own stylesheet, at [`DEMO_CSS_PATH`]: only tokens, no colors.
pub const DEMO_CSS: &str = ".demo-live { border: 1px solid var(--line); padding: 0.5rem; }\n";
/// Where the demo family's stylesheet is served.
pub const DEMO_CSS_PATH: &str = "/demo/demo.css";
/// The push stream the demo family names. The axe server answers it (axe/serve.mjs).
pub const PUSH_PATH: &str = "/_push";

/// The demo family: [`DEMO_CSS_PATH`] and [`PUSH_PATH`] in every page's head.
pub fn demo_pages() -> ikigai_core::Alias {
    ikigai_ui::Pages::new("demo", "urn:demo:")
        .stylesheet(DEMO_CSS_PATH)
        .push(PUSH_PATH)
        .build()
        .unwrap()
}

/// The demo pages: (name, title, fragment). `live` holds a view that refetches itself when
/// the push stream reports a cut on `/_live/count`, which axe/serve.mjs answers.
pub const PAGES: [(&str, &str, &str); 3] = [
    (
        "home",
        "Home & <welcome>",
        "<h1>Home</h1>\n<p>A page over the shared shell. <a href=\"/demo/page/about\">About</a>.</p>\n\
<p class=\"muted-note\">Status lives in one region.</p>\n\
<button type=\"button\" hx-get=\"/demo/fragment/missing\" hx-target=\"#detail\">Load details</button>\n\
<div id=\"detail\"></div>",
    ),
    (
        "about",
        "About",
        "<h1>About</h1>\n<p>The fragment is a resource; the page composes it.</p>",
    ),
    (
        "live",
        "Live",
        "<h1>Live</h1>\n<p>The count is refetched when the push stream reports a cut on it.</p>\n\
<div id=\"count\" class=\"demo-live\" hx-get=\"/_live/count\" hx-trigger=\"ikigai:cut\" hx-swap=\"outerHTML\" data-ui-push=\"\">\
<button id=\"count-focus\" type=\"button\">Count 0</button></div>\n\
<button id=\"elsewhere\" type=\"button\">Elsewhere</button>",
    ),
];

fn constant(id: &str, summary: &str, body: String) -> FnEndpoint {
    FnEndpoint::new(id, move |_: &Invocation<'_>| {
        Ok(Representation::new(ReprType::new(HTML), body.clone().into_bytes()).cacheable())
    })
    .with_description(
        Description::new(id)
            .title(id)
            .summary(summary)
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .output(HTML),
    )
}

fn per_page(
    id: &str,
    summary: &str,
    pick: fn(&(&'static str, &'static str, &'static str)) -> &'static str,
) -> FnEndpoint {
    FnEndpoint::new(id, move |inv: &Invocation<'_>| {
        let name = inv.bindings.get("name").unwrap_or_default();
        match PAGES.iter().find(|p| p.0 == name) {
            Some(page) => Ok(Representation::new(
                ReprType::new(HTML),
                pick(page).as_bytes().to_vec(),
            )
            .cacheable()),
            None => Err(Error::NotFound(format!("no demo page `{name}`"))),
        }
    })
    .with_description(
        Description::new(id)
            .title(id)
            .summary(summary)
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .input(ArgSpec::new("name").class(XSD_STRING).binding())
            .output(HTML),
    )
}

/// The demo family's own names: `urn:demo:header`, `urn:demo:title:{name}`,
/// `urn:demo:fragment:{name}`.
pub fn demo_app() -> EndpointSpace {
    EndpointSpace::new()
        .bind(
            Exact::new("urn:demo:header"),
            constant(
                "demo-header",
                "The demo family's header.",
                HEADER.to_string(),
            ),
        )
        .bind(
            UriTemplate::parse("urn:demo:title:{name}").unwrap(),
            per_page("demo-title", "A demo page's title, as text.", |p| p.1),
        )
        .bind(
            UriTemplate::parse("urn:demo:fragment:{name}").unwrap(),
            per_page("demo-fragment", "A demo page's fragment.", |p| p.2),
        )
}

/// A host. `dir` binds a directory at `urn:iki:ui:dir:{path}` (cacheable, so the golden thread
/// is observable); `None` binds nothing there, so every artifact is compiled in.
pub fn host(dir: Option<&Path>) -> Kernel {
    host_with(dir, Vec::new())
}

/// [`host`], with extra spaces ahead of the demo app.
pub fn host_with(dir: Option<&Path>, extra: Vec<Arc<dyn Space>>) -> Kernel {
    let mut spaces: Vec<Arc<dyn Space>> =
        vec![Arc::new(ikigai_ui::space()), Arc::new(demo_pages())];
    spaces.extend(extra);
    spaces.push(Arc::new(demo_app()));
    if let Some(dir) = dir {
        spaces.push(Arc::new(EndpointSpace::new().bind(
            UriTemplate::parse("urn:iki:ui:dir:{path}").unwrap(),
            ikigai_fs::FileEndpoint::new(dir.to_path_buf()).cacheable(),
        )));
    }
    Kernel::new(Arc::new(Fallback::new(spaces)))
}

pub fn issue(
    kernel: &Kernel,
    verb: Verb,
    iri: &str,
    args: &[(&str, &[u8])],
) -> Result<Representation> {
    let mut request = Request::new(verb, Iri::parse(iri).expect("a valid IRI"));
    for (k, v) in args {
        request = request.with_arg(*k, ArgRef::Inline(v.to_vec()));
    }
    block_on(kernel.issue(request, &Capability::root()))
}

pub fn source(kernel: &Kernel, iri: &str) -> Result<Representation> {
    issue(kernel, Verb::Source, iri, &[])
}

pub fn text(kernel: &Kernel, iri: &str, args: &[(&str, &str)]) -> String {
    ikigai_ui::testkit::render::source(kernel, iri, args).unwrap_or_else(|e| panic!("{iri}: {e}"))
}

/// A fresh, empty scratch directory under the system temp dir, unique to this test.
pub fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ikigai-ui-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A directory holding every artifact's compiled-in bytes under its override file name.
pub fn mirror(tag: &str) -> PathBuf {
    let dir = scratch(tag);
    for a in &ikigai_ui::ARTIFACTS {
        std::fs::write(dir.join(a.file), a.bytes).unwrap();
    }
    dir
}

pub fn snapshots() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots")
}
