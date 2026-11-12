//! The UI model: a renderer-neutral tree of nodes, each one of a closed set of [`KINDS`] with
//! its properties, that every target walks (ledger #3351, #3352).
//!
//! ## Faces
//!
//! - `application/vnd.ikigai.ui+json` ([`MEDIA_TYPE`], the default): a [`Node`] as serde writes it,
//!   `{"kind": "heading", "properties": {"content": "…", "level": 2}, "children": […]}`.
//! - `application/ld+json` ([`JSON_LD`]): the same document with [`CONTEXT`] as its `@context`.
//!   The context names a kind by `urn:iki:ui:kind:name:{kind}` (`kind` is `@type`), a property
//!   by `urn:iki:ui:property:name:{p}` (`properties` is `@nest`), and the children by
//!   `urn:iki:ui:property:name:child`; each node is named by its path, which keeps their order.
//!
//! ## Text is text
//!
//! There is no raw-HTML node and no script node, and no property carries markup: every string is
//! text, and a renderer escapes it (`ikigai_fn::html`). A URL property (`href`, `src`, `action`)
//! must pass [`ikigai_fn::html::safe_href`].
//!
//! ## Validation and bounds
//!
//! [`validate`] walks a tree iteratively and refuses an unknown kind, a property the kind does
//! not name, a missing required property, a value of the wrong type and a child the kind does
//! not take, each as `InvalidArgument` naming the node's path (a JSON pointer: `/`,
//! `/children/0`, …). The Turtle shapes in `assets/kind/` say the same; `tests/model.rs` holds the
//! two to agreement. Optional `replacement` subtrees receive the same validation
//! and bounds as children. [`parse`] bounds a model that arrives as input in size ([`MAX_BYTES`]) and
//! depth ([`MAX_DEPTH`]) with an iterative pre-scan BEFORE serde sees it.

use std::collections::BTreeMap;

use ikigai_core::Error;
use serde::{Deserialize, Serialize};

/// The model's own JSON face.
pub const MEDIA_TYPE: &str = "application/vnd.ikigai.ui+json";
/// The JSON-LD face.
pub const JSON_LD: &str = "application/ld+json";
/// The one JSON-LD context both faces share, served at [`CONTEXT_IRI`].
pub const CONTEXT: &str = include_str!("../assets/context.jsonld");
/// Where [`CONTEXT`] is served.
pub const CONTEXT_IRI: &str = "urn:iki:ui:context";
/// A kind's IRI is this plus its name.
pub const KIND_PREFIX: &str = "urn:iki:ui:kind:name:";
/// A property's IRI is this plus its name.
pub const PROPERTY_PREFIX: &str = "urn:iki:ui:property:name:";
/// The largest model [`parse`] reads: 1 MiB.
pub const MAX_BYTES: usize = 1 << 20;
/// The deepest JSON nesting [`parse`] reads (objects and arrays together; a node level is two).
pub const MAX_DEPTH: usize = 96;

/// A property's value type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    /// Non-empty text.
    Text,
    /// A URL that `safe_href` accepts.
    Url,
    /// A heading level, 1 to 6.
    Level,
    /// `get` or `post`.
    Method,
    /// A boolean, or the text `true` or `false`.
    Flag,
    /// An absolute IRI: a resource's name, which a target maps to its own door.
    Iri,
    /// A verb a form issues: `source`, `sink`, `new` or `delete`.
    Verb,
    /// A table row's header scope: `column` (the row is the column headers) or `row` (the
    /// row's first cell heads the row).
    Scope,
}

/// What children a kind takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Children {
    /// None.
    Leaf,
    /// Any kind.
    Any,
    /// Only this kind.
    Only(&'static str),
}

/// One node kind: its name, its fallback property (the text a target that cannot draw it
/// shows; always required), its other required and optional properties, what children it takes
/// and its SHACL shape.
#[derive(Debug, Clone, Copy)]
pub struct Kind {
    /// The kind's name, the `{kind}` of `urn:iki:ui:kind:name:{kind}`.
    pub name: &'static str,
    /// The fallback property, required.
    pub fallback: &'static str,
    /// The other required properties.
    pub required: &'static [(&'static str, Type)],
    /// The optional properties.
    pub optional: &'static [(&'static str, Type)],
    /// The children it takes.
    pub children: Children,
    /// Its SHACL shape, Turtle.
    pub shape: &'static str,
    /// Its example node, `application/vnd.ikigai.ui+json`.
    pub example: &'static str,
}

