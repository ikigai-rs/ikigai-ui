//! The workspace (ledger #3950, design `docs/design/workspace.md`): a tiled layout of panes,
//! saved as a resource, composed into an ORDINARY screen.
//!
//! ## The names (section 1, decision 2)
//!
//! | Name | What it is |
//! |---|---|
//! | `urn:iki:ui:workspace:person:{person}:name:{name}` | one person's workspace: the atom ([`Workspace`]), stored by the host |
//! | `…:name:{name}:version:{n}` | one saved version, never rewritten |
//! | `…:name:{name}:as-of:{instant}` | the version in force at an RFC 3339 instant |
//! | `…:name:{name}:edit` | the edit Sink the "Layout" form posts to, with `if-version` (section 5A) |
//! | [`SHAPE`] | the atom's closed SHACL shape; [`validate`] says the same in Rust |
//! | `urn:iki:ui:screen:workspace:{workspace-iri}` | the workspace as a screen ([`compose`]) |
//!
//! ikigai-ui stores nothing (decision 4). It validates the atom and composes it.
//!
//! ## What a workspace composes to (decision 1)
//!
//! The whole workspace is a `group` labeled with the workspace's label. Each split is a `group`
//! carrying [`SPLIT`] (its axis and shares, `row 300 700`), each tabs node a `group` carrying
//! [`TABS`] (the active pane's key), and each pane a [`crate::region`] labeled with the pane's
//! label, allowed to read only its content screen and attenuated by its `scopes`, whose child is
//! a [`crate::reference`] to that screen with the selection put in. A face that does not claim
//! [`HINTS`] drops the hints and keeps the panes in reading order: the linear form.
//!
//! ## No authority (decision 10)
//!
//! A workspace holds no grant. Every pane reads under the viewer's capability, narrowed by its
//! region; an action is only ever a generated form through `urn:iki:ui:lens:operation:`; and a
//! selection is part of the request ([`compose`] is a pure function of the atom and the
//! address), so a selection never writes.

use std::collections::{BTreeMap, BTreeSet};

use ikigai_core::{Error, Iri};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as Json};

use crate::model::{Hint, Node, Value};

/// Every workspace's prefix: `{PREFIX}{person}:name:{name}`.
pub const PREFIX: &str = "urn:iki:ui:workspace:person:";
/// The atom's closed shape.
pub const SHAPE: &str = "urn:iki:ui:shape:workspace";
/// The workspace screen's prefix: `{SCREEN}{workspace-iri}`.
pub const SCREEN: &str = "urn:iki:ui:screen:workspace:";
/// The workspace screen under its face name: `{FACE}[select:{role}={pct-iri}&…:]resource:{iri}`,
/// so an html face (`urn:iki:ui:html:screen:{name}`) reaches it like any named screen.
pub const FACE: &str = "urn:iki:ui:screen:name:workspace:";
/// The atom's shape as Turtle, served at [`SHAPE`]. It has no blank nodes: every node is named
/// under the shape's own IRI.
pub const SHAPE_TTL: &str = include_str!("../assets/workspace/shape.ttl");
/// The layout hints' namespace. A face that does not claim it gets the linear form.
pub const HINTS: &str = "urn:iki:ui:hint:layout:";
/// A split group: `{axis} {share} {share}…`, the axis `row` or `column`, shares in thousandths.
pub const SPLIT: &str = "urn:iki:ui:hint:layout:split";
/// A tabs group: the active pane's key.
pub const TABS: &str = "urn:iki:ui:hint:layout:tabs";
/// A pane's region: its key, which is also its fragment id's suffix (`#pane-{key}`).
pub const PANE: &str = "urn:iki:ui:hint:layout:pane";
/// A collapsed child of a split (section 5A): it keeps its share.
pub const COLLAPSED: &str = "urn:iki:ui:hint:layout:collapsed";

/// The most panes in one workspace.
pub const MAX_PANES: usize = 16;
/// How deep a layout nests (the root is depth 1).
pub const MAX_DEPTH: usize = 4;
/// The most panes in one tabs node.
pub const MAX_TABS: usize = 8;
/// The most selection roles.
pub const MAX_ROLES: usize = 8;
/// The longest pane key or role, in bytes.
pub const MAX_KEY: usize = 32;
/// What the shares in one split add up to.
pub const WHOLE: u32 = 1000;
/// The least share a split child may hold (5%, section 5A).
pub const MIN_SHARE: u32 = 50;
/// The longest atom, in bytes.
pub const MAX_BYTES: usize = 64 * 1024;
/// The longest label, in bytes.
pub const MAX_LABEL: usize = 256;

/// One workspace: its label, its layout, its panes and its saved selection. Closed.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Workspace {
    /// The workspace's accessible name.
    pub label: String,
    /// The layout tree.
    pub layout: Layout,
    /// The panes, by key.
    pub panes: BTreeMap<String, Pane>,
    /// The saved selection: role to IRI.
    pub selection: BTreeMap<String, String>,
}

/// A layout node.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Layout {
    /// A split: an axis and its children, each with its share.
    Split {
        /// `row` (side by side) or `column` (stacked).
        axis: Axis,
        /// The children, in reading order.
        children: Vec<Child>,
    },
    /// Tabs: the panes it holds and the active one.
    Tabs {
        /// The panes' keys, in order.
        tabs: Vec<String>,
        /// The active pane's key.
        active: String,
    },
    /// One pane, by key.
    Pane(String),
}

/// A split's child.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Child {
    /// Its share, in thousandths.
    pub share: u32,
    /// Collapsed to its header (a pane or tabs only); it keeps its share.
    pub collapsed: bool,
    /// What it is.
    pub node: Layout,
}

/// A split's axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Axis {
    /// Side by side.
    Row,
    /// Stacked.
    Column,
}

impl Axis {
    /// The axis's word.
    pub fn as_str(self) -> &'static str {
        match self {
            Axis::Row => "row",
            Axis::Column => "column",
        }
    }
}

/// One pane (section 2): one contract, with two flags.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pane {
    /// Its accessible name and title. Never empty.
    pub label: String,
    /// Its address: a screen name, with at most one `{role}` per followed role.
    pub content: String,
    /// The roles it follows, separated by spaces. Without it, the pane is pinned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follows: Option<String>,
    /// The one role its selection links set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emits: Option<String>,
    /// The classes a followed IRI must have, separated by spaces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accepts: Option<String>,
    /// A live stream.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub live: bool,
    /// The content answers `as-of=`.
    #[serde(default, rename = "as-of", skip_serializing_if = "std::ops::Not::not")]
    pub as_of: bool,
    /// Capability scopes the pane's reads are narrowed to, separated by spaces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scopes: Option<String>,
}

impl Pane {
    /// The roles it follows.
    pub fn followed(&self) -> Vec<&str> {
        self.follows
            .as_deref()
            .unwrap_or_default()
            .split_whitespace()
            .collect()
    }
}

fn refuse(detail: impl Into<String>) -> Error {
    Error::InvalidArgument {
        name: "workspace".into(),
        detail: detail.into(),
    }
}

/// Whether `key` is a pane key or a role: `[a-z0-9-]`, 1 to [`MAX_KEY`] bytes.
pub fn is_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= MAX_KEY
        && key
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn is_segment(s: &str) -> bool {
    !s.is_empty() && !s.contains([' ', '{', '}', '<', '>', '"']) && !s.chars().any(char::is_control)
}

/// A workspace's IRI: `urn:iki:ui:workspace:person:{person}:name:{name}`.
pub fn name(person: &str, name: &str) -> Result<String, Error> {
    if !is_segment(person) || Iri::parse(person).is_err() {
        return Err(refuse(format!("`{person}` is not a person's IRI")));
    }
    if !is_key(name) {
        return Err(refuse(format!(
            "`{name}` is not a workspace name ([a-z0-9-], at most {MAX_KEY} bytes)"
        )));
    }
    Ok(format!("{PREFIX}{person}:name:{name}"))
}

/// One saved version of a workspace.
pub fn version(workspace: &str, n: u64) -> String {
    format!("{workspace}:version:{n}")
}

/// The version of a workspace in force at an RFC 3339 instant.
pub fn as_of(workspace: &str, instant: &str) -> Result<String, Error> {
    chrono::DateTime::parse_from_rfc3339(instant)
        .map_err(|_| refuse(format!("`{instant}` is not an RFC 3339 instant")))?;
    Ok(format!("{workspace}:as-of:{instant}"))
}

