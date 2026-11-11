//! Views generated from a resource's own model, recursively, with curated views as overrides
//! (ledger #3366, under #3351; after the form, #3354, and the outline, #3365).
//!
//! | name | what it is |
//! |---|---|
//! | `urn:iki:ui:view:resource:{iri}` | the view of `{iri}`: the curated view for its type when one is bound, else the generated one |
//! | `urn:iki:ui:view:generated:{iri}` | always the generated view, so a curated view can start from it and override only what it needs (a `reference` to it) without naming itself |
//! | `urn:iki:ui:view:type:{class}` | an author's view for a type (a model); `{iri}` in any text property is the shown resource |
//! | `urn:iki:ui:shape:type:{class}` | the shape a generated view reads for a type (SHACL in any RDF syntax the host reads, below) |
//!
//! ## What a generated view reads, all through the kernel under the CALLER's capability
//!
//! - **Its contract**, `{iri}`'s Meta `as=application/json`: its title is the heading, its
//!   summary the first text, its verbs and outputs a `list` labeled "Contract".
//! - **Its data**, `{iri}`'s Source as a JSON object: `@type` names its classes; every other key
//!   is a property. A value `{"@id": …}` is another resource; an array is a list of values.
//! - **Its shape**, `urn:iki:ui:shape:type:{class}` for each class: SHACL in any RDF syntax.
//!   It is Sourced `as=application/ld+json` (through the kernel's transreptors when it answers
//!   another syntax, Turtle say), flattened and compacted by the host's `urn:jsonld:flatten`
//!   and `urn:jsonld:compact` against
//!   [`SHAPE_CONTEXT`], so whatever prefixes, `@graph` or expanded IRIs it was written with,
//!   it reads in one form: the node shape (its `sh:targetClass` the class, else the one with
//!   properties), its `rdfs:label`, `urn:iki:ui:shape:layout` (`"detail"` | `"table"`) and each
//!   `sh:property`'s `sh:path`, `sh:name` and `sh:order`. A path names the data's key that is
//!   the path IRI itself, else its local name (after the last `#`, `/` or `:`). A shape says
//!   which properties show, their labels and order; with none, every property shows under its
//!   key. A shape that is bound and cannot be read in that form is a note in the view, and the
//!   view shows as if there were none. The library parses no RDF: the host's modules do.
//!   A list of resources whose type's layout is `table` is a `table`, a column per property.
//! - **The caller's actions**, `urn:kernel:actions verb=sink`: when `{iri}` is one, its form
//!   (`urn:iki:ui:form:resource:{iri}`) follows; when it is not, there is no form. The
//!   manifold is capability-scoped, so this is the only permission code there is.
//! - **Its links**: a nested resource past a bound is a `link` to its own view.
//!
//! ## Recursion
//!
//! A property whose value is another resource renders through THAT resource's view (curated
//! for its type, else generated), [`MAX_DEPTH`] deep and up to [`MAX_NODES`] nodes; past
//! either, or at a repeat on the path (A shows B shows A), it is a link. Never an error.
//!
//! **Nothing the caller may not read appears.** A resource whose read is `Denied` is withheld
//! (the ladder's withheld rung, #3361): a `text` saying so under the property's label, never the
//! value. The kernel makes an answer that swallowed a denial uncacheable, and every read here
//! hangs the answer from what it read, so a changed contract, shape or datum recomputes it.

use ikigai_core::{ArgRef, Description, Error, Invocation, InvokeFuture, Iri, Request, Verb};
use serde_json::{Map, Value as Json};
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;

use crate::model::{self, Node, Value, JSON_LD};

