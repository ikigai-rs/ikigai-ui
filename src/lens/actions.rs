//! Lens gestures and the operations view (ledger #3522).
//!
//! Gestures are named Sinks with generated forms, never renderer instructions. A host
//! enables them in a lens's `actions` map, fixing each target graph in that data. The
//! operations view takes a request-local `dropped` JSON array; it stores no shared selection.

use super::*;
use ikigai_core::{ActionSpec, ArgSpec, InputSource};
use std::collections::BTreeSet;

/// The narrow store door used by both vocabulary gestures.
pub const GRAPH_UPDATE: &str = "urn:iki:store:graph-update";
/// A validated operation submission, followed by the operation's IRI.
pub const OPERATION: &str = "urn:iki:ui:lens:operation:";
/// Maximum personal selection and maximum offered operations. Past either bound is said.
pub const MAX_DROPPED: usize = 32;
/// The most operation forms shown, in manifold rank order.
pub const MAX_OPERATIONS: usize = 32;
const SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const RELATIONS: [&str; 4] = [
    SUBCLASS,
    "http://www.w3.org/2002/07/owl#equivalentClass",
    "http://www.w3.org/2004/02/skos/core#exactMatch",
    "http://www.w3.org/2004/02/skos/core#closeMatch",
];

/// A gesture's declarative configuration. Its graph cannot be overridden by a form.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Action {
    /// The one vocabulary graph the gesture writes.
    pub graph: String,
}

fn contract(action: &str) -> Description {
    let binding = || {
        ArgSpec::new("lens")
            .class(format!("{XSD}string"))
            .binding()
            .example("action-probe")
    };
    let mut sink = ActionSpec::new(Verb::Sink)
        .requires("urn:cap:store:write:graph:*")
        .input(binding())
        .input(
            ArgSpec::new("selected")
                .class(XSD_ANY_URI)
                .summary("The selected class or first term."),
        )
        .input(
            ArgSpec::new("target")
                .class(XSD_ANY_URI)
                .summary(if action == "extend" {
                    "The new subclass IRI."
                } else {
                    "The second selected term, including a term from another vocabulary."
                }),
        );
    if action == "connect" {
        sink = sink.input(
            ArgSpec::new("relation")
                .class(format!("{XSD}string"))
                .one_of(RELATIONS)
                .default_value(SUBCLASS),
        );
    }
    Description::new(format!("ui-lens-{action}"))
        .title(if action == "extend" { "Extend a class" } else { "Connect two terms" })
        .summary("A generated vocabulary gesture. Refusals stay with this form. The lens fixes the graph; the caller needs its narrow write grant. Shapes are checked before writing.")
        .action(ActionSpec::new(Verb::Source).input(binding()).output("application/json"))
        .action(sink)
}

async fn offered(inv: &Invocation<'_>, endpoint: &str) -> Result<bool, Error> {
    let rep = inv
        .issue(
            Request::new(Verb::Source, iri("urn:kernel:actions")?)
                .with_arg("verb", ArgRef::Inline(b"sink".to_vec())),
        )
        .await?;
    model::bound(&rep.bytes)?;
    Ok(String::from_utf8_lossy(&rep.bytes)
        .lines()
        .any(|line| line.trim() == endpoint))
}

async fn writable(inv: &Invocation<'_>, graph: &str) -> Result<bool, Error> {
    // The manifold's wildcard grant says SOME graph is writable. The bound graph's
    // exact grant is also necessary; another vocabulary's grant is insufficient.
    Ok(inv
        .capability
        .allows(&format!("urn:cap:store:write:graph:{graph}"))
        && offered(inv, GRAPH_UPDATE).await?)
}

pub(super) fn graph_term(value: &str) -> Result<String, Error> {
    iri(value)?;
    // Iri validates names, but a SPARQL IRIREF has a stricter lexical boundary.
    if value.chars().any(|c| c <= ' ' || "<>\"{}|^`\\".contains(c)) {
        return Err(refuse(
            "selected",
            "an IRI contains a character forbidden in an RDF IRIREF",
        ));
    }
    Ok(format!("<{value}>"))
}

async fn definition(inv: &Invocation<'_>, lens: &str, action: &str) -> Result<Action, Error> {
    let l = read_lens(inv, lens).await?;
    l.actions
        .get(action)
        .cloned()
        .ok_or_else(|| Error::NotFound(format!("{lens} has no {action} gesture")))
}