/// The edit Sink the "Layout" form posts to (section 5A).
pub fn edit(workspace: &str) -> String {
    format!("{workspace}:edit")
}

/// The workspace as a screen.
pub fn screen(workspace: &str) -> String {
    format!("{SCREEN}{workspace}")
}

/// A workspace IRI's person and name, or `None` when it is not one (a version, an as-of, an
/// edit or anything else is not the bare name).
pub fn parse_name(iri: &str) -> Option<(&str, &str)> {
    let rest = iri.strip_prefix(PREFIX)?;
    let (person, name) = rest.rsplit_once(":name:")?;
    (is_segment(person) && is_key(name)).then_some((person, name))
}

/// Parse and validate an atom: closed, bounded, every pane placed once, every share adding up.
/// The first fault is refused as `InvalidArgument`, naming it in words.
pub fn validate(bytes: &[u8]) -> Result<Workspace, Error> {
    if bytes.len() > MAX_BYTES {
        return Err(refuse(format!("the atom is over {MAX_BYTES} bytes")));
    }
    let json: Json = serde_json::from_slice(bytes).map_err(|e| refuse(format!("not JSON: {e}")))?;
    let top = object(&json, "the workspace")?;
    closed(
        top,
        "the workspace",
        &["label", "layout", "panes", "selection"],
    )?;
    let label = label(top.get("label"), "the workspace")?;
    let Some(Json::Object(raw_panes)) = top.get("panes") else {
        return Err(refuse("the workspace has no `panes` object"));
    };
    if raw_panes.len() > MAX_PANES {
        return Err(refuse(format!(
            "a workspace holds at most {MAX_PANES} panes"
        )));
    }
    let mut panes = BTreeMap::new();
    for (key, raw) in raw_panes {
        if !is_key(key) {
            return Err(refuse(format!(
                "pane key `{key}` is not [a-z0-9-], at most {MAX_KEY} bytes"
            )));
        }
        let pane: Pane = serde_json::from_value(raw.clone())
            .map_err(|e| refuse(format!("pane `{key}`: {e}")))?;
        check_pane(key, &pane)?;
        panes.insert(key.clone(), pane);
    }
    let selection = match top.get("selection") {
        None => BTreeMap::new(),
        Some(Json::Object(map)) => {
            if map.len() > MAX_ROLES {
                return Err(refuse(format!("at most {MAX_ROLES} selection roles")));
            }
            let mut out = BTreeMap::new();
            for (role, value) in map {
                let Json::String(iri) = value else {
                    return Err(refuse(format!("selection `{role}` is not an IRI")));
                };
                check_selection(role, iri)?;
                out.insert(role.clone(), iri.clone());
            }
            out
        }
        Some(_) => return Err(refuse("`selection` is not an object")),
    };
    let Some(raw_layout) = top.get("layout") else {
        return Err(refuse("the workspace has no `layout`"));
    };
    let mut placed = BTreeSet::new();
    let layout = layout(raw_layout, 1, &panes, &mut placed)?;
    if let Some(unplaced) = panes.keys().find(|k| !placed.contains(k.as_str())) {
        return Err(refuse(format!("pane `{unplaced}` is not in the layout")));
    }
    let roles: BTreeSet<&str> = panes
        .values()
        .flat_map(|p| p.followed().into_iter().chain(p.emits.as_deref()))
        .chain(selection.keys().map(String::as_str))
        .collect();
    if roles.len() > MAX_ROLES {
        return Err(refuse(format!("at most {MAX_ROLES} selection roles")));
    }
    Ok(Workspace {
        label,
        layout,
        panes,
        selection,
    })
}

/// A selection's IRI: absolute, one IRI, no placeholder.
fn check_selection(role: &str, iri: &str) -> Result<(), Error> {
    if !is_key(role) {
        return Err(refuse(format!("selection role `{role}` is not [a-z0-9-]")));
    }
    if !is_segment(iri) || Iri::parse(iri).is_err() {
        return Err(refuse(format!(
            "selection `{role}` is not one absolute IRI"
        )));
    }
    Ok(())
}

fn object<'a>(json: &'a Json, what: &str) -> Result<&'a Map<String, Json>, Error> {
    json.as_object()
        .ok_or_else(|| refuse(format!("{what} is not an object")))
}

fn closed(map: &Map<String, Json>, what: &str, known: &[&str]) -> Result<(), Error> {
    match map.keys().find(|k| !known.contains(&k.as_str())) {
        Some(field) => Err(refuse(format!("{what}: unknown field `{field}`"))),
        None => Ok(()),
    }
}

fn label(json: Option<&Json>, what: &str) -> Result<String, Error> {
    match json {
        Some(Json::String(s))
            if !s.trim().is_empty() && s.len() <= MAX_LABEL && !s.chars().any(char::is_control) =>
        {
            Ok(s.clone())
        }
        _ => Err(refuse(format!(
            "{what} needs a `label`: nonempty text of at most {MAX_LABEL} bytes"
        ))),
    }
}

/// The placeholders in a pane's content, in order.
fn placeholders(content: &str) -> Result<Vec<&str>, Error> {
    let mut out = Vec::new();
    let mut rest = content;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let close = after
            .find('}')
            .ok_or_else(|| refuse("a content placeholder is not closed"))?;
        out.push(&after[..close]);
        rest = &after[close + 1..];
    }
    if rest.contains('}') {
        return Err(refuse("a content placeholder is not opened"));
    }
    Ok(out)
}

fn check_pane(key: &str, pane: &Pane) -> Result<(), Error> {
    label(
        Some(&Json::String(pane.label.clone())),
        &format!("pane `{key}`"),
    )?;
    let content = pane.content.as_str();
    let shaped = content.starts_with("lens:") && content.contains(":resource:")
        || content.starts_with("view:resource:")
        || content.starts_with("urn:iki:ui:screen:name:");
    if !shaped || content.contains([' ', '<', '>', '"']) || content.len() > MAX_LABEL * 4 {
        return Err(refuse(format!(
            "pane `{key}`: content `{content}` is not a screen name (lens:{{lens}}:resource:{{iri}}, \
             view:resource:{{iri}} or urn:iki:ui:screen:name:{{name}})"
        )));
    }
    let followed = pane.followed();
    for role in &followed {
        if !is_key(role) {
            return Err(refuse(format!(
                "pane `{key}`: role `{role}` is not [a-z0-9-]"
            )));
        }
    }
    let mut seen = BTreeSet::new();
    for role in placeholders(content)? {
        if !followed.contains(&role) {
            return Err(refuse(format!(
                "pane `{key}`: content names `{{{role}}}`, a role it does not follow"
            )));
        }
        if !seen.insert(role) {
            return Err(refuse(format!(
                "pane `{key}`: content names `{{{role}}}` more than once"
            )));
        }
    }
    if let Some(emits) = &pane.emits {
        if !is_key(emits) {
            return Err(refuse(format!(
                "pane `{key}`: `emits` is not one role ([a-z0-9-])"
            )));
        }
    }
    for class in pane
        .accepts
        .as_deref()
        .unwrap_or_default()
        .split_whitespace()
    {
        if Iri::parse(class).is_err() {
            return Err(refuse(format!(
                "pane `{key}`: accepts `{class}`, not an IRI"
            )));
        }
    }
    if let Some(scopes) = &pane.scopes {
        if scopes.split_whitespace().count() == 0
            || scopes.split_whitespace().count() > crate::region::MAX_SCOPES
        {
            return Err(refuse(format!(
                "pane `{key}`: `scopes` names 1 to {} scopes",
                crate::region::MAX_SCOPES
            )));
        }
    }
    Ok(())
}

fn place(
    key: &str,
    panes: &BTreeMap<String, Pane>,
    placed: &mut BTreeSet<String>,
) -> Result<String, Error> {
    if !panes.contains_key(key) {
        return Err(refuse(format!(
            "the layout names pane `{key}`, which is not in `panes`"
        )));
    }
    if !placed.insert(key.to_string()) {
        return Err(refuse(format!(
            "pane `{key}` is in the layout more than once"
        )));
    }
    Ok(key.to_string())
}

