//! A `form` node generated from a MODEL rather than written by hand (ledger #3354, phase 1 of
//! #3351).
//!
//! A resource's contract is the model: what its `Sink` reads, input by input, is what a form
//! for it asks for. OpenAPI is one projection of that model, and this module reads an OpenAPI
//! 3.1 document too, so a service that is not an ikigai resource gets the same form. The same
//! generation works, in principle, from any other model of a request: a SHACL shape or a JSON
//! Schema says the same things (a name, required or not, a datatype, a closed set, a default).
//!
//! Both readers come down to one list of [`Input`]s and one function, [`form`], so the two
//! cannot drift: the same model gives the same form whichever face it arrives in.
//!
//! ## From an input to a field
//!
//! | the model says | the node |
//! |---|---|
//! | a name | a `field` (or `choice`) named and labeled by it |
//! | its summary / `description` | the field's `description` |
//! | required | `required: true`, else `false` |
//! | a closed set (`one_of` / `enum`) | a `choice` of `option`s, each value its own label |
//! | a default | the field's (or the choice's) `value` |
//! | `xsd:integer` and kin / `integer`, `number` | `input: number` |
//! | `xsd:boolean` / `boolean` | `input: checkbox` |
//! | `xsd:date` / `format: date` | `input: date` |
//! | `xsd:anyURI` or an RDF class / `format: uri`, `iri` (and `-reference`) | `input: url` |
//! | `schema:Text` / `format: long-text` | `input: textarea` (long text) |
//! | anything else | `input: text` |
//!
//! ## Why no OpenAPI crate
//!
//! The crates that read OpenAPI either stop at 3.0 (`openapiv3`) or model all of 3.1 with a
//! dependency tree far larger than the five schema keywords read here (`oas3`, and it is not
//! on the shared host's vetted list). The subset, `requestBody` → its JSON (or form-encoded)
//! schema → `properties` in order, `required`, `enum`, `default`, `type` and `format`, a local
//! `$ref` to `#/components/schemas/…`, lives in this one module, over `serde_json`.

use std::collections::BTreeMap;

use ikigai_core::{ActionSpec, ArgSpec, Description, Error, InputSource, Verb};
use serde::Deserialize;

use crate::model::{self, Node, Value};

/// What every generated form says it is, in its `description`.
pub const GENERATED: &str = "Generated from a model, not written by hand: a resource's \
     contract, or an OpenAPI document (one projection of it); a SHACL shape or a JSON Schema \
     would serve the same way.";

/// The largest OpenAPI document read: 1 MiB, the model's own bound.
pub const MAX_BYTES: usize = model::MAX_BYTES;

/// What an input's value is, as a form asks for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputType {
    /// Short text.
    Text,
    /// An integer (or a number).
    Integer,
    /// A boolean.
    Boolean,
    /// A date.
    Date,
    /// An IRI.
    Iri,
    /// Long text.
    LongText,
}

impl InputType {
    /// The field's `input`.
    pub fn input(self) -> &'static str {
        match self {
            InputType::Text => "text",
            InputType::Integer => "number",
            InputType::Boolean => "checkbox",
            InputType::Date => "date",
            InputType::Iri => "url",
            InputType::LongText => "textarea",
        }
    }
}

/// One input a model declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Input {
    /// Its name.
    pub name: String,
    /// Its help text (may be empty).
    pub summary: String,
    /// Whether it is required.
    pub required: bool,
    /// What its value is.
    pub input: InputType,
    /// Its closed set of values, if it is enumerated.
    pub one_of: Option<Vec<String>>,
    /// Its default.
    pub default: Option<String>,
}

fn invalid(name: &str, detail: impl Into<String>) -> Error {
    Error::InvalidArgument {
        name: name.to_string(),
        detail: detail.into(),
    }
}

fn text(p: &str, v: &str) -> (String, Value) {
    (p.to_string(), Value::Text(v.to_string()))
}

