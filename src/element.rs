//! Custom elements as compositions of kinds (ledger #3358).
//!
//! A node whose `kind` is an element's IRI, `urn:{ns}:ui:element:name:{name}` in its author's
//! namespace ([`model::is_element`]), names an element. Its DEFINITION is the element IRI's
//! Source, [`Definition`] as JSON: an argument contract (`arguments`, which the definition
//! endpoint also states as ArgSpecs in its Meta), an optional `requires` (a capability and a
//! version against the descriptor's `capabilities`) and an `expansion`, a model template over
//! core kinds and other elements. In the template a string that is exactly `{arg}` is the
//! argument's value (its type kept), `{arg}` inside a longer string is the argument's text,
//! and the string `{children}` in a `children` list is the element's own children. Every
//! value stays text: a renderer escapes it, so splicing cannot introduce markup.
//!
//! **The ladder.** [`expand`] walks a screen and, per element node: keeps it when the target
//! descriptor claims it natively (`elements`, with its `requires` met); else expands its
//! definition (recursively); else uses its declared fallback (`replacement`); else shows its
//! fallback text (its required `fallback` property).
//!
//! **Bounds.** Expansion is recursive and definitions may share structure, so a small input can
//! produce a large tree. [`expand`] refuses, as `InvalidArgument`, past [`MAX_DEPTH`] nested
//! expansions, past [`MAX_NODES`] produced nodes, and on a cycle (an element whose expansion
//! reaches itself).
//!
//! **Cacheable.** Each definition is read with `inv.issue`, so the screen expanded from it hangs
//! from every definition it read: editing one recomputes exactly the screens that use it.
//!
//! **Why Rust, not a compose template.** ikigai-fn compose splices text into a document with
//! HTML escaping; the expansion is a TREE: it keeps a number a number, splices a list of
//! children, recurses into other elements' definitions and counts what it produces against the
//! bounds. None of those is a text splice, so the splice is this small structural walk over
//! the model's JSON, and the definition itself stays data.

use std::collections::{BTreeMap, BTreeSet};

use ikigai_core::{Error, Invocation, Iri, Request, Verb};
use serde::{Deserialize, Serialize};

use crate::model::{self, Node, Value};
use crate::target::Descriptor;

/// Where an element's children go in its spliced expansion; never a kind or an element.
const SLOT: &str = "urn:iki:ui:element-slot";

/// The deepest chain of nested expansions [`expand`] follows.
pub const MAX_DEPTH: usize = 32;
/// The most nodes one expanded screen holds.
pub const MAX_NODES: usize = 10_000;
/// The most distinct definitions one screen reads.
pub const MAX_DEFINITIONS: usize = 256;
/// The largest definition read, in bytes.
pub const MAX_DEFINITION_BYTES: usize = 64 * 1024;

/// A capability and its least version, met by [`model::version_meets`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requires {
    /// The capability's name.
    pub capability: String,
    /// The least version.
    pub version: String,
}

/// One argument of an element's contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Argument {
    /// The argument's name, a property of the element node.
    pub name: String,
    /// What it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Whether the node must give it.
    #[serde(default)]
    pub required: bool,
    /// Its value when the node does not give it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
}

/// An element's definition: its contract and its expansion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    /// The argument contract. `fallback` is implicit: every element node carries it.
    pub arguments: Vec<Argument>,
    /// A capability the expansion needs; unmet, the ladder skips the expansion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires: Option<Requires>,
    /// The model template.
    pub expansion: serde_json::Value,
}

fn refuse(element: &str, detail: impl std::fmt::Display) -> Error {
    Error::InvalidArgument {
        name: "element".into(),
        detail: format!("{element}: {detail}"),
    }
}

