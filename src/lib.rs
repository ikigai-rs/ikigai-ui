//! `ikigai-ui`: the shared UI path for ikigai hosts, as resources under `urn:iki:ui:`.
//!
//! One crate every UI host binds to get the shared artifacts (UI conventions D7, D12, D14,
//! D17) and, behind the `testkit` feature, the shared tests (D18). It holds no state and does
//! no file I/O: the artifacts are compiled in, and a host that wants to override one binds a
//! directory at [`DIR`].
//!
//! ## The names
//!
//! | name | kind | what it is |
//! |---|---|---|
//! | `urn:iki:ui:htmx` | artifact | the vendored htmx ([`HTMX_VERSION`], [`HTMX_SHA256`]) |
//! | `urn:iki:ui:shell.js` | artifact | the shell script: refusals, the status region, focus after a swap, the theme (D14) |
//! | `urn:iki:ui:template:shell` | artifact | the page shell, a compose template (D1, D8, D13) |
//! | `urn:iki:ui:template:tokens` | artifact | the token stylesheet's template |
//! | `urn:iki:ui:palette:{name}` | data | a palette table ([`palette`]): CSS declarations, or Turtle with `as=text/turtle` |
//! | `urn:iki:ui:tokens.css` | composition | the template filled with the palettes: `auto`, light, dark, high contrast |
//! | a host's `{base}page:{name}` | composition | the shell over the host's fragment, from [`pages`] |
//!
//! ## Two deliveries, one set of names (D17)
//!
//! Each artifact is compiled in. A host that binds a directory at [`DIR`] (ikigai-fs's
//! `FileEndpoint` under `urn:iki:ui:dir:{path}`) overrides any artifact per name: the artifact
//! reads `urn:iki:ui:dir:{file}` first and falls back to its compiled-in bytes only when THAT read
//! is `NotFound` (the file is absent) or `Unresolved` (nothing is bound at [`DIR`]). Any other
//! failure, a denial included, passes through. Because the fallback is taken on the directory
//! atom's own absence, the answer hangs from that atom's golden thread, and a write to the
//! file through the kernel (or a watcher's cut) recomputes it. The palettes are not artifacts:
//! they are the data the contrast floor is tested over, so a directory cannot change a color.
//!
//! **Sealed.** A directory can replace only the artifacts [`ARTIFACTS`] names; it cannot add a
//! name, and nothing here serves a header or a CSP. Those are door configuration (D6).
//!
//! ## Binding it
//!
//! ```
//! # use std::sync::Arc;
//! # use futures::executor::block_on;
//! use ikigai_core::{
//!     Capability, EndpointSpace, Exact, Fallback, FnEndpoint, Iri, Kernel, ReprType,
//!     Representation, Request, UriTemplate, Verb,
//! };
//!
//! let html = |body: &'static str| {
//!     FnEndpoint::new("fixed", move |_| {
//!         Ok(Representation::new(ReprType::new("text/html"), body.as_bytes().to_vec()).cacheable())
//!     })
//! };
//! // The host's own names: a header, and one title and one fragment per page.
//! let app = EndpointSpace::new()
//!     .bind(Exact::new("urn:app:header"), html("<nav aria-label=\"Site\"><a href=\"/\">App</a></nav>"))
//!     .bind(Exact::new("urn:app:title:home"), html("Home"))
//!     .bind(Exact::new("urn:app:fragment:home"), html("<h1>Hello</h1>"));
//! let kernel = Kernel::new(Arc::new(Fallback::new(vec![
//!     Arc::new(ikigai_ui::space()),
//!     Arc::new(ikigai_ui::pages("app", "urn:app:").unwrap()),
//!     Arc::new(app),
//! ])));
//! let page = block_on(kernel.issue(
//!     Request::new(Verb::Source, Iri::parse("urn:app:page:home").unwrap()),
//!     &Capability::root(),
//! ))
//! .unwrap();
//! let page = String::from_utf8(page.bytes).unwrap();
//! assert!(page.contains("<html lang=\"en\" data-theme=\"auto\">"));
//! assert!(page.contains("<title>Home</title>"));
//! assert!(page.contains("<main id=\"main\" tabindex=\"-1\">\n<h1>Hello</h1>\n</main>"));
//! ```
//!
//! A door routes `/ui/{…}` to `urn:iki:ui:{…}`, which is where the shell links them:
//! `/ui/tokens.css`, `/ui/shell.js`, `/ui/htmx`. That is one route-table prefix entry (D10);
//! the mechanical rule alone would put them at `/iki/ui/{…}`.