fn layout(
    json: &Json,
    depth: usize,
    panes: &BTreeMap<String, Pane>,
    placed: &mut BTreeSet<String>,
) -> Result<Layout, Error> {
    if depth > MAX_DEPTH {
        return Err(refuse(format!("a layout is at most {MAX_DEPTH} deep")));
    }
    let map = object(json, "a layout node")?;
    let what = ["split", "tabs", "pane"]
        .into_iter()
        .filter(|k| map.contains_key(*k))
        .collect::<Vec<_>>();
    let [what] = what[..] else {
        return Err(refuse(
            "a layout node is exactly one of a `split`, a `tabs` or a `pane`",
        ));
    };
    match what {
        "pane" => {
            closed(map, "a pane leaf", &["pane", "share", "collapsed"])?;
            let Some(Json::String(key)) = map.get("pane") else {
                return Err(refuse("a pane leaf's `pane` is not a key"));
            };
            Ok(Layout::Pane(place(key, panes, placed)?))
        }
        "tabs" => {
            closed(map, "a tabs node", &["tabs", "active", "share", "collapsed"])?;
            let Some(Json::Array(keys)) = map.get("tabs") else {
                return Err(refuse("a tabs node's `tabs` is not a list of pane keys"));
            };
            if keys.is_empty() || keys.len() > MAX_TABS {
                return Err(refuse(format!("a tab group holds 1 to {MAX_TABS} panes")));
            }
            let mut tabs = Vec::new();
            for key in keys {
                let Json::String(key) = key else {
                    return Err(refuse("a tabs node's `tabs` is not a list of pane keys"));
                };
                tabs.push(place(key, panes, placed)?);
            }
            let active = match map.get("active") {
                None => tabs[0].clone(),
                Some(Json::String(a)) if tabs.contains(a) => a.clone(),
                Some(_) => return Err(refuse("a tabs node's `active` is not one of its tabs")),
            };
            Ok(Layout::Tabs { tabs, active })
        }
        _ => {
            closed(map, "a split", &["split", "children", "share", "collapsed"])?;
            let axis = match map.get("split") {
                Some(Json::String(s)) if s == "row" => Axis::Row,
                Some(Json::String(s)) if s == "column" => Axis::Column,
                _ => return Err(refuse("a split's axis is `row` or `column`")),
            };
            let Some(Json::Array(raw)) = map.get("children") else {
                return Err(refuse("a split has no `children` list"));
            };
            if raw.len() < 2 {
                return Err(refuse("a split has at least two children"));
            }
            let mut children = Vec::new();
            let mut total = 0u32;
            for child in raw {
                let map = object(child, "a split's child")?;
                let share = match map.get("share").and_then(Json::as_u64) {
                    Some(s) if (MIN_SHARE as u64..=WHOLE as u64).contains(&s) => s as u32,
                    _ => {
                        return Err(refuse(format!(
                            "a split's child needs a `share`: an integer from {MIN_SHARE} to {WHOLE} thousandths"
                        )))
                    }
                };
                let collapsed = match map.get("collapsed") {
                    None | Some(Json::Bool(false)) => false,
                    Some(Json::Bool(true)) if !map.contains_key("split") => true,
                    _ => {
                        return Err(refuse(
                            "`collapsed` is `true` on a pane or a tabs node only",
                        ))
                    }
                };
                total += share;
                children.push(Child {
                    share,
                    collapsed,
                    node: layout(child, depth + 1, panes, placed)?,
                });
            }
            if total != WHOLE {
                return Err(refuse(format!(
                    "the shares in a split add up to {total}, not {WHOLE}"
                )));
            }
            Ok(Layout::Split { axis, children })
        }
    }
    .and_then(|node| {
        if depth == 1 && (map.contains_key("share") || map.contains_key("collapsed")) {
            Err(refuse("the layout's root has no `share` and is not `collapsed`"))
        } else {
            Ok(node)
        }
    })
}

/// The selection in force: the address's, over the saved one. Each value is refused unless it
/// is one absolute IRI.
pub fn selection(
    workspace: &Workspace,
    address: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, Error> {
    let mut out = workspace.selection.clone();
    for (role, iri) in address {
        check_selection(role, iri)?;
        out.insert(role.clone(), iri.clone());
    }
    Ok(out)
}

/// A pane's content screen with the selection put in, or the role it is still waiting for.
pub fn content(pane: &Pane, selection: &BTreeMap<String, String>) -> Result<String, String> {
    let mut out = pane.content.clone();
    for role in pane.followed() {
        let slot = format!("{{{role}}}");
        if out.contains(&slot) {
            let iri = selection.get(role).ok_or_else(|| role.to_string())?;
            out = out.replace(&slot, iri);
        }
    }
    if out.starts_with("urn:iki:ui:screen:name:") {
        Ok(out)
    } else {
        Ok(format!("urn:iki:ui:screen:name:{out}"))
    }
}

fn text(name: &str, value: impl Into<String>) -> (String, Value) {
    (name.to_string(), Value::Text(value.into()))
}

fn node(kind: &str, properties: Vec<(String, Value)>, children: Vec<Node>) -> Node {
    Node {
        kind: kind.into(),
        hint: Default::default(),
        replacement: None,
        properties: properties.into_iter().collect(),
        children,
    }
}

fn hinted(mut n: Node, key: &str, value: impl Into<String>) -> Node {
    n.hint
        .insert(key.into(), Hint::Plain(Value::Text(value.into())));
    n
}

/// The words for what a pane shows: "now" or its live flag, and what it follows.
fn says(pane: &Pane, selection: &BTreeMap<String, String>) -> String {
    let mut words = Vec::new();
    for role in pane.followed() {
        match selection.get(role) {
            Some(iri) => words.push(format!("follows {role}: {iri}")),
            None => words.push(format!("follows {role}, nothing selected")),
        }
    }
    if pane.followed().is_empty() {
        words.push("pinned".into());
    }
    words.push(if pane.live { "live" } else { "now" }.into());
    words.join(", ")
}

/// What every pane's composition needs to know of the whole: the atom, its IRI and version.
struct Whole<'a> {
    workspace: &'a Workspace,
    iri: &'a str,
    version: u64,
}

fn hidden(label: &str, name: &str, value: impl Into<String>) -> Node {
    node(
        "field",
        vec![
            text("label", label),
            text("name", name),
            text("input", "hidden"),
            text("value", value),
        ],
        vec![],
    )
}

fn edit_form(
    whole: &Whole<'_>,
    label: String,
    description: Option<String>,
    mut fields: Vec<Node>,
) -> Node {
    let mut properties = vec![
        text("label", label),
        text("method", "post"),
        text("resource", edit(whole.iri)),
        text("verb", "sink"),
    ];
    if let Some(d) = description {
        properties.push(text("description", d));
    }
    fields.insert(
        0,
        hidden(
            "Made against version",
            "if-version",
            whole.version.to_string(),
        ),
    );
    node("form", properties, fields)
}

fn choice(label: &str, name: &str, options: Vec<(String, String)>) -> Node {
    node(
        "choice",
        vec![text("label", label), text("name", name)],
        options
            .into_iter()
            .map(|(value, label)| {
                node(
                    "option",
                    vec![text("label", label), text("value", value)],
                    vec![],
                )
            })
            .collect(),
    )
}