async fn write(inv: &Invocation<'_>, lens: &str, action: &str) -> Result<Representation, Error> {
    for key in inv.request.args.keys() {
        if !matches!(key.as_str(), "selected" | "target")
            && !(key == "relation" && action == "connect")
        {
            return Err(refuse(
                key,
                "this gesture does not accept that argument; the lens fixes its graph",
            ));
        }
    }
    let a = definition(inv, lens, action).await?;
    if !writable(inv, &a.graph).await? {
        return Err(Error::Denied(format!(
            "{action}: writing {} is not offered under your capability",
            a.graph
        )));
    }
    let selected = graph_term(inv.inline_str("selected")?)?;
    let target = graph_term(inv.inline_str("target")?)?;
    if action == "extend" {
        let report = inv.issue(Request::new(Verb::Source, iri("urn:iki:store:graph-ask")?)
            .with_arg("graph", ArgRef::Inline(a.graph.clone().into_bytes()))
            .with_arg("query", ArgRef::Inline(format!("ASK {{ {selected} a ?class . VALUES ?class {{ <http://www.w3.org/2000/01/rdf-schema#Class> <http://www.w3.org/2002/07/owl#Class> }} }}").into_bytes()))
            .with_arg("as",ArgRef::Inline(b"application/sparql-results+json".to_vec()))).await?;
        model::bound(&report.bytes)?;
        let report: Json =
            serde_json::from_slice(&report.bytes).map_err(|e| refuse("selected", e))?;
        if report["boolean"] != true {
            return Err(refuse(
                "selected",
                "extend needs a selected rdfs:Class or owl:Class in the vocabulary graph",
            ));
        }
    }
    let relation = if action == "extend" {
        SUBCLASS
    } else {
        inv.request
            .args
            .get("relation")
            .map(|_| inv.inline_str("relation"))
            .transpose()?
            .unwrap_or(SUBCLASS)
    };
    if !RELATIONS.contains(&relation) {
        return Err(refuse(
            "relation",
            "choose subClassOf, equivalentClass, exactMatch, or closeMatch",
        ));
    }
    // Direction matters: extending P creates C subClassOf P; connecting A and B
    // creates A relation B. No submitted value is interpreted as update syntax.
    let triple = if action == "extend" {
        format!("{target} <{SUBCLASS}> {selected} .\n{target} a <http://www.w3.org/2000/01/rdf-schema#Class> .")
    } else {
        format!("{selected} <{relation}> {target} .")
    };
    let shapes_at = format!("{SHAPES}{}", a.graph);
    if optional(inv, &shapes_at).await?.is_some() {
        let mut data = inv
            .issue(
                Request::new(Verb::Source, iri("urn:iki:store:graph-construct")?)
                    .with_arg("graph", ArgRef::Inline(a.graph.clone().into_bytes()))
                    .with_arg(
                        "query",
                        ArgRef::Inline(b"CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o }".to_vec()),
                    )
                    .with_arg("as", ArgRef::Inline(b"text/turtle".to_vec())),
            )
            .await?
            .bytes;
        model::bound(&data)?;
        data.extend_from_slice(format!("\n{triple}\n").as_bytes());
        model::bound(&data)?;
        let report = inv
            .issue(
                Request::new(Verb::Source, iri("urn:shacl:validate")?)
                    .with_arg("data", ArgRef::Inline(data))
                    .with_arg("shapes", ArgRef::Inline(shapes_at.into_bytes()))
                    .with_arg("as", ArgRef::Inline(b"application/json".to_vec())),
            )
            .await?;
        model::bound(&report.bytes)?;
        let report: Json =
            serde_json::from_slice(&report.bytes).map_err(|e| refuse("shapes", e))?;
        if report["conforms"] != true {
            return Err(refuse(
                "target",
                format!("{action}: vocabulary shapes refused the proposed graph: {report}"),
            ));
        }
    }
    inv.issue(
        Request::new(Verb::Sink, iri(GRAPH_UPDATE)?)
            .with_arg("graph", ArgRef::Inline(a.graph.clone().into_bytes()))
            .with_arg(
                "content",
                ArgRef::Inline(
                    format!(
                        "INSERT DATA {{ GRAPH {} {{ {triple} }} }}",
                        graph_term(&a.graph)?
                    )
                    .into_bytes(),
                ),
            ),
    )
    .await
}

