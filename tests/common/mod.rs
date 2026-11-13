//! A test host: ikigai-ui's space, one page family (`demo`, under `urn:demo:`, with its own
//! stylesheet and the push stream, D9) with a header, titles and fragments, and optionally a directory bound at `urn:iki:ui:dir:{path}` through
//! ikigai-fs, the way a host overrides an artifact (D17's Delivery A).
#![allow(dead_code)] // each test binary uses its own part of this module

pub mod lens_actions;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures::executor::block_on;
use ikigai_core::{
    ArgRef, ArgSpec, Capability, Description, EndpointSpace, Error, Exact, Fallback, FnEndpoint,
    Invocation, Iri, Kernel, ReprType, Representation, Request, Result, Space, UriTemplate, Verb,
};

pub const HTML: &str = "text/html; charset=utf-8";
const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

/// The demo family's header: a named nav. The theme chooser after it is the shell's.
pub const HEADER: &str = "<nav aria-label=\"Site\"><a href=\"/demo/page/home\">Demo</a> \
<a href=\"/demo/page/about\">About</a></nav>";

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
        .lenses(true)
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
    kernel(root(dir, extra))
}

/// A kernel over `root` with the Meta renderer a host configures, so a contract reads as
/// `application/json` (the forms, ledger #3354).
pub fn kernel(root: Arc<dyn Space>) -> Kernel {
    Kernel::with_meta_renderer(root, Arc::new(ikigai_vocab::TurtleRenderer))
}

/// The root space [`host_with`] builds its kernel from, for a test that needs the same `Arc`
/// beside the kernel (the conformance suite's `body_space`).
pub fn root(dir: Option<&Path>, extra: Vec<Arc<dyn Space>>) -> Arc<dyn Space> {
    let mut spaces: Vec<Arc<dyn Space>> =
        vec![Arc::new(ikigai_ui::space()), Arc::new(demo_pages())];
    spaces.extend(extra);
    spaces.push(Arc::new(demo_app()));
    // The example lenses over the contact graph and a real SPARQL store to run them (#3518).
    spaces.push(Arc::new(ikigai_ui::lens::examples()));
    spaces.push(Arc::new(ikigai_sparql::space()));
    // What the SHACL and asserted-vs-derived lenses join (#3521): the report, and the
    // reasoner's entailments.
    spaces.push(Arc::new(ikigai_shacl::space()));
    spaces.push(Arc::new(ikigai_reason::space()));
    if let Some(dir) = dir {
        spaces.push(Arc::new(EndpointSpace::new().bind(
            UriTemplate::parse("urn:iki:ui:dir:{path}").unwrap(),
            ikigai_fs::FileEndpoint::new(dir.to_path_buf()).cacheable(),
        )));
    }
    Arc::new(Fallback::new(spaces))
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

// --- models a form is generated from (ledger #3354) --------------------------------------------

pub const CONTACT: &str = "http://example.org/contact";
pub const CONTACT_FLAT: &str = "http://example.org/contact-flat";
/// The contact resource by a `urn:` name, which a door reaches only through a route: its
/// form has a `resource` and no `action`. Its Sink records each `name` it is sent in [`SUNK`].
pub const CONTACT_URN: &str = "urn:demo:contact";
/// The `name` of every message [`CONTACT_URN`]'s Sink accepted.
pub static SUNK: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
pub const OPENAPI: &str = "urn:demo:openapi";
const XSD: &str = "http://www.w3.org/2001/XMLSchema#";
/// What writing to the contact resources requires.
pub const CONTACT_WRITE: &str = "urn:cap:demo:contact:write";

/// The contact resource's Sink inputs: required, optional, enumerated and defaulted.
fn contact_inputs() -> Vec<ArgSpec> {
    vec![
        ArgSpec::new("name")
            .summary("Who is writing.")
            .class(XSD_STRING),
        ArgSpec::new("email").class(format!("{XSD}anyURI")),
        ArgSpec::new("message")
            .summary("What you want to say.")
            .class("https://schema.org/Text"),
        ArgSpec::new("topic")
            .class(XSD_STRING)
            .one_of(["sales", "support"])
            .default_value("support"),
        ArgSpec::new("copies")
            .class(format!("{XSD}integer"))
            .default_value(1),
        ArgSpec::new("subscribe")
            .class(format!("{XSD}boolean"))
            .optional(),
        ArgSpec::new("on").class(format!("{XSD}date")).optional(),
    ]
}

fn accepted(_: &Invocation<'_>) -> Result<Representation> {
    Ok(Representation::new(ReprType::new("text/plain"), b"sent".to_vec()).cacheable())
}

/// The OpenAPI 3.1 document with the same shape as the contact resource's Sink.
pub const OPENAPI_DOC: &str = r##"{
  "openapi": "3.1.0",
  "info": {"title": "Contact", "version": "1"},
  "servers": [{"url": "http://example.org"}],
  "paths": {
    "/contact": {
      "get": {"operationId": "look", "parameters": [{"name": "q", "in": "query"}]},
      "post": {
        "operationId": "contact",
        "summary": "Contact",
        "description": "Send us a message.",
        "requestBody": {"content": {"application/json": {"schema": {"$ref": "#/components/schemas/Contact"}}}}
      }
    }
  },
  "components": {"schemas": {"Contact": {
    "type": "object",
    "required": ["name", "email", "message"],
    "properties": {
      "name": {"type": "string", "description": "Who is writing."},
      "email": {"type": "string", "format": "uri"},
      "message": {"type": "string", "format": "long-text", "description": "What you want to say."},
      "topic": {"type": "string", "enum": ["sales", "support"], "default": "support"},
      "copies": {"type": "integer", "default": 1},
      "subscribe": {"type": ["boolean", "null"]},
      "on": {"type": "string", "format": "date"}
    }
  }}}
}"##;

