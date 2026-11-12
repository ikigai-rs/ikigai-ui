//! Regions (ledger #3361): a group whose children resolve in a NARROWED space.
//!
//! A `region` node carries `label` (its fallback text, and the label of the group it becomes),
//! optional `allow` (the absolute IRIs it may read, separated by spaces, at most
//! [`MAX_ALLOW`]) and optional `scopes` (capability scopes, separated by spaces). The face
//! ([`crate::target::compose`]) composes everything inside a region in a confined invocation:
//!
//! - **Confined** (`Invocation::confine`): the region's children resolve in a space binding
//!   only the names in `allow` and the screen's settings ([`crate::setting::NAMES`]), with the
//!   ROOT cut off. A reference, a custom element's definition or anything an expansion reads
//!   outside it is `Unresolved`: the root endpoint is never entered, and the face refuses.
//!   An outline's reference LINE inside a region is refused when the screen compiles
//!   ([`crate::screen::compile`]): it would be read then, outside the region; a `reference` node
//!   is what resolves inside it.
//! - **Attenuated** (`Capability::attenuate`): with `scopes`, what the region may read is read
//!   under the caller's capability narrowed to them, so a grant the caller holds but the region
//!   does not name is not used inside it. A name the narrowed grant may not read stays
//!   `Denied` inside, so a reference to it is withheld ([`crate::reference`]).
//! - **Inheriting**: the screen's settings are read in the screen's space and bound inside, so
//!   a region runs under the same target, theme and locale. A host's corridors stay ahead of
//!   the region's space (core's confinement narrows; it cannot shadow), so a region cannot
//!   override a setting the host's screen space binds.
//!
//! **Every read inside a region goes through its [`Reach`]** (ledger #3525). Core keeps a
//! host's corridors AHEAD of a confinement (so a region cannot shadow a host's setting), which
//! means a host scope whose corridor binds more than the settings would answer inside the
//! region a name it does not allow, under the caller's whole grant. So the confined space is
//! not the only wall: a reference or an element definition read inside a region is refused as
//! `Unresolved` unless the region (and every region around it) allows it, and what it may read
//! is read under the scopes of every region around it, wherever it is answered.
//!
//! The narrowed space is named by the SHA-256 of what it binds (each name, its media type and
//! bytes, or its refusal), so two regions share cached answers only when they bind the same
//! doors to the same answers. Regions nest: an inner region narrows the outer one's space, to
//! at most [`MAX_DEPTH`].

use std::sync::Arc;

use ikigai_core::{
    Description, EndpointSpace, Error, Exact, FnEndpoint, Invocation, Iri, Representation, Request,
    Verb,
};
use sha2::{Digest, Sha256};

use crate::model::{Node, Value};

/// The region node's kind name. Structural, like a reference: never a target kind.
pub const KIND: &str = "region";
/// The property naming what the region may read.
pub const ALLOW: &str = "allow";
/// The property naming the scopes the region reads under.
pub const SCOPES: &str = "scopes";
/// The most names one region allows.
pub const MAX_ALLOW: usize = 16;
/// The most scopes one region names.
pub const MAX_SCOPES: usize = 16;
/// How deep regions nest.
pub const MAX_DEPTH: usize = 4;
/// A region's narrowed space is named this plus the hex SHA-256 of what it binds.
pub const SPACE_PREFIX: &str = "urn:iki:ui:region:sha256:";

pub(crate) fn refuse(detail: impl Into<String>) -> Error {
    Error::InvalidArgument {
        name: "region".into(),
        detail: detail.into(),
    }
}

fn text<'a>(node: &'a Node, name: &str) -> Option<&'a str> {
    match node.properties.get(name) {
        Some(Value::Text(s)) => Some(s),
        _ => None,
    }
}

/// The names a region allows, parsed.
pub fn allowed(node: &Node) -> Result<Vec<Iri>, String> {
    let words: Vec<&str> = text(node, ALLOW)
        .unwrap_or_default()
        .split_whitespace()
        .collect();
    if words.len() > MAX_ALLOW {
        return Err(format!("a region allows at most {MAX_ALLOW} names"));
    }
    words
        .into_iter()
        .map(|w| Iri::parse(w).map_err(|e| format!("`{w}` in `{ALLOW}` is not an IRI: {e}")))
        .collect()
}

/// The scopes a region reads under, or `None` when it names none (the caller's own grant).
pub fn scopes(node: &Node) -> Result<Option<Vec<String>>, String> {
    let Some(raw) = text(node, SCOPES) else {
        return Ok(None);
    };
    let words: Vec<String> = raw.split_whitespace().map(str::to_string).collect();
    if words.is_empty() || words.len() > MAX_SCOPES {
        return Err(format!("`{SCOPES}` names 1 to {MAX_SCOPES} scopes"));
    }
    Ok(Some(words))
}

/// A region's own checks, after its kind's (called by [`crate::model::validate`]).
pub(crate) fn validate(node: &Node) -> Result<(), String> {
    allowed(node)?;
    scopes(node)?;
    Ok(())
}