pub(crate) fn endpoint(action: &'static str) -> AsyncFnEndpoint {
    AsyncFnEndpoint::new(
        format!("ui-lens-{action}"),
        move |inv: &Invocation<'_>| -> InvokeFuture<'_> {
            Box::pin(async move {
                let lens = inv.bindings.get("lens").unwrap_or_default().to_string();
                match inv.request.verb {
                    Verb::Source => Ok(data(
                        serde_json::to_vec(&definition(inv, &lens, action).await?)
                            .map_err(|e| refuse("action", e))?,
                        "application/json",
                    )),
                    Verb::Sink => write(inv, &lens, action).await,
                    _ => Err(refuse("verb", "a lens action supports Source and Sink")),
                }
            })
        },
    )
    .with_description(contract(action))
}

fn prefill(form: &mut Node, values: &BTreeMap<String, String>) {
    for field in &mut form.children {
        if let Some(model::Value::Text(name)) = field.properties.get("name") {
            if let Some(value) = values.get(name) {
                field
                    .properties
                    .insert("value".into(), model::Value::Text(value.clone()));
            }
        }
    }
}

/// The ordinary linked selection and generated gesture forms beside a renderer.
pub(crate) async fn controls(
    inv: &Invocation<'_>,
    name: &str,
    lens: &Lens,
    home: &Home,
    selection: &Selection,
    resource: &str,
    candidates: &BTreeSet<String>,
) -> Result<Vec<Node>, Error> {
    if lens.actions.is_empty() {
        return Ok(Vec::new());
    }
    let selected = selection.clone();
    for value in selected.values() {
        graph_term(value)?;
    }
    let mut out: Vec<Node> = vec![serde_json::from_value(json!({"kind":"form", "properties":{"label":"Select vocabulary terms", "method":"get", "action":html_href(name, resource, &Selection::new())}, "children":[
        {"kind":"field", "properties":{"label":"First selected term", "name":"selected", "input":"url", "value":selected.get("selected").cloned().unwrap_or_default()}},
        {"kind":"field", "properties":{"label":"Second selected term", "name":"target", "input":"url", "value":selected.get("target").cloned().unwrap_or_default()}},
        {"kind":"button", "properties":{"label":"Select terms"}}
    ]})).map_err(|e| refuse("selection", e))?];
    for field in &mut out[0].children {
        if field.properties.get("value") == Some(&model::Value::Text(String::new())) {
            field.properties.remove("value");
        }
    }
    let links: Vec<Json> = selected
        .values()
        .map(|v| json!({"kind":"link", "properties":{"label":v,"href":home.href(v)}}))
        .collect();
    out.push(
        serde_json::from_value(
            json!({"kind":"list", "properties":{"label":"Current selection"}, "children":links}),
        )
        .map_err(|e| refuse("selection", e))?,
    );
    let mut choices = Vec::new();
    for value in candidates.iter().take(MAX_DROPPED) {
        let mut first = selection.clone();
        first.insert("selected".into(), value.clone());
        let mut second = selection.clone();
        second.insert("target".into(), value.clone());
        choices.push(json!({"kind":"group","properties":{"label":value},"children":[
            {"kind":"link","properties":{"label":value,"href":home.href(value)}},
            {"kind":"list","properties":{"label":"Selection actions"},"children":[
                {"kind":"link","properties":{"label":"Select as first term","href":html_href(name,resource,&first)}},
                {"kind":"link","properties":{"label":"Select as second term","href":html_href(name,resource,&second)}}
            ]}
        ]}));
    }
    if !choices.is_empty() {
        out.push(serde_json::from_value(json!({"kind":"list","properties":{"label":"Select rendered terms (first 32 IRIs in this answer)"},"children":choices})).map_err(|e| refuse("selection",e))?);
    }
    for (action, config) in &lens.actions {
        if !writable(inv, &config.graph).await? {
            continue;
        }
        let resource = format!("{NAME}{name}:action:{action}");
        let rep = inv
            .issue(Request::new(
                Verb::Source,
                iri(&format!("{}{resource}", crate::FORM_RESOURCE))?,
            ))
            .await?;
        let mut form: Node = serde_json::from_slice(&rep.bytes).map_err(|e| refuse("action", e))?;
        prefill(&mut form, &selected);
        out.push(form);
    }
    Ok(out)
}