/// The form: `label`, a `description` (the model's summary, then [`GENERATED`]), `resource` (the
/// resource's name) and `verb` sink, `action` the same when it is a URL a door can post to, and a
/// field or choice per input, then a submit button. Validated before it is answered.
pub fn form(label: &str, summary: &str, resource: &str, inputs: &[Input]) -> Result<Node, Error> {
    let label = if label.trim().is_empty() {
        resource
    } else {
        label
    };
    let description = if summary.trim().is_empty() {
        GENERATED.to_string()
    } else {
        format!("{} {GENERATED}", summary.trim())
    };
    let mut properties: BTreeMap<String, Value> = [
        text("label", label),
        text("description", &description),
        text("resource", resource),
        text("verb", "sink"),
        text("method", "post"),
    ]
    .into();
    if ikigai_fn::html::safe_href(resource).is_some_and(|h| h == resource) {
        properties.insert("action".into(), Value::Text(resource.to_string()));
    }
    let mut children = Vec::new();
    for i in inputs {
        if i.name.is_empty() {
            return Err(invalid("model", "an input has no name"));
        }
        let mut p: BTreeMap<String, Value> = [
            text("label", &i.name),
            text("name", &i.name),
            ("required".to_string(), Value::Bool(i.required)),
        ]
        .into();
        if !i.summary.trim().is_empty() {
            p.insert("description".into(), Value::Text(i.summary.trim().into()));
        }
        if let Some(d) = i.default.as_deref().filter(|d| !d.is_empty()) {
            p.insert("value".into(), Value::Text(d.to_string()));
        }
        let node = match &i.one_of {
            Some(values) => Node {
                replacement: None,
                kind: "choice".into(),
                properties: p,
                children: values
                    .iter()
                    .filter(|v| !v.is_empty())
                    .map(|v| Node {
                        replacement: None,
                        kind: "option".into(),
                        properties: [text("label", v), text("value", v)].into(),
                        children: Vec::new(),
                    })
                    .collect(),
            },
            None => {
                p.insert("input".into(), Value::Text(i.input.input().into()));
                Node {
                    replacement: None,
                    kind: "field".into(),
                    properties: p,
                    children: Vec::new(),
                }
            }
        };
        children.push(node);
    }
    children.push(Node {
        replacement: None,
        kind: "button".into(),
        properties: [text("label", "Submit")].into(),
        children: Vec::new(),
    });
    let node = Node {
        replacement: None,
        kind: "form".into(),
        properties,
        children,
    };
    model::validate(&node)?;
    Ok(node)
}

// --- from a contract -------------------------------------------------------------------------

const XSD: &str = "http://www.w3.org/2001/XMLSchema#";

fn contract_type(class: Option<&str>) -> InputType {
    let Some(class) = class else {
        return InputType::Text;
    };
    if let Some(x) = class.strip_prefix(XSD) {
        return match x {
            "integer" | "int" | "long" | "short" | "byte" | "decimal" | "nonNegativeInteger"
            | "positiveInteger" | "negativeInteger" | "nonPositiveInteger" | "unsignedInt"
            | "unsignedLong" | "unsignedShort" | "unsignedByte" => InputType::Integer,
            "boolean" => InputType::Boolean,
            "date" => InputType::Date,
            "anyURI" => InputType::Iri,
            _ => InputType::Text,
        };
    }
    match class {
        "https://schema.org/Text" | "http://schema.org/Text" => InputType::LongText,
        // Any other class is an entity's: its value is the entity's IRI.
        _ => InputType::Iri,
    }
}

fn contract_input(a: &ArgSpec) -> Input {
    Input {
        name: a.name.clone(),
        summary: a.summary.clone(),
        required: a.required,
        input: contract_type(a.class.as_deref()),
        one_of: a.is_enumerated().then(|| a.one_of.clone()),
        default: a.default.clone(),
    }
}