/// A pane's "Pane" menu (section 5A "Rearrange", ledger #3952): the edit forms that do what a
/// drag does, by keyboard alone and with no script. "Move…" (another pane, and left, right,
/// above, below or into tabs with it), "Put in tabs with…" (the panes whose tab group has room;
/// a full one is said in words), and on a tab "Move tab earlier" and "Move tab later". Each is
/// one edit posted to the edit Sink with `if-version`. `None` for a workspace of one pane.
fn pane_menu(key: &str, whole: &Whole<'_>) -> Option<Node> {
    let ws = whole.workspace;
    let label = &ws.panes[key].label;
    let others: Vec<&str> = reading_order(&ws.layout)
        .into_iter()
        .filter(|k| *k != key)
        .collect();
    if others.is_empty() {
        return None;
    }
    let mut forms = Vec::new();
    let names = |keys: &[&str]| -> Vec<(String, String)> {
        keys.iter()
            .map(|k| (k.to_string(), ws.panes[*k].label.clone()))
            .collect()
    };
    forms.push(edit_form(
        whole,
        format!("Move {label}…"),
        Some(format!(
            "Puts {label} beside another pane, or into tabs with it. Saving writes a new version."
        )),
        vec![
            hidden("Edit", "op", "move"),
            hidden("Pane", "pane", key),
            choice("Next to", "to", names(&others)),
            choice(
                "Where",
                "place",
                [
                    ("left", "To its left"),
                    ("right", "To its right"),
                    ("above", "Above it"),
                    ("below", "Below it"),
                    ("tabs", "Into tabs with it"),
                ]
                .into_iter()
                .map(|(v, l)| (v.to_string(), l.to_string()))
                .collect(),
            ),
            node("button", vec![text("label", "Move")], vec![]),
        ],
    ));
    let own = tab_group(&ws.layout, key);
    let mut full = Vec::new();
    let room: Vec<&str> = others
        .iter()
        .copied()
        .filter(|k| match tab_group(&ws.layout, k) {
            Some((tabs, _)) if tabs.iter().any(|t| t == key) => false,
            Some((tabs, _)) if tabs.len() >= MAX_TABS => {
                full.push(ws.panes[*k].label.as_str());
                false
            }
            _ => true,
        })
        .collect();
    let why = (!full.is_empty()).then(|| {
        format!(
            "Not offered: {}, in a full tab group. A tab group holds at most {MAX_TABS} panes.",
            full.join(", ")
        )
    });
    if !room.is_empty() {
        forms.push(edit_form(
            whole,
            format!("Put {label} in tabs with…"),
            why,
            vec![
                hidden("Edit", "op", "move"),
                hidden("Pane", "pane", key),
                hidden("Where", "place", "tabs"),
                choice("Tabs with", "to", names(&room)),
                node("button", vec![text("label", "Put in tabs")], vec![]),
            ],
        ));
    } else if let Some(why) = why {
        forms.push(node("text", vec![text("content", why)], vec![]));
    }
    if let Some((tabs, i)) = own {
        for (to, word) in [
            (i.checked_sub(1), "earlier"),
            (Some(i + 1).filter(|n| *n < tabs.len()), "later"),
        ] {
            if let Some(to) = to {
                forms.push(edit_form(
                    whole,
                    format!("Move {label} tab {word}"),
                    None,
                    vec![
                        hidden("Edit", "op", "reorder"),
                        hidden("Pane", "pane", key),
                        hidden("To position", "index", to.to_string()),
                        node(
                            "button",
                            vec![text("label", format!("Move tab {word}"))],
                            vec![],
                        ),
                    ],
                ));
            }
        }
    }
    Some(hinted(
        node("group", vec![text("label", label)], forms),
        MENU,
        key,
    ))
}

fn pane_region(
    key: &str,
    pane: &Pane,
    selection: &BTreeMap<String, String>,
    collapsed: bool,
    whole: &Whole<'_>,
) -> Node {
    let mut children = vec![
        node(
            "heading",
            vec![
                text("content", &pane.label),
                ("level".into(), Value::Number(2)),
            ],
            vec![],
        ),
        node("text", vec![text("content", says(pane, selection))], vec![]),
    ];
    children.extend(pane_menu(key, whole));
    let mut properties = vec![text("label", &pane.label)];
    match content(pane, selection) {
        Ok(screen) => {
            let alone = screen.trim_start_matches("urn:iki:ui:screen:name:");
            children.push(node(
                "link",
                vec![
                    text("label", format!("Open {} alone", pane.label)),
                    text("href", alone_href(alone)),
                ],
                vec![],
            ));
            if !collapsed {
                children.push(node(
                    crate::reference::KIND,
                    vec![
                        text(crate::reference::RESOURCE, &screen),
                        text(
                            crate::reference::FALLBACK,
                            format!("{} is not shown under your capability.", pane.label),
                        ),
                    ],
                    vec![],
                ));
            }
            properties.push(text(crate::region::ALLOW, screen));
        }
        Err(role) => children.push(node(
            "text",
            vec![text(
                "content",
                format!("Nothing is selected for {role}. Select one to fill this pane."),
            )],
            vec![],
        )),
    }
    if let Some(scopes) = &pane.scopes {
        properties.push(text(crate::region::SCOPES, scopes));
    }
    let mut region = hinted(node(crate::region::KIND, properties, children), PANE, key);
    region = hinted(region, DROP, offered(whole.workspace, key).join(" "));
    if collapsed {
        region = hinted(region, COLLAPSED, "collapsed");
    }
    region
}

/// A pane's "Open alone" address: its lensed page, or its screen's fragment route.
fn alone_href(screen: &str) -> String {
    if let Some(rest) = screen.strip_prefix("lens:") {
        if let Some((lens, resource)) = rest.split_once(":resource:") {
            if !lens.contains(':') {
                return format!("/lens/name/{lens}/resource/{resource}");
            }
        }
    }
    format!("/ui/html:screen:{screen}")
}