/// A value dropped into a personal operations view. Resource types are resolved under
/// the caller's capability; the client cannot assert a resource's types.
#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Dropped {
    /// A resource, whose RDF types are read through its Source under the caller's capability.
    Resource(Resource),
    /// A literal with an explicit datatype (xsd:string by default).
    Literal(Literal),
}
/// A dropped resource's identity.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Resource {
    /// The resource IRI.
    pub iri: String,
}
/// A dropped literal's lexical value and datatype.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Literal {
    /// Its lexical value.
    pub value: String,
    /// Its datatype; xsd:string when absent.
    #[serde(default = "string_type")]
    pub datatype: String,
}
fn string_type() -> String {
    format!("{XSD}string")
}

async fn typed(
    inv: &Invocation<'_>,
    dropped: &[Dropped],
) -> Result<Vec<(String, BTreeSet<String>)>, Error> {
    let mut values = Vec::new();
    for d in dropped {
        match d {
            Dropped::Literal(l) => {
                iri(&l.datatype)?;
                values.push((l.value.clone(), BTreeSet::from([l.datatype.clone()])));
            }
            Dropped::Resource(r) => {
                iri(&r.iri)?;
                let rep = inv.issue(Request::new(Verb::Source, iri(&r.iri)?)).await?;
                model::bound(&rep.bytes)?;
                let data = serde_json::from_slice::<Json>(&rep.bytes).ok();
                let mut types = BTreeSet::new();
                if let Some(data) =
                    data.filter(|d| d.get("@context").is_none() && d.get("@graph").is_none())
                {
                    if let Some(t) = data["@type"].as_str() {
                        iri(t)?;
                        types.insert(t.to_string());
                    }
                    if let Some(ts) = data["@type"].as_array() {
                        for t in ts {
                            let t = t.as_str().ok_or_else(|| {
                                refuse("dropped", "@type is an IRI or array of IRIs")
                            })?;
                            iri(t)?;
                            types.insert(t.to_string());
                        }
                    }
                } else {
                    // RDF syntax and JSON-LD contexts belong to the host's RDF reader,
                    // not a second parser in the UI. The resource remains a bound term.
                    let rep = inv.issue(Request::new(Verb::Source, iri("urn:sparql:select")?)
                        .with_arg("graph",ArgRef::Inline(r.iri.clone().into_bytes()))
                        .with_arg("query",ArgRef::Inline(b"SELECT ?focus ?type WHERE { GRAPH ?focus { ?focus a ?type } } LIMIT 65".to_vec()))
                        .with_arg("bindings",ArgRef::Inline(json!({"focus":{"type":"uri","value":r.iri}}).to_string().into_bytes()))).await?;
                    model::bound(&rep.bytes)?;
                    let rows = rows(&rep.bytes, 64)?;
                    if rows.past {
                        return Err(refuse("dropped","A dropped resource has more than 64 types; no operations were inferred."));
                    }
                    for row in rows.rows {
                        if let Some(t) = row.get("type") {
                            iri(t)?;
                            types.insert(t.clone());
                        }
                    }
                }
                values.push((r.iri.clone(), types));
            }
        }
    }
    Ok(values)
}

async fn meta(inv: &Invocation<'_>, at: &str) -> Result<Vec<u8>, Error> {
    let bytes = inv
        .issue(
            Request::new(Verb::Meta, iri(at)?)
                .with_arg("as", ArgRef::Inline(b"application/json".to_vec())),
        )
        .await?
        .bytes;
    model::bound(&bytes)?;
    Ok(bytes)
}