impl Definition {
    /// A bounded definition of `element`, its contract checked.
    pub fn parse(bytes: &[u8], element: &str) -> Result<Self, Error> {
        if bytes.len() > MAX_DEFINITION_BYTES {
            return Err(refuse(element, "the definition is too large"));
        }
        model::bound(bytes)?;
        let definition: Self = serde_json::from_slice(bytes)
            .map_err(|e| refuse(element, format!("not an element definition: {e}")))?;
        let mut names = BTreeSet::new();
        for argument in &definition.arguments {
            let ok = !argument.name.is_empty()
                && argument
                    .name
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
            if !ok || argument.name == model::ELEMENT_FALLBACK || argument.name == "children" {
                return Err(refuse(
                    element,
                    format!("`{}` is not an argument name", argument.name),
                ));
            }
            if !names.insert(argument.name.as_str()) {
                return Err(refuse(
                    element,
                    format!("`{}` is declared twice", argument.name),
                ));
            }
        }
        if !definition.expansion.is_object() {
            return Err(refuse(element, "the expansion is not a node"));
        }
        Ok(definition)
    }

    /// The expansion of `node` (an element of this definition): its arguments spliced in.
    fn splice(&self, element: &str, node: &Node) -> Result<Node, Error> {
        let mut args: BTreeMap<&str, Value> = BTreeMap::new();
        args.insert(
            model::ELEMENT_FALLBACK,
            node.properties[model::ELEMENT_FALLBACK].clone(),
        );
        for (p, v) in &node.properties {
            if p != model::ELEMENT_FALLBACK && !self.arguments.iter().any(|a| &a.name == p) {
                return Err(refuse(
                    element,
                    format!("`{p}` is not one of its arguments"),
                ));
            }
            let _ = v;
        }
        for argument in &self.arguments {
            match (node.properties.get(&argument.name), &argument.default) {
                (Some(v), _) => {
                    args.insert(&argument.name, v.clone());
                }
                (None, Some(d)) => {
                    args.insert(&argument.name, Value::Text(d.clone()));
                }
                (None, None) if argument.required => {
                    return Err(refuse(element, format!("needs `{}`", argument.name)))
                }
                (None, None) => {}
            }
        }
        // The children splice as one slot node, which the walk replaces with the element's
        // children already walked in the caller's context.
        let slot = serde_json::json!({ "kind": SLOT });
        let mut doc = self.expansion.clone();
        // Iterative, so a deep template cannot overflow (it is already bounded by `bound`).
        let mut stack = vec![&mut doc];
        while let Some(value) = stack.pop() {
            match value {
                serde_json::Value::String(s) => {
                    if let Some(v) = s
                        .strip_prefix('{')
                        .and_then(|t| t.strip_suffix('}'))
                        .and_then(|name| args.get(name))
                    {
                        *value = serde_json::to_value(v).expect("a value serializes");
                    } else if s.contains('{') {
                        let mut out = s.clone();
                        for (name, v) in &args {
                            let text = match v {
                                Value::Text(t) => t.clone(),
                                Value::Number(n) => n.to_string(),
                                Value::Bool(b) => b.to_string(),
                            };
                            out = out.replace(&format!("{{{name}}}"), &text);
                        }
                        *s = out;
                    }
                }
                serde_json::Value::Array(items) => {
                    if items.iter().any(|i| i == "{children}") {
                        let mut spliced = Vec::with_capacity(items.len());
                        for item in items.drain(..) {
                            if item == "{children}" {
                                spliced.push(slot.clone());
                            } else {
                                spliced.push(item);
                            }
                        }
                        *items = spliced;
                    }
                    stack.extend(items.iter_mut());
                }
                serde_json::Value::Object(map) => {
                    for (key, v) in map.iter_mut() {
                        // Spliced children are already nodes; only the template's own strings
                        // are template text, and a hint's key is never spliced.
                        if key != "kind" {
                            stack.push(v);
                        }
                    }
                }
                _ => {}
            }
        }
        // Validated whole after the walk (the slot is not a kind until it is filled).
        serde_json::from_value(doc)
            .map_err(|e| refuse(element, format!("the expansion is not a model: {e}")))
    }
}

/// Whether `descriptor` meets `requires` (absent: always).
pub fn meets(descriptor: &Descriptor, capability: &str, version: &str) -> bool {
    descriptor
        .capabilities
        .iter()
        .any(|c| c.name == capability && model::version_meets(&c.version, version))
}

