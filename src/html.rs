//! The `html` target as one compose template per node kind (ledger #3355).
//!
//! | name | what it is |
//! |---|---|
//! | `urn:iki:ui:template:kind:{kind}` | the kind's ikigai-fn compose template, `assets/template/kind/{kind}.html`, overridable from `template/kind/{kind}.html` in the UI directory |
//! | `urn:iki:ui:html:screen:{screen}` | the screen's HTML: its root node's view |
//! | `urn:iki:ui:html:screen:{screen}:node:{node}` | one node's view: its kind's template, composed |
//! | `urn:iki:ui:html:screen:{screen}:node:{node}:property:{property}` | one property's value, text |
//! | `urn:iki:ui:html:screen:{screen}:node:{node}:child` | the node's children's views, in order |
//! | `urn:iki:ui:html:empty` | nothing: the fallback a template names for an absent optional property |
//!
//! A node is named by its position (`{node}` is `n` for the root, `n.0.2` for the root's first
//! child's third child), so no node is blank and the name is a valid HTML id fragment.
//!
//! **What is a template, and what is Rust.** All markup a kind produces is in its template:
//! every value splices with `$h{…}` (escaped, never expanded) and only the composed child views
//! splice raw with `$r{…}`. The Rust is data selection, not markup:
//!
//! - the node view picks the template by the node's kind and binds `{screen}` and `{node}`: a
//!   compose template names a template by a fixed IRI, so the dispatch on data (the kind) is
//!   the one step a composition cannot take;
//! - the property face answers one property as text, with the kind's documented defaults
//!   (`input` is `text`, `required` and `invalid` are `false`) and `fallback`, the kind's
//!   fallback text, so a template needs no conditional;
//! - the child face joins the children's views in order. A compose template has no loop over
//!   a computed list, and `each` takes its list and its template as inline arguments, which a
//!   template cannot compute from `{node}`. Where the parent's element needs one per child
//!   (`<li>` in a `list` and a `nav`; `<tr>`, `<td>` and `<th scope=…>` in a `table`) the
//!   child face writes that wrapper: it depends on the child's position, the row's `header`
//!   (see [`crate::model::SCOPES`]) and the child's role ([`crate::model::ROLE`], a
//!   `ui-role-{role}` class from a fixed set), which a template filled once per node cannot see.
//!   This is the table exception the item allows.
//!
//! An `option` outside a `choice` has no `<select>` to belong to, so it renders through the
//! `text` template, which shows the kind's fallback text (its label).

use std::collections::BTreeMap;
use std::sync::Arc;

use ikigai_core::{
    ArgSpec, AsyncFnEndpoint, Bindings, Description, Endpoint, Error, FnEndpoint, Invocation,
    InvokeFuture, Iri, ReprType, Representation, Request, Verb,
};

use crate::model::{self, Node, Value};

/// A kind's template is this plus the kind's name.
pub const TEMPLATE_PREFIX: &str = "urn:iki:ui:template:kind:";
/// A screen's HTML is this plus the screen's name.
pub const SCREEN_PREFIX: &str = "urn:iki:ui:html:screen:";
/// Nothing, the fallback for an absent optional property.
pub const EMPTY: &str = "urn:iki:ui:html:empty";

macro_rules! template {
    ($($kind:literal),*) => {
        [$(crate::Artifact {
            iri: concat!("urn:iki:ui:template:kind:", $kind),
            id: "ui-template-kind",
            file: concat!("template/kind/", $kind, ".html"),
            media_type: crate::HTML,
            bytes: include_bytes!(concat!("../assets/template/kind/", $kind, ".html")),
        }),*]
    };
}

/// The compiled-in templates, one per kind of [`model::KINDS`].
pub const TEMPLATES: [crate::Artifact; 16] = template!(
    "heading", "text", "field", "choice", "option", "button", "link", "list", "table", "form",
    "nav", "status", "image", "group", "visual", "scene"
);

/// An element's native template is this plus the element's name.
pub const ELEMENT_TEMPLATE_PREFIX: &str = "urn:iki:ui:template:element:";

/// The disclosure element's IRI, an element the html face claims natively.
pub const DISCLOSURE: &str = "urn:iki:ui:element:name:disclosure";

/// The compiled-in native templates of the elements the html descriptor claims
/// (`elements`): `<details>`/`<summary>` for disclosure and a labeled `<meter>`.
pub const ELEMENT_TEMPLATES: [crate::Artifact; 2] = [
    crate::Artifact {
        iri: "urn:iki:ui:template:element:disclosure",
        id: "ui-template-element",
        file: "template/element/disclosure.html",
        media_type: crate::HTML,
        bytes: include_bytes!("../assets/template/element/disclosure.html"),
    },
    crate::Artifact {
        iri: "urn:iki:ui:template:element:meter",
        id: "ui-template-element",
        file: "template/element/meter.html",
        media_type: crate::HTML,
        bytes: include_bytes!("../assets/template/element/meter.html"),
    },
];

/// A hinted template is this plus its name: `{kind}-{presentation}`.
pub const HINT_TEMPLATE_PREFIX: &str = "urn:iki:ui:template:hint:";