fn compose_layout(
    layout: &Layout,
    whole: &Whole<'_>,
    selection: &BTreeMap<String, String>,
    collapsed: bool,
) -> Node {
    let workspace = whole.workspace;
    match layout {
        Layout::Pane(key) => pane_region(key, &workspace.panes[key], selection, collapsed, whole),
        Layout::Tabs { tabs, active } => {
            let label = tabs
                .iter()
                .map(|k| workspace.panes[k].label.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            let children = tabs
                .iter()
                .map(|k| pane_region(k, &workspace.panes[k], selection, collapsed, whole))
                .collect();
            let mut group = node(
                "group",
                vec![text("label", format!("Tabs: {label}"))],
                children,
            );
            group = hinted(group, TABS, active);
            if collapsed {
                group = hinted(group, COLLAPSED, "collapsed");
            }
            group
        }
        Layout::Split { axis, children } => {
            let shares = children
                .iter()
                .map(|c| c.share.to_string())
                .collect::<Vec<_>>()
                .join(" ");
            let names = children
                .iter()
                .map(|c| first_label(&c.node, workspace))
                .collect::<Vec<_>>()
                .join(", ");
            let composed = children
                .iter()
                .map(|c| compose_layout(&c.node, whole, selection, c.collapsed))
                .collect();
            let word = match axis {
                Axis::Row => "Side by side",
                Axis::Column => "Stacked",
            };
            hinted(
                node(
                    "group",
                    vec![text("label", format!("{word}: {names}"))],
                    composed,
                ),
                SPLIT,
                format!("{} {shares}", axis.as_str()),
            )
        }
    }
}

fn first_label<'a>(layout: &'a Layout, workspace: &'a Workspace) -> &'a str {
    match layout {
        Layout::Pane(k) => &workspace.panes[k].label,
        Layout::Tabs { active, .. } => &workspace.panes[active].label,
        Layout::Split { children, .. } => first_label(&children[0].node, workspace),
    }
}

/// The panes in reading order (layout order, depth first).
pub fn reading_order(layout: &Layout) -> Vec<&str> {
    match layout {
        Layout::Pane(k) => vec![k.as_str()],
        Layout::Tabs { tabs, .. } => tabs.iter().map(String::as_str).collect(),
        Layout::Split { children, .. } => children
            .iter()
            .flat_map(|c| reading_order(&c.node))
            .collect(),
    }
}

/// The "Layout" form (section 5A): every split child's share as a number, posting one
/// `resize` edit to the edit Sink with `if-version`. It works with no script.
pub fn layout_form(workspace: &Workspace, iri: &str, version: u64) -> Node {
    let mut fields = vec![
        node(
            "field",
            vec![
                text("label", "Edit"),
                text("name", "op"),
                text("input", "hidden"),
                text("value", "resize"),
            ],
            vec![],
        ),
        node(
            "field",
            vec![
                text("label", "Made against version"),
                text("name", "if-version"),
                text("input", "hidden"),
                text("value", version.to_string()),
            ],
            vec![],
        ),
    ];
    fn walk(layout: &Layout, ws: &Workspace, path: &str, out: &mut Vec<Node>) {
        if let Layout::Split { axis, children } = layout {
            let word = match axis {
                Axis::Row => "width",
                Axis::Column => "height",
            };
            for (i, child) in children.iter().enumerate() {
                let at = format!("{path}.{i}");
                out.push(node(
                    "field",
                    vec![
                        text(
                            "label",
                            format!("{} {word}, in thousandths", first_label(&child.node, ws)),
                        ),
                        text("name", format!("share{at}")),
                        text("input", "number"),
                        text("value", child.share.to_string()),
                        text(
                            "description",
                            format!(
                                "From {MIN_SHARE} to {WHOLE}. The shares side by side add up to {WHOLE}."
                            ),
                        ),
                    ],
                    vec![],
                ));
                walk(&child.node, ws, &at, out);
            }
        }
    }
    walk(&workspace.layout, workspace, "", &mut fields);
    fields.push(node("button", vec![text("label", "Save layout")], vec![]));
    node(
        "form",
        vec![
            text("label", "Layout"),
            text("method", "post"),
            text("resource", edit(iri)),
            text("verb", "sink"),
            text(
                "description",
                "Sizes each pane as a number. Saving writes a new version.",
            ),
        ],
        fields,
    )
}

/// One `resize` edit (section 5A) applied to a workspace: the "Layout" form's arguments, or
/// the same arguments the shell's splitter sends at the end of a drag or a second after the last
/// key. Each `share{path}` (`share.0`, `share.1.0`, as [`layout_form`] names them) sets that
/// split child's share; a child not named keeps its own. `op` must be `resize`; `if-version`
/// is the host's to check against its latest version and is not looked at here. Anything else,
/// a path that is not a split child, or a result whose shares do not add up to [`WHOLE`] in
/// every split, each at least [`MIN_SHARE`], is refused naming it, and nothing is changed.
/// The edit only moves shares: no pane, address or selection changes (decision 10).
pub fn resize(workspace: &Workspace, args: &BTreeMap<String, String>) -> Result<Workspace, Error> {
    match args.get("op").map(String::as_str) {
        Some("resize") => {}
        Some(op) => return Err(refuse(format!("`{op}` is not a resize edit"))),
        None => return Err(refuse("the edit has no `op`")),
    }
    let mut out = workspace.clone();
    for (name, value) in args {
        if name == "op" || name == "if-version" {
            continue;
        }
        let Some(path) = name.strip_prefix("share.") else {
            return Err(refuse(format!(
                "`{name}` is not an argument of a resize edit"
            )));
        };
        let share: u32 = value
            .trim()
            .parse()
            .map_err(|_| refuse(format!("`{name}` is not a whole number of thousandths")))?;
        let mut at = &mut out.layout;
        let steps = path.split('.').collect::<Vec<_>>();
        for (n, step) in steps.iter().enumerate() {
            let i: usize = step
                .parse()
                .map_err(|_| refuse(format!("`{name}` names no split child")))?;
            let Layout::Split { children, .. } = at else {
                return Err(refuse(format!("`{name}` names no split child")));
            };
            let Some(child) = children.get_mut(i) else {
                return Err(refuse(format!("`{name}` names no split child")));
            };
            if n + 1 == steps.len() {
                child.share = share;
            }
            at = &mut child.node;
        }
    }
    fn check(layout: &Layout, path: &str) -> Result<(), Error> {
        if let Layout::Split { children, .. } = layout {
            let mut total = 0;
            for (i, child) in children.iter().enumerate() {
                if !(MIN_SHARE..=WHOLE).contains(&child.share) {
                    return Err(refuse(format!(
                        "`share{path}.{i}` is {}: a share is from {MIN_SHARE} to {WHOLE} thousandths",
                        child.share
                    )));
                }
                total += child.share;
                check(&child.node, &format!("{path}.{i}"))?;
            }
            if total != WHOLE {
                let at = if path.is_empty() {
                    "the layout's root"
                } else {
                    path
                };
                return Err(refuse(format!(
                    "the shares in the split at {at} add up to {total}, not {WHOLE}"
                )));
            }
        }
        Ok(())
    }
    check(&out.layout, "")?;
    Ok(out)
}

/// The workspace as an ordinary screen: its heading, the pane list, the layout (groups with
/// the split and tabs hints, panes as regions) and the "Layout" form. A pure function of the
/// atom, its IRI, its version and the selection in the address: it reads nothing and writes
/// nothing.
pub fn compose(
    workspace: &Workspace,
    iri: &str,
    version: u64,
    address: &BTreeMap<String, String>,
) -> Result<Node, Error> {
    let selection = selection(workspace, address)?;
    let list = reading_order(&workspace.layout)
        .into_iter()
        .map(|k| {
            let pane = &workspace.panes[k];
            node(
                "link",
                vec![
                    text(
                        "label",
                        format!("{}: {}", pane.label, says(pane, &selection)),
                    ),
                    text("href", format!("/workspace/{iri}#pane-{k}")),
                ],
                vec![],
            )
        })
        .collect();
    Ok(node(
        "group",
        vec![text("label", &workspace.label)],
        vec![
            node(
                "heading",
                vec![
                    text("content", &workspace.label),
                    ("level".into(), Value::Number(1)),
                ],
                vec![],
            ),
            node("nav", vec![text("label", "Panes")], list),
            compose_layout(
                &workspace.layout,
                &Whole {
                    workspace,
                    iri,
                    version,
                },
                &selection,
                false,
            ),
            layout_form(workspace, iri, version),
        ],
    ))
}

/// The workspace screen, `urn:iki:ui:screen:workspace:{iri}`: reads the atom at `{iri}` under
/// the caller's capability (`Source`, never anything else), validates it and [`compose`]s it.
/// The selection is the request's `select.{role}` arguments; `version` (the version the host's
/// store answered for the bare name, default 1) is what the "Layout" form's `if-version` says.
pub(crate) fn screen_endpoint() -> ikigai_core::AsyncFnEndpoint {
    use ikigai_core::{
        ArgSpec, AsyncFnEndpoint, Description, Invocation, InvokeFuture, ReprType, Representation,
        Request, Verb,
    };
    AsyncFnEndpoint::new(
        "ui-workspace-screen",
        |inv: &Invocation<'_>| -> InvokeFuture<'_> {
            Box::pin(async move {
                let iri = inv.bindings.get("iri").unwrap_or_default().to_string();
                if parse_name(&iri).is_none() && !iri.contains(":version:") {
                    return Err(refuse(format!("`{iri}` is not a workspace")));
                }
                let mut address = BTreeMap::new();
                let mut version = 1u64;
                for name in inv.request.args.keys() {
                    let value = inv.inline_str(name)?;
                    if let Some(role) = name.strip_prefix("select.") {
                        address.insert(role.to_string(), value.to_string());
                    } else if name == "version" {
                        version = value
                            .parse()
                            .map_err(|_| refuse("`version` is not a whole number"))?;
                    }
                }
                let atom = inv
                    .issue(Request::new(
                        Verb::Source,
                        Iri::parse(iri.as_str()).map_err(|e| refuse(e.to_string()))?,
                    ))
                    .await?;
                // A host's store may say which version it answered (`application/json;
                // version=3`, ledger #3952), so the edit forms carry the right `if-version`.
                if !inv.request.args.contains_key("version") {
                    if let Some(n) = atom
                        .repr_type
                        .params
                        .get("version")
                        .and_then(|v| v.parse().ok())
                    {
                        version = n;
                    }
                }
                let workspace = validate(&atom.bytes)?;
                let node = compose(&workspace, &iri, version, &address)?;
                crate::model::validate(&node)?;
                let bytes = serde_json::to_vec(&node).map_err(|e| refuse(e.to_string()))?;
                Ok(Representation::new(ReprType::new(crate::model::MEDIA_TYPE), bytes).cacheable())
            })
        },
    )
    .with_description(
        Description::new("ui-workspace-screen")
            .title("A workspace as a screen")
            .summary(
                "Reads the workspace atom under the caller's capability, validates it and \
                 composes its layout into groups with the layout hints and its panes into \
                 regions, each referencing its content screen with the selection put in. \
                 Reads only; a selection never writes.",
            )
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .input(
                ArgSpec::new("iri")
                    .class("http://www.w3.org/2001/XMLSchema#anyURI")
                    .binding(),
            )
            .output(crate::model::MEDIA_TYPE),
    )
}

/// The workspace screen's face name, without [`FACE`]'s `urn:iki:ui:screen:name:` part:
/// `workspace:resource:{iri}`, or `workspace:select:{role}={pct-iri}&…:resource:{iri}` with a
/// selection. Each selected IRI is percent-encoded, so no `:` in it splits the name.
pub fn face_name(workspace: &str, selection: &BTreeMap<String, String>) -> String {
    if selection.is_empty() {
        return format!("workspace:resource:{workspace}");
    }
    let select = selection
        .iter()
        .map(|(role, iri)| format!("{role}={}", crate::lens::pct(iri)))
        .collect::<Vec<_>>()
        .join("&");
    format!("workspace:select:{select}:resource:{workspace}")
}

/// A face name read back: the workspace's IRI and the selection by role.
pub type FaceName = (String, BTreeMap<String, String>);

/// A face name's workspace and selection, from what follows `workspace:`. `None` when it is
/// not one; a refusal when its selection is malformed.
pub fn parse_face_name(rest: &str) -> Option<Result<FaceName, Error>> {
    if let Some(iri) = rest.strip_prefix("resource:") {
        return (!iri.is_empty()).then(|| Ok((iri.to_string(), BTreeMap::new())));
    }
    let (select, iri) = rest.strip_prefix("select:")?.split_once(":resource:")?;
    if iri.is_empty() {
        return None;
    }
    let mut out = BTreeMap::new();
    for pair in select.split('&').filter(|p| !p.is_empty()) {
        let Some((role, value)) = pair.split_once('=') else {
            return Some(Err(refuse(format!("`{pair}` is not role=iri"))));
        };
        match crate::lens::unpct(value) {
            Ok(v) => out.insert(role.to_string(), v),
            Err(e) => return Some(Err(e)),
        };
    }
    Some(Ok((iri.to_string(), out)))
}

/// The screen a browser's request names: a workspace face name with the request's
/// `select.{role}` parameters folded in over the name's own (an empty value drops the role),
/// else `screen` itself.
pub fn fold(inv: &ikigai_core::Invocation<'_>, screen: &str) -> Result<String, Error> {
    let Some(rest) = screen.strip_prefix("workspace:") else {
        return Ok(screen.to_string());
    };
    let Some(parsed) = parse_face_name(rest) else {
        return Ok(screen.to_string());
    };
    let (iri, mut chosen) = parsed?;
    let mut changed = false;
    for name in inv.request.args.keys() {
        if let Some(role) = name.strip_prefix("select.") {
            let value = inv.inline_str(name)?;
            changed = true;
            if value.is_empty() {
                chosen.remove(role);
            } else {
                chosen.insert(role.to_string(), value.to_string());
            }
        }
    }
    Ok(if changed {
        face_name(&iri, &chosen)
    } else {
        screen.to_string()
    })
}

/// [`FACE`]`{rest}`: the workspace screen under its face name, read through
/// `urn:iki:ui:screen:workspace:{iri}` with the name's selection as `select.{role}` arguments.
/// Reads only, under the caller's capability.
pub(crate) fn face_endpoint() -> ikigai_core::AsyncFnEndpoint {
    use ikigai_core::{
        ArgRef, ArgSpec, AsyncFnEndpoint, Description, Invocation, InvokeFuture, Request, Verb,
    };
    AsyncFnEndpoint::new(
        "ui-workspace-face",
        |inv: &Invocation<'_>| -> InvokeFuture<'_> {
            Box::pin(async move {
                let rest = inv.bindings.get("rest").unwrap_or_default();
                let (iri, selection) = parse_face_name(rest)
                    .ok_or_else(|| Error::NotFound(format!("workspace:{rest}")))??;
                let mut request = Request::new(
                    Verb::Source,
                    Iri::parse(screen(&iri)).map_err(|e| refuse(e.to_string()))?,
                );
                for (role, value) in &selection {
                    request = request.with_arg(
                        format!("select.{role}"),
                        ArgRef::Inline(value.as_bytes().to_vec()),
                    );
                }
                inv.issue(request).await
            })
        },
    )
    .with_description(
        Description::new("ui-workspace-face")
            .title("A workspace as a named screen")
            .summary(
                "The workspace screen under a screen name, workspace:resource:{iri} or \
                 workspace:select:{role}={iri}&…:resource:{iri}, so a face reaches it like any \
                 named screen. Reads only.",
            )
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .input(
                ArgSpec::new("rest")
                    .summary("resource:{iri}, or select:{role}={pct-iri}&…:resource:{iri}")
                    .class("http://www.w3.org/2001/XMLSchema#string")
                    .binding()
                    .example("resource:urn:iki:ui:workspace:person:urn:person:probe:name:probe"),
            )
            .output(crate::model::MEDIA_TYPE),
    )
}

/// `urn:iki:ui:workspace:title:{rest}`: a workspace page's title, `{rest}` as in
/// [`parse_face_name`]: the atom's `label`, read under the caller's capability, or "Workspace"
/// when it cannot be read or is not a workspace (the page's main content says why).
pub const TITLE: &str = "urn:iki:ui:workspace:title:";

/// The screen a workspace PAGE shows (`{base}page:workspace:{iri}`, [`crate::Pages::workspaces`]):
/// the IRI as the door gave it, percent-encoded or not, decoded and checked (an absolute IRI with
/// no `?`, `#`, `%`, `<`, `>`, `"` or space, naming a workspace), as its face name
/// (`workspace:resource:{iri}`, [`face_name`]) with the request's `select.{role}` parameters
/// folded in ([`fold`]). Anything else is refused with `InvalidArgument` before anything is read.
pub fn page_screen(inv: &ikigai_core::Invocation<'_>, given: &str) -> Result<String, Error> {
    let iri = crate::lens::unpct(given)?;
    if iri
        .chars()
        .any(|c| c.is_whitespace() || matches!(c, '?' | '#' | '%' | '<' | '>' | '"'))
    {
        return Err(refuse(format!(
            "`{iri}`: a workspace page's IRI carries no ? # % < > \" or space"
        )));
    }
    if parse_name(&iri).is_none() && !iri.contains(":version:") {
        return Err(refuse(format!("`{iri}` is not a workspace")));
    }
    Iri::parse(iri.as_str()).map_err(|e| refuse(e.to_string()))?;
    fold(inv, &face_name(&iri, &BTreeMap::new()))
}

/// [`TITLE`]`{rest}`: the atom's label, as text, for the page's `<title>`.
pub(crate) fn title_endpoint() -> ikigai_core::AsyncFnEndpoint {
    use ikigai_core::{
        ArgSpec, AsyncFnEndpoint, Description, Invocation, InvokeFuture, ReprType, Representation,
        Request, Verb,
    };
    AsyncFnEndpoint::new(
        "ui-workspace-title",
        |inv: &Invocation<'_>| -> InvokeFuture<'_> {
            Box::pin(async move {
                let rest = inv.bindings.get("rest").unwrap_or_default();
                let (iri, _) = parse_face_name(rest)
                    .ok_or_else(|| Error::NotFound(format!("{TITLE}{rest}")))??;
                let label = match Iri::parse(iri.as_str()) {
                    Ok(at) => match inv.issue(Request::new(Verb::Source, at)).await {
                        Ok(atom) => validate(&atom.bytes).ok().map(|w| w.label),
                        Err(_) => None,
                    },
                    Err(_) => None,
                };
                let label = label.unwrap_or_else(|| "Workspace".to_string());
                Ok(Representation::new(ReprType::new(crate::HTML), label.into_bytes()).cacheable())
            })
        },
    )
    .with_description(
        Description::new("ui-workspace-title")
            .title("A workspace page's title")
            .summary(
                "The title a workspace page's shell shows: the atom's label, read under the \
                 caller's capability, as text; Workspace when it cannot be read.",
            )
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .input(
                ArgSpec::new("rest")
                    .summary("resource:{iri}, or select:{role}={pct-iri}&…:resource:{iri}")
                    .class("http://www.w3.org/2001/XMLSchema#string")
                    .binding()
                    .example("resource:urn:iki:ui:workspace:person:urn:person:probe:name:probe"),
            )
            .output(crate::HTML),
    )
}

/// [`SHAPE`]: the atom's closed SHACL shape, [`SHAPE_TTL`].
pub(crate) fn shape_endpoint() -> ikigai_core::FnEndpoint {
    use ikigai_core::{Description, Verb};
    crate::fixed("ui-shape-workspace", crate::TURTLE, SHAPE_TTL).with_description(
        Description::new("ui-shape-workspace")
            .title("The workspace atom's shape")
            .summary(
                "The closed SHACL shape of a workspace atom: its label, layout (a split, tabs \
                 or one pane), panes and selection. It has no blank nodes.",
            )
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .output(crate::TURTLE),
    )
}

/// A rearrange hint on a pane's region (ledger #3952): the places a pane dragged onto it may go,
/// `left right above below tabs` or fewer ([`offered`]). The shell's drag shows only these.
pub const DROP: &str = "urn:iki:ui:hint:layout:drop";
/// A pane's "Pane" menu (ledger #3952): a group of the edit forms that rearrange it.
pub const MENU: &str = "urn:iki:ui:hint:layout:menu";

/// Where a `move` edit puts a pane, relative to another (section 5A "Rearrange").
pub const PLACES: [&str; 5] = ["left", "right", "above", "below", "tabs"];

/// The atom as JSON, in the shape [`validate`] reads: `validate(&atom(ws))` gives `ws` back.
pub fn atom(workspace: &Workspace) -> Vec<u8> {
    fn node(layout: &Layout, out: &mut Map<String, Json>) {
        match layout {
            Layout::Pane(k) => {
                out.insert("pane".into(), Json::String(k.clone()));
            }
            Layout::Tabs { tabs, active } => {
                out.insert(
                    "tabs".into(),
                    Json::Array(tabs.iter().cloned().map(Json::String).collect()),
                );
                out.insert("active".into(), Json::String(active.clone()));
            }
            Layout::Split { axis, children } => {
                out.insert("split".into(), Json::String(axis.as_str().into()));
                let kids = children
                    .iter()
                    .map(|c| {
                        let mut m = Map::new();
                        m.insert("share".into(), Json::from(c.share));
                        if c.collapsed {
                            m.insert("collapsed".into(), Json::Bool(true));
                        }
                        node(&c.node, &mut m);
                        Json::Object(m)
                    })
                    .collect();
                out.insert("children".into(), Json::Array(kids));
            }
        }
    }
    let mut layout = Map::new();
    node(&workspace.layout, &mut layout);
    let json = serde_json::json!({
        "label": workspace.label,
        "layout": Json::Object(layout),
        "panes": workspace.panes,
        "selection": workspace.selection,
    });
    serde_json::to_vec(&json).expect("a workspace is JSON")
}

fn depth_of(layout: &Layout) -> usize {
    match layout {
        Layout::Split { children, .. } => {
            1 + children
                .iter()
                .map(|c| depth_of(&c.node))
                .max()
                .unwrap_or(0)
        }
        _ => 1,
    }
}

/// Whether `layout` is the unit a drop on `key` lands beside: the pane leaf itself, or the
/// tabs node holding it (a pane in tabs is placed beside its whole tab group).
fn is_unit(layout: &Layout, key: &str) -> bool {
    match layout {
        Layout::Pane(k) => k == key,
        Layout::Tabs { tabs, .. } => tabs.iter().any(|t| t == key),
        Layout::Split { .. } => false,
    }
}

/// The tabs node holding `key`, and its position there.
fn tab_group<'a>(layout: &'a Layout, key: &str) -> Option<(&'a [String], usize)> {
    match layout {
        Layout::Tabs { tabs, .. } => tabs.iter().position(|t| t == key).map(|i| (&tabs[..], i)),
        Layout::Pane(_) => None,
        Layout::Split { children, .. } => children.iter().find_map(|c| tab_group(&c.node, key)),
    }
}