/// The view of a resource: curated for its type, else generated.
pub const RESOURCE: &str = "urn:iki:ui:view:resource:";
/// Always the generated view.
pub const GENERATED: &str = "urn:iki:ui:view:generated:";
/// A curated view for a type.
pub const TYPE: &str = "urn:iki:ui:view:type:";
/// The shape a generated view reads for a type.
pub const SHAPE: &str = "urn:iki:ui:shape:type:";
/// The host's JSON-LD flattening, which makes a shape one node per subject.
pub const FLATTEN: &str = "urn:jsonld:flatten";
/// The host's JSON-LD compaction, which reads a shape in [`SHAPE_CONTEXT`]'s terms.
pub const COMPACT: &str = "urn:jsonld:compact";
/// The context a shape is compacted against, whatever prefixes or form it was written in.
pub const SHAPE_CONTEXT: &str = r#"{"@context": {
  "sh": "http://www.w3.org/ns/shacl#",
  "rdfs": "http://www.w3.org/2000/01/rdf-schema#",
  "targetClass": {"@id": "sh:targetClass", "@type": "@id"},
  "label": "rdfs:label",
  "layout": "urn:iki:ui:shape:layout",
  "property": {"@id": "sh:property", "@container": "@set"},
  "path": {"@id": "sh:path", "@type": "@id"},
  "name": "sh:name",
  "order": "sh:order"
}}"#;
/// How deep nested resources render before they become links.
pub const MAX_DEPTH: usize = 3;
/// How many nodes one view produces before nested resources become links.
pub const MAX_NODES: usize = 256;
/// What a curated view writes for the resource it shows.
pub const PLACEHOLDER: &str = "{iri}";

fn node(kind: &str, properties: Vec<(&str, Value)>, children: Vec<Node>) -> Node {
    Node {
        replacement: None,
        hint: Default::default(),
        kind: kind.to_string(),
        properties: properties
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect::<BTreeMap<_, _>>(),
        children,
    }
}

fn t(s: impl Into<String>) -> Value {
    Value::Text(s.into())
}

fn text(content: impl Into<String>) -> Node {
    node("text", vec![("content", t(content))], Vec::new())
}

fn nonempty(s: &str, or: &str) -> String {
    if s.trim().is_empty() {
        or.to_string()
    } else {
        s.trim().to_string()
    }
}

/// The door path of `iri`'s view (`/ui/{…}` is `urn:iki:ui:{…}`).
pub fn href(iri: &str) -> String {
    let mut out = String::from("/ui/view/resource/");
    for b in iri.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~:@!$&'()*+,;=".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn link(label: &str, iri: &str) -> Node {
    node(
        "link",
        vec![("label", t(nonempty(label, iri))), ("href", t(href(iri)))],
        Vec::new(),
    )
}

fn count(n: &Node) -> usize {
    1 + n.children.iter().map(count).sum::<usize>()
}

fn iri(s: &str) -> Result<Iri, Error> {
    Iri::parse(s).map_err(|e| Error::InvalidArgument {
        name: "iri".into(),
        detail: format!("`{s}` is not an IRI: {e}"),
    })
}

/// One property a shape shows.
#[derive(Debug, Clone, PartialEq)]
struct Property {
    path: String,
    name: String,
    order: f64,
}

/// A type's shape, as a generated view reads it.
#[derive(Debug, Clone, Default)]
struct Shape {
    label: Option<String>,
    table: bool,
    properties: Option<Vec<Property>>,
}

/// One value of a compacted term: the first of a set, a value object's `@value`.
fn first(v: &Json) -> Option<&Json> {
    match v {
        Json::Array(a) => a.first().and_then(first),
        Json::Object(o) if o.contains_key("@value") => o.get("@value"),
        v => Some(v),
    }
}

fn string(o: &Map<String, Json>, term: &str) -> Option<String> {
    match first(o.get(term)?)? {
        Json::String(s) => Some(s.clone()),
        Json::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// `node`, or the node of `graph` it only refers to (`{"@id": "_:b0"}`).
fn resolve<'a>(
    node: &'a Json,
    graph: &'a BTreeMap<&'a str, &'a Map<String, Json>>,
) -> Option<&'a Map<String, Json>> {
    let o = node.as_object()?;
    match o.get("@id").and_then(Json::as_str) {
        Some(id) if o.len() == 1 => graph.get(id).copied(),
        _ => Some(o),
    }
}