/// The compiled-in templates of the presentation hints the html descriptor claims (`hints`):
/// a `choice` as radio buttons, a `<fieldset>` with its `<legend>`, and each of its options as
/// one radio input with its label (ledger #3537, [`model::RADIO`]); groups as wrapping columns;
/// plain GET navigation forms, which must not swap a page into the form alone; and the
/// workspace's layout hints (ledger #3950, [`crate::workspace::HINTS`]): a split as a flex row
/// or column sized by its shares, tabs as a real tab list, and a pane as a section with its
/// fragment id; and a field whose `input` is `hidden` as the hidden input alone, with no label,
/// hint or note (ledger #3967), since there is no control for a label to name; and a pane's
/// "Pane" menu as a `<details>` of its edit forms (ledger #3952); a pane's header toggle (its
/// Collapse or Expand with `aria-expanded`, its Maximize or Restore with `aria-pressed`) as a
/// one-button form, Restore's describing the sentence the maximized page says, which is
/// rendered at load as `#ui-said` (not a live change) and marked for the shell's script to move
/// focus to Restore, so a screen reader says its name, role and the sentence once (ledger
/// #3953).
pub const HINT_TEMPLATES: [crate::Artifact; 13] = [
    crate::Artifact {
        iri: "urn:iki:ui:template:hint:choice-radio",
        id: "ui-template-hint",
        file: "template/hint/choice-radio.html",
        media_type: crate::HTML,
        bytes: include_bytes!("../assets/template/hint/choice-radio.html"),
    },
    crate::Artifact {
        iri: "urn:iki:ui:template:hint:option-radio",
        id: "ui-template-hint",
        file: "template/hint/option-radio.html",
        media_type: crate::HTML,
        bytes: include_bytes!("../assets/template/hint/option-radio.html"),
    },
    crate::Artifact {
        iri: "urn:iki:ui:template:hint:group-columns",
        id: "ui-template-hint",
        file: "template/hint/group-columns.html",
        media_type: crate::HTML,
        bytes: include_bytes!("../assets/template/hint/group-columns.html"),
    },
    crate::Artifact {
        iri: "urn:iki:ui:template:hint:form-navigation",
        id: "ui-template-hint",
        file: "template/hint/form-navigation.html",
        media_type: crate::HTML,
        bytes: include_bytes!("../assets/template/hint/form-navigation.html"),
    },
    crate::Artifact {
        iri: "urn:iki:ui:template:hint:text-cited",
        id: "ui-template-hint",
        file: "template/hint/text-cited.html",
        media_type: crate::HTML,
        bytes: include_bytes!("../assets/template/hint/text-cited.html"),
    },
    crate::Artifact {
        iri: "urn:iki:ui:template:hint:group-split",
        id: "ui-template-hint",
        file: "template/hint/group-split.html",
        media_type: crate::HTML,
        bytes: include_bytes!("../assets/template/hint/group-split.html"),
    },
    crate::Artifact {
        iri: "urn:iki:ui:template:hint:group-tabs",
        id: "ui-template-hint",
        file: "template/hint/group-tabs.html",
        media_type: crate::HTML,
        bytes: include_bytes!("../assets/template/hint/group-tabs.html"),
    },
    crate::Artifact {
        iri: "urn:iki:ui:template:hint:group-pane",
        id: "ui-template-hint",
        file: "template/hint/group-pane.html",
        media_type: crate::HTML,
        bytes: include_bytes!("../assets/template/hint/group-pane.html"),
    },
    crate::Artifact {
        iri: "urn:iki:ui:template:hint:field-hidden",
        id: "ui-template-hint",
        file: "template/hint/field-hidden.html",
        media_type: crate::HTML,
        bytes: include_bytes!("../assets/template/hint/field-hidden.html"),
    },
    crate::Artifact {
        iri: "urn:iki:ui:template:hint:group-menu",
        id: "ui-template-hint",
        file: "template/hint/group-menu.html",
        media_type: crate::HTML,
        bytes: include_bytes!("../assets/template/hint/group-menu.html"),
    },
    crate::Artifact {
        iri: "urn:iki:ui:template:hint:form-toggle",
        id: "ui-template-hint",
        file: "template/hint/form-toggle.html",
        media_type: crate::HTML,
        bytes: include_bytes!("../assets/template/hint/form-toggle.html"),
    },
    crate::Artifact {
        iri: "urn:iki:ui:template:hint:form-restore",
        id: "ui-template-hint",
        file: "template/hint/form-restore.html",
        media_type: crate::HTML,
        bytes: include_bytes!("../assets/template/hint/form-restore.html"),
    },
    crate::Artifact {
        iri: "urn:iki:ui:template:hint:text-said",
        id: "ui-template-hint",
        file: "template/hint/text-said.html",
        media_type: crate::HTML,
        bytes: include_bytes!("../assets/template/hint/text-said.html"),
    },
];

fn argument(name: &str, summary: &str) -> ArgSpec {
    ArgSpec::new(name)
        .summary(summary)
        .class(crate::XSD_STRING)
        .binding()
}

fn kind_names() -> Vec<&'static str> {
    model::KINDS.iter().map(|k| k.name).collect()
}

pub(crate) fn template_endpoint() -> AsyncFnEndpoint {
    AsyncFnEndpoint::new(
        "ui-template-kind",
        |inv: &Invocation<'_>| -> InvokeFuture<'_> {
            Box::pin(async move {
                let kind = inv.bindings.get("kind").unwrap_or_default();
                let artifact = TEMPLATES
                    .iter()
                    .find(|a| a.iri.strip_prefix(TEMPLATE_PREFIX) == Some(kind))
                    .ok_or_else(|| Error::NotFound(format!("{kind}: not a kind")))?;
                let file = Iri::parse(format!("{}{}", crate::DIR, artifact.file))
                    .map_err(|e| Error::Endpoint(e.to_string()))?;
                crate::artifact_endpoint(*artifact, file, crate::DIR)
                    .invoke(inv)
                    .await
            })
        },
    )
    .with_description(
        Description::new("ui-template-kind")
            .title("A UI kind's HTML template")
            .summary(
                "The ikigai-fn compose template the html target fills for one node kind: values \
                 splice escaped ($h), child views raw ($r). From template/kind/{kind}.html in \
                 the UI directory, else compiled in.",
            )
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .input(argument("kind", "the kind").one_of(kind_names()))
            .output(crate::HTML),
    )
}

