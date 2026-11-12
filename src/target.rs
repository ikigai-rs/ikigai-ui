//! Target contracts, independent of renderer availability. The GPUI descriptor is a contract
//! for its future renderer, not a claim that this crate already ships one.
//!
//! Descriptors advertise versioned capabilities separately from kind coverage. For every
//! node the ladder tries native rendering, core expansion (reserved for future custom
//! elements), an explicit node replacement or descriptor kind fallback, then the original
//! kind's required fallback text. Native kinds can declare fallbacks for limits or child
//! constraints. Descriptor `text` requests the source kind's text directly. Projection keeps
//! only destination properties and permitted children, removing replacement metadata.
//!
//! The face is the only entry that selects a target; it never passes a target argument to
//! another resource. It preserves the caller's resolution scope. There is no endpoint API
//! for injecting an additional corridor while retaining root resolution: `Invocation::confine`
//! severs the root, hiding host screens and directory overrides. So the HOST injects the
//! screen space with `Kernel::issue_in` ([`crate::setting`]), and
//! `urn:iki:ui:face:screen:{screen}` ([`SCREEN_FACE`]) reads its target from the setting
//! `urn:iki:ui:setting:name:target` there; `urn:iki:ui:face:target:{target}:screen:{screen}`
//! stays the entry point that names its target. A face narrows only, for a region
//! ([`crate::region`]).

use std::collections::{BTreeMap, BTreeSet};

use ikigai_core::Error;
use serde::{Deserialize, Serialize};

use crate::model::{self, Children, Node, Value};

/// The sealed descriptor names.
pub const NAMES: [&str; 2] = ["html", "gpui"];
/// A descriptor's name prefix.
pub const PREFIX: &str = "urn:iki:ui:target:name:";
/// The support matrix, JSON by default or `as=text/html`.
pub const SUPPORT: &str = "urn:iki:ui:support";

/// A table bound. Rows are its group children; columns are the largest row's children.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TableLimit {
    /// Maximum row count (inclusive).
    pub max_rows: usize,
    /// Maximum cells per row (inclusive).
    pub max_columns: usize,
}

/// A target's limits. An absent limit imposes no additional target bound.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    /// Table dimensions; larger tables try the fallback ladder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<TableLimit>,
}

/// A versioned feature advertised by a face, independent of its kind coverage.
/// Versions are nonempty identifiers; requirement matching belongs to the future
/// element/hint contract rather than silently assuming a version comparison scheme.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capability {
    /// Feature name, for example `hover` or `multi-window`.
    pub name: String,
    /// The supported feature version, for example `1.0`.
    pub version: String,
}

/// One target's complete contract over the closed kind vocabulary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Descriptor {
    /// The target name.
    pub name: String,
    /// Versioned features this face offers. An empty list claims no features.
    pub capabilities: Vec<Capability>,
    /// Kinds the target can walk directly.
    pub renders: Vec<String>,
    /// Kind replacements, including optional replacements for native kinds that hit limits.
    /// Every nonnative kind must have a decision; `text` uses the original fallback text.
    pub fallback: BTreeMap<String, String>,
    /// Target-specific bounds.
    pub limits: Limits,
    /// Custom elements (`urn:{ns}:ui:element:name:{name}`) this face renders natively; every
    /// other element is expanded from its definition (ledger #3358).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub elements: Vec<String>,
    /// The hint namespaces this face reads (`urn:{ns}:ui:hint:{face}:`); it drops every other
    /// hint, and one whose `requires` its capabilities do not meet.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hints: Vec<String>,
    /// Where a form submits when it names a `resource` and no `action`: a URI template over
    /// the one variable `resource`, `{resource}` (RFC 6570 simple expansion, so the IRI is
    /// one path segment) or `{+resource}` (reserved). The `html` descriptor's rule is the
    /// ikigai-web door's [`SUBMIT`] route; a host whose door serves another path overrides
    /// the descriptor from `target/html.json` in the UI directory. Absent, such a form has
    /// no submit URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submit: Option<String>,
}

/// The `html` descriptor's submit rule: the path an ikigai-web door routes to the resource
/// named in it, with the route `/resolution/iri/{+iri}` → `{+iri}` (the door decodes the
/// segment, so a `/` in the IRI arrives whole).
pub const SUBMIT: &str = "/resolution/iri/{resource}";