/// The group a region composes to: its label and its children.
pub fn group(node: &Node) -> Node {
    Node {
        kind: "group".into(),
        hint: node.hint.clone(),
        replacement: None,
        properties: [("label".into(), node.properties["label"].clone())].into(),
        children: node.children.clone(),
    }
}

/// What a composition may read where it stands: everything, outside every region; inside one,
/// only the names every region around it allows (and the screen's settings), under the scopes
/// every region around it names.
#[derive(Clone, Debug, Default)]
pub struct Reach {
    /// `None` outside every region; else the names that may be read.
    allow: Option<Vec<Iri>>,
    /// `None` when no region around names scopes (the caller's own grant).
    scopes: Option<Vec<String>>,
}

impl Reach {
    /// Whether `iri` may be read here.
    pub fn permits(&self, iri: &Iri) -> bool {
        self.allow.as_ref().is_none_or(|a| a.contains(iri))
    }

    /// The scopes reads here are attenuated to, if any.
    pub fn scopes(&self) -> Option<&[String]> {
        self.scopes.as_deref()
    }

    /// `Source` on `iri` from here: `Unresolved` when it may not be read, else issued in `inv`
    /// under the attenuated grant.
    pub async fn source(&self, inv: &Invocation<'_>, iri: Iri) -> Result<Representation, Error> {
        if !self.permits(&iri) {
            return Err(Error::Unresolved(iri));
        }
        let request = Request::new(Verb::Source, iri);
        match &self.scopes {
            Some(s) => inv.issue_attenuated(request, s.clone()).await,
            None => inv.issue(request).await,
        }
    }
}

/// `inv` narrowed to `node`'s region: confined to a space binding what the region allows (read
/// now, in `inv`'s space, under the region's scopes) and the screen's settings, and the
/// [`Reach`] every read inside it goes through. `outer` is the reach where the region stands:
/// a nested region allows only what the outer one does, under both one's scopes. `inv` is the
/// SCREEN's invocation, never an outer region's: a region's bound answers were read under its
/// own scopes and would answer a nested region's narrower ones.
pub async fn narrow<'a>(
    inv: &'a Invocation<'_>,
    outer: &Reach,
    node: &Node,
) -> Result<(Invocation<'a>, Reach), Error> {
    let allow = allowed(node).map_err(refuse)?;
    let own = scopes(node).map_err(refuse)?;
    let scopes = match (outer.scopes.clone(), own) {
        (None, s) | (s, None) => s,
        (Some(a), Some(b)) => Some(a.into_iter().filter(|s| b.contains(s)).collect()),
    };
    let settings = crate::setting::NAMES
        .iter()
        .map(|n| Iri::parse(format!("{}{n}", crate::setting::PREFIX)).expect("a setting's IRI"));
    let mut names: Vec<Iri> = allow;
    for s in settings {
        if !names.contains(&s) {
            names.push(s);
        }
    }
    let reach = Reach {
        allow: Some(names.iter().filter(|n| outer.permits(n)).cloned().collect()),
        scopes,
    };
    let scopes = reach.scopes.clone();
    let mut space = EndpointSpace::new();
    let mut digest = Sha256::new();
    for iri in names {
        digest.update(iri.as_str().as_bytes());
        digest.update([0]);
        if !outer.permits(&iri) {
            // Not readable where the region stands: not bound inside it either.
            digest.update(b"outside\0");
            continue;
        }
        let request = Request::new(Verb::Source, iri.clone());
        let read = match &scopes {
            Some(s) => inv.issue_attenuated(request, s.clone()).await,
            None => inv.issue(request).await,
        };
        let endpoint = match read {
            Ok(rep) => {
                digest.update(b"ok\0");
                digest.update(rep.repr_type.media_type.as_bytes());
                digest.update([0]);
                digest.update((rep.bytes.len() as u64).to_be_bytes());
                digest.update(&rep.bytes);
                FnEndpoint::new("ui-region-name", move |_: &Invocation<'_>| Ok(rep.clone()))
            }
            Err(Error::Denied(why)) => {
                digest.update(b"denied\0");
                FnEndpoint::new("ui-region-name", move |_: &Invocation<'_>| {
                    Err(Error::Denied(why.clone()))
                })
            }
            // Absent outside, absent inside: nothing is bound.
            Err(Error::NotFound(_) | Error::Unresolved(_)) => {
                digest.update(b"absent\0");
                continue;
            }
            Err(e) => return Err(e),
        };
        space = space.bind(
            Exact::new(iri.as_str()),
            endpoint.with_description(
                Description::new("ui-region-name")
                    .title("A name a region may read")
                    .summary("What the region read in the screen's space, answered inside it.")
                    .verb(Verb::Source)
                    .verb(Verb::Meta),
            ),
        );
    }
    let hex: String = digest
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let name = Iri::parse(format!("{SPACE_PREFIX}{hex}")).expect("a region's IRI");
    Ok((inv.confine(name, Arc::new(space)), reach))
}