pub(crate) fn element_template_endpoint() -> AsyncFnEndpoint {
    AsyncFnEndpoint::new(
        "ui-template-element",
        |inv: &Invocation<'_>| -> InvokeFuture<'_> {
            Box::pin(async move {
                let name = inv.bindings.get("name").unwrap_or_default();
                let artifact = ELEMENT_TEMPLATES
                    .iter()
                    .find(|a| a.iri.strip_prefix(ELEMENT_TEMPLATE_PREFIX) == Some(name))
                    .ok_or_else(|| Error::NotFound(format!("{name}: no native html template")))?;
                let file = Iri::parse(format!("{}{}", crate::DIR, artifact.file))
                    .map_err(|e| Error::Endpoint(e.to_string()))?;
                crate::artifact_endpoint(*artifact, file, crate::DIR)
                    .invoke(inv)
                    .await
            })
        },
    )
    .with_description(
        Description::new("ui-template-element")
            .title("A UI element's native HTML template")
            .summary(
                "The ikigai-fn compose template the html target fills for a custom element it \
                 claims natively (its descriptor's elements): values splice escaped ($h), child \
                 views raw ($r). From template/element/{name}.html in the UI directory, else \
                 compiled in.",
            )
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .input(
                argument("name", "the element's name").one_of(
                    ELEMENT_TEMPLATES
                        .iter()
                        .map(|a| &a.iri[ELEMENT_TEMPLATE_PREFIX.len()..]),
                ),
            )
            .output(crate::HTML),
    )
}

pub(crate) fn hint_template_endpoint() -> AsyncFnEndpoint {
    AsyncFnEndpoint::new(
        "ui-template-hint",
        |inv: &Invocation<'_>| -> InvokeFuture<'_> {
            Box::pin(async move {
                let name = inv.bindings.get("name").unwrap_or_default();
                let artifact = HINT_TEMPLATES
                    .iter()
                    .find(|a| a.iri.strip_prefix(HINT_TEMPLATE_PREFIX) == Some(name))
                    .ok_or_else(|| Error::NotFound(format!("{name}: no hinted html template")))?;
                let file = Iri::parse(format!("{}{}", crate::DIR, artifact.file))
                    .map_err(|e| Error::Endpoint(e.to_string()))?;
                crate::artifact_endpoint(*artifact, file, crate::DIR)
                    .invoke(inv)
                    .await
            })
        },
    )
    .with_description(
        Description::new("ui-template-hint")
            .title("A UI kind's HTML template under a presentation hint")
            .summary(
                "The ikigai-fn compose template the html target fills for a node whose \
                 presentation hint it claims (its descriptor's hints), for example a choice as \
                 radio buttons: values splice escaped ($h), child views raw ($r). From \
                 template/hint/{name}.html in the UI directory, else compiled in.",
            )
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .input(
                argument("name", "the template's name, {kind}-{presentation}").one_of(
                    HINT_TEMPLATES
                        .iter()
                        .map(|a| &a.iri[HINT_TEMPLATE_PREFIX.len()..]),
                ),
            )
            .output(crate::HTML),
    )
}

pub(crate) fn empty_endpoint() -> FnEndpoint {
    FnEndpoint::new("ui-html-empty", |_: &Invocation<'_>| {
        Ok(Representation::new(ReprType::new(crate::TEXT), Vec::new()).cacheable())
    })
    .with_description(
        Description::new("ui-html-empty")
            .title("Nothing")
            .summary(
                "Empty text: the fallback a kind template names for an absent optional property.",
            )
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .output(crate::TEXT),
    )
}

/// The screen's model, read through the kernel so every view hangs from it.
async fn screen(inv: &Invocation<'_>) -> Result<Node, Error> {
    let name = crate::lens::unescape_screen(inv.bindings.get("screen").unwrap_or_default());
    let iri = Iri::parse(format!("urn:iki:ui:screen:name:{name}")).map_err(|e| {
        Error::InvalidArgument {
            name: "screen".into(),
            detail: e.to_string(),
        }
    })?;
    let rep = inv.issue(Request::new(Verb::Source, iri)).await?;
    let node = model::parse(&rep.bytes)?;
    // Composed for the html descriptor (ledger #3358, #3361): references resolved under the
    // caller's capability (withheld when denied), regions in their narrowed spaces, custom
    // elements down the ladder (kept whole when the descriptor claims them, `disclosure` by
    // its <details> template; else expanded, else their fallback, else their fallback text) and write-only fields made their own forms.
    let descriptor = crate::target::read_descriptor(inv, "html").await?;
    let node = crate::target::compose(inv, &node, &descriptor).await?;
    // A descriptor that does not claim the choice hints (a host's override) keeps the select.
    let node = if descriptor.hints.iter().any(|ns| ns == model::CHOICE_HINTS) {
        node
    } else {
        without_hint_namespace(node, model::CHOICE_HINTS)
    };
    let node = if descriptor.hints.iter().any(|ns| ns == model::GROUP_HINTS) {
        node
    } else {
        without_hint_namespace(node, model::GROUP_HINTS)
    };
    // The workspace's layout (ledger #3950): a descriptor that does not claim the layout hints
    // gets the linear form, the panes in reading order, each still its own labeled section.
    let node = if descriptor
        .hints
        .iter()
        .any(|ns| ns == crate::workspace::HINTS)
    {
        node
    } else {
        without_hint_namespace(node, crate::workspace::HINTS)
    };
    // A descriptor that does not claim `scene` (a host serving no WebGPU renderer) takes the
    // ladder's next rung for each one: its replacement, else its label as text (ledger #3514).
    if descriptor.renders.iter().any(|k| k == "scene") {
        return Ok(node);
    }
    Ok(without_scenes(node))
}