/// The form for `resource` from its contract (its Meta, `as=application/json`): its
/// Sink's inputs, through both authoring forms ([`Description::action_specs`]), each but a
/// binding (that is in the name already). A contract with no Sink is `InvalidArgument`.
pub fn from_contract(resource: &str, contract: &[u8]) -> Result<Node, Error> {
    let d: Description = serde_json::from_slice(contract)
        .map_err(|e| invalid("iri", format!("{resource}: not a contract: {e}")))?;
    let sink: ActionSpec = d
        .action_specs()
        .into_iter()
        .find(|a| a.verb == Verb::Sink)
        .ok_or_else(|| invalid("iri", format!("{resource}: its contract declares no Sink")))?;
    let inputs: Vec<Input> = sink
        .inputs
        .iter()
        .filter(|a| a.source != InputSource::Binding)
        .map(contract_input)
        .collect();
    let summary = if sink.summary.is_empty() {
        &d.summary
    } else {
        &sink.summary
    };
    let label = if d.title.is_empty() { &d.id } else { &d.title };
    form(label, summary, resource, &inputs)
}

// --- from an OpenAPI 3.1 document ------------------------------------------------------------

#[derive(Deserialize)]
struct Doc {
    openapi: String,
    #[serde(default)]
    servers: Vec<Server>,
    #[serde(default)]
    paths: BTreeMap<String, PathItem>,
    #[serde(default)]
    components: Components,
}

#[derive(Deserialize)]
struct Server {
    url: String,
}

#[derive(Deserialize, Default)]
struct Components {
    #[serde(default)]
    schemas: BTreeMap<String, Schema>,
}

#[derive(Deserialize)]
struct PathItem {
    get: Option<Operation>,
    put: Option<Operation>,
    post: Option<Operation>,
    delete: Option<Operation>,
    patch: Option<Operation>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Operation {
    operation_id: Option<String>,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    description: String,
    request_body: Option<RequestBody>,
}

#[derive(Deserialize)]
struct RequestBody {
    #[serde(default)]
    content: BTreeMap<String, MediaType>,
}

#[derive(Deserialize)]
struct MediaType {
    schema: Option<Schema>,
}

#[derive(Deserialize, Default)]
struct Schema {
    #[serde(rename = "$ref")]
    reference: Option<String>,
    #[serde(rename = "type")]
    kind: Option<serde_json::Value>,
    format: Option<String>,
    #[serde(default)]
    description: String,
    properties: Option<Ordered>,
    #[serde(default)]
    required: Vec<String>,
    #[serde(rename = "enum")]
    one_of: Option<Vec<serde_json::Value>>,
    default: Option<serde_json::Value>,
}

/// `properties` in the document's order (a JSON object, read as a list: the order is the
/// form's).
#[derive(Default)]
struct Ordered(Vec<(String, Schema)>);

impl<'de> Deserialize<'de> for Ordered {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = Ordered;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object of schemas")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Ordered, A::Error> {
                let mut out = Vec::new();
                while let Some(entry) = map.next_entry::<String, Schema>()? {
                    out.push(entry);
                }
                Ok(Ordered(out))
            }
        }
        d.deserialize_map(V)
    }
}

