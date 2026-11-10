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
//! | `urn:iki:ui:template:shell` | artifact | the page shell, a compose template (D1, D8, D13), the theme chooser in its header |
//! | `urn:iki:ui:template:tokens` | artifact | the token stylesheet's template |
//! | `urn:iki:ui:palette:{name}` | data | a palette table ([`palette`]): CSS declarations, or Turtle with `as=text/turtle` |
//! | `urn:iki:ui:tokens.css` | composition | the template filled with the palettes: `auto`, light, dark, high contrast |
//! | a host's `{base}page:{name}` | composition | the shell over the host's fragment, from [`pages`] or [`Pages`] |
//! | `urn:iki:ui:head:{family}` | data | a family's `<head>` additions, fixed by the host ([`Pages`]) |
//! | `urn:iki:ui:theme-chooser:{theme}` | data | the theme chooser ([`theme_chooser`]), `{theme}` selected |
//! | `urn:iki:ui:chooser:{family}:{theme}` | composition | what the shell puts in a family's `<header>`, for ANY family: its override if bound, else the theme chooser ([`CHOOSER`]) |
//! | `urn:iki:ui:form:resource:{iri}` | data | a `form` node generated from `{iri}`'s contract, its Sink's inputs ([`form`]) |
//! | `urn:iki:ui:form:openapi:{doc}:operation:{op}` | data | the same from an OpenAPI 3.1 document's operation ([`form`]) |
//! | `urn:iki:ui:chooser-override:{family}:{theme}` | a host's | a family's own chooser; bound to nothing, it turns the chooser off ([`CHOOSER_OVERRIDE`]) |
//!
//! The model's target resources are [`target::PREFIX`] (`html` and `gpui` descriptors),
//! [`target::SUPPORT`] (the JSON/HTML matrix), and
//! `urn:iki:ui:face:target:{target}:screen:{screen}` (a projected model, in JSON or JSON-LD).
//! See [`target`] for fallback semantics and the kernel's corridor limitation.
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
//! **Sealed.** A directory can replace the artifacts [`ARTIFACTS`] names and the two
//! [`target`] descriptors (`target/html.json`, `target/gpui.json`); it cannot add a name.
//! Nothing here serves a header or a CSP. Those are door configuration (D6).
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

pub mod form;
pub mod model;
pub mod palette;
pub mod target;
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

/// The theme chooser's names: `urn:iki:ui:theme-chooser:{theme}`, one per [`THEMES`] value, each
/// [`theme_chooser`]'s markup with that theme selected.
pub const THEME_CHOOSER: &str = "urn:iki:ui:theme-chooser:";

/// The shell's header chooser: `urn:iki:ui:chooser:{family}:{theme}`, which the shell splices
/// after the family's header. Bound by [`space`] for ANY family: the family's override at
/// [`CHOOSER_OVERRIDE`] when it binds one, else [`theme_chooser`] with `{theme}` selected.
pub const CHOOSER: &str = "urn:iki:ui:chooser:";

/// A family's own header chooser: `urn:iki:ui:chooser-override:{family}:{theme}`. When it is
/// bound, the shell's chooser for that family is what it answers (an empty answer turns the
/// chooser off, which is what [`Pages::theme_chooser(false)`](Pages::theme_chooser) binds).
pub const CHOOSER_OVERRIDE: &str = "urn:iki:ui:chooser-override:";

/// What the theme chooser calls each of [`THEMES`]: words, never an icon alone.
pub const THEME_LABELS: [&str; 4] = ["Auto (system)", "Light", "Dark", "High contrast"];

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

/// The bundled shell artifacts a directory can override. The [`target`] descriptors have
/// their own validated file-backed resources.
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

// --- the theme chooser --------------------------------------------------------------------