/// A unit, its depth (the root is 1), and the axis and share it has in the split around it.
type Unit<'a> = (&'a Layout, usize, Option<(Axis, u32)>);

/// The unit holding `key`, its depth (the root is 1), and the axis and share it has in the split
/// around it, if any.
fn unit_of<'a>(
    layout: &'a Layout,
    key: &str,
    depth: usize,
    around: Option<(Axis, u32)>,
) -> Option<Unit<'a>> {
    if is_unit(layout, key) {
        return Some((layout, depth, around));
    }
    if let Layout::Split { axis, children } = layout {
        for c in children {
            if let Some(found) = unit_of(&c.node, key, depth + 1, Some((*axis, c.share))) {
                return Some(found);
            }
        }
    }
    None
}

fn axis_of(place: &str) -> Axis {
    if matches!(place, "left" | "right") {
        Axis::Row
    } else {
        Axis::Column
    }
}

/// The places a pane may be dropped on `target` without breaking a bound (section 5A
/// "Bounds"): `tabs` unless its tab group holds [`MAX_TABS`]; an edge unless the split it makes
/// would nest the layout deeper than [`MAX_DEPTH`]. The edit Sink checks again, whatever is
/// offered.
pub fn offered(workspace: &Workspace, target: &str) -> Vec<&'static str> {
    let Some((unit, depth, around)) = unit_of(&workspace.layout, target, 1, None) else {
        return Vec::new();
    };
    PLACES
        .into_iter()
        .filter(|place| match *place {
            "tabs" => !matches!(unit, Layout::Tabs { tabs, .. } if tabs.len() >= MAX_TABS),
            edge => {
                let beside = matches!(around, Some((a, share)) if a == axis_of(edge) && share / 2 >= MIN_SHARE);
                beside || depth < MAX_DEPTH
            }
        })
        .collect()
}