#![forbid(unsafe_code)]

use std::sync::Arc;

use ikigai_core::{
    space_iri, Alias, AliasTable, ArgRef, ArgSpec, AsyncFnEndpoint, Bindings, Description,
    Endpoint, EndpointSpace, Error, Exact, FnEndpoint, Invocation, InvokeFuture, Iri, ReprType,
    Representation, Request, UriTemplate, Verb,
};

pub mod palette;
#[cfg(feature = "testkit")]
pub mod testkit;

/// The name [`space`] claims: `urn:iki:space:ui`.
pub const SPACE_ID: &str = "urn:iki:space:ui";

/// Where an artifact looks for its override: `urn:iki:ui:dir:{file}`, for each file [`ARTIFACTS`]
/// names. A host binds a directory here; with nothing bound, every artifact is compiled in.
pub const DIR: &str = "urn:iki:ui:dir:";

/// The vendored htmx's release.
pub const HTMX_VERSION: &str = "2.0.4";
/// The SHA-256 of the vendored htmx, as upstream publishes `dist/htmx.min.js` for
/// [`HTMX_VERSION`]. `tests/htmx.rs` holds the bytes to it.
pub const HTMX_SHA256: &str = "e209dda5c8235479f3166defc7750e1dbcd5a5c1808b7792fc2e6733768fb447";
/// The `integrity=` value the shell's `<script>` carries for htmx (D7).
pub const HTMX_INTEGRITY: &str =
    "sha384-HGfztofotfshcF7+8n44JQL2oJmowVChPTg48S+jvZoztPfvwD79OC/LTtG6dMp+";

/// The values `theme=` takes (D12). `auto` follows the viewer's system.
pub const THEMES: [&str; 4] = ["auto", "light", "dark", "high-contrast"];

/// The shell template: `urn:iki:ui:template:shell`.
pub const SHELL: &str = "urn:iki:ui:template:shell";
/// The token stylesheet's template: `urn:iki:ui:template:tokens`.
pub const TOKENS_TEMPLATE: &str = "urn:iki:ui:template:tokens";
/// The token stylesheet: `urn:iki:ui:tokens.css`.
pub const TOKENS_CSS: &str = "urn:iki:ui:tokens.css";

const HTML: &str = "text/html; charset=utf-8";
const JS: &str = "text/javascript";
const CSS: &str = "text/css";
const TEXT: &str = "text/plain; charset=utf-8";
const TURTLE: &str = "text/turtle";
const OCTET_STREAM: &str = "application/octet-stream";
const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

/// One artifact: a name, the file that overrides it, the media type its name fixes, and the
/// compiled-in bytes.
#[derive(Debug, Clone, Copy)]
pub struct Artifact {
    /// The artifact's IRI.
    pub iri: &'static str,
    /// Its endpoint id.
    pub id: &'static str,
    /// The file under [`DIR`] that overrides it.
    pub file: &'static str,
    /// The media type it is served with, from either delivery.
    pub media_type: &'static str,
    /// The compiled-in delivery.
    pub bytes: &'static [u8],
}

/// Every artifact, the whole set a directory can override.
pub const ARTIFACTS: [Artifact; 4] = [
    Artifact {
        iri: "urn:iki:ui:htmx",
        id: "ui-htmx",
        file: "htmx.min.js",
        media_type: JS,
        bytes: include_bytes!("../vendor/htmx-2.0.4.min.js"),
    },
    Artifact {
        iri: "urn:iki:ui:shell.js",
        id: "ui-shell-js",
        file: "shell.js",
        media_type: JS,
        bytes: include_bytes!("../assets/shell.js"),
    },
    Artifact {
        iri: SHELL,
        id: "ui-template-shell",
        file: "shell.html",
        media_type: HTML,
        bytes: include_bytes!("../assets/shell.html"),
    },
    Artifact {
        iri: TOKENS_TEMPLATE,
        id: "ui-template-tokens",
        file: "tokens.template.css",
        media_type: CSS,
        bytes: include_bytes!("../assets/tokens.template.css"),
    },
];

/// The artifact named `iri`, or `None`.
pub fn artifact(iri: &str) -> Option<&'static Artifact> {
    ARTIFACTS.iter().find(|a| a.iri == iri)
}

// --- the artifacts ------------------------------------------------------------------------

