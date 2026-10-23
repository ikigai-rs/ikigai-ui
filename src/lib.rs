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
//! | a host's `{base}page:{name}` | composition | the shell over the host's fragment, from [`pages`] or [`Pages`] |
//! | `urn:iki:ui:head:{family}` | data | a family's `<head>` additions, fixed by the host ([`Pages`]) |
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
//! A host's OWN artifacts get the same two deliveries from [`artifacts`]: its list, its
//! directory prefix, the same fallback rule.
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

/// Why a host's configuration ([`artifacts`], [`Pages`]) was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError(pub String);

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ConfigError {}

/// Why [`pages`] refused a family. The same type as [`ConfigError`], under its 0.1 name.
pub type PagesError = ConfigError;

/// A host's own artifacts, with the two deliveries this crate's have (D17): each `iri` answers
/// the file `{dir}{file}` when a directory bound at `dir` has it, and its compiled-in `bytes`
/// otherwise, always with its `media_type`.
///
/// `dir` is an IRI prefix the host binds a directory under (ikigai-fs's `FileEndpoint` at
/// `{dir}{path}`), conventionally ending in `:`. The fallback is taken ONLY on that file read's
/// own `NotFound` (the file is absent) or `Unresolved` (nothing is bound at `dir`); any other
/// failure, a denial included, passes through. So the answer hangs from the directory atom's
/// golden thread and a write to the file recomputes everything over it (the field guide's
/// invalidation trap 1). [`space`] builds this crate's own set with it, at [`DIR`].
///
/// Refused: an empty list, an `iri` or `{dir}{file}` that is not an IRI, and two artifacts
/// with one `iri`. Like [`pages`], the space is anonymous: its names are the host's.
///
/// ```
/// # use futures::executor::block_on;
/// use ikigai_core::{Capability, Iri, Kernel, Request, Verb};
/// use ikigai_ui::Artifact;
///
/// const MINE: [Artifact; 1] = [Artifact {
///     iri: "urn:app:app.css",
///     id: "app-css",
///     file: "app.css",
///     media_type: "text/css",
///     bytes: b"main { margin: 0 auto; }\n",
/// }];
/// let space = ikigai_ui::artifacts(&MINE, "urn:app:dir:").unwrap();
/// // Nothing is bound at urn:app:dir:, so the compiled-in bytes answer.
/// let kernel = Kernel::new(std::sync::Arc::new(space));
/// let css = block_on(kernel.issue(
///     Request::new(Verb::Source, Iri::parse("urn:app:app.css").unwrap()),
///     &Capability::root(),
/// ))
/// .unwrap();
/// assert_eq!(css.bytes, MINE[0].bytes);
/// assert_eq!(css.repr_type.media_type, "text/css");
/// ```
pub fn artifacts(list: &[Artifact], dir: &str) -> Result<EndpointSpace, ConfigError> {
    if list.is_empty() {
        return Err(ConfigError("no artifacts to bind".into()));
    }
    let mut space = EndpointSpace::new();
    let mut seen: Vec<&str> = Vec::new();
    for a in list {
        if seen.contains(&a.iri) {
            return Err(ConfigError(format!(
                "`{}`: two artifacts with one IRI",
                a.iri
            )));
        }
        seen.push(a.iri);
        Iri::parse(a.iri).map_err(|e| ConfigError(format!("`{}`: not an IRI: {e}", a.iri)))?;
        let file = Iri::parse(format!("{dir}{}", a.file)).map_err(|e| {
            ConfigError(format!(
                "`{dir}{}`: {}'s override is not an IRI: {e}",
                a.file, a.iri
            ))
        })?;
        space = space.bind(Exact::new(a.iri), artifact_endpoint(*a, file, dir));
    }
    Ok(space)
}