fn met(descriptor: &Descriptor, requires: Option<&Requires>) -> bool {
    requires.is_none_or(|r| meets(descriptor, &r.capability, &r.version))
}

/// Read every definition reachable from `root` (through other definitions too), once each,
/// through the kernel. An element with no definition (NotFound) is absent from the map: the
/// ladder goes on to its fallback.
pub async fn definitions(
    inv: &Invocation<'_>,
    root: &Node,
    descriptor: &Descriptor,
) -> Result<BTreeMap<String, Definition>, Error> {
    let mut read: BTreeMap<String, Option<Definition>> = BTreeMap::new();
    let mut pending: Vec<Node> = vec![root.clone()];
    while let Some(node) = pending.pop() {
        let mut stack = vec![&node];
        let mut found = Vec::new();
        while let Some(n) = stack.pop() {
            if model::is_element(&n.kind) && !read.contains_key(&n.kind) {
                found.push(n.kind.clone());
            }
            // A region's elements are read inside the region, in its narrowed space.
            if n.kind != crate::region::KIND {
                stack.extend(n.children.iter());
                stack.extend(n.replacement.as_deref());
            }
        }
        for element in found {
            if read.contains_key(&element) {
                continue;
            }
            if read.len() >= MAX_DEFINITIONS {
                return Err(refuse(
                    &element,
                    format!("past {MAX_DEFINITIONS} definitions"),
                ));
            }
            let iri = Iri::parse(element.as_str()).map_err(|e| refuse(&element, e))?;
            let definition = match inv.issue(Request::new(Verb::Source, iri)).await {
                Ok(rep) => Some(Definition::parse(&rep.bytes, &element)?),
                Err(Error::NotFound(_)) => None,
                Err(e) => return Err(e),
            };
            if let Some(d) = &definition {
                if met(descriptor, d.requires.as_ref()) {
                    // A template is a model whose strings may still be placeholders; parse it
                    // loosely just to find the elements it names.
                    if let Ok(template) = serde_json::from_value::<Node>(strip(&d.expansion)) {
                        pending.push(template);
                    }
                }
            }
            read.insert(element, definition);
        }
    }
    Ok(read
        .into_iter()
        .filter_map(|(k, v)| Some((k, v?)))
        .collect())
}

/// A template with its `{children}` placeholders removed, so it parses as a node.
fn strip(template: &serde_json::Value) -> serde_json::Value {
    let mut doc = template.clone();
    let mut stack = vec![&mut doc];
    while let Some(value) = stack.pop() {
        match value {
            serde_json::Value::Array(items) => {
                items.retain(|i| i != "{children}");
                stack.extend(items.iter_mut());
            }
            serde_json::Value::Object(map) => stack.extend(map.values_mut()),
            _ => {}
        }
    }
    doc
}

struct Expander<'a> {
    descriptor: &'a Descriptor,
    definitions: &'a BTreeMap<String, Definition>,
    produced: usize,
    open: Vec<String>,
    slots: Vec<Vec<Node>>,
}