/// Pre-flight the exact submitted values. Core's `args` grammar has no escaping, so
/// delimiter-bearing values are refused explicitly instead of validating different values.
async fn preflight(
    inv: &Invocation<'_>,
    endpoint: &str,
    values: &BTreeMap<String, String>,
) -> Result<(), Error> {
    let description: Description =
        serde_json::from_slice(&meta(inv, endpoint).await?).map_err(|e| refuse("operation", e))?;
    let sink = description
        .action_specs()
        .into_iter()
        .find(|s| s.verb == Verb::Sink)
        .ok_or_else(|| refuse("operation", "the operation has no Sink"))?;
    let violations = sink.check(&ikigai_core::ProposedArgs::from_pairs(
        values.iter().map(|(k, v)| (k.clone(), v.clone())),
    ));
    if let Some(violation) = violations.first() {
        let name = match &violation.path {
            Some(ikigai_core::ViolationPath::Input(name)) => name.as_str(),
            _ => "operation",
        };
        return Err(refuse(
            name,
            format!("Pre-flight refused: {}", violation.message),
        ));
    }
    let mut pairs = Vec::new();
    for (k, v) in values {
        if k.contains(['=', '&', '\n']) || v.contains(['&', '\n']) {
            return Err(refuse(
                k,
                "kernel validation cannot yet represent ampersands or newlines in an argument",
            ));
        }
        pairs.push(format!("{k}={v}"));
    }
    let rep = inv
        .issue(
            Request::new(Verb::Source, iri("urn:kernel:validate")?)
                .with_arg("endpoint", ArgRef::Inline(endpoint.as_bytes().to_vec()))
                .with_arg("verb", ArgRef::Inline(b"sink".to_vec()))
                .with_arg("args", ArgRef::Inline(pairs.join("&").into_bytes())),
        )
        .await?;
    model::bound(&rep.bytes)?;
    if !String::from_utf8_lossy(&rep.bytes).contains("sh:conforms true") {
        return Err(refuse(
            "operation",
            format!(
                "Pre-flight refused: {}",
                String::from_utf8_lossy(&rep.bytes)
            ),
        ));
    }
    Ok(())
}

pub(crate) fn operation_endpoint() -> AsyncFnEndpoint {
    AsyncFnEndpoint::new("ui-lens-operation", |inv: &Invocation<'_>| -> InvokeFuture<'_> {
        Box::pin(async move {
            let endpoint = inv.bindings.get("iri").unwrap_or_default().to_string();
            if endpoint.starts_with(OPERATION) { return Err(refuse("operation", "an operation cannot invoke this wrapper recursively")); }
            if !offered(inv, &endpoint).await? { return Err(Error::Denied("this operation is no longer offered".into())); }
            let mut values = BTreeMap::new();
            for name in inv.request.args.keys() {
                values.insert(name.clone(), inv.inline_str(name)?.to_string());
            }
            preflight(inv, &endpoint, &values).await?;
            let mut request = Request::new(Verb::Sink, iri(&endpoint)?);
            for (name, value) in values { request.args.insert(name, ArgRef::Inline(value.into_bytes())); }
            inv.issue(request).await
        })
    }).with_description(Description::new("ui-lens-operation")
        .title("Validate and invoke an operation")
        .summary("Pre-flights submitted values through urn:kernel:validate before invoking the caller's Sink. The generated form retains that Sink's fields.")
        .verb(Verb::Sink)
        .input(ArgSpec::new("iri").class(XSD_ANY_URI).binding().example("urn:demo:contact")))
}