/// The shape for `class` in a document compacted against [`SHAPE_CONTEXT`]: its one node, or
/// the node of its `@graph` whose `targetClass` is `class`, else the first with properties.
fn shape_from(json: &Json, class: &str) -> Result<Shape, String> {
    let nodes: Vec<&Map<String, Json>> = match json {
        Json::Object(o) => match o.get("@graph") {
            Some(Json::Array(g)) => g.iter().filter_map(Json::as_object).collect(),
            Some(g) => g.as_object().into_iter().collect(),
            None => vec![o],
        },
        _ => Vec::new(),
    };
    let graph: BTreeMap<&str, &Map<String, Json>> = nodes
        .iter()
        .filter_map(|n| Some((n.get("@id")?.as_str()?, *n)))
        .collect();
    let targets = |n: &Map<String, Json>| match n.get("targetClass") {
        Some(Json::Array(a)) => a.iter().any(|c| c.as_str() == Some(class)),
        Some(c) => c.as_str() == Some(class),
        None => false,
    };
    let o = nodes
        .iter()
        .find(|n| targets(n))
        .or_else(|| nodes.iter().find(|n| n.contains_key("property")))
        .or_else(|| nodes.first().filter(|_| nodes.len() == 1))
        .ok_or_else(|| "it names no shape".to_string())?;
    if !["property", "label", "layout", "targetClass"]
        .iter()
        .any(|k| o.contains_key(*k))
    {
        return Err("it says nothing a view reads (sh:property, rdfs:label, layout)".to_string());
    }
    let label = string(o, "label").or_else(|| string(o, "name"));
    let table = string(o, "layout").as_deref() == Some("table");
    let properties = match o.get("property") {
        None => None,
        Some(ps) => {
            let ps = match ps {
                Json::Array(a) => a.iter().collect(),
                p => vec![p],
            };
            let mut out = Vec::new();
            for (i, p) in ps.into_iter().enumerate() {
                let p = resolve(p, &graph).ok_or_else(|| "a property is not a node".to_string())?;
                let path = string(p, "path")
                    .ok_or_else(|| "a property has no single IRI path".to_string())?;
                let name = string(p, "name")
                    .or_else(|| string(p, "label"))
                    .unwrap_or_else(|| local(&path).to_string());
                let order = string(p, "order")
                    .and_then(|s| s.parse::<f64>().ok())
                    .unwrap_or(i as f64 + 1e6);
                out.push(Property { path, name, order });
            }
            out.sort_by(|a, b| a.order.total_cmp(&b.order));
            Some(out)
        }
    };
    Ok(Shape {
        label,
        table,
        properties,
    })
}

/// A path IRI's local name: what follows its last `#`, `/` or `:`.
fn local(path: &str) -> &str {
    path.rsplit(['#', '/', ':']).next().unwrap_or(path)
}

/// `path`'s value in `data`: under the path itself, else under its local name.
fn get<'a>(data: &'a Map<String, Json>, path: &str) -> Option<&'a Json> {
    data.get(path).or_else(|| data.get(local(path)))
}