impl Expander<'_> {
    fn count(&mut self) -> Result<(), Error> {
        self.produced += 1;
        if self.produced > MAX_NODES {
            return Err(Error::InvalidArgument {
                name: "element".into(),
                detail: format!("the expansion produces more than {MAX_NODES} nodes"),
            });
        }
        Ok(())
    }

    fn children(&mut self, children: &[Node], depth: usize) -> Result<Vec<Node>, Error> {
        let mut out = Vec::with_capacity(children.len());
        for child in children {
            if child.kind == SLOT {
                let filled = self.slots.last().cloned().unwrap_or_default();
                out.extend(filled);
            } else {
                out.push(self.walk(child, depth)?);
            }
        }
        Ok(out)
    }

    /// `node` with every element down the ladder. `depth` is the number of open expansions;
    /// recursion is bounded by [`MAX_DEPTH`] expansions times the model's own depth bound.
    /// `nodes` with the open element's slot filled and nothing expanded: a region's interior,
    /// which expands inside the region ([`crate::region`]).
    fn fill(&mut self, nodes: &[Node]) -> Result<Vec<Node>, Error> {
        let mut out = Vec::with_capacity(nodes.len());
        for n in nodes {
            if n.kind == SLOT {
                out.extend(self.slots.last().cloned().unwrap_or_default());
            } else {
                self.count()?;
                let mut c = n.clone();
                c.children = self.fill(&n.children)?;
                out.push(c);
            }
        }
        Ok(out)
    }

    fn walk(&mut self, node: &Node, depth: usize) -> Result<Node, Error> {
        if node.kind == crate::region::KIND {
            self.count()?;
            let mut out = node.clone();
            out.children = self.fill(&node.children)?;
            return Ok(out);
        }
        if !model::is_element(&node.kind) {
            self.count()?;
            let mut out = node.clone();
            out.children = self.children(&node.children, depth)?;
            out.replacement = match &node.replacement {
                Some(r) => Some(Box::new(self.walk(r, depth)?)),
                None => None,
            };
            return Ok(out);
        }
        let element = node.kind.as_str();
        let definition = self.definitions.get(element);
        // 1. Native, when the descriptor claims it.
        if self.descriptor.elements.iter().any(|e| e == element)
            && met(
                self.descriptor,
                definition.and_then(|d| d.requires.as_ref()),
            )
        {
            self.count()?;
            let mut out = node.clone();
            out.children = self.children(&node.children, depth)?;
            out.replacement = match &node.replacement {
                Some(r) => Some(Box::new(self.walk(r, depth)?)),
                None => None,
            };
            return Ok(out);
        }
        // 2. Its expansion, recursively.
        if let Some(d) = definition.filter(|d| met(self.descriptor, d.requires.as_ref())) {
            if self.open.iter().any(|e| e == element) {
                return Err(refuse(
                    element,
                    format!(
                        "its expansion is a cycle: {} → {element}",
                        self.open.join(" → ")
                    ),
                ));
            }
            if depth >= MAX_DEPTH {
                return Err(refuse(
                    element,
                    format!("expansion nested past the bound of {MAX_DEPTH}"),
                ));
            }
            let walked = self.children(&node.children, depth)?;
            let expanded = d.splice(element, node)?;
            self.open.push(element.to_string());
            self.slots.push(walked);
            let out = self.walk(&expanded, depth + 1);
            self.slots.pop();
            self.open.pop();
            return out;
        }
        // 3. Its declared fallback.
        if let Some(r) = &node.replacement {
            return self.walk(r, depth);
        }
        // 4. Its fallback text.
        self.count()?;
        Ok(Node {
            kind: "text".into(),
            replacement: None,
            hint: Default::default(),
            properties: [(
                "content".into(),
                node.properties[model::ELEMENT_FALLBACK].clone(),
            )]
            .into(),
            children: Vec::new(),
        })
    }
}

/// `root` with every element taken down the ladder for `descriptor`, synchronously, from
/// definitions already read ([`definitions`]). Bounded by [`MAX_DEPTH`] and [`MAX_NODES`]; a
/// cycle refuses.
pub fn expand_with(
    root: &Node,
    descriptor: &Descriptor,
    definitions: &BTreeMap<String, Definition>,
) -> Result<Node, Error> {
    model::validate(root)?;
    let mut expander = Expander {
        descriptor,
        definitions,
        produced: 0,
        open: Vec::new(),
        slots: Vec::new(),
    };
    let out = expander.walk(root, 0)?;
    model::validate(&out)?;
    Ok(out)
}

/// `root` with every element down the ladder for `descriptor`, its definitions read through
/// the kernel so the answer hangs from each one.
pub async fn expand(
    inv: &Invocation<'_>,
    root: &Node,
    descriptor: &Descriptor,
) -> Result<Node, Error> {
    let definitions = definitions(inv, root, descriptor).await?;
    expand_with(root, descriptor, &definitions)
}

// --- this crate's own elements -----------------------------------------------------------------

use ikigai_core::{ArgSpec, AsyncFnEndpoint, Description, Endpoint, InvokeFuture};