/// An artifact: the directory's file if there is one, else the compiled-in bytes.
///
/// The fallback is taken ONLY on the directory atom's own `NotFound` (or `Unresolved`, when
/// no directory is bound), never on any other failure: the field guide's invalidation trap 1.
/// The kernel records the failed read as a dependency of this answer, so the answer is cut
/// when the file appears.
fn artifact_endpoint(a: &'static Artifact) -> AsyncFnEndpoint {
    AsyncFnEndpoint::new(a.id, move |inv: &Invocation<'_>| -> InvokeFuture<'_> {
        Box::pin(async move {
            let file = Iri::parse(format!("{DIR}{}", a.file))
                .map_err(|e| Error::Endpoint(format!("{}: no override IRI: {e}", a.iri)))?;
            let request = Request::new(Verb::Source, file)
                .with_arg("as", ArgRef::Inline(OCTET_STREAM.as_bytes().to_vec()));
            let bytes = match inv.issue(request).await {
                Ok(rep) => rep.bytes,
                Err(Error::NotFound(_)) | Err(Error::Unresolved(_)) => a.bytes.to_vec(),
                Err(other) => return Err(other),
            };
            Ok(Representation::new(ReprType::new(a.media_type), bytes).cacheable())
        })
    })
    .with_description(
        Description::new(a.id)
            .title(format!("The UI artifact {}", a.iri))
            .summary(format!(
                "The shared UI artifact {}: compiled in, or the file {} in the directory a \
                 host binds at {DIR}, which wins when it is there.",
                a.iri, a.file
            ))
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .output(a.media_type),
    )
}

// --- the palettes ------------------------------------------------------------------------

/// `urn:iki:ui:palette:{name}`: a palette table, as CSS declarations (`text/plain`, the face the
/// token template splices) or as Turtle (`as=text/turtle`).
fn palette_endpoint() -> FnEndpoint {
    FnEndpoint::new("ui-palette", |inv: &Invocation<'_>| {
        let asked = inv.request.target.as_str();
        let name = inv.bindings.get("name").unwrap_or_default();
        let Some(p) = palette::palette(name) else {
            return Err(Error::NotFound(format!(
                "{asked}: not a palette (the palettes are: {})",
                palette::PALETTES.map(|p| p.name).join(", ")
            )));
        };
        let turtle = inv
            .inline_str("as")
            .ok()
            .is_some_and(|t| t.split(';').next().unwrap_or_default().trim() == TURTLE);
        Ok(if turtle {
            Representation::new(ReprType::new(TURTLE), p.turtle().into_bytes())
        } else {
            Representation::new(ReprType::new(TEXT), p.declarations().into_bytes())
        }
        .cacheable())
    })
    .with_description(
        Description::new("ui-palette")
            .title("A UI palette")
            .summary(
                "One of the shared palettes (UI conventions D12) as CSS custom-property \
                 declarations, or as Turtle with one triple per token. Compiled in: the \
                 palettes are the data the contrast floor is tested over.",
            )
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .input(
                ArgSpec::new("name")
                    .summary("the palette")
                    .class(XSD_STRING)
                    .binding()
                    .one_of(palette::PALETTES.map(|p| p.name)),
            )
            .output(TEXT)
            .output(TURTLE),
    )
}

// --- the token stylesheet -------------------------------------------------------------------

/// `urn:iki:ui:tokens.css`: the token template filled with the palettes. No view code: a
/// `composeOver` under its own name and contract, as ikigai-toolbox's views are.
fn tokens_endpoint() -> AsyncFnEndpoint {
    let composer = Arc::new(
        ikigai_fn::compose_over(Iri::parse(TOKENS_TEMPLATE).expect("an IRI"))
            .named("ui-tokens-css"),
    );
    AsyncFnEndpoint::new(
        "ui-tokens-css",
        move |inv: &Invocation<'_>| -> InvokeFuture<'_> {
            let composer = composer.clone();
            Box::pin(async move { composer.invoke(inv).await })
        },
    )
    .with_description(
        Description::new("ui-tokens-css")
            .title("The UI token stylesheet")
            .summary(
                "The shared token stylesheet (UI conventions D12): every palette behind \
                 prefers-color-scheme, prefers-contrast and data-theme on <html>, plus the \
                 shell's own rules. Composed from urn:iki:ui:template:tokens and the palettes.",
            )
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .output(CSS),
    )
}

/// The shared artifacts, the palettes and the token stylesheet, named [`SPACE_ID`].
///
/// Configuration-free: the same doors on every call. A host binds this, a directory at
/// [`DIR`] if it overrides anything, and [`pages`] for its own pages.
pub fn space() -> EndpointSpace {
    let mut space = EndpointSpace::new();
    for a in &ARTIFACTS {
        space = space.bind(Exact::new(a.iri), artifact_endpoint(a));
    }
    space
        .bind(Exact::new(TOKENS_CSS), tokens_endpoint())
        .bind(template("urn:iki:ui:palette:{name}"), palette_endpoint())
        .named(space_iri("ui"))
}