/// An artifact: the directory's file if there is one, else the compiled-in bytes.
///
/// The fallback is taken ONLY on the directory atom's own `NotFound` (or `Unresolved`, when
/// no directory is bound), never on any other failure: the field guide's invalidation trap 1.
/// The kernel records the failed read as a dependency of this answer, so the answer is cut
/// when the file appears.
fn artifact_endpoint(a: Artifact, file: Iri, dir: &str) -> AsyncFnEndpoint {
    let description = Description::new(a.id)
        .title(format!("The UI artifact {}", a.iri))
        .summary(format!(
            "The UI artifact {}: compiled in, or the file {} in the directory a host binds \
             at {dir}, which wins when it is there.",
            a.iri, a.file
        ))
        .verb(Verb::Source)
        .verb(Verb::Meta)
        .output(a.media_type);
    AsyncFnEndpoint::new(a.id, move |inv: &Invocation<'_>| -> InvokeFuture<'_> {
        let file = file.clone();
        Box::pin(async move {
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
    .with_description(description)
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
    artifacts(&ARTIFACTS, DIR)
        .expect("the compiled-in artifacts are a valid set")
        .bind(Exact::new(TOKENS_CSS), tokens_endpoint())
        .bind(template("urn:iki:ui:palette:{name}"), palette_endpoint())
        .named(space_iri("ui"))
}

fn template(pattern: &str) -> UriTemplate {
    UriTemplate::parse(pattern).expect("a valid URI template")
}

// --- pages ---------------------------------------------------------------------------------

/// A family of pages over the shell: `{base}page:{name}` is the shell filled with the host's
/// `{base}header`, `{base}title:{name}` and `{base}fragment:{name}` (D8: the page composes the
/// fragment, so the two cannot drift). The same as `Pages::new(family, base).build()`; see
/// [`Pages`] for a family with its own stylesheet, script or push stream.
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
/// **A page name reaches the title and the fragment whole** (`{+name}`, RFC 6570's reserved
/// expansion, ikigai-fn 0.5.1): `page:item:5` shows `{base}title:item:5` and
/// `{base}fragment:item:5`, so a host binds a name with `:` in it once, in its own spelling.
/// Because the name is captured from the REQUESTED page IRI, it is bounded first: one or more
/// of `A-Z a-z 0-9 : . _ ~ -` (RFC 3986's unreserved characters and `:`), and anything else, a
/// `/ ? # %` above all, is refused with `InvalidArgument` before anything is read. So a request
/// picks among the host's own `{base}fragment:…` names and nothing more: a host binds nothing
/// under `{base}fragment:` or `{base}title:` that a viewer of its pages should not see.
pub fn pages(family: &str, base: &str) -> Result<Alias, PagesError> {
    Pages::new(family, base).build()
}

/// A page family with the host's own `<head>` additions: [`pages`], configured.
///
/// Everything here is fixed by the host when it builds the family, never by a request. Each
/// value is a URL the door serves, checked with [`ikigai_fn::html::safe_href`] (a path on the
/// page's own origin, or `http(s)://`) and escaped with [`ikigai_fn::html::attr`], the rule `$h`
/// uses, so a value cannot leave its attribute. They are served as `urn:iki:ui:head:{family}`,
/// which the shell splices after its own three, in this order:
///
/// - each [`stylesheet`](Self::stylesheet): `<link rel="stylesheet" href="…">`, after the token
///   stylesheet, so the host's rules win a tie;
/// - [`push`](Self::push): `<meta name="ikigai-ui-push" content="…">`, the door's push stream
///   (D9), which the shell script reads (below);
/// - each [`script`](Self::script): `<script src="…" defer></script>`, after htmx, so it runs
///   with htmx loaded and the shell script's handlers in place.
///
/// **Push (D9 inside D14).** With a push URL (a cli door's is `/_ikigai/push`), the shell
/// script opens ONE `EventSource` on it for every element on the page that declares what it
/// shows with `data-ui-push`: whitespace-separated paths, or empty for the element's own
/// `hx-get`. It subscribes with one `path=` per path; a `cut` event's `paths` are matched to
/// the elements that declared them, and a `resync` matches all of them. A matched element is
/// sent the `ikigai:cut` event, so it declares `hx-trigger="ikigai:cut"` (with `hx-swap` as its
/// answer needs, often `outerHTML`). Its own requests are quiet (no status message, no note,
/// so nothing stacks), and focus is kept: a swap that took the focused element gives focus to
/// the element with the same id. The subscription follows the page: after every swap the
/// script re-reads the declared paths and reopens the stream when they changed. With no push
/// URL, or no element that declares a path, no stream is opened.
///
/// ```
/// # use std::sync::Arc;
/// # use futures::executor::block_on;
/// use ikigai_core::{Capability, EndpointSpace, Exact, Fallback, FnEndpoint, Iri, Kernel,
///     ReprType, Representation, Request, Verb};
///
/// let html = |body: &'static str| {
///     FnEndpoint::new("fixed", move |_| {
///         Ok(Representation::new(ReprType::new("text/html"), body.as_bytes().to_vec()).cacheable())
///     })
/// };
/// let app = EndpointSpace::new()
///     .bind(Exact::new("urn:app:header"), html("<nav aria-label=\"Site\"></nav>"))
///     .bind(Exact::new("urn:app:title:item:5"), html("Item 5"))
///     .bind(Exact::new("urn:app:fragment:item:5"), html("<h1>Item 5</h1>"));
/// let family = ikigai_ui::Pages::new("app", "urn:app:")
///     .stylesheet("/app/app.css")
///     .script("/app/app.js")
///     .push("/_ikigai/push")
///     .build()
///     .unwrap();
/// let kernel = Kernel::new(Arc::new(Fallback::new(vec![
///     Arc::new(ikigai_ui::space()),
///     Arc::new(family),
///     Arc::new(app),
/// ])));
/// let page = block_on(kernel.issue(
///     Request::new(Verb::Source, Iri::parse("urn:app:page:item:5").unwrap()),
///     &Capability::root(),
/// ))
/// .unwrap();
/// let page = String::from_utf8(page.bytes).unwrap();
/// assert!(page.contains("<title>Item 5</title>"));
/// assert!(page.contains("\n<link rel=\"stylesheet\" href=\"/app/app.css\">"));
/// assert!(page.contains("\n<meta name=\"ikigai-ui-push\" content=\"/_ikigai/push\">"));
/// assert!(page.contains("\n<script src=\"/app/app.js\" defer></script>\n</head>"));
/// ```
#[derive(Debug, Clone)]
pub struct Pages {
    family: String,
    base: String,
    stylesheets: Vec<String>,
    scripts: Vec<String>,
    push: Option<String>,
}

impl Pages {
    /// A family named `family` over the host's names under `base`; see [`pages`].
    pub fn new(family: impl Into<String>, base: impl Into<String>) -> Self {
        Pages {
            family: family.into(),
            base: base.into(),
            stylesheets: Vec::new(),
            scripts: Vec::new(),
            push: None,
        }
    }

    /// Link a stylesheet from every page of the family. Repeats, in order.
    pub fn stylesheet(mut self, url: impl Into<String>) -> Self {
        self.stylesheets.push(url.into());
        self
    }

    /// Load a script, deferred, on every page of the family. Repeats, in order.
    pub fn script(mut self, url: impl Into<String>) -> Self {
        self.scripts.push(url.into());
        self
    }

    /// The door's push stream (D9), for the shell script's refetch. A later call replaces an
    /// earlier one.
    pub fn push(mut self, url: impl Into<String>) -> Self {
        self.push = Some(url.into());
        self
    }

    /// The family's names, or why the configuration is refused: a family that is not
    /// `[a-z0-9-]+`, a base that does not end in `:` or is not an IRI prefix, or a URL that
    /// [`safe_href`](ikigai_fn::html::safe_href) refuses.
    pub fn build(self) -> Result<Alias, ConfigError> {
        let Pages {
            family,
            base,
            stylesheets,
            scripts,
            push,
        } = self;
        if family.is_empty()
            || !family
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return Err(ConfigError(format!(
                "`{family}`: a family is one or more of a-z, 0-9 and -"
            )));
        }
        if !base.ends_with(':') || Iri::parse(format!("{base}x")).is_err() {
            return Err(ConfigError(format!(
                "`{base}`: a base is an IRI prefix that ends in `:`"
            )));
        }
        let url = |what: &str, url: &str| -> Result<String, ConfigError> {
            match ikigai_fn::html::safe_href(url) {
                Some(href) if href == url => Ok(ikigai_fn::html::attr(url)),
                _ => Err(ConfigError(format!(
                    "`{url}`: a {what} URL is a path on the page's origin (`/…`) or http(s)://"
                ))),
            }
        };
        let mut head = String::new();
        for sheet in &stylesheets {
            head.push_str(&format!(
                "\n<link rel=\"stylesheet\" href=\"{}\">",
                url("stylesheet", sheet)?
            ));
        }
        if let Some(push) = &push {
            head.push_str(&format!(
                "\n<meta name=\"ikigai-ui-push\" content=\"{}\">",
                url("push", push)?
            ));
        }
        for script in &scripts {
            head.push_str(&format!(
                "\n<script src=\"{}\" defer></script>",
                url("script", script)?
            ));
        }
        let doors = EndpointSpace::new()
            .bind(
                template(&format!("{base}page:{{name}}")),
                page_endpoint(family.clone()),
            )
            .bind(
                Exact::new(format!("urn:iki:ui:head:{family}")),
                head_endpoint(&family, head),
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
}

/// `urn:iki:ui:head:{family}`: the family's `<head>` additions, built once from the host's
/// [`Pages`] and never from a request. Empty for a family with none.
fn head_endpoint(family: &str, head: String) -> FnEndpoint {
    let id = format!("ui-head-{family}");
    FnEndpoint::new(id.clone(), move |_: &Invocation<'_>| {
        Ok(Representation::new(ReprType::new(HTML), head.clone().into_bytes()).cacheable())
    })
    .with_description(
        Description::new(id.as_str())
            .title("A page family's head additions")
            .summary(format!(
                "The `{family}` family's own stylesheets, push stream and scripts, as the \
                 markup the shell splices into <head>. Fixed by the host when it built the \
                 family."
            ))
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .output(HTML),
    )
}

/// Whether `name` is a page name [`pages`] accepts: RFC 3986's unreserved characters and `:`.
fn page_name_ok(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'.' | b'_' | b'~' | b'-'))
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
            // The name reaches the host's names under `{+name}`, so it is bounded before
            // anything is read: no `/ ? # %`, nothing that leaves `{base}fragment:{name}`.
            if !page_name_ok(name) {
                return Err(Error::InvalidArgument {
                    name: "name".to_string(),
                    detail: format!(
                        "`{name}`: a page name is one or more of A-Z, a-z, 0-9 and : . _ ~ -"
                    ),
                });
            }
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