impl Kind {
    /// The type of property `p` on this kind, if the kind names it.
    pub fn property(&self, p: &str) -> Option<Type> {
        if p == self.fallback {
            return Some(Type::Text);
        }
        self.required
            .iter()
            .chain(self.optional)
            .find(|(n, _)| *n == p)
            .map(|(_, t)| *t)
    }
}

macro_rules! kind {
    ($name:literal, $fallback:literal, [$($r:literal: $rt:ident),*], [$($o:literal: $ot:ident),*], $children:expr) => {
        Kind {
            name: $name,
            fallback: $fallback,
            required: &[$(($r, Type::$rt)),*],
            optional: &[$(($o, Type::$ot)),*],
            children: $children,
            shape: include_str!(concat!("../assets/kind/", $name, ".ttl")),
            example: include_str!(concat!("../assets/example/", $name, ".json")),
        }
    };
}

/// The closed node vocabulary. Sealed: nothing else is a kind.
pub const KINDS: [Kind; 16] = [
    kind!("heading", "content", ["level": Level], [], Children::Leaf),
    kind!("text", "content", [], [], Children::Leaf),
    kind!(
        "field",
        "label",
        ["name": Text],
        [
            "input": Text,
            "value": Text,
            "required": Flag,
            "description": Text,
            "error": Text,
            "resource": Iri
        ],
        Children::Leaf
    ),
    kind!(
        "choice",
        "label",
        ["name": Text],
        ["value": Text, "required": Flag, "description": Text, "error": Text],
        Children::Only("option")
    ),
    kind!("option", "label", ["value": Text], [], Children::Leaf),
    kind!("button", "label", [], ["name": Text, "value": Text], Children::Leaf),
    kind!("link", "label", ["href": Url], [], Children::Leaf),
    kind!("list", "label", [], [], Children::Any),
    kind!("table", "caption", [], [], Children::Only("group")),
    kind!(
        "form",
        "label",
        [],
        [
            "action": Url,
            "method": Method,
            "description": Text,
            "resource": Iri,
            "verb": Verb
        ],
        Children::Any
    ),
    kind!("nav", "label", [], [], Children::Only("link")),
    kind!("status", "content", [], [], Children::Leaf),
    kind!("image", "alt", ["src": Url], [], Children::Leaf),
    kind!("group", "label", [], ["header": Scope], Children::Any),
    kind!("visual", "label", [], ["description": Text], Children::Leaf),
    kind!("scene", "label", ["datum": Iri], [], Children::Leaf),
];

macro_rules! structure {
    ($name:literal, $fallback:literal, [$($r:literal: $rt:ident),*], [$($o:literal: $ot:ident),*], $children:expr) => {
        Kind {
            name: $name,
            fallback: $fallback,
            required: &[$(($r, Type::$rt)),*],
            optional: &[$(($o, Type::$ot)),*],
            children: $children,
            shape: include_str!(concat!("../assets/structure/", $name, ".ttl")),
            example: include_str!(concat!("../assets/example/structure/", $name, ".json")),
        }
    };
}

/// The structural nodes (ledger #3361): a `reference` names a resource ([`crate::reference`]),
/// a `region` narrows the space its children resolve in ([`crate::region`]). They are the
/// model's, with shapes at `urn:iki:ui:structure:name:{name}`, but never a target kind: the face
/// composes them away before a descriptor projects (a reference to its value or its withheld
/// rung, a region to a `group`), so no descriptor covers them and the kinds stay sealed.
/// Neither takes a replacement.
pub const STRUCTURES: [Kind; 2] = [
    structure!("reference", "fallback", ["resource": Iri], [], Children::Leaf),
    structure!("region", "label", [], ["allow": Text, "scopes": Text], Children::Any),
];

/// A structural node's IRI is this plus its name.
pub const STRUCTURE_PREFIX: &str = "urn:iki:ui:structure:name:";