/// `node` as a face that does not claim the workspace's layout hints sees it (ledger #3950):
/// every `urn:iki:ui:hint:layout:` hint dropped, so splits and tabs are plain groups and the
/// panes come in reading order.
pub fn without_layout_hints(node: Node) -> Node {
    without_hint_namespace(node, crate::workspace::HINTS)
}

fn without_hint_namespace(mut node: Node, namespace: &str) -> Node {
    node.hint.retain(|key, _| !key.starts_with(namespace));
    node.children = node
        .children
        .into_iter()
        .map(|n| without_hint_namespace(n, namespace))
        .collect();
    node.replacement = node
        .replacement
        .map(|r| Box::new(without_hint_namespace(*r, namespace)));
    node
}

fn without_scenes(mut node: Node) -> Node {
    if node.kind == "scene" {
        return match node.replacement.take() {
            Some(replacement) => without_scenes(*replacement),
            None => Node {
                kind: "text".into(),
                hint: Default::default(),
                replacement: None,
                properties: node
                    .properties
                    .remove("label")
                    .map(|label| [("content".to_string(), label)].into_iter().collect())
                    .unwrap_or_default(),
                children: Vec::new(),
            },
        };
    }
    node.children = node.children.into_iter().map(without_scenes).collect();
    node
}

/// The node at `{node}` and its parent.
fn locate<'a>(root: &'a Node, at: &str) -> Result<(&'a Node, Option<&'a Node>), Error> {
    let missing = || Error::NotFound(format!("{at}: no such node"));
    let mut parts = at.split('.');
    if parts.next() != Some("n") {
        return Err(missing());
    }
    let (mut node, mut parent) = (root, None);
    for part in parts {
        // `r` is the node's replacement: a scene's accessible form, drawn beside its canvas.
        if part == "r" {
            let replacement = node.replacement.as_deref().ok_or_else(missing)?;
            parent = Some(node);
            node = replacement;
            continue;
        }
        let i: usize = part.parse().map_err(|_| missing())?;
        let child = node.children.get(i).ok_or_else(missing)?;
        parent = Some(node);
        node = child;
    }
    Ok((node, parent))
}

fn screen_arg() -> ArgSpec {
    argument("screen", "the screen, urn:iki:ui:screen:name:{screen}")
}

fn node_arg() -> ArgSpec {
    argument("node", "the node's position: n, then .{index} per level")
}

fn html_rep(bytes: Vec<u8>) -> Representation {
    Representation::new(ReprType::new(crate::HTML), bytes).cacheable()
}

/// One composer per kind template, and one per native element template keyed by the
/// element's IRI.
fn composers() -> Arc<BTreeMap<&'static str, ikigai_fn::ComposeOver>> {
    let kinds = model::KINDS.iter().map(|k| {
        let iri = Iri::parse(format!("{TEMPLATE_PREFIX}{}", k.name)).expect("an IRI");
        (k.name, ikigai_fn::compose_over(iri).named("ui-html-node"))
    });
    let elements = ELEMENT_TEMPLATES.iter().map(|a| {
        let name = &a.iri[ELEMENT_TEMPLATE_PREFIX.len()..];
        let element: &'static str = match name {
            "disclosure" => DISCLOSURE,
            "meter" => crate::meter::IRI,
            _ => unreachable!("every native template names its element"),
        };
        let iri = Iri::parse(a.iri).expect("an IRI");
        (element, ikigai_fn::compose_over(iri).named("ui-html-node"))
    });
    let hinted = HINT_TEMPLATES.iter().map(|a| {
        let iri = Iri::parse(a.iri).expect("an IRI");
        (
            &a.iri[HINT_TEMPLATE_PREFIX.len()..],
            ikigai_fn::compose_over(iri).named("ui-html-node"),
        )
    });
    Arc::new(kinds.chain(elements).chain(hinted).collect())
}