impl Descriptor {
    /// The submit URL for `resource` through [`Self::submit`], escaped by `ikigai_fn::html`'s
    /// RFC 6570 rules. `None` without a rule.
    pub fn submit_url(&self, resource: &str) -> Option<String> {
        let rule = self.submit.as_deref()?;
        Some(if let Some((head, tail)) = rule.split_once("{+resource}") {
            format!("{head}{}{tail}", ikigai_fn::html::url_reserved(resource))
        } else {
            let (head, tail) = rule.split_once("{resource}")?;
            format!("{head}{}{tail}", ikigai_fn::html::url_segment(resource))
        })
    }
}

fn invalid(detail: impl Into<String>) -> Error {
    Error::InvalidArgument {
        name: "descriptor".into(),
        detail: detail.into(),
    }
}

impl Descriptor {
    /// Read a bounded descriptor, refusing unknown fields and invalid contracts.
    pub fn parse(bytes: &[u8], name: &str) -> Result<Self, Error> {
        model::bound(bytes)?;
        let descriptor: Self = serde_json::from_slice(bytes)
            .map_err(|e| invalid(format!("not a target descriptor: {e}")))?;
        if descriptor.name != name {
            return Err(invalid("the descriptor's name differs from its resource"));
        }
        descriptor.validate()?;
        Ok(descriptor)
    }

    /// Require complete coverage, a rendered text floor, unique versioned capabilities
    /// and terminating fallback chains, even when a chain starts at a native kind.
    /// Also checked by [`Self::apply`] for directly edited/deserialized contracts.
    pub fn validate(&self) -> Result<(), Error> {
        if let Some(rule) = &self.submit {
            let vars = rule.matches('{').count();
            let one = rule.matches("{resource}").count() + rule.matches("{+resource}").count();
            if vars != 1 || one != 1 || !(rule.starts_with('/') || rule.starts_with("http")) {
                return Err(invalid(
                    "submit is a path or URL template with exactly one {resource} or {+resource}",
                ));
            }
        }
        let mut capabilities = BTreeSet::new();
        for capability in &self.capabilities {
            if capability.name.trim().is_empty() || capability.version.trim().is_empty() {
                return Err(invalid(
                    "capabilities need nonempty name and version fields",
                ));
            }
            if !capabilities.insert(&capability.name) {
                return Err(invalid(format!("duplicate capability {}", capability.name)));
            }
        }
        for element in &self.elements {
            if !model::is_element(element) {
                return Err(invalid(format!("{element} is not a custom element's IRI")));
            }
        }
        for namespace in &self.hints {
            let ok = namespace.starts_with("urn:")
                && namespace.ends_with(':')
                && namespace.contains(model::HINT_MARK)
                && namespace
                    .split_once(model::HINT_MARK)
                    .is_some_and(|(_, face)| {
                        face.len() > 1 && !face[..face.len() - 1].contains(':')
                    });
            if !ok {
                return Err(invalid(format!(
                    "{namespace} is not a hint namespace (urn:{{ns}}{}{{face}}:)",
                    model::HINT_MARK
                )));
            }
        }
        let rendered: BTreeSet<_> = self.renders.iter().map(String::as_str).collect();
        if rendered.len() != self.renders.len() || !rendered.contains("text") {
            return Err(invalid("renders must be unique and include text"));
        }
        if self.fallback.contains_key("text") {
            return Err(invalid(
                "text is the fallback floor and cannot have a replacement",
            ));
        }
        for kind in rendered
            .iter()
            .copied()
            .chain(self.fallback.keys().map(String::as_str))
            .chain(self.fallback.values().map(String::as_str))
        {
            if model::kind(kind).is_none() {
                return Err(invalid(format!("unknown kind {kind}")));
            }
        }
        for kind in model::KINDS {
            if !rendered.contains(kind.name) && !self.fallback.contains_key(kind.name) {
                return Err(invalid(format!(
                    "{} needs a render or fallback decision",
                    kind.name
                )));
            }
            let mut at = kind.name;
            let mut seen = BTreeSet::new();
            while let Some(next) = self.fallback.get(at) {
                if !seen.insert(at) {
                    return Err(invalid(format!("fallback cycle at {at}")));
                }
                at = next;
            }
            if !rendered.contains(at) {
                return Err(invalid(format!(
                    "fallback chain ends at unsupported kind {at}"
                )));
            }
        }
        Ok(())
    }