fn types(data: &Map<String, Json>) -> Vec<String> {
    match data.get("@type") {
        Some(Json::String(s)) => vec![s.clone()],
        Some(Json::Array(a)) => a
            .iter()
            .filter_map(Json::as_str)
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

fn resource_of(v: &Json) -> Option<&str> {
    v.as_object()?.get("@id")?.as_str()
}

fn scalar(v: &Json) -> Option<String> {
    match v {
        Json::String(s) => Some(s.clone()),
        Json::Number(n) => Some(n.to_string()),
        Json::Bool(b) => Some(b.to_string()),
        Json::Object(o) => o.get("@value").and_then(scalar),
        _ => None,
    }
}

/// `{iri}` in every text property of `n`, written as `iri`.
fn substitute(n: &mut Node, iri: &str) {
    for v in n.properties.values_mut() {
        if let Value::Text(s) = v {
            if s.contains(PLACEHOLDER) {
                *s = s.replace(PLACEHOLDER, iri);
            }
        }
    }
    for c in &mut n.children {
        substitute(c, iri);
    }
}

/// One walk: the path (for cycles) and the nodes produced so far.
struct Walk {
    path: Vec<String>,
    nodes: usize,
}

type Boxed<'a, T> = Pin<Box<dyn Future<Output = Result<T, Error>> + Send + 'a>>;

/// Reads `request` and `None` when it is `NotFound` (nothing bound says so).
async fn optional(inv: &Invocation<'_>, request: Request) -> Result<Option<Vec<u8>>, Error> {
    match inv.issue(request).await {
        Ok(rep) => Ok(Some(rep.bytes)),
        Err(Error::NotFound(_) | Error::Unresolved(_)) => Ok(None),
        Err(e) => Err(e),
    }
}

/// The shape for `class` in the known form: Sourced `as=application/ld+json` (through the
/// kernel's transreptors when it answers another RDF syntax), flattened and compacted against
/// [`SHAPE_CONTEXT`] by the host's `urn:jsonld:flatten` and `urn:jsonld:compact`. `None` when no shape is bound or the
/// caller may not read it; `Some(Err(why))` when one is bound and cannot be read that way.
async fn shape(inv: &Invocation<'_>, class: &str) -> Result<Option<Result<Shape, String>>, Error> {
    let Ok(at) = Iri::parse(format!("{SHAPE}{class}")) else {
        return Ok(None);
    };
    let asked =
        Request::new(Verb::Source, at.clone()).with_arg("as", ArgRef::Inline(JSON_LD.into()));
    let rep = match inv.issue(asked).await {
        // A shape with one face refuses the face it does not have; read the one it has.
        Err(Error::InvalidArgument { name, .. }) if name == "as" => {
            inv.issue(Request::new(Verb::Source, at)).await
        }
        r => r,
    };
    let rep = match rep {
        Ok(rep) => rep,
        Err(Error::NotFound(_) | Error::Unresolved(_) | Error::Denied(_)) => return Ok(None),
        Err(e) => return Err(e),
    };
    let media = rep.repr_type.media_type.clone();
    let bytes = if media == JSON_LD || media == "application/json" {
        rep.bytes
    } else {
        let Some(plan) = inv.select_transreptor(&media, JSON_LD) else {
            return Ok(Some(Err(format!(
                "it is {media}, which nothing here reads as JSON-LD"
            ))));
        };
        match inv.transrept(rep.into_body(), &plan).await {
            Ok(r) => inv.collect(r.bytes).await?.to_vec(),
            Err(e) => return Ok(Some(Err(format!("it is {media}, and as JSON-LD: {e}")))),
        }
    };
    // Flattened first, so a node said in pieces (one object per triple) is one node.
    let flatten =
        Request::new(Verb::Source, iri(FLATTEN)?).with_arg("content", ArgRef::Inline(bytes));
    let flat = match inv.issue(flatten).await {
        Ok(r) => r.bytes,
        Err(e) => return Ok(Some(Err(format!("it does not flatten ({FLATTEN}): {e}")))),
    };
    let compact = Request::new(Verb::Source, iri(COMPACT)?)
        .with_arg("content", ArgRef::Inline(flat))
        .with_arg("context", ArgRef::Inline(SHAPE_CONTEXT.into()));
    let compacted = match inv.issue(compact).await {
        Ok(r) => r.bytes,
        Err(e) => return Ok(Some(Err(format!("it does not compact ({COMPACT}): {e}")))),
    };
    Ok(Some(
        serde_json::from_slice::<Json>(&compacted)
            .map_err(|e| format!("its compaction is not JSON: {e}"))
            .and_then(|j| shape_from(&j, class)),
    ))
}

/// A note that `class`'s shape is bound and could not be read.
fn unreadable(class: &str, why: &str) -> Node {
    text(format!("The shape for {class} could not be read: {why}"))
}

/// The curated view for one of `classes`, `{iri}` filled in.
async fn curated(
    inv: &Invocation<'_>,
    classes: &[String],
    resource: &str,
) -> Result<Option<Node>, Error> {
    for class in classes {
        let Ok(at) = Iri::parse(format!("{TYPE}{class}")) else {
            continue;
        };
        match optional(inv, Request::new(Verb::Source, at)).await {
            Ok(Some(bytes)) => {
                model::bound(&bytes)?;
                let mut n: Node =
                    serde_json::from_slice(&bytes).map_err(|e| Error::InvalidArgument {
                        name: "model".into(),
                        detail: format!("{class}'s curated view is not a model: {e}"),
                    })?;
                substitute(&mut n, resource);
                model::validate(&n)?;
                return Ok(Some(n));
            }
            Ok(None) | Err(Error::Denied(_)) => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(None)
}

/// The resource's data, `None` when it says nothing JSON about itself.
async fn data(inv: &Invocation<'_>, resource: &str) -> Result<Option<Map<String, Json>>, Error> {
    let bytes = optional(inv, Request::new(Verb::Source, iri(resource)?)).await?;
    Ok(bytes
        .filter(|b| model::bound(b).is_ok())
        .and_then(|b| serde_json::from_slice::<Json>(&b).ok())
        .and_then(|j| j.as_object().cloned()))
}

async fn contract(inv: &Invocation<'_>, resource: &str) -> Result<Option<Description>, Error> {
    let request = Request::new(Verb::Meta, iri(resource)?)
        .with_arg("as", ArgRef::Inline(b"application/json".to_vec()));
    match optional(inv, request).await {
        Ok(bytes) => Ok(bytes.and_then(|b| serde_json::from_slice(&b).ok())),
        Err(Error::Denied(_)) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Whether the caller may Sink `resource`: the capability-scoped manifold lists it.
async fn may_sink(inv: &Invocation<'_>, resource: &str) -> Result<bool, Error> {
    let request = Request::new(Verb::Source, iri("urn:kernel:actions")?)
        .with_arg("verb", ArgRef::Inline(b"sink".to_vec()));
    let rep = inv.issue(request).await?;
    Ok(String::from_utf8_lossy(&rep.bytes)
        .lines()
        .any(|l| l.trim() == resource))
}

/// The view of a nested `resource` (under `label`): curated, else generated, else a link at a
/// bound or a repeat, else withheld when the caller may not read it.
fn nested<'a, 'b: 'a>(
    inv: &'a Invocation<'b>,
    walk: &'a mut Walk,
    resource: String,
    label: String,
) -> Boxed<'a, Node> {
    Box::pin(async move {
        if walk.path.len() > MAX_DEPTH || walk.nodes >= MAX_NODES || walk.path.contains(&resource) {
            walk.nodes += 1;
            return Ok(link(&label, &resource));
        }
        match view(inv, walk, resource.clone(), true).await {
            Ok(n) => Ok(n),
            Err(Error::Denied(_)) => {
                walk.nodes += 1;
                Ok(text(format!("{label}: withheld")))
            }
            Err(e) => Err(e),
        }
    })
}

/// One property's value as nodes.
fn value<'a, 'b: 'a>(
    inv: &'a Invocation<'b>,
    walk: &'a mut Walk,
    label: String,
    v: &'a Json,
) -> Boxed<'a, Node> {
    Box::pin(async move {
        if let Some(r) = resource_of(v) {
            let inner = nested(inv, walk, r.to_string(), label.clone()).await?;
            walk.nodes += 1;
            return Ok(node("group", vec![("label", t(&label))], vec![inner]));
        }
        if let Json::Array(items) = v {
            // A list of resources whose type lays out as a table is a table.
            if let Some(table) = table(inv, walk, &label, items).await? {
                return Ok(table);
            }
            let mut children = Vec::new();
            for (i, item) in items.iter().enumerate() {
                let item_label = format!("{label} {}", i + 1);
                let n = match resource_of(item) {
                    Some(r) => nested(inv, walk, r.to_string(), item_label).await?,
                    None => {
                        walk.nodes += 1;
                        text(scalar(item).unwrap_or_default())
                    }
                };
                children.push(n);
            }
            walk.nodes += 1;
            return Ok(node("list", vec![("label", t(&label))], children));
        }
        walk.nodes += 1;
        let s = scalar(v).unwrap_or_default();
        Ok(text(format!("{label}: {s}")))
    })
}

/// `items` as a table when every one is a readable resource whose type's shape is `table`.
async fn table(
    inv: &Invocation<'_>,
    walk: &mut Walk,
    label: &str,
    items: &[Json],
) -> Result<Option<Node>, Error> {
    let Some(first) = items.first().and_then(resource_of) else {
        return Ok(None);
    };
    if walk.path.len() > MAX_DEPTH || walk.nodes >= MAX_NODES {
        return Ok(None);
    }
    let Some(d) = data(inv, first).await.ok().flatten() else {
        return Ok(None);
    };
    let mut layout = None;
    for class in types(&d) {
        if let Some(Ok(s)) = shape(inv, &class).await? {
            if s.table {
                layout = s.properties;
                break;
            }
        }
    }
    let Some(columns) = layout else {
        return Ok(None);
    };
    let header = node(
        "group",
        vec![("label", t("Headers")), ("header", t("column"))],
        columns.iter().map(|c| text(&c.name)).collect(),
    );
    let mut rows = vec![header];
    for (i, item) in items.iter().enumerate() {
        let Some(r) = resource_of(item) else {
            return Ok(None);
        };
        let cells = match data(inv, r).await {
            Ok(Some(d)) => columns
                .iter()
                .map(|c| match get(&d, &c.path) {
                    Some(v) => match resource_of(v) {
                        Some(n) => link(&c.name, n),
                        None => text(nonempty(&scalar(v).unwrap_or_default(), "-")),
                    },
                    None => text("-"),
                })
                .collect(),
            Ok(None) => columns.iter().map(|_| text("-")).collect(),
            Err(Error::Denied(_)) => columns.iter().map(|_| text("withheld")).collect(),
            Err(e) => return Err(e),
        };
        rows.push(node(
            "group",
            vec![("label", t(format!("Row {}", i + 1))), ("header", t("row"))],
            cells,
        ));
    }
    let n = node("table", vec![("caption", t(label))], rows);
    walk.nodes += count(&n);
    Ok(Some(n))
}

/// The view of `resource`: curated for its type when `curate` and one is bound, else generated.
fn view<'a, 'b: 'a>(
    inv: &'a Invocation<'b>,
    walk: &'a mut Walk,
    resource: String,
    curate: bool,
) -> Boxed<'a, Node> {
    Box::pin(async move {
        let d = data(inv, &resource).await?;
        let classes = d.as_ref().map(types).unwrap_or_default();
        if curate {
            if let Some(n) = curated(inv, &classes, &resource).await? {
                walk.nodes += count(&n);
                return Ok(n);
            }
        }
        walk.path.push(resource.clone());
        let n = generated(inv, walk, &resource, d, &classes).await;
        walk.path.pop();
        n
    })
}