/// The theme chooser (D12) with `theme` selected, or `None` for a value [`THEMES`] does not hold.
///
/// A GET form with no `action`, so it submits to the page it is on: a visible "Theme" label, a
/// native `<select name="theme" data-ui-theme>` with the four [`THEMES`] in words
/// ([`THEME_LABELS`]), and an Apply button. With no script, Apply reloads the page with
/// `theme=`, which the page renders (the form's `theme=` replaces the page's query, as a GET
/// form's does). With the shell script, a change applies at once and is remembered in this
/// browser's `localStorage` only, the button is hidden, and the select is kept showing the
/// theme in force. The control's id is `ui-theme`, so a page holds one.
///
/// ```
/// let html = ikigai_ui::theme_chooser("dark").unwrap();
/// assert!(html.contains("<option value=\"dark\" selected>Dark</option>"));
/// assert!(ikigai_ui::theme_chooser("sepia").is_none());
/// ```
pub fn theme_chooser(theme: &str) -> Option<String> {
    if !THEMES.contains(&theme) {
        return None;
    }
    let mut html = String::from(
        "<form class=\"ui-theme\" method=\"get\"><label for=\"ui-theme\">Theme</label> \
         <select id=\"ui-theme\" name=\"theme\" data-ui-theme autocomplete=\"off\">",
    );
    for (value, label) in THEMES.iter().zip(THEME_LABELS) {
        let selected = if *value == theme { " selected" } else { "" };
        html.push_str(&format!(
            "<option value=\"{value}\"{selected}>{label}</option>"
        ));
    }
    html.push_str(
        "</select> <button type=\"submit\" class=\"ui-theme-apply\">Apply</button></form>",
    );
    Some(html)
}

/// `urn:iki:ui:theme-chooser:{theme}`: [`theme_chooser`]'s markup, for the shell and for a
/// family that splices it somewhere else.
fn theme_chooser_endpoint() -> FnEndpoint {
    FnEndpoint::new("ui-theme-chooser", |inv: &Invocation<'_>| {
        let theme = inv.bindings.get("theme").unwrap_or_default();
        let Some(html) = theme_chooser(theme) else {
            return Err(Error::NotFound(format!(
                "{}: not a theme (the themes are: {})",
                inv.request.target.as_str(),
                THEMES.join(", ")
            )));
        };
        Ok(Representation::new(ReprType::new(HTML), html.into_bytes()).cacheable())
    })
    .with_description(
        Description::new("ui-theme-chooser")
            .title("The theme chooser")
            .summary(
                "A GET form that chooses the page's theme (UI conventions D12): a labeled \
                 native select of auto, light, dark and high contrast, with this theme \
                 selected, and a submit button for a page with no script. The shell puts it \
                 in every page's header.",
            )
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .input(
                ArgSpec::new("theme")
                    .summary("the theme shown as selected")
                    .class(XSD_STRING)
                    .binding()
                    .one_of(THEMES),
            )
            .output(HTML),
    )
}