    /// Apply this contract to a validated screen. Unsupported content never raises a
    /// rendering error: the source kind's required fallback text is the final floor.
    pub fn apply(&self, screen: &Node) -> Result<Node, Error> {
        self.validate()?;
        model::validate(screen)?;
        let result = self.project(screen);
        model::validate(&result)?;
        Ok(result)
    }

    /// The hints of `source` this face reads: under one of its namespaces, `requires` met.
    fn hints(&self, source: &Node) -> BTreeMap<String, model::Hint> {
        source
            .hint
            .iter()
            .filter(|(key, hint)| {
                self.hints.iter().any(|ns| key.starts_with(ns.as_str()))
                    && match hint {
                        model::Hint::Plain(_) => true,
                        model::Hint::Requires {
                            capability,
                            version,
                            ..
                        } => crate::element::meets(self, capability, version),
                    }
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    fn project(&self, source: &Node) -> Node {
        let mut node = self.project_kind(source);
        // Hints follow the node they were written on when it survives as itself.
        if node.kind == source.kind {
            node.hint = self.hints(source);
        }
        node
    }

    fn project_kind(&self, source: &Node) -> Node {
        // A reference the face did not resolve ([`crate::reference::resolve`] runs first)
        // takes the withheld rung: its fallback text, never a value.
        if source.kind == crate::reference::KIND {
            return crate::reference::withheld(source);
        }
        // A region the face did not narrow ([`compose`] runs first) is its group; what inside
        // it is unresolved takes its own rung.
        if source.kind == crate::region::KIND {
            return self.project(&crate::region::group(source));
        }
        // 0. A custom element that reaches projection unexpanded ([`crate::element::expand`]
        // runs first): kept when claimed, else its declared fallback, else its fallback text.
        if model::is_element(&source.kind) {
            if self.elements.contains(&source.kind) {
                let mut node = source.clone();
                node.children = source.children.iter().map(|c| self.project(c)).collect();
                node.replacement = None;
                return node;
            }
            if let Some(replacement) = &source.replacement {
                return self.project(replacement);
            }
            return Node {
                kind: "text".into(),
                replacement: None,
                hint: Default::default(),
                properties: [(
                    "content".into(),
                    source.properties[model::ELEMENT_FALLBACK].clone(),
                )]
                .into(),
                children: Vec::new(),
            };
        }
        // 1. Native rendering, including limits and projected child constraints.
        if let Some(native) = self.native(source, &source.kind) {
            return native;
        }
        // 2. Expansion into core kinds (custom elements expand before projection).
        if let Some(expansion) = Self::expand(source, &source.kind) {
            return self.project(&expansion);
        }
        // 3. The node's explicit replacement takes precedence over the descriptor.
        if let Some(replacement) = &source.replacement {
            return self.project(replacement);
        }
        let mut destination = source.kind.as_str();
        while let Some(next) = self.fallback.get(destination) {
            destination = next;
            if destination == "text" {
                break;
            }
            if let Some(native) = self.native(source, destination) {
                return native;
            }
            if let Some(expansion) = Self::expand(source, destination) {
                return self.project(&expansion);
            }
        }
        // 4. The original kind's required fallback text is always renderable.
        let source_kind = model::kind(&source.kind).expect("validated model");
        Node {
            kind: "text".into(),
            replacement: None,
            hint: Default::default(),
            properties: [(
                "content".into(),
                source.properties[source_kind.fallback].clone(),
            )]
            .into(),
            children: Vec::new(),
        }
    }

    // Reserved expansion stage: the current closed vocabulary contains core kinds only.
    // This is deliberately not descriptor fallback, which has lower precedence.
    fn expand(_source: &Node, _destination: &str) -> Option<Node> {
        None
    }

    fn native(&self, source: &Node, destination: &str) -> Option<Node> {
        if !self.renders.iter().any(|kind| kind == destination) {
            return None;
        }
        if destination == "table"
            && self.limits.table.as_ref().is_some_and(|limit| {
                source.children.len() > limit.max_rows
                    || source
                        .children
                        .iter()
                        .any(|row| row.children.len() > limit.max_columns)
            })
        {
            return None;
        }
        let source_kind = model::kind(&source.kind).expect("validated model");
        let kind = model::kind(destination).expect("validated descriptor");
        let mut properties: BTreeMap<String, Value> = source
            .properties
            .iter()
            .filter(|(name, _)| kind.property(name).is_some())
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect();
        properties.insert(
            kind.fallback.into(),
            source.properties[source_kind.fallback].clone(),
        );
        let children = if kind.children == Children::Leaf {
            Vec::new()
        } else {
            source
                .children
                .iter()
                .map(|child| self.project(child))
                .collect()
        };
        // A scene drawn natively keeps its replacement, projected: the accessible form a page
        // shows beside the canvas, and in its place when the face cannot draw.
        let replacement = if destination == "scene" {
            source
                .replacement
                .as_ref()
                .map(|r| Box::new(self.project(r)))
        } else {
            None
        };
        let result = Node {
            kind: destination.into(),
            replacement,
            hint: Default::default(),
            properties,
            children,
        };
        model::validate(&result).ok().map(|()| result)
    }
}

use ikigai_core::{
    ArgSpec, AsyncFnEndpoint, Description, Endpoint, Invocation, InvokeFuture, Iri, ReprType,
    Representation, Request, Verb,
};

const JSON: &str = "application/json";
const TARGETS: [crate::Artifact; 2] = [
    crate::Artifact {
        iri: "urn:iki:ui:target:name:html",
        id: "ui-target-name",
        file: "target/html.json",
        media_type: JSON,
        bytes: include_bytes!("../assets/target/html.json"),
    },
    crate::Artifact {
        iri: "urn:iki:ui:target:name:gpui",
        id: "ui-target-name",
        file: "target/gpui.json",
        media_type: JSON,
        bytes: include_bytes!("../assets/target/gpui.json"),
    },
];

fn known(name: &str) -> Result<&'static crate::Artifact, Error> {
    TARGETS
        .iter()
        .find(|a| a.iri.strip_prefix(PREFIX) == Some(name))
        .ok_or_else(|| Error::NotFound(format!("{name}: not a UI target")))
}

fn argument(name: &str, summary: &str) -> ArgSpec {
    ArgSpec::new(name)
        .summary(summary)
        .class(crate::XSD_STRING)
        .binding()
}

pub(crate) fn descriptor_endpoint() -> AsyncFnEndpoint {
    AsyncFnEndpoint::new("ui-target-name", |inv: &Invocation<'_>| -> InvokeFuture<'_> {
        Box::pin(async move {
            let name = inv.bindings.get("name").unwrap_or_default();
            let artifact = known(name)?;
            let file = Iri::parse(format!("{}{}", crate::DIR, artifact.file))
                .map_err(|e| Error::Endpoint(e.to_string()))?;
            // Use the same directory absence/denial and dependency rules as other artifacts.
            let rep = crate::artifact_endpoint(*artifact, file, crate::DIR).invoke(inv).await?;
            Descriptor::parse(&rep.bytes, name)?;
            Ok(rep)
        })
    }).with_description(
        Description::new("ui-target-name")
            .title("A UI target descriptor")
            .summary("The complete target contract, from target/{name}.json in the UI directory or compiled-in data. GPUI describes its future renderer.")
            .verb(Verb::Source).verb(Verb::Meta)
            .input(argument("name", "the target descriptor name").one_of(NAMES))
            .output(JSON),
    )
}

pub(crate) async fn read_descriptor(inv: &Invocation<'_>, name: &str) -> Result<Descriptor, Error> {
    let artifact = known(name)?;
    let iri = Iri::parse(artifact.iri).expect("compiled-in descriptor IRI");
    let rep = inv.issue(Request::new(Verb::Source, iri)).await?;
    Descriptor::parse(&rep.bytes, name)
}

#[derive(Serialize)]
struct Decision<'a> {
    renders: bool,
    fallback: Option<&'a str>,
}

#[derive(Serialize)]
struct Support<'a> {
    kind: BTreeMap<&'static str, Decision<'a>>,
    /// Each of this crate's elements: whether the face renders it natively (else it expands).
    element: BTreeMap<&'static str, bool>,
    hints: &'a [String],
    limits: &'a Limits,
    capabilities: &'a [Capability],
}