fn template(pattern: &str) -> UriTemplate {
    UriTemplate::parse(pattern).expect("a valid URI template")
}

// --- pages ---------------------------------------------------------------------------------

/// Why [`pages`] refused a family.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PagesError(pub String);

impl std::fmt::Display for PagesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PagesError {}

/// A family of pages over the shell: `{base}page:{name}` is the shell filled with the host's
/// `{base}header`, `{base}title:{name}` and `{base}fragment:{name}` (D8: the page composes the
/// fragment, so the two cannot drift).
///
/// `family` is a short name (`[a-z0-9-]+`) that keeps two families in one kernel apart, and
/// `base` is the host's namespace, ending in `:`. gonk's names already have this shape:
/// `urn:iki:gonk:page:…` and `urn:iki:gonk:fragment:…`.
///
/// **Which fragment a page shows is the HOST's decision, never the request's.** The shell
/// splices its fragment raw (`$r`, it is trusted markup), so the fragment's name is fixed by
/// the page's own name and `family`, both bound here: the page sets them as template
/// variables, which win over any request argument of the same name, and the shell reaches
/// the host's names only through this family's aliases. A request cannot name another
/// resource to splice.
///
/// `theme=` is the one request argument the page reads; a value [`THEMES`] does not hold is
/// `auto`, so an unknown theme degrades to a readable page.
///
/// ⚠ `{name}` reaches the host's title and fragment as ONE percent-encoded segment (compose's
/// rule for a template argument in an IRI), so a page name with `:` in it (`item:5`) arrives
/// as `item%3A5`. Bind the fragment and title with a single variable and decode it, or give
/// pages names without reserved characters.
pub fn pages(family: &str, base: &str) -> Result<Alias, PagesError> {
    if family.is_empty()
        || !family
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(PagesError(format!(
            "`{family}`: a family is one or more of a-z, 0-9 and -"
        )));
    }
    if !base.ends_with(':') || Iri::parse(format!("{base}x")).is_err() {
        return Err(PagesError(format!(
            "`{base}`: a base is an IRI prefix that ends in `:`"
        )));
    }
    let doors = EndpointSpace::new().bind(
        template(&format!("{base}page:{{name}}")),
        page_endpoint(family.to_string()),
    );
    let aliases = AliasTable::new()
        .exact_at_root(
            format!("urn:iki:ui:header:{family}"),
            format!("{base}header"),
        )
        .prefix_at_root(
            format!("urn:iki:ui:title:{family}:"),
            format!("{base}title:"),
        )
        .prefix_at_root(
            format!("urn:iki:ui:main:{family}:"),
            format!("{base}fragment:"),
        );
    Ok(Alias::new(Arc::new(aliases), Arc::new(doors)))
}

fn page_endpoint(family: String) -> AsyncFnEndpoint {
    let id = format!("ui-page-{family}");
    let composer =
        Arc::new(ikigai_fn::compose_over(Iri::parse(SHELL).expect("an IRI")).named(id.clone()));
    let description = Description::new(id.as_str())
        .title("A page over the UI shell")
        .summary(format!(
            "The shared page shell (UI conventions D1, D8, D13) filled with the `{family}` \
             family's header, this page's title and this page's fragment."
        ))
        .verb(Verb::Source)
        .verb(Verb::Meta)
        .input(
            ArgSpec::new("name")
                .summary("the page")
                .class(XSD_STRING)
                .binding(),
        )
        .input(
            ArgSpec::new("theme")
                .summary("the palette: auto follows the viewer's system (UI conventions D12)")
                .class(XSD_STRING)
                .one_of(THEMES)
                .default_value("auto"),
        )
        .output(HTML);
    AsyncFnEndpoint::new(id, move |inv: &Invocation<'_>| -> InvokeFuture<'_> {
        let composer = composer.clone();
        let family = family.clone();
        Box::pin(async move {
            let name = inv
                .bindings
                .get("name")
                .ok_or_else(|| Error::MissingArgument("name".to_string()))?;
            let theme = inv
                .inline_str("theme")
                .ok()
                .filter(|t| THEMES.contains(t))
                .unwrap_or("auto");
            let mut vars = Bindings::new();
            vars.insert("family", family.as_str());
            vars.insert("name", name);
            vars.insert("theme", theme);
            composer.invoke(&inv.with_bindings(&vars)).await
        })
    })
    .with_description(description)
}