/// The structural node named `name`, or `None`.
pub fn structure(name: &str) -> Option<&'static Kind> {
    STRUCTURES.iter().find(|k| k.name == name)
}

/// The list of kinds, Turtle: the node shape every node meets (`urn:iki:ui:kind`).
pub const KIND_LIST: &str = include_str!("../assets/kind.ttl");

/// The gallery's sections, data. `urn:iki:ui:screen:name:gallery` is compiled from its outline
/// (`assets/screen/gallery.outline`, ledger #3365), which `tests/screen.rs` holds equal to the
/// model these sections assemble.
pub const GALLERY: &str = include_str!("../assets/gallery.json");

/// The verbs a form's `verb` names.
pub const VERBS: [&str; 4] = ["source", "sink", "new", "delete"];

/// The scopes a table row's `header` names: `column` makes every cell of the row a column
/// header (`<th scope="col">`), `row` makes the row's first cell its row header
/// (`<th scope="row">`). A property on the row rather than a `cell` kind: a header is a role a
/// row plays in a table, the row is already a `group`, and the closed kind set (and every
/// target descriptor's coverage of it) stays as it is.
pub const SCOPES: [&str; 2] = ["column", "row"];

/// The kind named `name`, or `None`.
pub fn kind(name: &str) -> Option<&'static Kind> {
    KINDS.iter().find(|k| k.name == name)
}

/// A property value: text, a number or a boolean. Never markup.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    /// A number (a heading's level).
    Number(i64),
    /// A boolean (a field's `required`).
    Bool(bool),
    /// Text.
    Text(String),
}

/// A hint's value: text or a number, or the same stating the capability it `requires`
/// (ledger #3358). Never markup, never a boolean.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Hint {
    /// A bare value.
    Plain(Value),
    /// A value a face uses only when its descriptor claims `capability` at `version`.
    Requires {
        /// The value.
        is: Value,
        /// The capability's name.
        capability: String,
        /// The least version, compared by [`version_meets`].
        version: String,
    },
}

impl Hint {
    /// The hint's value.
    pub fn value(&self) -> &Value {
        match self {
            Hint::Plain(v) | Hint::Requires { is: v, .. } => v,
        }
    }
}

/// The marker every hint namespace carries: `urn:{ns}:ui:hint:{face}:` (for example
/// `urn:iki:ui:hint:gpui:`), and a hint's key is its namespace plus the hint's name.
pub const HINT_MARK: &str = ":ui:hint:";
/// The presentation hints a `choice` takes, read by every face that claims this namespace in
/// its descriptor (`hints`) and dropped by every other, which keeps its usual choice control.
pub const CHOICE_HINTS: &str = "urn:iki:ui:hint:choice:";
/// A `choice` shown as radio buttons (ledger #3537), its value `radio`: the html face draws a
/// `<fieldset>` with a `<legend>` and one radio input per option (native radios, so the arrow
/// keys move within the group), the GPUI face a radio group (one tab stop, the arrows move the
/// selection, the chosen option announced). A face that does not claim [`CHOICE_HINTS`]
/// keeps its select.
pub const RADIO: &str = "urn:iki:ui:hint:choice:radio";

/// Whether `node` is a choice that asks to be shown as radio buttons ([`RADIO`]).
pub fn is_radio(node: &Node) -> bool {
    node.kind == "choice" && node.hint.get(RADIO) == Some(&Hint::Plain(Value::Text("radio".into())))
}

/// The presentation hints a list item or a table row takes (ledger #3521), read by every face
/// that claims this namespace and dropped by every other: the row's TEXT says the same, so a face
/// without them loses a line or a color, never the meaning.
pub const ROLE_HINTS: &str = "urn:iki:ui:hint:role:";
/// The role a list item or a table row plays, one of [`ROLES`]: `danger` (a SHACL violation,
/// drawn in the palette's danger color with a solid danger rule), `asserted` (a solid rule),
/// `derived` (a dashed rule). A lens's query states it per row as `?role`.
pub const ROLE: &str = "urn:iki:ui:hint:role:mark";
/// The roles [`ROLE`] names.
pub const ROLES: [&str; 3] = ["danger", "asserted", "derived"];

