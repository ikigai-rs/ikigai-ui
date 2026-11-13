//! Real store and typed-operation fixtures shared by behavioral and HTML accessibility tests.
use futures::executor::block_on;
use ikigai_core::{ArgRef, Capability, Iri, Request, Verb};
use ikigai_core::{
    ArgSpec, AsyncFnEndpoint, Description, EndpointSpace, Error, Exact, FnEndpoint, Invocation,
    InvokeFuture, Kernel, ReprType, Representation,
};
use ikigai_ui::model::Node;
use ikigai_ui::{
    lens::{self, actions},
    model::Value,
};
use serde_json::json;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

pub(crate) const GRAPH: &str = "urn:test:vocabulary";
pub(crate) const PERSON: &str = "urn:test:Person";
pub(crate) const INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
pub(crate) const CLASS: &str = "http://www.w3.org/2000/01/rdf-schema#Class";
pub(crate) const SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
pub(crate) const CALL: &str = "urn:test:operation";
pub(crate) const INVOKE: &str = "urn:cap:test:invoke";

pub(crate) fn fixed(id: &'static str, body: String, media: &'static str) -> FnEndpoint {
    FnEndpoint::new(id, move |_: &Invocation<'_>| {
        Ok(Representation::new(ReprType::new(media), body.clone().into_bytes()).cacheable())
    })
    .with_description(
        Description::new(id)
            .title(id)
            .summary(id)
            .verb(Verb::Source)
            .output(media),
    )
}
pub(crate) fn issue(
    kernel: &Kernel,
    cap: &Capability,
    verb: Verb,
    at: &str,
    args: &[(&str, &str)],
) -> Result<Representation, Error> {
    let mut request = Request::new(verb, Iri::parse(at).unwrap());
    for (k, v) in args {
        request = request.with_arg(*k, ArgRef::Inline(v.as_bytes().to_vec()));
    }
    block_on(kernel.issue(request, cap))
}
pub(crate) fn nodes<'a>(n: &'a Node, kind: &str) -> Vec<&'a Node> {
    let mut out = Vec::new();
    if n.kind == kind {
        out.push(n);
    }
    for child in &n.children {
        out.extend(nodes(child, kind));
    }
    out
}
pub(crate) fn string<'a>(n: &'a Node, key: &str) -> &'a str {
    match n.properties.get(key) {
        Some(Value::Text(t)) => t,
        _ => "",
    }
}
pub(crate) fn model(rep: Representation) -> Node {
    serde_json::from_slice(&rep.bytes).unwrap()
}
pub(crate) fn writer() -> Capability {
    Capability::scoped([
        format!("urn:cap:store:read:graph:{GRAPH}"),
        format!("urn:cap:store:write:graph:{GRAPH}"),
    ])
}
pub(crate) fn action(name: &str) -> String {
    format!("{}vocabulary:action:{name}", lens::NAME)
}
pub(crate) fn vocabulary_view(kernel: &Kernel, cap: &Capability) -> Node {
    model(
        issue(
            kernel,
            cap,
            Verb::Source,
            &format!("{}vocabulary:resource:{GRAPH}", lens::VIEW),
            &[
                ("selected", "urn:test:Parent"),
                ("target", "urn:other:Term"),
            ],
        )
        .unwrap(),
    )
}
pub(crate) fn vocabulary(shapes: Option<&str>) -> Kernel {
    let graph = AsyncFnEndpoint::new(
        "test-vocabulary-graph",
        |inv: &Invocation<'_>| -> InvokeFuture<'_> {
            Box::pin(async move {
                inv.issue(
                    Request::new(
                        Verb::Source,
                        Iri::parse("urn:iki:store:graph-construct").unwrap(),
                    )
                    .with_arg("graph", ArgRef::Inline(GRAPH.as_bytes().to_vec()))
                    .with_arg(
                        "query",
                        ArgRef::Inline(b"CONSTRUCT {?s ?p ?o} WHERE {?s ?p ?o}".to_vec()),
                    )
                    .with_arg("as", ArgRef::Inline(b"text/turtle".to_vec())),
                )
                .await
            })
        },
    )
    .with_description(
        Description::new("test-vocabulary-graph")
            .title("Vocabulary")
            .summary("The store graph, read under the caller's grant.")
            .verb(Verb::Source)
            .output("text/turtle"),
    );
    let mut host = EndpointSpace::new()
        .bind(Exact::new(format!("{}{GRAPH}",lens::INSTANCE)),fixed("test-vocabulary-instance",r#"["vocabulary"]"#.into(),"application/json"))
        .bind(Exact::new(GRAPH), graph)
        .bind(
            Exact::new(format!("{}vocabulary", lens::NAME)),
            fixed(
                "test-vocabulary-lens",
                json!({
                    "renderer":"table", "bound":10, "legend":"Vocabulary", "graphs":[GRAPH],
                    "actions":{"extend":{"graph":GRAPH}, "connect":{"graph":GRAPH}}
                })
                .to_string(),
                "application/json",
            ),
        )
        .bind(
            Exact::new(format!("{}vocabulary:query", lens::NAME)),
            fixed(
                "test-vocabulary-query",
                format!("SELECT ?focus ?node WHERE {{ GRAPH ?focus {{ ?node a <{CLASS}> }} }} ORDER BY ?node"),
                "application/sparql-query",
            ),
        )
        .bind(
            Exact::new(lens::HOME_TABLE),
            fixed(
                "test-home",
                r#"[{"prefix":"urn:test:","href":"/terms/"}]"#.into(),
                "application/json",
            ),
        );
    if let Some(shapes) = shapes {
        host = host.bind(
            Exact::new(format!("{}{GRAPH}", lens::SHAPES)),
            fixed("test-shapes", shapes.into(), "text/turtle"),
        );
    }
    let kernel = super::host_with(
        None,
        vec![
            Arc::new(host),
            Arc::new(ikigai_store::space(
                ikigai_store::DurableStore::in_memory().unwrap(),
            )),
        ],
    );
    issue(
        &kernel,
        &Capability::root(),
        Verb::Sink,
        actions::GRAPH_UPDATE,
        &[
            ("graph", GRAPH),
            (
                "content",
                &format!("INSERT DATA {{GRAPH <{GRAPH}> {{<urn:test:Parent> a <{CLASS}>}}}}"),
            ),
        ],
    )
    .unwrap();
    kernel
}
pub(crate) fn ask(kernel: &Kernel, pattern: &str) -> bool {
    let r = issue(
        kernel,
        &Capability::root(),
        Verb::Source,
        "urn:iki:store:graph-ask",
        &[
            ("graph", GRAPH),
            ("query", &format!("ASK {{ {pattern} }}")),
            ("as", "application/sparql-results+json"),
        ],
    )
    .unwrap();
    serde_json::from_slice::<serde_json::Value>(&r.bytes).unwrap()["boolean"] == true
}

pub(crate) type Calls = Arc<Mutex<Vec<BTreeMap<String, String>>>>;
pub(crate) fn operations_host() -> (Kernel, Calls) {
    let calls = Calls::default();
    let received = calls.clone();
    let operation = FnEndpoint::new("test-operation", move |inv: &Invocation<'_>| {
        received.lock().unwrap().push(
            inv.request
                .args
                .keys()
                .map(|k| (k.clone(), inv.inline_str(k).unwrap().to_string()))
                .collect(),
        );
        Ok(Representation::new(
            ReprType::new("text/plain"),
            b"Done".to_vec(),
        ))
    })
    .with_description(
        Description::new("test-operation")
            .title("Repeat a greeting")
            .summary("A typed operation.")
            .verb(Verb::Sink)
            .requires(INVOKE)
            .input(ArgSpec::new("person").class(PERSON))
            .input(ArgSpec::new("times").class(INTEGER)),
    );
    let host = EndpointSpace::new()
        .bind(
            Exact::new("urn:test:Turtle"),
            fixed(
                "test-turtle",
                format!("<urn:test:Turtle> a <{PERSON}> ."),
                "text/turtle",
            ),
        )
        .bind(
            Exact::new("urn:test:Context"),
            fixed(
                "test-context",
                r#"{"@context":{"ex":"urn:test:"},"@id":"urn:test:Context","@type":"ex:Person"}"#
                    .into(),
                "application/ld+json",
            ),
        )
        .bind(Exact::new(CALL), operation)
        .bind(
            Exact::new("urn:test:Alice"),
            fixed(
                "test-alice",
                json!({"@id":"urn:test:Alice","@type":PERSON}).to_string(),
                "application/ld+json",
            ),
        )
        .bind(
            Exact::new("urn:test:Hidden"),
            fixed(
                "test-hidden",
                json!({"@type":PERSON}).to_string(),
                "application/ld+json",
            )
            .with_description(
                Description::new("test-hidden")
                    .verb(Verb::Source)
                    .requires("urn:cap:test:secret"),
            ),
        );
    (super::host_with(None, vec![Arc::new(host)]), calls)
}
pub(crate) fn dropped(times: &str) -> String {
    json!([{"iri":"urn:test:Alice"},{"value":times,"datatype":INTEGER}]).to_string()
}
pub(crate) fn operations(kernel: &Kernel, cap: &Capability, raw: &str) -> Node {
    model(
        issue(
            kernel,
            cap,
            Verb::Source,
            "urn:iki:ui:view:lens:operations:resource:urn:viewer:personal",
            &[("dropped", raw)],
        )
        .unwrap(),
    )
}