/// Whether `family` is a family name: one or more of `a-z 0-9 -`, the rule [`Pages`] checks.
fn family_ok(family: &str) -> bool {
    !family.is_empty()
        && family
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// `urn:iki:ui:chooser:{family}:{theme}`, for ANY family: the family's override at
/// `urn:iki:ui:chooser-override:{family}:{theme}` when one is bound, else [`theme_chooser`].
///
/// Bound in [`space`], so a page composed over the shell straight from a names file (no
/// [`Pages`]) resolves its header without naming the chooser (ledger #2885). The override is a
/// SEPARATE name because resolution takes the first space that binds a name and hosts put this
/// crate's space first, and a names layer last: a family binding the shell's own name would be
/// shadowed. Reading the override instead lets a family turn the chooser off (or replace it)
/// wherever it sits in the host.
///
/// The fallback is taken only on the override read's own `NotFound` or `Unresolved`, as an
/// artifact's is, so the answer hangs from that atom's thread and binding an override later
/// recomputes it; any other failure, a denial included, passes through.
fn chooser_endpoint() -> AsyncFnEndpoint {
    let description = Description::new("ui-chooser")
        .title("A page family's header chooser")
        .summary(
            "The theme chooser the shell puts in a page family's header, after the family's \
             own header: the family's override at urn:iki:ui:chooser-override:{family}:{theme} \
             when it binds one (an empty one turns the chooser off), else \
             urn:iki:ui:theme-chooser:{theme}.",
        )
        .verb(Verb::Source)
        .verb(Verb::Meta)
        .input(
            ArgSpec::new("family")
                .summary("the page family, one or more of a-z, 0-9 and -")
                .class(XSD_STRING)
                .binding(),
        )
        .input(
            ArgSpec::new("theme")
                .summary("the theme shown as selected")
                .class(XSD_STRING)
                .binding()
                .one_of(THEMES),
        )
        .output(HTML);
    AsyncFnEndpoint::new("ui-chooser", |inv: &Invocation<'_>| -> InvokeFuture<'_> {
        Box::pin(async move {
            let asked = inv.request.target.as_str();
            let family = inv.bindings.get("family").unwrap_or_default();
            let theme = inv.bindings.get("theme").unwrap_or_default();
            if !family_ok(family) {
                return Err(Error::NotFound(format!(
                    "{asked}: not a family (a family is one or more of a-z, 0-9 and -)"
                )));
            }
            let Some(default) = theme_chooser(theme) else {
                return Err(Error::NotFound(format!(
                    "{asked}: not a theme (the themes are: {})",
                    THEMES.join(", ")
                )));
            };
            let own = Iri::parse(format!("{CHOOSER_OVERRIDE}{family}:{theme}"))
                .expect("a checked family and theme make an IRI");
            let bytes = match inv.issue(Request::new(Verb::Source, own)).await {
                Ok(rep) => rep.bytes,
                Err(Error::NotFound(_)) | Err(Error::Unresolved(_)) => default.into_bytes(),
                Err(other) => return Err(other),
            };
            Ok(Representation::new(ReprType::new(HTML), bytes).cacheable())
        })
    })
    .with_description(description)
}

/// `urn:iki:ui:chooser-override:{family}:{theme}` for a family that opted out of the shell's
/// chooser: nothing.
fn no_chooser_endpoint(family: &str) -> FnEndpoint {
    let id = format!("ui-chooser-{family}");
    FnEndpoint::new(id.clone(), |_: &Invocation<'_>| {
        Ok(Representation::new(ReprType::new(HTML), Vec::new()).cacheable())
    })
    .with_description(
        Description::new(id.as_str())
            .title("A page family's header chooser, turned off")
            .summary(format!(
                "The `{family}` family turned the shell's theme chooser off, so its pages' \
                 headers hold only the family's own header."
            ))
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .input(
                ArgSpec::new("theme")
                    .summary("the page's theme, unused")
                    .class(XSD_STRING)
                    .binding(),
            )
            .output(HTML),
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
        .bind(
            template(&format!("{THEME_CHOOSER}{{theme}}")),
            theme_chooser_endpoint(),
        )
        .bind(
            template(&format!("{CHOOSER}{{family}}:{{theme}}")),
            chooser_endpoint(),
        )
        .bind(Exact::new(KIND), kind_list_endpoint())
        .bind(template(&format!("{KIND}:name:{{kind}}")), kind_endpoint())
        .bind(template(&format!("{KIND}:{{rest}}")), kind_seal_endpoint())
        .bind(
            template("urn:iki:ui:example:kind:{kind}"),
            example_endpoint(),
        )
        .bind(Exact::new(model::CONTEXT_IRI), context_endpoint())
        .bind(Exact::new(GALLERY), gallery_endpoint())
        .bind(
            template(&format!("{FORM_RESOURCE}{{iri}}")),
            form_resource_endpoint(),
        )
        .bind(
            template(&format!("{FORM_OPENAPI}{{doc}}:operation:{{op}}")),
            form_openapi_endpoint(),
        )
        .bind(
            template("urn:iki:ui:target:name:{name}"),
            target::descriptor_endpoint(),
        )
        .bind(Exact::new(target::SUPPORT), target::support_endpoint())
        .bind(
            template("urn:iki:ui:face:target:{target}:screen:{screen}"),
            target::face_endpoint(),
        )
        .named(space_iri("ui"))
}

fn template(pattern: &str) -> UriTemplate {
    UriTemplate::parse(pattern).expect("a valid URI template")
}

// --- the node kinds and the model ------------------------------------------------------------

/// The list of node kinds, `urn:iki:ui:kind`, and the prefix of every kind's name,
/// `urn:iki:ui:kind:name:{kind}`. **Sealed**: [`space`] binds `urn:iki:ui:kind:name:{kind}` for
/// EVERY `{kind}` and answers `NotFound` for a name [`model::KINDS`] does not hold, and binds
/// every other name under `urn:iki:ui:kind:` to `NotFound` too (`urn:iki:ui:kind:{rest}`,
/// after the kinds' own), so a host that puts this space before another module never reaches
/// that module anywhere under this prefix (a
/// [`Fallback`](ikigai_core::Fallback) takes the first space that binds a name, and a refusal
/// from it is the answer).
pub const KIND: &str = "urn:iki:ui:kind";
/// The form generated from a resource's contract: `urn:iki:ui:form:resource:{iri}` ([`form`]).
pub const FORM_RESOURCE: &str = "urn:iki:ui:form:resource:";
/// The form generated from an OpenAPI 3.1 document's operation:
/// `urn:iki:ui:form:openapi:{doc}:operation:{op}` ([`form`]).
pub const FORM_OPENAPI: &str = "urn:iki:ui:form:openapi:";
/// The gallery: `urn:iki:ui:screen:name:gallery`.
pub const GALLERY: &str = "urn:iki:ui:screen:name:gallery";

fn fixed(id: &'static str, media_type: &'static str, body: &'static str) -> FnEndpoint {
    FnEndpoint::new(id, move |_: &Invocation<'_>| {
        Ok(Representation::new(ReprType::new(media_type), body.as_bytes().to_vec()).cacheable())
    })
}