/// The role `node` plays as a list item or a table row, when it carries [`ROLE`] with one of
/// [`ROLES`]; anything else is no role.
pub fn role(node: &Node) -> Option<&'static str> {
    match node.hint.get(ROLE) {
        Some(Hint::Plain(Value::Text(r))) => ROLES.iter().find(|x| **x == r).copied(),
        _ => None,
    }
}

/// The most hints one node carries.
pub const MAX_HINTS: usize = 16;
/// The longest hint key or text value, in bytes.
pub const MAX_HINT_BYTES: usize = 256;
/// The marker an element's IRI carries: `urn:{ns}:ui:element:name:{name}`.
pub const ELEMENT_MARK: &str = ":ui:element:name:";
/// The property every element node carries: its fallback text, the ladder's floor.
pub const ELEMENT_FALLBACK: &str = "fallback";

/// Whether `name` is a custom element's IRI, `urn:{ns}:ui:element:name:{name}`: `{ns}` one or
/// more `:`-separated segments and `{name}` one, each of `[a-z0-9-]`. Never under
/// `urn:iki:ui:kind:`, which is sealed to the core kinds.
pub fn is_element(name: &str) -> bool {
    let segment = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    };
    let Some(rest) = name.strip_prefix("urn:") else {
        return false;
    };
    let Some((ns, element)) = rest.split_once(ELEMENT_MARK) else {
        return false;
    };
    !name.starts_with("urn:iki:ui:kind:") && ns.split(':').all(segment) && segment(element)
}

/// Whether `key` is a hint key: an absolute `urn:` IRI under a hint namespace (it carries
/// [`HINT_MARK`]), with a nonempty name after the namespace, bounded by [`MAX_HINT_BYTES`].
pub fn is_hint_key(key: &str) -> bool {
    key.len() <= MAX_HINT_BYTES
        && key.starts_with("urn:")
        && key
            .split_once(HINT_MARK)
            .is_some_and(|(_, rest)| rest.contains(':') && !rest.ends_with(':'))
        && ikigai_core::Iri::parse(key).is_ok()
}

/// Whether an offered capability `version` meets a `required` one: both dot-separated
/// numbers, the same major and no less; otherwise only an identical identifier meets it.
pub fn version_meets(offered: &str, required: &str) -> bool {
    let numbers = |v: &str| {
        v.split('.')
            .map(|p| p.parse::<u64>())
            .collect::<Result<Vec<_>, _>>()
            .ok()
    };
    match (numbers(offered), numbers(required)) {
        (Some(o), Some(r)) => o[0] == r[0] && o >= r,
        _ => offered == required,
    }
}

/// One node: a kind (or a custom element's IRI), its properties, ordered children, optional
/// hints and an optional replacement subtree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    /// The kind's name, or a custom element's IRI ([`is_element`]).
    pub kind: String,
    /// Hints, keyed by IRI under a face's namespace ([`is_hint_key`]). A face uses the ones it
    /// knows and ignores the rest; nothing a screen means depends on one.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub hint: BTreeMap<String, Hint>,
    /// An explicit replacement, tried after native rendering and core expansion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replacement: Option<Box<Node>>,
    /// The properties, by name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<String, Value>,
    /// The children, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Node>,
}

fn refuse(path: &str, detail: String) -> Error {
    Error::InvalidArgument {
        name: "model".to_string(),
        detail: format!("{path}: {detail}"),
    }
}