async fn view(
    inv: &Invocation<'_>,
    composers: &BTreeMap<&'static str, ikigai_fn::ComposeOver>,
    at: &str,
) -> Result<Representation, Error> {
    // A lensed screen folds the browser's `lens=` and facet parameters into its name, so every
    // node below is composed from the same, cacheable name (ledger #3531).
    let given = inv.bindings.get("screen").unwrap_or_default().to_string();
    let screen = crate::lens::fold(inv, &given)?;
    let screen = crate::workspace::fold(inv, &screen)?;
    let mut named = Bindings::new();
    named.insert("screen", screen.as_str());
    let inv = &inv.with_bindings(&named);
    let root = self::screen(inv).await?;
    let (node, parent) = locate(&root, at)?;
    if !composers.contains_key(node.kind.as_str()) && node.kind != "option" {
        return Err(Error::NotFound(format!(
            "{at}: the html face has no template for {}",
            node.kind
        )));
    }
    // A choice the screen asks to show as radio buttons, its hint kept by [`screen`] only when
    // the descriptor claims it (ledger #3537): a fieldset, and a radio input per option.
    let in_choice = parent.filter(|p| p.kind == "choice");
    let kind = match node.kind.as_str() {
        "option" if in_choice.is_some_and(model::is_radio) => "option-radio",
        "option" if in_choice.is_none() => "text",
        "choice" if model::is_radio(node) => "choice-radio",
        "text" if model::role(node) == Some("cited") => "text-cited",
        "group" if model::is_columns(node) => "group-columns",
        "group" if layout(node, crate::workspace::SPLIT).is_some() => "group-split",
        "group" if layout(node, crate::workspace::TABS).is_some() => "group-tabs",
        "group" if layout(node, crate::workspace::PANE).is_some() => "group-pane",
        "group" if layout(node, crate::workspace::MENU).is_some() => "group-menu",
        "field" if property(node, "input").as_deref() == Some("hidden") => "field-hidden",
        // Restore (only on a maximized page, which says its sentence as `#ui-said`) is described
        // by that sentence (ledger #3953).
        "form" if hint_text(node, crate::workspace::TOGGLE) == Some("pressed true") => {
            "form-restore"
        }
        "form" if hint_text(node, crate::workspace::TOGGLE).is_some() => "form-toggle",
        "text" if hint_text(node, crate::workspace::SAID).is_some() => "text-said",
        // A read-only form without a Sink resource navigates. Enhancing it with
        // hx-target=this would put the returned page inside the controls alone.
        "form"
            if property(node, "method").as_deref() == Some("get")
                && !node.properties.contains_key("resource") =>
        {
            "form-navigation"
        }
        kind => kind,
    };
    let mut vars = Bindings::new();
    vars.insert("screen", screen.as_str());
    vars.insert("node", at);
    let rep = composers[kind].invoke(&inv.with_bindings(&vars)).await?;
    Ok(html_rep(rep.bytes))
}

pub(crate) fn screen_endpoint() -> AsyncFnEndpoint {
    let composers = composers();
    AsyncFnEndpoint::new(
        "ui-html-screen",
        move |inv: &Invocation<'_>| -> InvokeFuture<'_> {
            let composers = composers.clone();
            Box::pin(async move { view(inv, &composers, "n").await })
        },
    )
    .with_description(
        Description::new("ui-html-screen")
            .title("A screen as HTML")
            .summary(
                "The screen's html face: its root node's kind template, composed over the \
                 node's properties and its children's views.",
            )
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .input(screen_arg())
            .output(crate::HTML),
    )
}

pub(crate) fn node_endpoint() -> AsyncFnEndpoint {
    let composers = composers();
    AsyncFnEndpoint::new(
        "ui-html-node",
        move |inv: &Invocation<'_>| -> InvokeFuture<'_> {
            let composers = composers.clone();
            Box::pin(async move {
                let at = inv.bindings.get("node").unwrap_or_default().to_string();
                view(inv, &composers, &at).await
            })
        },
    )
    .with_description(
        Description::new("ui-html-node")
            .title("A UI node as HTML")
            .summary(
                "One node of a screen through its kind's compose template \
                 (urn:iki:ui:template:kind:{kind}).",
            )
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .input(screen_arg())
            .input(node_arg())
            .output(crate::HTML),
    )
}

/// A layout hint's text on a group (ledger #3950), kept by [`screen`] only when the descriptor
/// claims [`crate::workspace::HINTS`].
fn layout<'a>(node: &'a Node, key: &str) -> Option<&'a str> {
    match node.hint.get(key) {
        Some(model::Hint::Plain(Value::Text(s))) if node.kind == "group" => Some(s),
        _ => None,
    }
}

/// A workspace hint's text on a node of any kind (a pane's header toggle, ledger #3953).
fn hint_text<'a>(node: &'a Node, key: &str) -> Option<&'a str> {
    match node.hint.get(key) {
        Some(model::Hint::Plain(Value::Text(s))) => Some(s),
        _ => None,
    }
}

/// A pane key as it enters markup: only `[a-z0-9-]`, which [`crate::workspace::validate`]
/// already holds every key to; anything else is refused rather than escaped.
fn pane_key(node: &Node) -> Result<&str, Error> {
    let key = layout(node, crate::workspace::PANE).unwrap_or_default();
    if crate::workspace::is_key(key) {
        Ok(key)
    } else {
        Err(Error::Endpoint(format!("`{key}` is not a pane key")))
    }
}

/// Text for an attribute or an element's content.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// A layout child's lead pane: its key and its label (a pane's own; a tabs group's active
/// pane's; a split's first child's), the pane a splitter names for the space it sizes.
fn lead(node: &Node) -> Result<Option<(&str, &str)>, Error> {
    if layout(node, crate::workspace::PANE).is_some() {
        let key = pane_key(node)?;
        return Ok(Some((key, text_of(node, "label").unwrap_or(key))));
    }
    if let Some(active) = layout(node, crate::workspace::TABS) {
        for child in &node.children {
            if pane_key(child)? == active {
                return lead(child);
            }
        }
        return Ok(None);
    }
    match node.children.first() {
        Some(first) if layout(node, crate::workspace::SPLIT).is_some() => lead(first),
        _ => Ok(None),
    }
}