/// A JSON scalar as the lexical form a field carries.
fn lexical(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn openapi_type(s: &Schema) -> InputType {
    // 3.1 allows a list of types (`["string", "null"]`): the first that is not null.
    let kind = match &s.kind {
        Some(serde_json::Value::String(t)) => t.as_str(),
        Some(serde_json::Value::Array(ts)) => ts
            .iter()
            .filter_map(|t| t.as_str())
            .find(|t| *t != "null")
            .unwrap_or("string"),
        _ => "string",
    };
    match (kind, s.format.as_deref()) {
        ("integer" | "number", _) => InputType::Integer,
        ("boolean", _) => InputType::Boolean,
        (_, Some("date")) => InputType::Date,
        (_, Some("uri" | "iri" | "uri-reference" | "iri-reference")) => InputType::Iri,
        (_, Some("long-text")) => InputType::LongText,
        _ => InputType::Text,
    }
}

fn resolve<'a>(doc: &'a Doc, s: &'a Schema, at: &str) -> Result<&'a Schema, Error> {
    let Some(r) = &s.reference else {
        return Ok(s);
    };
    let name = r.strip_prefix("#/components/schemas/").ok_or_else(|| {
        invalid(
            "doc",
            format!("{at}: only a local `#/components/schemas/` $ref is read, not `{r}`"),
        )
    })?;
    let target = doc
        .components
        .schemas
        .get(name)
        .ok_or_else(|| invalid("doc", format!("{at}: `{r}` names no schema")))?;
    if target.reference.is_some() {
        return Err(invalid("doc", format!("{at}: `{r}` is itself a $ref")));
    }
    Ok(target)
}

/// The form for operation `op` (its `operationId`) of the OpenAPI 3.1 document `doc`. The
/// resource is the first server's URL joined with the operation's path. A document that is not
/// 3.1 JSON is `InvalidArgument`; an operation it does not hold is `NotFound`.
pub fn from_openapi(doc_iri: &str, doc: &[u8], op: &str) -> Result<Node, Error> {
    if doc.len() > MAX_BYTES {
        return Err(invalid(
            "doc",
            format!("{doc_iri}: past the bound of {MAX_BYTES} bytes"),
        ));
    }
    model::bound(doc).map_err(|_| invalid("doc", format!("{doc_iri}: nested too deep")))?;
    let doc: Doc = serde_json::from_slice(doc)
        .map_err(|e| invalid("doc", format!("{doc_iri}: not an OpenAPI document: {e}")))?;
    if !(doc.openapi == "3.1" || doc.openapi.starts_with("3.1.")) {
        return Err(invalid(
            "doc",
            format!("{doc_iri}: OpenAPI {} is not 3.1", doc.openapi),
        ));
    }
    let found = doc.paths.iter().find_map(|(path, item)| {
        [&item.post, &item.put, &item.patch, &item.delete, &item.get]
            .into_iter()
            .flatten()
            .find(|o| o.operation_id.as_deref() == Some(op))
            .map(|o| (path, o))
    });
    let (path, operation) =
        found.ok_or_else(|| Error::NotFound(format!("{doc_iri}: no operation `{op}`")))?;
    let at = format!("{doc_iri}: operation `{op}`");
    let empty = Schema::default();
    let schema = match &operation.request_body {
        None => &empty,
        Some(body) => {
            let media = body
                .content
                .get("application/json")
                .or_else(|| body.content.get("application/x-www-form-urlencoded"))
                .or_else(|| body.content.values().next())
                .ok_or_else(|| invalid("doc", format!("{at}: its requestBody has no content")))?;
            match &media.schema {
                Some(s) => resolve(&doc, s, &at)?,
                None => &empty,
            }
        }
    };
    let mut inputs = Vec::new();
    for (name, property) in schema.properties.iter().flat_map(|p| &p.0) {
        let property = resolve(&doc, property, &at)?;
        let one_of = match &property.one_of {
            None => None,
            Some(values) => Some(
                values
                    .iter()
                    .map(|v| {
                        lexical(v).ok_or_else(|| {
                            invalid(
                                "doc",
                                format!("{at}: `{name}` has an enum value that is not a scalar"),
                            )
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            ),
        };
        inputs.push(Input {
            name: name.clone(),
            summary: property.description.clone(),
            required: schema.required.contains(name),
            input: openapi_type(property),
            one_of,
            default: property.default.as_ref().and_then(lexical),
        });
    }
    let server = doc
        .servers
        .first()
        .map(|s| s.url.trim_end_matches('/'))
        .unwrap_or("");
    let resource = format!("{server}{path}");
    let label = match (&operation.summary, &operation.operation_id) {
        (s, _) if !s.is_empty() => s.as_str(),
        (_, Some(id)) => id.as_str(),
        _ => op,
    };
    form(label, &operation.description, &resource, &inputs)
}