async fn generated(
    inv: &Invocation<'_>,
    walk: &mut Walk,
    resource: &str,
    d: Option<Map<String, Json>>,
    classes: &[String],
) -> Result<Node, Error> {
    let meta = contract(inv, resource).await?;
    let mut shape_of = Shape::default();
    let mut notes = Vec::new();
    for class in classes {
        match shape(inv, class).await? {
            Some(Ok(s)) => {
                shape_of = s;
                break;
            }
            Some(Err(why)) => notes.push(unreadable(class, &why)),
            None => {}
        }
    }
    let title = meta
        .as_ref()
        .map(|m| nonempty(&m.title, ""))
        .filter(|s| !s.is_empty())
        .or_else(|| shape_of.label.clone())
        .unwrap_or_else(|| resource.to_string());
    let mut children = vec![node(
        "heading",
        vec![
            ("content", t(&title)),
            ("level", Value::Number((walk.path.len() as i64).clamp(1, 6))),
        ],
        Vec::new(),
    )];
    if let Some(m) = &meta {
        if !m.summary.trim().is_empty() {
            children.push(text(m.summary.trim()));
        }
        let mut facts = Vec::new();
        let verbs: Vec<String> = m
            .verbs
            .iter()
            .map(|v| format!("{v:?}").to_lowercase())
            .collect();
        if !verbs.is_empty() {
            facts.push(text(format!("Verbs: {}", verbs.join(", "))));
        }
        let inputs: Vec<&str> = m.inputs.iter().map(|i| i.name.as_str()).collect();
        if !inputs.is_empty() {
            facts.push(text(format!("Inputs: {}", inputs.join(", "))));
        }
        if !m.outputs.is_empty() {
            facts.push(text(format!("Faces: {}", m.outputs.join(", "))));
        }
        if !facts.is_empty() {
            children.push(node("list", vec![("label", t("Contract"))], facts));
        }
    }
    children.extend(notes);
    walk.nodes += children.iter().map(count).sum::<usize>();
    if let Some(d) = &d {
        let shown: Vec<(String, String)> = match &shape_of.properties {
            Some(ps) => ps
                .iter()
                .map(|p| (p.path.clone(), p.name.clone()))
                .collect(),
            None => d
                .keys()
                .filter(|k| !k.starts_with('@'))
                .map(|k| (k.clone(), k.clone()))
                .collect(),
        };
        let mut props = Vec::new();
        for (path, label) in shown {
            if let Some(v) = get(d, &path) {
                props.push(value(inv, walk, label, v).await?);
            }
        }
        if !props.is_empty() {
            walk.nodes += 1;
            children.push(node("group", vec![("label", t("Properties"))], props));
        }
    }
    if may_sink(inv, resource).await? {
        let at = iri(&format!("{}{resource}", crate::FORM_RESOURCE))?;
        let rep = inv.issue(Request::new(Verb::Source, at)).await?;
        let form = model::parse(&rep.bytes)?;
        walk.nodes += count(&form);
        children.push(form);
    }
    walk.nodes += 1;
    Ok(node("group", vec![("label", t(&title))], children))
}