fn kind_list_endpoint() -> FnEndpoint {
    fixed("ui-kind", TURTLE, model::KIND_LIST).with_description(
        Description::new("ui-kind")
            .title("The UI node kinds")
            .summary(
                "The closed list of UI node kinds, as the SHACL node shape every node meets: \
                 its type is one of urn:iki:ui:kind:name:{kind}, each listed by rdfs:member.",
            )
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .output(TURTLE),
    )
}

/// Every name under `urn:iki:ui:kind:` that is not a kind's: `NotFound`, here, so no later
/// module answers it. The only Rust is the refusal; an alias cannot answer a refusal.
fn kind_seal_endpoint() -> FnEndpoint {
    FnEndpoint::new("ui-kind-seal", |inv: &Invocation<'_>| {
        Err(Error::NotFound(format!(
            "{}: nothing is named here (the kinds are {KIND}:name:{{kind}})",
            inv.request.target.as_str()
        )))
    })
    .with_description(
        Description::new("ui-kind-seal")
            .title("The rest of the UI kinds' prefix")
            .summary(
                "Every name under urn:iki:ui:kind: that is not a kind's, refused NotFound so \
                 that no other module answers under the prefix.",
            )
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .input(
                ArgSpec::new("rest")
                    .summary("the rest of the name")
                    .class(XSD_STRING)
                    .binding(),
            )
            .output(TURTLE),
    )
}

fn kind_names() -> Vec<&'static str> {
    model::KINDS.iter().map(|k| k.name).collect()
}

fn kind_arg(summary: &str) -> ArgSpec {
    ArgSpec::new("kind")
        .summary(summary)
        .class(XSD_STRING)
        .binding()
        .one_of(kind_names())
}