pub(crate) fn support_endpoint() -> AsyncFnEndpoint {
    AsyncFnEndpoint::new("ui-support", |inv: &Invocation<'_>| -> InvokeFuture<'_> {
        Box::pin(async move {
            let mut descriptors = Vec::new();
            for name in NAMES {
                descriptors.push(read_descriptor(inv, name).await?);
            }
            let matrix: BTreeMap<_, _> = descriptors.iter().map(|descriptor| {
                let kind = model::KINDS.iter().map(|kind| (kind.name, Decision {
                    renders: descriptor.renders.iter().any(|name| name == kind.name),
                    fallback: descriptor.fallback.get(kind.name).map(String::as_str),
                })).collect();
                let element = crate::element::ELEMENTS.iter().map(|e| (e.iri, descriptor.elements.iter().any(|n| n == e.iri))).collect();
                (descriptor.name.as_str(), Support { kind, element, hints: &descriptor.hints, limits: &descriptor.limits, capabilities: &descriptor.capabilities })
            }).collect();
            if inv.inline_str("as").ok().is_some_and(|as_type| {
                as_type.split(';').next().unwrap_or_default().trim() == "text/html"
            }) {
                let mut html = String::from("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>UI target support</title></head><body><h1>UI target support</h1><p>Descriptor contracts; GPUI's renderer is not yet implemented. Text means the kind's required fallback text.</p><table><caption>Every target against every kind</caption><thead><tr><th scope=\"col\">Kind</th>");
                for name in matrix.keys() {
                    html.push_str(&format!("<th scope=\"col\">{}</th>", ikigai_fn::html::html_text(name)));
                }
                html.push_str("</tr></thead><tbody>");
                for kind in model::KINDS {
                    html.push_str(&format!("<tr><th scope=\"row\">{}</th>", ikigai_fn::html::html_text(kind.name)));
                    for support in matrix.values() {
                        let decision = &support.kind[kind.name];
                        let label = match (decision.renders, decision.fallback) {
                            (true, Some(name)) => format!("Renders; fallback: {name}"),
                            (true, None) => "Renders".into(),
                            (false, Some(name)) => format!("Fallback: {name}"),
                            (false, None) => unreachable!("validated complete descriptor"),
                        };
                        html.push_str(&format!("<td>{}</td>", ikigai_fn::html::html_text(&label)));
                    }
                    html.push_str("</tr>");
                }
                html.push_str("</tbody></table><h2>Custom elements</h2><table><caption>Every target against every example element</caption><thead><tr><th scope=\"col\">Element</th>");
                for name in matrix.keys() {
                    html.push_str(&format!("<th scope=\"col\">{}</th>", ikigai_fn::html::html_text(name)));
                }
                html.push_str("</tr></thead><tbody>");
                for element in crate::element::ELEMENTS {
                    html.push_str(&format!("<tr><th scope=\"row\">{}</th>", ikigai_fn::html::html_text(element.iri)));
                    for support in matrix.values() {
                        let label = if support.element[element.iri] { "Native" } else { "Expanded from its definition" };
                        html.push_str(&format!("<td>{label}</td>"));
                    }
                    html.push_str("</tr>");
                }
                html.push_str("</tbody></table><h2>Hint namespaces</h2>");
                for (name, support) in &matrix {
                    html.push_str(&format!("<h3>{}</h3><ul>", ikigai_fn::html::html_text(name)));
                    for namespace in support.hints {
                        html.push_str(&format!("<li>{}</li>", ikigai_fn::html::html_text(namespace)));
                    }
                    if support.hints.is_empty() {
                        html.push_str("<li>No hints read.</li>");
                    }
                    html.push_str("</ul>");
                }
                html.push_str("<h2>Capabilities</h2>");
                for (name, support) in &matrix {
                    html.push_str(&format!("<h3>{}</h3><ul>", ikigai_fn::html::html_text(name)));
                    for capability in support.capabilities {
                        html.push_str(&format!("<li>{}: {}</li>", ikigai_fn::html::html_text(&capability.name), ikigai_fn::html::html_text(&capability.version)));
                    }
                    if support.capabilities.is_empty() {
                        html.push_str("<li>No capabilities declared.</li>");
                    }
                    html.push_str("</ul>");
                }
                html.push_str("<h2>Limits</h2>");
                for (name, support) in &matrix {
                    let limits = support.limits.table.as_ref().map(|limit| format!(
                        "Table: at most {} rows and {} columns; larger tables try the fallback ladder.",
                        limit.max_rows, limit.max_columns,
                    )).unwrap_or_else(|| "No additional target limits.".into());
                    html.push_str(&format!("<p>{}: {}</p>", ikigai_fn::html::html_text(name), ikigai_fn::html::html_text(&limits)));
                }
                html.push_str("</body></html>");
                return Ok(Representation::new(ReprType::new(crate::HTML), html.into_bytes()).cacheable());
            }
            let bytes = serde_json::to_vec(&matrix).map_err(|e| Error::Endpoint(e.to_string()))?;
            Ok(Representation::new(ReprType::new(JSON), bytes).cacheable())
        })
    }).with_description(
        Description::new("ui-support")
            .title("UI target support")
            .summary("Every target against every kind, composed from descriptor resources. JSON by default; as=text/html for the support page. Includes versioned capabilities and table limits.")
            .verb(Verb::Source).verb(Verb::Meta).output(JSON).output(crate::HTML),
    )
}