/// The models: the contact resource, its per-verb form (its Source reads `q`, its Sink the
/// contact inputs), the same Sink authored flat, and the OpenAPI document.
pub fn form_models() -> EndpointSpace {
    let by_urn = contact_description("demo-contact-urn");
    let sunk = |inv: &Invocation<'_>| {
        if inv.request.verb == Verb::Sink {
            let name = String::from_utf8_lossy(inv.inline_arg("name")?).into_owned();
            SUNK.lock().unwrap().push(name);
        }
        accepted(inv)
    };
    let flat = contact_inputs().into_iter().fold(
        Description::new("demo-contact-flat")
            .title("Contact")
            .summary("Send us a message.")
            .verb(Verb::Sink)
            .verb(Verb::Meta)
            .requires(CONTACT_WRITE)
            .output("text/plain"),
        |d, i| d.input(i),
    );
    EndpointSpace::new()
        .bind(
            Exact::new(CONTACT),
            FnEndpoint::new("demo-contact", accepted)
                .with_description(contact_description("demo-contact")),
        )
        .bind(
            Exact::new(CONTACT_URN),
            FnEndpoint::new("demo-contact-urn", sunk).with_description(by_urn),
        )
        .bind(
            Exact::new(CONTACT_FLAT),
            FnEndpoint::new("demo-contact-flat", accepted).with_description(flat),
        )
        .bind(
            Exact::new(OPENAPI),
            FnEndpoint::new("demo-openapi", |_: &Invocation<'_>| {
                Ok(Representation::new(
                    ReprType::new("application/json"),
                    OPENAPI_DOC.as_bytes().to_vec(),
                )
                .cacheable())
            })
            .with_description(
                Description::new("demo-openapi")
                    .title("demo-openapi")
                    .summary("An OpenAPI 3.1 document.")
                    .verb(Verb::Source)
                    .verb(Verb::Meta)
                    .output("application/json"),
            ),
        )
}

/// The contact resource's description: its Source reads `q`, its Sink the contact inputs.
fn contact_description(id: &str) -> Description {
    let contact = Description::new(id)
        .title("Contact")
        .summary("The contact resource.")
        .verb(Verb::Source)
        .verb(Verb::Sink)
        .verb(Verb::Meta)
        .action(
            ikigai_core::ActionSpec::new(Verb::Source)
                .input(ArgSpec::new("q").class(XSD_STRING).optional())
                .output("text/plain"),
        );
    let sink = contact_inputs().into_iter().fold(
        ikigai_core::ActionSpec::new(Verb::Sink)
            .summary("Send us a message.")
            .requires(CONTACT_WRITE)
            .output("text/plain"),
        |a, i| a.input(i),
    );
    contact.action(sink)
}

/// A host whose screen `urn:iki:ui:screen:name:contact` is the form generated from
/// `resource`'s contract (`urn:iki:ui:form:resource:{resource}`).
pub fn form_host(resource: &'static str) -> Kernel {
    form_host_in(None, resource)
}

/// [`form_host`], with a UI directory.
pub fn form_host_in(dir: Option<&Path>, resource: &'static str) -> Kernel {
    let screen = ikigai_core::AsyncFnEndpoint::new(
        "test-screen",
        move |inv: &Invocation<'_>| -> ikigai_core::InvokeFuture<'_> {
            Box::pin(async move {
                let iri = Iri::parse(format!("{}{resource}", ikigai_ui::FORM_RESOURCE)).unwrap();
                inv.issue(Request::new(Verb::Source, iri)).await
            })
        },
    );
    let extra = EndpointSpace::new().bind(Exact::new("urn:iki:ui:screen:name:contact"), screen);
    kernel(root(dir, vec![Arc::new(extra), Arc::new(form_models())]))
}

/// The value of the first `name="…"` attribute in `html`.
pub fn attribute<'a>(html: &'a str, name: &str) -> &'a str {
    let at = html
        .find(&format!(" {name}=\""))
        .unwrap_or_else(|| panic!("no {name} in {html}"));
    let rest = &html[at + name.len() + 3..];
    &rest[..rest.find('"').unwrap()]
}