/// The operations composition. Its selection lives only in this request, so one
/// viewer cannot overwrite another viewer's dropped set. Results are never cached.
pub(crate) async fn operations(
    inv: &Invocation<'_>,
    resource: &str,
    chosen: &Selection,
) -> Result<Node, Error> {
    let raw = if inv.request.args.contains_key("dropped") {
        inv.inline_str("dropped")?
    } else {
        chosen.get("dropped").map(String::as_str).unwrap_or("[]")
    };
    model::bound(raw.as_bytes())?;
    let dropped: Vec<Dropped> = serde_json::from_str(raw).map_err(|e| refuse("dropped", e))?;
    if dropped.len() > MAX_DROPPED {
        return Err(refuse(
            "dropped",
            format!("At most {MAX_DROPPED} values may be dropped; nothing was changed."),
        ));
    }
    let mut children = vec![
        json!({"kind":"heading", "properties":{"content":"Operations", "level":2}}),
        json!({"kind":"text", "properties":{"content":format!("Drop resources or literals into your personal selection (at most {MAX_DROPPED}). Shows at most {MAX_OPERATIONS} operations in manifold rank order; additional operations are omitted with a notice. Nothing is shared or invoked by dropping.")}}),
        json!({"kind":"form", "properties":{"label":"Drop resources or literals", "method":"get", "action":html_href("operations", resource, &Selection::new())}, "children":[
            {"kind":"field", "properties":{"label":"Personal dropped set (JSON)","name":"dropped", "input":"text", "value":raw, "description":"Example: [{\"iri\":\"urn:person:alice\"},{\"value\":\"hello\",\"datatype\":\"http://www.w3.org/2001/XMLSchema#string\"}]"}},
            {"kind":"button","properties":{"label":"Find operations"}}
        ]}),
    ];
    children[2]["children"]
        .as_array_mut()
        .expect("drop controls")
        .insert(0, chooser_node(&["operations".into()], "operations"));
    let home = match optional(inv, HOME_TABLE).await? {
        Some(b) => Home::parse(&b)?,
        None => Home::default(),
    };
    let selection: Vec<Json> = dropped.iter().map(|d| match d {
        Dropped::Resource(r) => json!({"kind":"link", "properties":{"label":r.iri,"href":home.href(&r.iri)}}),
        Dropped::Literal(l) => json!({"kind":"text", "properties":{"content":format!("{} ({})",l.value,l.datatype)}}),
    }).collect();
    children.push(
        json!({"kind":"list", "properties":{"label":"Current selection"}, "children":selection}),
    );
    let values = match typed(inv, &dropped).await {
        Ok(v) => v,
        Err(Error::Denied(_)) => {
            children.push(json!({"kind":"text", "properties":{"content":"Selection withheld: you may not read one of the dropped resources. No operations were inferred."}}));
            return model::parse(
                &json!({"kind":"group","properties":{"label":"Operations"},"children":children})
                    .to_string()
                    .into_bytes(),
            );
        }
        Err(e) => return Err(e),
    };
    let types: BTreeSet<String> = values
        .iter()
        .flat_map(|(_, ts)| ts.iter().cloned())
        .collect();
    let mut forms = Vec::new();
    if !types.is_empty() {
        let rep = inv
            .issue(
                Request::new(Verb::Source, iri("urn:kernel:actions")?)
                    .with_arg("verb", ArgRef::Inline(b"sink".to_vec()))
                    .with_arg(
                        "types",
                        ArgRef::Inline(
                            types.into_iter().collect::<Vec<_>>().join(",").into_bytes(),
                        ),
                    ),
            )
            .await?;
        model::bound(&rep.bytes)?;
        for endpoint in String::from_utf8_lossy(&rep.bytes).lines().map(str::trim) {
            if endpoint.is_empty()
                || endpoint.contains(['{', '}'])
                || endpoint.starts_with(OPERATION)
            {
                continue;
            }
            let bytes = meta(inv, endpoint).await?;
            let description: Description =
                serde_json::from_slice(&bytes).map_err(|e| refuse("operation", e))?;
            let Some(sink) = description
                .action_specs()
                .into_iter()
                .find(|s| s.verb == Verb::Sink)
            else {
                continue;
            };
            let mut filled = BTreeMap::new();
            for input in sink
                .inputs
                .iter()
                .filter(|i| i.source != InputSource::Binding)
            {
                if let Some(class) = &input.class {
                    if let Some((value, _)) = values.iter().find(|(_, types)| types.contains(class))
                    {
                        filled.insert(input.name.clone(), value.clone());
                    }
                }
            }
            if filled.is_empty() {
                continue;
            }
            if forms.len() == MAX_OPERATIONS {
                children.push(json!({"kind":"text","properties":{"content":format!("More than {MAX_OPERATIONS} operations fit. Only the first {MAX_OPERATIONS} are shown; narrow your selection.")}}));
                break;
            }
            // Read the same generated form resource used everywhere else, then change
            // only its submission name to the pre-flight Sink.
            let rep = inv
                .issue(Request::new(
                    Verb::Source,
                    iri(&format!("{}{endpoint}", crate::FORM_RESOURCE))?,
                ))
                .await?;
            let mut form: Node =
                serde_json::from_slice(&rep.bytes).map_err(|e| refuse("operation", e))?;
            prefill(&mut form, &filled);
            form.properties.remove("action");
            form.properties.insert(
                "resource".into(),
                model::Value::Text(format!("{OPERATION}{endpoint}")),
            );
            forms.push(serde_json::to_value(form).map_err(|e| refuse("operation", e))?);
        }
    }
    if forms.is_empty() {
        children.push(json!({"kind":"text","properties":{"content":"No operations fit this selection under your capability."}}));
    }
    children.extend(forms);
    model::parse(
        &json!({"kind":"group","properties":{"label":"Operations"},"children":children})
            .to_string()
            .into_bytes(),
    )
}

/// The operations lens is a composition over the manifold, rather than a SPARQL renderer.
pub(crate) fn operations_endpoint() -> FnEndpoint {
    fixed(
        "ui-lens-operations",
        "Operations over a personal dropped set",
        "application/json",
        r#"{"composition":"urn:kernel:actions","personal":true,"bound":32,"view":"urn:iki:ui:view:lens:operations:resource:{iri}"}"#,
    )
}