/// A screen composed for `descriptor` in the space `inv` runs in (ledger #3361), before the
/// descriptor projects it: in rounds, every reference resolved under the caller's capability
/// ([`crate::reference`]), then every custom element down its ladder ([`crate::element`]),
/// until no reference is left (at most [`crate::reference::MAX_DEPTH`] rounds, else refused);
/// then each region composed the same way in its narrowed space ([`crate::region`]), to a
/// `group`; then each write-only field made the form that sinks to its own resource. What
/// comes out holds no reference and no region.
pub async fn compose(
    inv: &Invocation<'_>,
    node: &Node,
    descriptor: &Descriptor,
) -> Result<Node, Error> {
    let mut budget = crate::reference::Budget::default();
    let node = compose_in(inv, node, descriptor, &mut budget, 0).await?;
    let node = write_only(&node, false)?;
    model::validate(&node)?;
    Ok(node)
}

type Composed<'a> =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<Node, Error>> + Send + 'a>>;

fn compose_in<'a>(
    inv: &'a Invocation<'_>,
    node: &'a Node,
    descriptor: &'a Descriptor,
    budget: &'a mut crate::reference::Budget,
    regions: usize,
) -> Composed<'a> {
    Box::pin(async move {
        let mut node = node.clone();
        let mut rounds = 0;
        loop {
            node = crate::reference::resolve_in(inv, &node, budget).await?;
            node = crate::element::expand(inv, &node, descriptor).await?;
            if !crate::reference::any(&node) {
                break;
            }
            rounds += 1;
            if rounds >= crate::reference::MAX_DEPTH {
                return Err(Error::InvalidArgument {
                    name: "reference".into(),
                    detail: format!(
                        "expansions introduce references past {} rounds",
                        crate::reference::MAX_DEPTH
                    ),
                });
            }
        }
        open_regions(inv, &node, descriptor, budget, regions).await
    })
}

