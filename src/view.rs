//! Views generated from a resource's own model, recursively, with curated views as overrides
//! (ledger #3366, under #3351; after the form, #3354, and the outline, #3365).
//!
//! | name | what it is |
//! |---|---|
//! | `urn:iki:ui:view:resource:{iri}` | the view of `{iri}`: the curated view for its type when one is bound, else the generated one |
//! | `urn:iki:ui:view:generated:{iri}` | always the generated view, so a curated view can start from it and override only what it needs (a `reference` to it) without naming itself |
//! | `urn:iki:ui:view:type:{class}` | an author's view for a type (a model); `{iri}` in any text property is the shown resource |
//! | `urn:iki:ui:shape:type:{class}` | the shape a generated view reads for a type (SHACL in compact JSON-LD, below) |
//!
//! ## What a generated view reads, all through the kernel under the CALLER's capability
//!
//! - **Its contract**, `{iri}`'s Meta `as=application/json`: its title is the heading, its
//!   summary the first text, its verbs and outputs a `list` labeled "Contract".
//! - **Its data**, `{iri}`'s Source as a JSON object: `@type` names its classes; every other key
//!   is a property. A value `{"@id": …}` is another resource; an array is a list of values.
//! - **Its shape**, `urn:iki:ui:shape:type:{class}` for each class, a JSON object:
//!   `{"label": …, "layout": "detail" | "table", "property": [{"path": …, "name": …,
//!   "order": …}]}` (the `sh:`/`rdfs:` prefixed keys are read too). A shape says which
//!   properties show, their labels and order; with none, every property shows under its key.
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

use crate::model::{self, Node, Value};

/// The view of a resource: curated for its type, else generated.
pub const RESOURCE: &str = "urn:iki:ui:view:resource:";
/// Always the generated view.
pub const GENERATED: &str = "urn:iki:ui:view:generated:";
/// A curated view for a type.
pub const TYPE: &str = "urn:iki:ui:view:type:";
/// The shape a generated view reads for a type.
pub const SHAPE: &str = "urn:iki:ui:shape:type:";
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

fn key<'a>(o: &'a Map<String, Json>, names: &[&str]) -> Option<&'a Json> {
    names.iter().find_map(|n| o.get(*n))
}

fn shape_from(json: &Json) -> Shape {
    let Some(o) = json.as_object() else {
        return Shape::default();
    };
    let label = key(o, &["label", "rdfs:label", "name", "sh:name"])
        .and_then(Json::as_str)
        .map(str::to_string);
    let table = key(o, &["layout", "ui:layout"]).and_then(Json::as_str) == Some("table");
    let properties = key(o, &["property", "sh:property"])
        .and_then(Json::as_array)
        .map(|ps| {
            let mut out: Vec<Property> = ps
                .iter()
                .enumerate()
                .filter_map(|(i, p)| {
                    let p = p.as_object()?;
                    let path = key(p, &["path", "sh:path"])?;
                    let path = path
                        .as_str()
                        .or_else(|| path.get("@id").and_then(Json::as_str))?
                        .to_string();
                    let name = key(p, &["name", "sh:name", "rdfs:label"])
                        .and_then(Json::as_str)
                        .unwrap_or(&path)
                        .to_string();
                    let order = key(p, &["order", "sh:order"])
                        .and_then(Json::as_f64)
                        .unwrap_or(i as f64 + 1e6);
                    Some(Property { path, name, order })
                })
                .collect();
            out.sort_by(|a, b| a.order.total_cmp(&b.order));
            out
        });
    Shape {
        label,
        table,
        properties,
    }
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

async fn shape(inv: &Invocation<'_>, class: &str) -> Result<Option<Shape>, Error> {
    let Ok(at) = Iri::parse(format!("{SHAPE}{class}")) else {
        return Ok(None);
    };
    match optional(inv, Request::new(Verb::Source, at)).await {
        Ok(Some(bytes)) => Ok(serde_json::from_slice::<Json>(&bytes)
            .ok()
            .map(|j| shape_from(&j))),
        Ok(None) | Err(Error::Denied(_)) => Ok(None),
        Err(e) => Err(e),
    }
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
        if let Some(s) = shape(inv, &class).await? {
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
                .map(|c| match d.get(&c.path) {
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
    for class in classes {
        if let Some(s) = shape(inv, class).await? {
            shape_of = s;
            break;
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
            if let Some(v) = d.get(&path) {
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
    fn a_shape_orders_and_labels() {
        let s = shape_from(&serde_json::json!({
            "sh:property": [
                {"sh:path": "b", "sh:name": "Bee", "sh:order": 2},
                {"path": "a", "name": "Ay", "order": 1}
            ],
            "layout": "table"
        }));
        assert!(s.table);
        let ps = s.properties.unwrap();
        assert_eq!(ps[0].name, "Ay");
        assert_eq!(ps[1].path, "b");
    }

    #[test]
    fn an_href_is_a_door_path() {
        assert_eq!(href("urn:a:b c"), "/ui/view/resource/urn:a:b%20c");
        assert!(ikigai_fn::html::safe_href(&href("urn:a:b")).is_some());
    }
}