fn args_get<'a>(args: &'a BTreeMap<String, String>, name: &str) -> Result<&'a str, Error> {
    args.get(name)
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| refuse(format!("the edit has no `{name}`")))
}

fn closed_args(args: &BTreeMap<String, String>, op: &str, known: &[&str]) -> Result<(), Error> {
    match args
        .keys()
        .find(|k| !known.contains(&k.as_str()) && *k != "op" && *k != "if-version")
    {
        Some(k) => Err(refuse(format!("`{k}` is not an argument of a {op} edit"))),
        None => Ok(()),
    }
}

/// `layout` with pane `key` taken out: its tab group loses it (a group of one becomes the pane
/// alone), its split loses it and the neighbor before it (or after it) takes its share, and a
/// split left with one child becomes that child. `None` when nothing is left.
fn take(layout: Layout, key: &str) -> Option<Layout> {
    match layout {
        Layout::Pane(k) if k == key => None,
        Layout::Pane(k) => Some(Layout::Pane(k)),
        Layout::Tabs { mut tabs, active } => {
            let Some(i) = tabs.iter().position(|t| t == key) else {
                return Some(Layout::Tabs { tabs, active });
            };
            tabs.remove(i);
            match tabs.len() {
                0 => None,
                1 => Some(Layout::Pane(tabs.remove(0))),
                _ => {
                    let active = if active == key {
                        tabs[i.saturating_sub(1)].clone()
                    } else {
                        active
                    };
                    Some(Layout::Tabs { tabs, active })
                }
            }
        }
        Layout::Split { axis, children } => {
            let mut out: Vec<Child> = Vec::new();
            let mut freed = 0;
            let mut gone_at = None;
            for (i, c) in children.into_iter().enumerate() {
                match take(c.node, key) {
                    Some(node) => out.push(Child { node, ..c }),
                    None => {
                        freed = c.share;
                        gone_at = Some(i);
                    }
                }
            }
            if let Some(i) = gone_at {
                let n = if i > 0 { i - 1 } else { 0 };
                if let Some(c) = out.get_mut(n) {
                    c.share += freed;
                }
            }
            match out.len() {
                0 => None,
                1 => Some(out.remove(0).node),
                _ => Some(Layout::Split {
                    axis,
                    children: out,
                }),
            }
        }
    }
}