fn the_kind(inv: &Invocation<'_>) -> Result<&'static model::Kind, Error> {
    let name = inv.bindings.get("kind").unwrap_or_default();
    model::kind(name).ok_or_else(|| {
        Error::NotFound(format!(
            "{}: not a kind (the kinds are: {})",
            inv.request.target.as_str(),
            kind_names().join(", ")
        ))
    })
}

/// Whether the request asked for the JSON-LD face.
fn wants_json_ld(inv: &Invocation<'_>) -> bool {
    inv.inline_str("as")
        .ok()
        .is_some_and(|t| t.split(';').next().unwrap_or_default().trim() == model::JSON_LD)
}

/// A model in the face the request asked for.
fn model_face(inv: &Invocation<'_>, node: &model::Node) -> Result<Representation, Error> {
    let (media_type, bytes) = if wants_json_ld(inv) {
        (
            model::JSON_LD,
            serde_json::to_vec(&model::json_ld(node, inv.request.target.as_str())),
        )
    } else {
        (model::MEDIA_TYPE, serde_json::to_vec(node))
    };
    let bytes = bytes.map_err(|e| Error::Endpoint(format!("a model serializes: {e}")))?;
    Ok(Representation::new(ReprType::new(media_type), bytes).cacheable())
}

fn kind_endpoint() -> FnEndpoint {
    FnEndpoint::new("ui-kind-name", |inv: &Invocation<'_>| {
        let kind = the_kind(inv)?;
        Ok(Representation::new(ReprType::new(TURTLE), kind.shape.as_bytes().to_vec()).cacheable())
    })
    .with_description(
        Description::new("ui-kind-name")
            .title("A UI node kind")
            .summary(
                "One UI node kind as a closed SHACL shape: its properties, the one that is its \
                 fallback text (required), and the children it takes.",
            )
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .input(kind_arg("the kind"))
            .output(TURTLE),
    )
}

fn example_endpoint() -> FnEndpoint {
    FnEndpoint::new("ui-example-kind", |inv: &Invocation<'_>| {
        let kind = the_kind(inv)?;
        let node: model::Node = serde_json::from_str(kind.example)
            .map_err(|e| Error::Endpoint(format!("the {} example: {e}", kind.name)))?;
        model_face(inv, &node)
    })
    .with_description(
        Description::new("ui-example-kind")
            .title("A UI node kind's example")
            .summary(
                "One illustrative node of a kind, as application/vnd.ikigai.ui+json, or as \
                 JSON-LD with as=application/ld+json.",
            )
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .input(kind_arg("the kind"))
            .output(model::MEDIA_TYPE)
            .output(model::JSON_LD),
    )
}

fn context_endpoint() -> FnEndpoint {
    fixed("ui-context", model::JSON_LD, model::CONTEXT).with_description(
        Description::new("ui-context")
            .title("The UI model's JSON-LD context")
            .summary(
                "The one context both faces of a UI model share: a kind is \
                 urn:iki:ui:kind:name:{kind}, a property urn:iki:ui:property:name:{p}.",
            )
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .output(model::JSON_LD),
    )
}

/// The gallery's sections, as [`model::GALLERY`] lists them.
#[derive(serde::Deserialize)]
struct Gallery {
    title: String,
    section: Vec<Section>,
}

#[derive(serde::Deserialize)]
struct Section {
    label: String,
    kind: Vec<String>,
}