/// A share as a percentage in words: `30%`, or `33.3%`.
fn percent(share: u32) -> String {
    if share.is_multiple_of(10) {
        format!("{}%", share / 10)
    } else {
        format!("{}.{}%", share / 10, share % 10)
    }
}

/// The splitter between a split's children `i` and `i + 1` (ledger #3951, design section 5A
/// "Resize by dragging"): a `role="separator"` whose value is child `i`'s share in thousandths,
/// from [`crate::workspace::MIN_SHARE`] to what leaves the other its least, said in words
/// ("Dashboard 30%, Session 70%") and naming the two panes it sizes. It is not focusable in
/// markup: with no script nothing would move it, and the "Layout" form sizes every pane. The
/// shell's script (section 8) puts it in the tab order and moves it by pointer and keyboard,
/// saving through that form. None beside a collapsed child, which keeps its share.
fn splitter(node: &Node, shares: &[&str], i: usize) -> Result<String, Error> {
    let (Some(a), Some(b)) = (node.children.get(i), node.children.get(i + 1)) else {
        return Ok(String::new());
    };
    if [a, b]
        .iter()
        .any(|c| c.hint.contains_key(crate::workspace::COLLAPSED))
    {
        return Ok(String::new());
    }
    let share = |j: usize| -> Result<u32, Error> {
        shares
            .get(j)
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| Error::Endpoint(format!("a split's share {j} is not a number")))
    };
    let (now, next) = (share(i)?, share(i + 1)?);
    let (Some((key_a, label_a)), Some((key_b, label_b))) = (lead(a)?, lead(b)?) else {
        return Ok(String::new());
    };
    let orientation =
        match layout(node, crate::workspace::SPLIT).and_then(|s| s.split_whitespace().next()) {
            Some("column") => "horizontal",
            _ => "vertical",
        };
    let min = crate::workspace::MIN_SHARE;
    Ok(format!(
        "<div class=\"ui-splitter\" role=\"separator\" data-ui-splitter=\"{i}\" \
         aria-orientation=\"{orientation}\" aria-label=\"Resize {} and {}\" \
         aria-controls=\"pane-{key_a} pane-{key_b}\" aria-valuenow=\"{now}\" \
         aria-valuemin=\"{min}\" aria-valuemax=\"{}\" aria-valuetext=\"{} {}, {} {}\"></div>",
        escape(label_a),
        escape(label_b),
        (now + next).saturating_sub(min),
        escape(label_a),
        percent(now),
        escape(label_b),
        percent(next),
    ))
}

/// A split child's share as a class from a fixed set: `ui-share-{n}`, the share to the nearest
/// ten thousandths, from 50 to 1000 (the door's CSP allows no inline style).
fn share_class(share: &str) -> String {
    let n: u32 = share.parse().unwrap_or(crate::workspace::WHOLE / 2);
    let n = (n + 5) / 10 * 10;
    format!(
        "ui-share-{}",
        n.clamp(crate::workspace::MIN_SHARE, crate::workspace::WHOLE)
    )
}

/// A property's text, with the defaults a template relies on.
fn property(node: &Node, name: &str) -> Option<String> {
    let text = |v: &Value| match v {
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Text(s) => s.clone(),
    };
    // A claimed element (kept whole by the ladder): its own arguments, as text.
    if model::is_element(&node.kind) {
        return node.properties.get(name).map(text);
    }
    let kind = model::kind(&node.kind)?;
    match name {
        "fallback" => node.properties.get(kind.fallback).map(text),
        "invalid" => Some(node.properties.contains_key("error").to_string()),
        _ => node
            .properties
            .get(name)
            .map(text)
            .or_else(|| match (kind.name, name) {
                ("field", "input") => Some("text".into()),
                ("field" | "choice", "required") => Some("false".into()),
                _ => None,
            }),
    }
}

fn text_of<'a>(node: &'a Node, name: &str) -> Option<&'a str> {
    match node.properties.get(name) {
        Some(Value::Text(s)) => Some(s),
        _ => None,
    }
}

/// A form's submit URL: its `action` when it has one, else its `resource` through the `html`
/// descriptor's submit rule ([`crate::target::Descriptor::submit`]).
async fn submit(inv: &Invocation<'_>, node: &Node) -> Result<Option<String>, Error> {
    if let Some(action) = text_of(node, "action") {
        return Ok(Some(action.to_string()));
    }
    let Some(resource) = text_of(node, "resource") else {
        return Ok(None);
    };
    let descriptor = crate::target::read_descriptor(inv, "html").await?;
    Ok(descriptor.submit_url(resource))
}

/// A form's HTTP method: its `method` when it has one, else from its `verb` the way the
/// ikigai-web door maps methods to verbs: `source` is GET and `sink` (the default) POST. An
/// HTML form has no method for `new` or `delete`, so the html face refuses them.
pub fn form_method(node: &Node) -> Result<String, Error> {
    if let Some(method) = text_of(node, "method") {
        return Ok(method.to_string());
    }
    match text_of(node, "verb") {
        None | Some("sink") => Ok("post".into()),
        Some("source") => Ok("get".into()),
        Some(verb) => Err(Error::InvalidArgument {
            name: "verb".into(),
            detail: format!(
                "the html face submits a form by GET (source) or POST (sink); an HTML form has \
                 no method for {verb}"
            ),
        }),
    }
}