/// This crate's elements' prefix: `urn:iki:ui:element:name:{name}`.
pub const PREFIX: &str = "urn:iki:ui:element:name:";
/// The screen that shows every element of [`ELEMENTS`]: `urn:iki:ui:screen:name:elements`.
pub const SCREEN: &str = "urn:iki:ui:screen:name:elements";

/// This crate's example elements, compiled in, each overridable from `element/{name}.json` in
/// the UI directory (so an edit there recomputes exactly the screens that use it).
pub const ELEMENTS: [crate::Artifact; 1] = [crate::Artifact {
    iri: "urn:iki:ui:element:name:rating",
    id: "ui-element-name",
    file: "element/rating.json",
    media_type: "application/json",
    bytes: include_bytes!("../assets/element/rating.json"),
}];

/// Each element's example node, in [`ELEMENTS`]' order.
pub const EXAMPLES: [&str; 1] = [include_str!("../assets/example/element/rating.json")];

fn known(name: &str) -> Result<&'static crate::Artifact, Error> {
    ELEMENTS
        .iter()
        .find(|a| a.iri.strip_prefix(PREFIX) == Some(name))
        .ok_or_else(|| Error::NotFound(format!("{PREFIX}{name}: no such element")))
}

pub(crate) fn definition_endpoint() -> AsyncFnEndpoint {
    let description = Description::new("ui-element-name")
        .title("A UI custom element's definition")
        .summary(
            "A custom element as a composition: its argument contract (arguments: each a name, \
             summary, required and default, the ArgSpecs of the element node), an optional requires and its expansion, a model template over core kinds \
             and other elements. From element/{name}.json in the UI directory, else compiled in.",
        )
        .verb(Verb::Source)
        .verb(Verb::Meta)
        .input(
            ArgSpec::new("name")
                .summary("the element's name")
                .class(crate::XSD_STRING)
                .binding()
                .one_of(ELEMENTS.iter().map(|a| &a.iri[PREFIX.len()..])),
        )
        .output("application/json");
    AsyncFnEndpoint::new(
        "ui-element-name",
        |inv: &Invocation<'_>| -> InvokeFuture<'_> {
            Box::pin(async move {
                let name = inv.bindings.get("name").unwrap_or_default();
                let artifact = known(name)?;
                let file = Iri::parse(format!("{}{}", crate::DIR, artifact.file))
                    .map_err(|e| Error::Endpoint(e.to_string()))?;
                let rep = crate::artifact_endpoint(*artifact, file, crate::DIR)
                    .invoke(inv)
                    .await?;
                Definition::parse(&rep.bytes, artifact.iri)?;
                Ok(rep)
            })
        },
    )
    .with_description(description)
}

/// `urn:iki:ui:screen:name:elements`: a heading and each example element, unexpanded; a face
/// takes each down its ladder.
pub(crate) fn screen_endpoint() -> ikigai_core::FnEndpoint {
    ikigai_core::FnEndpoint::new("ui-screen-elements", |inv: &Invocation<'_>| {
        let text = |v: &str| Value::Text(v.to_string());
        let mut root = Node {
            kind: "group".into(),
            replacement: None,
            hint: Default::default(),
            properties: [("label".to_string(), text("Custom elements"))].into(),
            children: vec![Node {
                kind: "heading".into(),
                replacement: None,
                hint: Default::default(),
                properties: [
                    ("content".to_string(), text("Custom elements")),
                    ("level".to_string(), Value::Number(2)),
                ]
                .into(),
                children: Vec::new(),
            }],
        };
        for example in EXAMPLES {
            root.children.push(
                serde_json::from_str(example)
                    .map_err(|e| Error::Endpoint(format!("an element example: {e}")))?,
            );
        }
        model::validate(&root)?;
        crate::model_face(inv, &root)
    })
    .with_description(
        Description::new("ui-screen-elements")
            .title("The UI custom elements")
            .summary(
                "Every example custom element as one screen, unexpanded: \
                 application/vnd.ikigai.ui+json, or JSON-LD with as=application/ld+json.",
            )
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .output(model::MEDIA_TYPE)
            .output(model::JSON_LD),
    )
}