/// `urn:iki:ui:screen:name:gallery`: a group per section of [`model::GALLERY`], each a heading
/// and the section's examples, READ from `urn:iki:ui:example:kind:{kind}` so the screen hangs
/// from them. The only Rust is the assembly: a compose template splices text, and this answer
/// is a tree in two faces.
fn gallery_endpoint() -> AsyncFnEndpoint {
    let description = Description::new("ui-screen-gallery")
        .title("The UI gallery")
        .summary(
            "Every node kind's example, grouped into sections, as one screen: \
             application/vnd.ikigai.ui+json, or JSON-LD with as=application/ld+json.",
        )
        .verb(Verb::Source)
        .verb(Verb::Meta)
        .output(model::MEDIA_TYPE)
        .output(model::JSON_LD);
    AsyncFnEndpoint::new(
        "ui-screen-gallery",
        |inv: &Invocation<'_>| -> InvokeFuture<'_> {
            Box::pin(async move {
                let gallery: Gallery = serde_json::from_str(model::GALLERY)
                    .map_err(|e| Error::Endpoint(format!("the gallery list: {e}")))?;
                let text = |p: &str, v: &str| (p.to_string(), model::Value::Text(v.to_string()));
                let mut root = model::Node {
                    kind: "group".into(),
                    fallback: None,
                    properties: [text("label", &gallery.title)].into(),
                    children: Vec::new(),
                };
                for section in gallery.section {
                    let mut group = model::Node {
                        kind: "group".into(),
                        fallback: None,
                        properties: [text("label", &section.label)].into(),
                        children: vec![model::Node {
                            kind: "heading".into(),
                            fallback: None,
                            properties: [
                                text("content", &section.label),
                                ("level".to_string(), model::Value::Number(2)),
                            ]
                            .into(),
                            children: Vec::new(),
                        }],
                    };
                    for kind in &section.kind {
                        let iri = Iri::parse(format!("urn:iki:ui:example:kind:{kind}"))
                            .map_err(|e| Error::Endpoint(format!("{kind}: {e}")))?;
                        let rep = inv.issue(Request::new(Verb::Source, iri)).await?;
                        let node: model::Node = serde_json::from_slice(&rep.bytes)
                            .map_err(|e| Error::Endpoint(format!("{kind}'s example: {e}")))?;
                        group.children.push(node);
                    }
                    root.children.push(group);
                }
                model::validate(&root)?;
                model_face(inv, &root)
            })
        },
    )
    .with_description(description)
}

// --- forms from models (ledger #3354) ---------------------------------------------------------

const XSD_ANY_URI: &str = "http://www.w3.org/2001/XMLSchema#anyURI";

fn bound_iri(inv: &Invocation<'_>, var: &str) -> Result<Iri, Error> {
    let value = inv.bindings.get(var).unwrap_or_default();
    Iri::parse(value).map_err(|e| Error::InvalidArgument {
        name: var.to_string(),
        detail: format!("`{value}` is not an IRI: {e}"),
    })
}

fn form_description(id: &str, title: &str, summary: &str) -> Description {
    Description::new(id)
        .title(title)
        .summary(summary)
        .verb(Verb::Source)
        .verb(Verb::Meta)
        .output(model::MEDIA_TYPE)
        .output(model::JSON_LD)
}

/// `urn:iki:ui:form:resource:{iri}`: [`form::from_contract`] over `{iri}`'s Meta, read as
/// `application/json` through the kernel, so the form hangs from the contract and a changed
/// contract recomputes it. Rust because the mapping from inputs to fields is a function over a
/// parsed contract, which no alias or compose template can express; the read itself is one
/// `issue`.
fn form_resource_endpoint() -> AsyncFnEndpoint {
    let description = form_description(
        "ui-form-resource",
        "A form from a resource's contract",
        "A form node generated from the resource's own contract (its Meta): one field per \
         input its Sink reads, required or optional, typed by its datatype, a choice for a \
         closed set, its default as the initial value. The contract is the model; OpenAPI is \
         one projection of it (urn:iki:ui:form:openapi:{doc}:operation:{op}).",
    )
    .input(
        ArgSpec::new("iri")
            .summary("the resource whose Sink the form fills in")
            .class(XSD_ANY_URI)
            .binding()
            .example("urn:iki:ui:kind"),
    );
    AsyncFnEndpoint::new(
        "ui-form-resource",
        |inv: &Invocation<'_>| -> InvokeFuture<'_> {
            Box::pin(async move {
                let iri = bound_iri(inv, "iri")?;
                let request = Request::new(Verb::Meta, iri.clone())
                    .with_arg("as", ArgRef::Inline(b"application/json".to_vec()));
                let contract = inv.issue(request).await?;
                let node = form::from_contract(iri.as_str(), &contract.bytes)?;
                model_face(inv, &node)
            })
        },
    )
    .with_description(description)
}