pub(crate) fn property_endpoint() -> AsyncFnEndpoint {
    AsyncFnEndpoint::new(
        "ui-html-property",
        |inv: &Invocation<'_>| -> InvokeFuture<'_> {
            Box::pin(async move {
                let root = screen(inv).await?;
                let at = inv.bindings.get("node").unwrap_or_default();
                let (node, _) = locate(&root, at)?;
                let name = inv.bindings.get("property").unwrap_or_default();
                let value = match (node.kind.as_str(), name) {
                    (crate::meter::IRI, "control-id") => Some(format!("ui-meter-{at}")),
                    (crate::meter::IRI, "description") => {
                        Some(crate::meter::Reading::parse(node)?.description(node))
                    }
                    (crate::meter::IRI, "value" | "min" | "max" | "low" | "high" | "optimum") => {
                        let reading = crate::meter::Reading::parse(node)?;
                        Some(reading.property(name).to_string())
                    }
                    ("form", "submit") => submit(inv, node).await?,
                    ("group", "axis") => layout(node, crate::workspace::SPLIT)
                        .and_then(|s| s.split_whitespace().next())
                        .filter(|a| ["row", "column"].contains(a))
                        .map(str::to_string),
                    ("group", "pane-key") => Some(pane_key(node)?.to_string()),
                    // Collapsed to its header (ledger #3953): a fixed word, never the hint's text.
                    ("group", "collapsed") => Some(
                        if node.hint.contains_key(crate::workspace::COLLAPSED) {
                            "true"
                        } else {
                            "false"
                        }
                        .to_string(),
                    ),
                    // The places a drag may drop on this pane (ledger #3952): only the words of
                    // [`crate::workspace::PLACES`] enter markup.
                    ("group", "drop") => Some(
                        layout(node, crate::workspace::DROP)
                            .unwrap_or_default()
                            .split_whitespace()
                            .filter(|p| crate::workspace::PLACES.contains(p))
                            .collect::<Vec<_>>()
                            .join(" "),
                    ),
                    // A header toggle's state attribute and value (ledger #3953): fixed words only.
                    ("form", "toggle-attr" | "toggle-state") => {
                        let hint = hint_text(node, crate::workspace::TOGGLE).unwrap_or_default();
                        let (attr, state) = match hint {
                            "expanded true" => ("aria-expanded", "true"),
                            "expanded false" => ("aria-expanded", "false"),
                            "pressed true" => ("aria-pressed", "true"),
                            "pressed false" => ("aria-pressed", "false"),
                            other => {
                                return Err(Error::Endpoint(format!(
                                    "`{other}` is not a pane toggle"
                                )))
                            }
                        };
                        Some(if name == "toggle-attr" { attr } else { state }.to_string())
                    }
                    ("group", "menu-key") => {
                        let key = layout(node, crate::workspace::MENU).unwrap_or_default();
                        if !crate::workspace::is_key(key) {
                            return Err(Error::Endpoint(format!("`{key}` is not a pane key")));
                        }
                        Some(key.to_string())
                    }
                    ("form", "method") => Some(form_method(node)?),
                    ("scene", "datum-url") => match text_of(node, "datum") {
                        Some(datum) => crate::target::read_descriptor(inv, "html")
                            .await?
                            .submit_url(datum),
                        None => None,
                    },
                    // A boolean HTML attribute: only these fixed strings enter markup. The model
                    // carries only the initial state; the browser keeps it from then on.
                    (DISCLOSURE, "open") => Some(
                        if node.properties.get("open") == Some(&Value::Bool(true)) {
                            " open"
                        } else {
                            ""
                        }
                        .to_string(),
                    ),
                    // A checkbox's initial state, from the html face's hint (ledger #3531).
                    ("field", "checked") => Some(
                        if node.hint.get(crate::lens::CHECKED)
                            == Some(&model::Hint::Plain(Value::Text("checked".into())))
                        {
                            " checked"
                        } else {
                            ""
                        }
                        .to_string(),
                    ),
                    // A radio option's group is its choice's name (ledger #3537).
                    ("option", "group") => at
                        .rsplit_once('.')
                        .and_then(|(parent, _)| locate(&root, parent).ok())
                        .and_then(|(parent, _)| text_of(parent, "name"))
                        .map(str::to_string),
                    ("option", "selected" | "checked") => {
                        let selected = at
                            .rsplit_once('.')
                            .and_then(|(parent, _)| locate(&root, parent).ok())
                            .is_some_and(|(parent, _)| {
                                parent.kind == "choice"
                                    && parent.properties.get("value")
                                        == node.properties.get("value")
                            });
                        // A boolean HTML attribute: only these fixed strings enter markup.
                        Some(
                            if !selected {
                                ""
                            } else if name == "checked" {
                                " checked"
                            } else {
                                " selected"
                            }
                            .to_string(),
                        )
                    }
                    _ => property(node, name),
                };
                let value =
                    value.ok_or_else(|| Error::NotFound(format!("{at}: no property {name}")))?;
                Ok(Representation::new(ReprType::new(crate::TEXT), value.into_bytes()).cacheable())
            })
        },
    )
    .with_description(
        Description::new("ui-html-property")
            .title("A UI node's property, as text")
            .summary(
                "One property of a screen's node as plain text, for a kind template to splice \
                 escaped. `fallback` is the kind's fallback text and `invalid` whether the node \
                 carries an error; a form's `submit` is its action, else its resource through \
                 the html descriptor's submit rule, and its `method` get or post from its verb; \
                 an option's `selected` is a fixed attribute when its value is its choice's value; \
                 an absent property is NotFound unless the kind defaults it.",
            )
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .input(screen_arg())
            .input(node_arg())
            .input(argument("property", "the property's name"))
            .output(crate::TEXT),
    )
}