/// `layout` with pane `moved` put at `place` of the unit holding `target`.
fn put(layout: Layout, moved: &str, target: &str, place: &str) -> Layout {
    if is_unit(&layout, target) {
        if place == "tabs" {
            return match layout {
                Layout::Tabs { mut tabs, .. } => {
                    tabs.push(moved.to_string());
                    Layout::Tabs {
                        tabs,
                        active: moved.to_string(),
                    }
                }
                unit => Layout::Tabs {
                    tabs: vec![reading_order(&unit)[0].to_string(), moved.to_string()],
                    active: moved.to_string(),
                },
            };
        }
        let new = Child {
            share: WHOLE / 2,
            collapsed: false,
            node: Layout::Pane(moved.to_string()),
        };
        let old = Child {
            share: WHOLE / 2,
            collapsed: false,
            node: layout,
        };
        let children = if matches!(place, "left" | "above") {
            vec![new, old]
        } else {
            vec![old, new]
        };
        return Layout::Split {
            axis: axis_of(place),
            children,
        };
    }
    match layout {
        Layout::Split { axis, children } => {
            let mut out = Vec::new();
            for c in children {
                let beside = place != "tabs"
                    && is_unit(&c.node, target)
                    && axis == axis_of(place)
                    && c.share / 2 >= MIN_SHARE;
                if beside {
                    let half = c.share / 2;
                    let new = Child {
                        share: half,
                        collapsed: false,
                        node: Layout::Pane(moved.to_string()),
                    };
                    let old = Child {
                        share: c.share - half,
                        ..c
                    };
                    if matches!(place, "left" | "above") {
                        out.extend([new, old]);
                    } else {
                        out.extend([old, new]);
                    }
                } else if c.collapsed && is_unit(&c.node, target) && place != "tabs" {
                    // A collapsed unit stays collapsed inside the split it now shares.
                    let share = c.share;
                    let inner = Child {
                        share: WHOLE / 2,
                        collapsed: true,
                        node: c.node,
                    };
                    let new = Child {
                        share: WHOLE / 2,
                        collapsed: false,
                        node: Layout::Pane(moved.to_string()),
                    };
                    let children = if matches!(place, "left" | "above") {
                        vec![new, inner]
                    } else {
                        vec![inner, new]
                    };
                    out.push(Child {
                        share,
                        collapsed: false,
                        node: Layout::Split {
                            axis: axis_of(place),
                            children,
                        },
                    });
                } else {
                    out.push(Child {
                        node: put(c.node, moved, target, place),
                        ..c
                    });
                }
            }
            Layout::Split {
                axis,
                children: out,
            }
        }
        other => other,
    }
}

/// The sentence `#status` says after a move (section 5A "Accessibility of the interaction").
fn moved_sentence(moved: &str, target: &str, place: &str) -> String {
    match place {
        "left" => format!("Moved {moved} to the left of {target}."),
        "right" => format!("Moved {moved} to the right of {target}."),
        "above" => format!("Moved {moved} above {target}."),
        "below" => format!("Moved {moved} below {target}."),
        _ => format!("Put {moved} in tabs with {target}."),
    }
}

/// One `move` edit (section 5A "Rearrange"): `pane` taken out of where it is and put at
/// `place` (`left`, `right`, `above`, `below` or `tabs`, [`PLACES`]) of `to`. A pane in tabs is
/// placed beside, or added to, its whole tab group. Beside a pane in a split of the same axis
/// it takes half that pane's share; otherwise the two make a new split, half each. Into tabs, it
/// joins the tab group (or makes one of the two) and is its active tab. Out of tabs to an edge,
/// it leaves its group (a group of one becomes the pane alone). Refused, naming why, when a pane
/// is not in the workspace, the move is onto itself, or the result breaks a bound. It moves only
/// keys the atom already holds: no pane, address or selection is added (decision 10).
pub fn move_pane(
    workspace: &Workspace,
    args: &BTreeMap<String, String>,
) -> Result<(Workspace, String), Error> {
    closed_args(args, "move", &["pane", "to", "place"])?;
    let pane = args_get(args, "pane")?;
    let to = args_get(args, "to")?;
    let place = args_get(args, "place")?;
    let label = |k: &str| {
        workspace
            .panes
            .get(k)
            .map(|p| p.label.clone())
            .ok_or_else(|| refuse(format!("there is no pane `{k}` in this workspace")))
    };
    let (moved, target) = (label(pane)?, label(to)?);
    if !PLACES.contains(&place) {
        return Err(refuse(format!(
            "`{place}` is not a place: left, right, above, below or tabs"
        )));
    }
    if pane == to {
        return Err(refuse(format!("{moved} cannot be moved beside itself")));
    }
    if place == "tabs" {
        if let Some((tabs, _)) = tab_group(&workspace.layout, to) {
            if tabs.iter().any(|t| t == pane) {
                return Err(refuse(format!("{moved} is already in tabs with {target}")));
            }
            if tabs.len() >= MAX_TABS {
                return Err(refuse(format!(
                    "A tab group holds at most {MAX_TABS} panes"
                )));
            }
        }
    }
    let rest = take(workspace.layout.clone(), pane)
        .ok_or_else(|| refuse("a workspace keeps at least one pane"))?;
    let layout = put(rest, pane, to, place);
    if depth_of(&layout) > MAX_DEPTH {
        return Err(refuse(format!(
            "A layout nests at most {MAX_DEPTH} deep: {moved} cannot go there"
        )));
    }
    let out = Workspace {
        layout,
        ..workspace.clone()
    };
    // The bounds and every share, checked as the atom is read back.
    let out = validate(&atom(&out))?;
    Ok((out, moved_sentence(&moved, &target, place)))
}

/// One `reorder` edit (section 5A "Reorder tabs"): pane `pane` moved to position `index` (from
/// 0) of its tab group, and made its active tab. Refused when the pane is not in tabs or the
/// position is not in the group.
pub fn reorder(
    workspace: &Workspace,
    args: &BTreeMap<String, String>,
) -> Result<(Workspace, String), Error> {
    closed_args(args, "reorder", &["pane", "index"])?;
    let pane = args_get(args, "pane")?;
    let index: usize = args_get(args, "index")?
        .parse()
        .map_err(|_| refuse("`index` is not a whole number"))?;
    let label = workspace
        .panes
        .get(pane)
        .map(|p| p.label.clone())
        .ok_or_else(|| refuse(format!("there is no pane `{pane}` in this workspace")))?;
    fn walk(layout: &mut Layout, pane: &str, index: usize) -> Option<Result<usize, usize>> {
        match layout {
            Layout::Tabs { tabs, active } => {
                let i = tabs.iter().position(|t| t == pane)?;
                if index >= tabs.len() {
                    return Some(Err(tabs.len()));
                }
                let key = tabs.remove(i);
                tabs.insert(index, key);
                *active = pane.to_string();
                Some(Ok(tabs.len()))
            }
            Layout::Pane(_) => None,
            Layout::Split { children, .. } => children
                .iter_mut()
                .find_map(|c| walk(&mut c.node, pane, index)),
        }
    }
    let mut layout = workspace.layout.clone();
    let count = match walk(&mut layout, pane, index) {
        None => return Err(refuse(format!("{label} is not in tabs"))),
        Some(Err(n)) => {
            return Err(refuse(format!(
                "{label}'s tab group has {n} tabs: there is no tab {}",
                index + 1
            )))
        }
        Some(Ok(n)) => n,
    };
    let out = Workspace {
        layout,
        ..workspace.clone()
    };
    Ok((
        out,
        format!("Moved {label} to tab {} of {count}.", index + 1),
    ))
}

/// One edit applied (section 5A "The edit"): `op` is `resize` ([`resize`]), `move`
/// ([`move_pane`]) or `reorder` ([`reorder`]); the result and the one sentence `#status` says
/// for it (empty for a resize, whose splitter says it). `if-version` is the host's to check.
/// The result is validated as the atom: a host stores [`atom`] of it as the next version.
pub fn apply(
    workspace: &Workspace,
    args: &BTreeMap<String, String>,
) -> Result<(Workspace, String), Error> {
    match args.get("op").map(String::as_str) {
        Some("resize") => Ok((resize(workspace, args)?, String::new())),
        Some("move") => move_pane(workspace, args),
        Some("reorder") => reorder(workspace, args),
        Some(op) => Err(refuse(format!(
            "`{op}` is not an edit this workspace makes yet (resize, move or reorder)"
        ))),
        None => Err(refuse("the edit has no `op`")),
    }
}

/// What an edit Sink answers a form that posted with no script (section 5A): the sentence, in
/// a status paragraph marked `data-ui-said` (which the shell's script reads, as text), and a
/// link back to the workspace at the pane that changed.
pub fn edited_html(workspace: &str, pane: Option<&str>, sentence: &str) -> String {
    fn esc(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    }
    let said = if sentence.is_empty() {
        "Saved."
    } else {
        sentence
    };
    let at = pane
        .filter(|k| is_key(k))
        .map(|k| format!("#pane-{k}"))
        .unwrap_or_default();
    format!(
        "<p role=\"status\" data-ui-said>{}</p><p><a href=\"/workspace/{}{at}\">Back to the workspace</a></p>",
        esc(said),
        esc(workspace)
    )
}