/// `urn:iki:ui:form:openapi:{doc}:operation:{op}`: [`form::from_openapi`] over the document
/// `{doc}`, read through the kernel (so the form hangs from it). Rust for the same reason as
/// [`form_resource_endpoint`], and because no vetted crate reads OpenAPI 3.1 ([`form`]).
fn form_openapi_endpoint() -> AsyncFnEndpoint {
    let description = form_description(
        "ui-form-openapi",
        "A form from an OpenAPI operation",
        "A form node generated from an OpenAPI 3.1 document's operation (by operationId): \
         one field per property of its request body's schema, the same function as \
         urn:iki:ui:form:resource:{iri}, so the same model gives the same form.",
    )
    .input(
        ArgSpec::new("doc")
            .summary("the OpenAPI 3.1 document, JSON, by its IRI")
            .class(XSD_ANY_URI)
            .binding()
            .example("urn:example:openapi"),
    )
    .input(
        ArgSpec::new("op")
            .summary("the operation's operationId")
            .class(XSD_STRING)
            .binding()
            .example("contact"),
    );
    AsyncFnEndpoint::new(
        "ui-form-openapi",
        |inv: &Invocation<'_>| -> InvokeFuture<'_> {
            Box::pin(async move {
                let doc = bound_iri(inv, "doc")?;
                let op = inv.bindings.get("op").unwrap_or_default().to_string();
                let bytes = inv.issue(Request::new(Verb::Source, doc.clone())).await?;
                let node = form::from_openapi(doc.as_str(), &bytes.bytes, &op)?;
                model_face(inv, &node)
            })
        },
    )
    .with_description(description)
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
/// **The theme chooser.** Every page's `<header>` holds the family's `{base}header` and then
/// [`theme_chooser`] with the page's theme selected (`urn:iki:ui:chooser:{family}:{theme}`,
/// which [`space`] binds for every family). A family that wants it somewhere else turns it off
/// with [`theme_chooser(false)`](Self::theme_chooser) and splices
/// `urn:iki:ui:theme-chooser:{theme}` where it wants it; a family that wants none turns it off.
/// Turning it off binds the family's override, `urn:iki:ui:chooser-override:{family}:{theme}`
/// ([`CHOOSER_OVERRIDE`]), to nothing; a page composed over the shell without [`Pages`] does
/// the same by binding that name itself.
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
    chooser: bool,
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
            chooser: true,
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

    /// Whether the shell puts the theme chooser in the family's headers: on unless turned off.
    /// Off binds `urn:iki:ui:chooser-override:{family}:{theme}` to nothing, so it holds wherever
    /// the host puts this family beside [`space`].
    pub fn theme_chooser(mut self, on: bool) -> Self {
        self.chooser = on;
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
            chooser,
        } = self;
        if !family_ok(&family) {
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
        let mut doors = EndpointSpace::new()
            .bind(
                template(&format!("{base}page:{{name}}")),
                page_endpoint(family.clone()),
            )
            .bind(
                Exact::new(format!("urn:iki:ui:head:{family}")),
                head_endpoint(&family, head),
            );
        // The chooser itself is [`space`]'s, for every family; turning it off binds the
        // family's override, which [`space`]'s chooser reads wherever this family sits.
        if !chooser {
            doors = doors.bind(
                template(&format!("{CHOOSER_OVERRIDE}{family}:{{theme}}")),
                no_chooser_endpoint(&family),
            );
        }
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