pub(crate) fn child_endpoint() -> AsyncFnEndpoint {
    AsyncFnEndpoint::new(
        "ui-html-child",
        |inv: &Invocation<'_>| -> InvokeFuture<'_> {
            Box::pin(async move {
                let root = screen(inv).await?;
                let at = inv.bindings.get("node").unwrap_or_default();
                let (node, _) = locate(&root, at)?;
                let screen = inv.bindings.get("screen").unwrap_or_default();
                let child_view = |path: String| async move {
                    let iri = Iri::parse(format!("{SCREEN_PREFIX}{screen}:node:{path}"))
                        .map_err(|e| Error::Endpoint(e.to_string()))?;
                    let rep = inv.issue(Request::new(Verb::Source, iri)).await?;
                    String::from_utf8(rep.bytes)
                        .map_err(|_| Error::Endpoint(format!("{path}: not UTF-8")))
                };
                let mut out = String::new();
                let shares: Vec<&str> = layout(node, crate::workspace::SPLIT)
                    .map(|s| s.split_whitespace().skip(1).collect())
                    .unwrap_or_default();
                // A tabs group (ledger #3950): a real tab list first, a link per pane so it
                // works with no script (the shell's script adds the arrow keys), then each pane
                // in its tab panel. The active key is the hint's.
                let active = layout(node, crate::workspace::TABS);
                if let Some(active) = active {
                    let label = text_of(node, "label").unwrap_or_default();
                    out.push_str(&format!(
                        "<div role=\"tablist\" aria-label=\"{}\">",
                        escape(label)
                    ));
                    for child in &node.children {
                        let key = pane_key(child)?;
                        let selected = key == active;
                        out.push_str(&format!(
                            "<a role=\"tab\" id=\"tab-{key}\" href=\"#pane-{key}\" \
                             aria-controls=\"panel-{key}\" aria-selected=\"{selected}\">{}</a>",
                            escape(text_of(child, "label").unwrap_or(key))
                        ));
                    }
                    out.push_str("</div>");
                }
                if node.kind == "scene" && node.replacement.is_some() {
                    out.push_str(&child_view(format!("{at}.r")).await?);
                }
                for (i, child) in node.children.iter().enumerate() {
                    let path = format!("{at}.{i}");
                    // An item's or a row's role (ledger #3521): a class from a fixed set, kept
                    // by [`screen`] only when the descriptor claims the role hints. Its text
                    // says the same, so the class is never the only carrier.
                    let class = model::role(child)
                        .map(|r| format!(" class=\"ui-role-{r}\""))
                        .unwrap_or_default();
                    match node.kind.as_str() {
                        "list" | "nav" => {
                            out.push_str(&format!("<li{class}>"));
                            out.push_str(&child_view(path).await?);
                            out.push_str("</li>");
                        }
                        "table" => {
                            let header = child.properties.get("header").and_then(|v| match v {
                                Value::Text(s) => Some(s.as_str()),
                                _ => None,
                            });
                            out.push_str(&format!("<tr{class}>"));
                            for j in 0..child.children.len() {
                                let cell = child_view(format!("{path}.{j}")).await?;
                                let (open, close) = match (header, j) {
                                    (Some("column"), _) => ("<th scope=\"col\">", "</th>"),
                                    (Some("row"), 0) => ("<th scope=\"row\">", "</th>"),
                                    _ => ("<td>", "</td>"),
                                };
                                out.push_str(open);
                                out.push_str(&cell);
                                out.push_str(close);
                            }
                            out.push_str("</tr>");
                        }
                        _ if active.is_some() => {
                            let key = pane_key(child)?;
                            let state = if Some(key) == active {
                                "ui-tabpanel-active"
                            } else {
                                "ui-tabpanel-inactive"
                            };
                            out.push_str(&format!(
                                "<div role=\"tabpanel\" id=\"panel-{key}\" \
                                 aria-labelledby=\"tab-{key}\" class=\"ui-tabpanel {state}\">"
                            ));
                            out.push_str(&child_view(path).await?);
                            out.push_str("</div>");
                        }
                        _ if !shares.is_empty() => {
                            // Between two split children, the splitter (ledger #3951).
                            if i > 0 {
                                out.push_str(&splitter(node, &shares, i - 1)?);
                            }
                            let mut class = share_class(shares.get(i).copied().unwrap_or_default());
                            if child.hint.contains_key(crate::workspace::COLLAPSED) {
                                class.push_str(" ui-share-collapsed");
                            }
                            out.push_str(&format!("<div class=\"ui-share {class}\">"));
                            out.push_str(&child_view(path).await?);
                            out.push_str("</div>");
                        }
                        _ => out.push_str(&child_view(path).await?),
                    }
                }
                Ok(html_rep(out.into_bytes()))
            })
        },
    )
    .with_description(
        Description::new("ui-html-child")
            .title("A UI node's children, as HTML")
            .summary(
                "The views of a screen node's children, in order: one <li> each in a list or \
                 nav; one <tr> per row in a table (either with a ui-role-{role} class when the \
                 child carries urn:iki:ui:hint:role:mark), its cells <td>, or <th scope=col> in a \
                 header=column row and <th scope=row> first in a header=row row.",
            )
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .input(screen_arg())
            .input(node_arg())
            .output(crate::HTML),
    )
}