/// `node` with each outermost region composed in its narrowed space.
fn open_regions<'a>(
    inv: &'a Invocation<'_>,
    node: &'a Node,
    descriptor: &'a Descriptor,
    budget: &'a mut crate::reference::Budget,
    regions: usize,
) -> Composed<'a> {
    Box::pin(async move {
        if node.kind == crate::region::KIND {
            if regions >= crate::region::MAX_DEPTH {
                return Err(crate::region::refuse(format!(
                    "regions nested past {}",
                    crate::region::MAX_DEPTH
                )));
            }
            let narrowed = crate::region::narrow(inv, node).await?;
            let group = crate::region::group(node);
            return compose_in(&narrowed, &group, descriptor, budget, regions + 1).await;
        }
        let mut out = node.clone();
        let mut children = Vec::with_capacity(node.children.len());
        for child in &node.children {
            children.push(open_regions(inv, child, descriptor, budget, regions).await?);
        }
        out.children = children;
        if let Some(r) = &node.replacement {
            out.replacement = Some(Box::new(
                open_regions(inv, r, descriptor, budget, regions).await?,
            ));
        }
        Ok(out)
    })
}

/// The label of the button a write-only field's form submits with.
pub const WRITE_ONLY_SUBMIT: &str = "Save";

/// `node` with each write-only field (a `field` naming its own `resource`) made the form that
/// sinks to that resource: the field (its input `password` unless it names one) and a submit
/// button. It holds no value, so nothing is read back. One inside another form is refused:
/// it is its own form.
fn write_only(node: &Node, in_form: bool) -> Result<Node, Error> {
    if node.kind == "field" && node.properties.contains_key("resource") {
        if in_form {
            return Err(Error::InvalidArgument {
                name: "field".into(),
                detail: "a write-only field is its own form; it cannot sit inside another".into(),
            });
        }
        let mut field = node.clone();
        let resource = field.properties.remove("resource").expect("checked above");
        field
            .properties
            .entry("input".into())
            .or_insert(Value::Text("password".into()));
        let label = field.properties["label"].clone();
        let mut properties: BTreeMap<String, Value> = [
            ("label".into(), label),
            ("resource".into(), resource),
            ("verb".into(), Value::Text("sink".into())),
        ]
        .into();
        if let Some(d) = field.properties.get("description") {
            properties.insert("description".into(), d.clone());
        }
        let button = Node {
            kind: "button".into(),
            hint: Default::default(),
            replacement: None,
            properties: [("label".into(), Value::Text(WRITE_ONLY_SUBMIT.into()))].into(),
            children: Vec::new(),
        };
        return Ok(Node {
            kind: "form".into(),
            hint: Default::default(),
            replacement: None,
            properties,
            children: vec![field, button],
        });
    }
    let in_form = in_form || node.kind == "form";
    let mut out = node.clone();
    out.children = node
        .children
        .iter()
        .map(|c| write_only(c, in_form))
        .collect::<Result<_, _>>()?;
    if let Some(r) = &node.replacement {
        out.replacement = Some(Box::new(write_only(r, in_form)?));
    }
    Ok(out)
}

