//! Target contracts, independent of renderer availability. The GPUI descriptor is a contract
//! for its future renderer, not a claim that this crate already ships one.
//!
//! A fallback names another kind, or `text` for the source kind's required fallback text.
//! Projection keeps only destination properties and permitted children. If a required
//! destination property cannot be supplied, or a restricted child no longer fits, the whole
//! node becomes fallback text. Limits use that same last resort.
//!
//! The face is the only entry that selects a target; it never passes a target argument to
//! another resource. It preserves the caller's resolution scope. There is no endpoint API
//! for injecting an additional corridor while retaining root resolution: `Invocation::confine`
//! severs the root, hiding host screens and directory overrides. Until the host can arrange
//! that corridor, this face does not install one or bypass the caller's scope.

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
    /// Table dimensions; a larger table becomes its caption as text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<TableLimit>,
}

/// One target's complete contract over the closed kind vocabulary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Descriptor {
    /// The target name.
    pub name: String,
    /// Kinds the target can walk directly.
    pub renders: Vec<String>,
    /// Every remaining kind, mapped to a kind eventually rendered by this target.
    pub fallback: BTreeMap<String, String>,
    /// Target-specific bounds.
    pub limits: Limits,
}

fn invalid(detail: impl Into<String>) -> Error {
    Error::InvalidArgument {
        name: "descriptor".into(),
        detail: detail.into(),
    }
}

impl Descriptor {
    /// Read a bounded descriptor and require complete, disjoint coverage, a rendered text
    /// floor, known kinds and terminating fallback chains.
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

    /// Require complete coverage, a rendered text floor and terminating fallback chains.
    /// Also checked by [`Self::apply`] so an edited or directly deserialized contract is safe.
    pub fn validate(&self) -> Result<(), Error> {
        let rendered: BTreeSet<_> = self.renders.iter().map(String::as_str).collect();
        if rendered.len() != self.renders.len() || !rendered.contains("text") {
            return Err(invalid("renders must be unique and include text"));
        }
        for kind in rendered
            .iter()
            .copied()
            .chain(self.fallback.keys().map(String::as_str))
        {
            if model::kind(kind).is_none() {
                return Err(invalid(format!("unknown kind {kind}")));
            }
        }
        for kind in model::KINDS {
            if rendered.contains(kind.name) == self.fallback.contains_key(kind.name) {
                return Err(invalid(format!(
                    "{} needs exactly one render or fallback decision",
                    kind.name
                )));
            }
            let mut at = kind.name;
            let mut seen = BTreeSet::new();
            while !rendered.contains(at) {
                if !seen.insert(at) {
                    return Err(invalid(format!("fallback cycle at {at}")));
                }
                at = self
                    .fallback
                    .get(at)
                    .ok_or_else(|| invalid(format!("no fallback for {at}")))?;
                if model::kind(at).is_none() {
                    return Err(invalid(format!("unknown fallback kind {at}")));
                }
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

    fn project(&self, source: &Node) -> Node {
        let source_kind = model::kind(&source.kind).expect("validated model");
        let text = source.properties[source_kind.fallback].clone();
        let floor = || Node {
            kind: "text".into(),
            properties: [("content".into(), text.clone())].into(),
            children: Vec::new(),
        };
        if source.kind == "table"
            && self.limits.table.as_ref().is_some_and(|limit| {
                source.children.len() > limit.max_rows
                    || source
                        .children
                        .iter()
                        .any(|row| row.children.len() > limit.max_columns)
            })
        {
            return floor();
        }
        let mut destination = source.kind.as_str();
        while let Some(next) = self.fallback.get(destination) {
            destination = next;
        }
        if destination == "text" && source.kind != "text" {
            return floor();
        }
        let kind = model::kind(destination).expect("validated descriptor");
        let mut properties: BTreeMap<String, Value> = source
            .properties
            .iter()
            .filter(|(name, _)| kind.property(name).is_some())
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect();
        properties.insert(kind.fallback.into(), text.clone());
        let children = if kind.children == Children::Leaf {
            Vec::new()
        } else {
            source
                .children
                .iter()
                .map(|child| self.project(child))
                .collect()
        };
        let result = Node {
            kind: destination.into(),
            properties,
            children,
        };
        if model::validate(&result).is_ok() {
            result
        } else {
            floor()
        }
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

async fn read_descriptor(inv: &Invocation<'_>, name: &str) -> Result<Descriptor, Error> {
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
    limits: &'a Limits,
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
                (descriptor.name.as_str(), Support { kind, limits: &descriptor.limits })
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
                        let label = decision.fallback.map(|name| format!("Fallback: {name}"))
                            .unwrap_or_else(|| "Renders".into());
                        html.push_str(&format!("<td>{}</td>", ikigai_fn::html::html_text(&label)));
                    }
                    html.push_str("</tr>");
                }
                html.push_str("</tbody></table><h2>Limits</h2>");
                for (name, support) in &matrix {
                    let limits = support.limits.table.as_ref().map(|limit| format!(
                        "Table: at most {} rows and {} columns; larger tables use caption text.",
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
            .summary("Every target against every kind, composed from descriptor resources. JSON by default; as=text/html for the support page. Includes table limits.")
            .verb(Verb::Source).verb(Verb::Meta).output(JSON).output(crate::HTML),
    )
}

pub(crate) fn face_endpoint() -> AsyncFnEndpoint {
    AsyncFnEndpoint::new("ui-face-target-screen", |inv: &Invocation<'_>| -> InvokeFuture<'_> {
        Box::pin(async move {
            let name = inv.bindings.get("target").unwrap_or_default();
            let descriptor = read_descriptor(inv, name).await?;
            let screen = inv.bindings.get("screen").unwrap_or_default();
            let iri = Iri::parse(format!("urn:iki:ui:screen:name:{screen}"))
                .map_err(|e| Error::InvalidArgument { name: "screen".into(), detail: e.to_string() })?;
            let rep = inv.issue(Request::new(Verb::Source, iri)).await?;
            let node = model::parse(&rep.bytes)?;
            crate::model_face(inv, &descriptor.apply(&node)?)
        })
    }).with_description(
        Description::new("ui-face-target-screen")
            .title("A screen through a UI target")
            .summary("The target entry point: read the screen and descriptor, then project unsupported kinds and over-limit tables to supported kinds or fallback text. No target argument is passed downstream.")
            .verb(Verb::Source).verb(Verb::Meta)
            .input(argument("target", "the target name").one_of(NAMES))
            .input(argument("screen", "the screen name"))
            .output(model::MEDIA_TYPE).output(model::JSON_LD),
    )
}