/// The view of `resource` for `inv`'s caller: curated for its type when `curate`, else
/// generated, nested resources recursively. Validated before it is answered.
pub async fn of(inv: &Invocation<'_>, resource: &str, curate: bool) -> Result<Node, Error> {
    let mut walk = Walk {
        path: Vec::new(),
        nodes: 0,
    };
    let n = view(inv, &mut walk, resource.to_string(), curate).await?;
    model::validate(&n)?;
    Ok(n)
}

pub(crate) fn endpoint(id: &'static str, curate: bool) -> ikigai_core::AsyncFnEndpoint {
    let (title, summary) = if curate {
        (
            "A resource's view",
            "The view of a resource: the curated view for its type \
             (urn:iki:ui:view:type:{class}) when one is bound, else the view generated from \
             what it says about itself (its contract, its data, its types' shapes, the \
             caller's actions), nested resources recursively, bounded by depth and nodes.",
        )
    } else {
        (
            "A resource's generated view",
            "The view generated from what a resource says about itself: its contract, its \
             data, its types' shapes (urn:iki:ui:shape:type:{class}) and a form for each \
             action the caller may take; nested resources through their own views, bounded \
             by depth and nodes, a cycle a link.",
        )
    };
    let description = Description::new(id)
        .title(title)
        .summary(summary)
        .verb(Verb::Source)
        .verb(Verb::Meta)
        .output(model::MEDIA_TYPE)
        .output(model::JSON_LD)
        .input(
            ikigai_core::ArgSpec::new("iri")
                .summary("the resource the view shows")
                .class("http://www.w3.org/2001/XMLSchema#anyURI")
                .binding()
                .example("urn:iki:ui:kind"),
        );
    ikigai_core::AsyncFnEndpoint::new(id, move |inv: &Invocation<'_>| -> InvokeFuture<'_> {
        Box::pin(async move {
            let resource = inv.bindings.get("iri").unwrap_or_default().to_string();
            iri(&resource)?;
            let n = of(inv, &resource, curate).await?;
            crate::model_face(inv, &n)
        })
    })
    .with_description(description)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_compacted_shape_orders_labels_and_resolves_its_graph() {
        let s = shape_from(
            &serde_json::json!({"@graph": [
                {"@id": "_:p", "path": "urn:demo:b", "name": "Bee", "order": {"@value": "2"}},
                {"@id": "urn:other", "targetClass": "urn:demo:Other", "property": []},
                {"@id": "urn:s", "targetClass": "urn:demo:T", "layout": "table", "property": [
                    {"@id": "_:p"},
                    {"path": "urn:demo:a", "name": "Ay", "order": 1}
                ]}
            ]}),
            "urn:demo:T",
        )
        .unwrap();
        assert!(s.table);
        let ps = s.properties.unwrap();
        assert_eq!(ps[0].name, "Ay");
        assert_eq!(ps[1].path, "urn:demo:b");
        assert_eq!(local(&ps[1].path), "b");
        assert!(shape_from(&serde_json::json!([]), "urn:demo:T").is_err());
    }

    #[test]
    fn an_href_is_a_door_path() {
        assert_eq!(href("urn:a:b c"), "/ui/view/resource/urn:a:b%20c");
        assert!(ikigai_fn::html::safe_href(&href("urn:a:b")).is_some());
    }
}