/// Validate a tree over the closed vocabulary, iteratively. The first fault found is refused as
/// `InvalidArgument`, its detail starting with the node's path.
pub fn validate(root: &Node) -> Result<(), Error> {
    let mut stack: Vec<(String, &Node)> = vec![("/".to_string(), root)];
    while let Some((path, node)) = stack.pop() {
        validate_hints(&path, node)?;
        if is_element(&node.kind) {
            validate_element(&path, node)?;
            push_children(&path, node, &mut stack);
            continue;
        }
        let structural = structure(&node.kind);
        let Some(k) = kind(&node.kind).or(structural) else {
            return Err(refuse(&path, format!("`{}` is not a kind", node.kind)));
        };
        if structural.is_some() && node.replacement.is_some() {
            return Err(refuse(&path, format!("a {} takes no replacement", k.name)));
        }
        // A scene is a picture a face may not draw (no GPU, no script, no 3D): its
        // `replacement` is its accessible form, so it is required, not optional.
        if k.name == "scene" && node.replacement.is_none() {
            return Err(refuse(
                &path,
                "a scene needs a `replacement`, its accessible form".into(),
            ));
        }
        if k.name == crate::region::KIND {
            crate::region::validate(node).map_err(|detail| refuse(&path, detail))?;
        }
        // A write-only field (one naming its own `resource`) is written, never read back.
        if k.name == "field"
            && node.properties.contains_key("resource")
            && node.properties.contains_key("value")
        {
            return Err(refuse(&path, "a write-only field holds no `value`".into()));
        }
        for (p, v) in &node.properties {
            let Some(t) = k.property(p) else {
                return Err(refuse(&path, format!("a {} has no property `{p}`", k.name)));
            };
            let ok = match (t, v) {
                (Type::Level, Value::Number(n)) => (1..=6).contains(n),
                (Type::Text, Value::Text(s)) => !s.is_empty(),
                (Type::Url, Value::Text(s)) => {
                    ikigai_fn::html::safe_href(s).is_some_and(|h| h == s.as_str())
                }
                (Type::Method, Value::Text(s)) => s == "get" || s == "post",
                (Type::Flag, Value::Bool(_)) => true,
                (Type::Flag, Value::Text(s)) => s == "true" || s == "false",
                (Type::Iri, Value::Text(s)) => ikigai_core::Iri::parse(s.as_str()).is_ok(),
                (Type::Verb, Value::Text(s)) => VERBS.contains(&s.as_str()),
                (Type::Scope, Value::Text(s)) => SCOPES.contains(&s.as_str()),
                _ => false,
            };
            if !ok {
                return Err(refuse(&path, format!("`{p}` is not a valid {t:?}")));
            }
        }
        for p in std::iter::once(&k.fallback).chain(k.required.iter().map(|(n, _)| n)) {
            if !node.properties.contains_key(*p) {
                return Err(refuse(&path, format!("a {} needs `{p}`", k.name)));
            }
        }
        if let Some(replacement) = &node.replacement {
            let at = if path == "/" {
                "/replacement".into()
            } else {
                format!("{path}/replacement")
            };
            stack.push((at, replacement));
        }
        for (i, child) in node.children.iter().enumerate() {
            let at = if path == "/" {
                format!("/children/{i}")
            } else {
                format!("{path}/children/{i}")
            };
            match k.children {
                Children::Leaf => {
                    return Err(refuse(&path, format!("a {} takes no children", k.name)))
                }
                Children::Only(only) if child.kind != only => {
                    return Err(refuse(
                        &at,
                        format!("a {} takes only {only} children", k.name),
                    ))
                }
                _ => {}
            }
            stack.push((at, child));
        }
    }
    Ok(())
}

fn push_children<'a>(path: &str, node: &'a Node, stack: &mut Vec<(String, &'a Node)>) {
    let at = |tail: String| {
        if path == "/" {
            format!("/{tail}")
        } else {
            format!("{path}/{tail}")
        }
    };
    if let Some(replacement) = &node.replacement {
        stack.push((at("replacement".into()), replacement));
    }
    for (i, child) in node.children.iter().enumerate() {
        stack.push((at(format!("children/{i}")), child));
    }
}

fn validate_hints(path: &str, node: &Node) -> Result<(), Error> {
    if node.hint.len() > MAX_HINTS {
        return Err(refuse(path, format!("more than {MAX_HINTS} hints")));
    }
    for (key, hint) in &node.hint {
        if !is_hint_key(key) {
            return Err(refuse(
                path,
                format!("`{key}` is not a hint IRI (urn:{{ns}}{HINT_MARK}{{face}}:{{name}})"),
            ));
        }
        let ok = match hint.value() {
            Value::Number(_) => true,
            Value::Text(s) => {
                !s.is_empty()
                    && s.len() <= MAX_HINT_BYTES
                    && !s.contains(['<', '>'])
                    && !s.chars().any(char::is_control)
            }
            Value::Bool(_) => false,
        };
        if !ok {
            return Err(refuse(
                path,
                format!("hint `{key}` is not text of at most {MAX_HINT_BYTES} bytes or a number"),
            ));
        }
        if let Hint::Requires {
            capability,
            version,
            ..
        } = hint
        {
            if capability.trim().is_empty() || version.trim().is_empty() {
                return Err(refuse(path, format!("hint `{key}` requires nothing")));
            }
        }
    }
    Ok(())
}