/// The face: `screen` composed ([`compose`]) and projected through `target`'s descriptor.
async fn face(inv: &Invocation<'_>, target: &str, screen: &str) -> Result<Representation, Error> {
    let descriptor = read_descriptor(inv, target).await?;
    let iri = Iri::parse(format!("urn:iki:ui:screen:name:{screen}")).map_err(|e| {
        Error::InvalidArgument {
            name: "screen".into(),
            detail: e.to_string(),
        }
    })?;
    let rep = inv.issue(Request::new(Verb::Source, iri)).await?;
    let node = model::parse(&rep.bytes)?;
    let node = compose(inv, &node, &descriptor).await?;
    crate::model_face(inv, &descriptor.apply(&node)?)
}

pub(crate) fn face_endpoint() -> AsyncFnEndpoint {
    AsyncFnEndpoint::new("ui-face-target-screen", |inv: &Invocation<'_>| -> InvokeFuture<'_> {
        Box::pin(async move {
            let target = inv.bindings.get("target").unwrap_or_default();
            let screen = inv.bindings.get("screen").unwrap_or_default();
            face(inv, target, screen).await
        })
    }).with_description(
        Description::new("ui-face-target-screen")
            .title("A screen through a UI target")
            .summary("The target entry point: read the screen and descriptor, resolve each reference under the caller's capability (withheld when denied) and each region in its narrowed space, take each custom element down its ladder (native when claimed, else its definition's expansion, else its declared fallback, else its fallback text), then apply native rendering, declared fallback and finally fallback text per kind, keeping only the hints the face reads. No target argument is passed downstream.")
            .verb(Verb::Source).verb(Verb::Meta)
            .input(argument("target", "the target name").one_of(NAMES))
            .input(argument("screen", "the screen name"))
            .output(model::MEDIA_TYPE).output(model::JSON_LD),
    )
}

/// The screen-space face, `urn:iki:ui:face:screen:{screen}`: the target is the setting
/// [`crate::setting::TARGET`] in the space the request runs in, never a name or an argument.
pub const SCREEN_FACE: &str = "urn:iki:ui:face:screen:";

pub(crate) fn screen_face_endpoint() -> AsyncFnEndpoint {
    AsyncFnEndpoint::new("ui-face-screen", |inv: &Invocation<'_>| -> InvokeFuture<'_> {
        Box::pin(async move {
            let target = crate::setting::read(inv, "target").await?;
            let screen = inv.bindings.get("screen").unwrap_or_default();
            face(inv, &target, screen).await
        })
    }).with_description(
        Description::new("ui-face-screen")
            .title("A screen in its screen space")
            .summary("The face of a screen under the settings of the space it runs in: the target is urn:iki:ui:setting:name:target there (a host's screen space, injected with Kernel::issue_in, else the default), composed and projected as urn:iki:ui:face:target:{target}:screen:{screen} is.")
            .verb(Verb::Source).verb(Verb::Meta)
            .input(argument("screen", "the screen name"))
            .output(model::MEDIA_TYPE).output(model::JSON_LD),
    )
}
