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
/// one radio input with its label (ledger #3537, [`model::RADIO`]).
pub const HINT_TEMPLATES: [crate::Artifact; 2] = [
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
        without_choice_hints(node)
    };
    // A descriptor that does not claim `scene` (a host serving no WebGPU renderer) takes the
    // ladder's next rung for each one: its replacement, else its label as text (ledger #3514).
    if descriptor.renders.iter().any(|k| k == "scene") {
        return Ok(node);
    }
    Ok(without_scenes(node))
}

fn without_choice_hints(mut node: Node) -> Node {
    node.hint
        .retain(|key, _| !key.starts_with(model::CHOICE_HINTS));
    node.children = node
        .children
        .into_iter()
        .map(without_choice_hints)
        .collect();
    node.replacement = node.replacement.map(|r| Box::new(without_choice_hints(*r)));
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