/// An element node, before its definition is read: its fallback text is present, and every
/// property is text, a number or a boolean (its definition's contract checks the rest).
fn validate_element(path: &str, node: &Node) -> Result<(), Error> {
    match node.properties.get(ELEMENT_FALLBACK) {
        Some(Value::Text(s)) if !s.is_empty() => {}
        _ => {
            return Err(refuse(
                path,
                format!("an element needs `{ELEMENT_FALLBACK}`, its fallback text"),
            ))
        }
    }
    for (p, v) in &node.properties {
        if matches!(v, Value::Text(s) if s.is_empty()) {
            return Err(refuse(path, format!("`{p}` is empty")));
        }
    }
    Ok(())
}

/// Refuse `bytes` past [`MAX_BYTES`] or nested past [`MAX_DEPTH`], scanning iteratively (strings
/// and their escapes skipped), so nothing recursive sees an unbounded input.
pub fn bound(bytes: &[u8]) -> Result<(), Error> {
    if bytes.len() > MAX_BYTES {
        return Err(refuse(
            "/",
            format!("{} bytes is past the bound of {MAX_BYTES}", bytes.len()),
        ));
    }
    let (mut depth, mut in_string, mut escaped) = (0usize, false, false);
    for &b in bytes {
        if in_string {
            match (escaped, b) {
                (true, _) => escaped = false,
                (false, b'\\') => escaped = true,
                (false, b'"') => in_string = false,
                _ => {}
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' | b'[' => {
                depth += 1;
                if depth > MAX_DEPTH {
                    return Err(refuse("/", format!("nested past the bound of {MAX_DEPTH}")));
                }
            }
            b'}' | b']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    Ok(())
}

/// A model from request input: bounded ([`bound`]), parsed, then [`validate`]d.
pub fn parse(bytes: &[u8]) -> Result<Node, Error> {
    bound(bytes)?;
    let node: Node =
        serde_json::from_slice(bytes).map_err(|e| refuse("/", format!("not a model: {e}")))?;
    validate(&node)?;
    Ok(node)
}

/// The JSON-LD face of a model's JSON: the same document with [`CONTEXT`] as its `@context` and
/// each node named `{iri}:node:path:{path}` (its JSON pointer), so no node is blank and the
/// path carries the children's order.
pub fn json_ld(node: &Node, iri: &str) -> serde_json::Value {
    let context: serde_json::Value =
        serde_json::from_str(CONTEXT).expect("the compiled-in context is JSON");
    let mut doc = serde_json::to_value(node).expect("a node serializes");
    let mut stack: Vec<(String, &mut serde_json::Value)> = vec![(String::new(), &mut doc)];
    while let Some((path, value)) = stack.pop() {
        let serde_json::Value::Object(map) = value else {
            continue;
        };
        let at = if path.is_empty() { "/" } else { path.as_str() };
        map.insert(
            "@id".to_string(),
            serde_json::Value::String(format!("{iri}:node:path:{at}")),
        );
        // Borrow the two branches together so both enter the iterative walk.
        for (key, value) in map.iter_mut() {
            if key == "hint" {
                let serde_json::Value::Object(hints) = value else {
                    continue;
                };
                for (name, hint) in hints.iter_mut() {
                    if let serde_json::Value::Object(h) = hint {
                        h.insert(
                            "@id".to_string(),
                            serde_json::Value::String(format!("{iri}:node:path:{at}:hint:{name}")),
                        );
                    }
                }
            } else if key == "replacement" {
                stack.push((format!("{path}/replacement"), value));
            } else if key == "children" {
                let serde_json::Value::Array(children) = value else {
                    continue;
                };
                for (i, child) in children.iter_mut().enumerate() {
                    stack.push((format!("{path}/children/{i}"), child));
                }
            }
        }
    }
    if let serde_json::Value::Object(map) = &mut doc {
        map.insert("@context".to_string(), context["@context"].clone());
    }
    doc
}
