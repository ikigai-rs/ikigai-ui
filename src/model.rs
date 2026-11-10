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
pub const KINDS: [Kind; 15] = [
    kind!("heading", "content", ["level": Level], [], Children::Leaf),
    kind!("text", "content", [], [], Children::Leaf),
    kind!(
        "field",
        "label",
        ["name": Text],
        ["input": Text, "value": Text, "required": Flag, "description": Text, "error": Text],
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
    kind!("group", "label", [], [], Children::Any),
    kind!("visual", "label", [], ["description": Text], Children::Leaf),
];

/// The list of kinds, Turtle: the node shape every node meets (`urn:iki:ui:kind`).
pub const KIND_LIST: &str = include_str!("../assets/kind.ttl");

/// The gallery's sections, data: `urn:iki:ui:screen:name:gallery` assembles it.
pub const GALLERY: &str = include_str!("../assets/gallery.json");

/// The verbs a form's `verb` names.
pub const VERBS: [&str; 4] = ["source", "sink", "new", "delete"];

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

/// One node: a kind, its properties, ordered children and an optional replacement subtree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    /// The kind's name.
    pub kind: String,
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
        let Some(k) = kind(&node.kind) else {
            return Err(refuse(&path, format!("`{}` is not a kind", node.kind)));
        };
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
            if key == "replacement" {
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
