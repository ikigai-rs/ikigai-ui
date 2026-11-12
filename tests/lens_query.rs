//! A lens query as CODE AS A RESOURCE (ledger #3530): `urn:iki:ui:lens:name:{lens}:query` is the
//! source, `urn:iki:ui:lens:name:{lens}:prepared` its prepared form, reached through the
//! `urn:iki:ui:lens:prepare` transreptor, cached and golden-threaded from the source. An edit
//! through Sink cuts the prepared form and the results over it, and nothing else; an unchanged
//! query is prepared once. Its own test binary: [`lens::prepared_count`] is per process.

mod common;

use std::sync::{Arc, Mutex};

use futures_executor::block_on;
use ikigai_core::{
    ArgRef, Capability, Description, EndpointSpace, Error, Exact, FnEndpoint, Invocation, Iri,
    Kernel, ReprType, Representation, Request, Space, Verb,
};
use ikigai_ui::lens;
use ikigai_ui::scene::EXAMPLE_GRAPH;

const BEFORE: &str = "PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>\n\
SELECT ?focus ?node ?label WHERE { GRAPH ?focus { ?node rdfs:label ?label } } ORDER BY ?label";
const AFTER: &str = "PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>\n\
SELECT ?focus ?label WHERE { GRAPH ?focus { ?node rdfs:label ?label } } ORDER BY ?label";
const OTHER: &str = "SELECT ?focus ?s WHERE { GRAPH ?focus { ?s ?p ?o } } ORDER BY ?s";
const LENS: &str = r#"{"renderer": "table", "bound": 50, "legend": "Labels", "graphs": ["urn:iki:ui:example:graph:contact"]}"#;

/// A stateful query: Source reads it (cacheable, on the thread named after itself), Sink
/// replaces it with `content`.
fn cell(initial: &str) -> FnEndpoint {
    let value = Arc::new(Mutex::new(initial.to_string()));
    FnEndpoint::new("test-lens-query", move |cx: &Invocation<'_>| {
        match cx.request.verb {
            Verb::Source => Ok(Representation::new(
                ReprType::new(lens::QUERY),
                value.lock().unwrap().clone().into_bytes(),
            )
            .cacheable()
            .depends_on(cx.request.target.as_str())),
            Verb::Sink => {
                *value.lock().unwrap() = String::from_utf8(cx.inline_arg("content")?.to_vec())
                    .map_err(|e| Error::Endpoint(e.to_string()))?;
                Ok(Representation::new(
                    ReprType::new("text/plain"),
                    b"ok".to_vec(),
                ))
            }
            other => Err(Error::Endpoint(format!("unsupported {other:?}"))),
        }
    })
    .with_description(
        Description::new("test-lens-query")
            .title("An editable lens query")
            .summary("An editable lens query")
            .verb(Verb::Source)
            .verb(Verb::Sink)
            .output(lens::QUERY),
    )
}

fn fixed(id: &str, media: &'static str, bytes: &'static str) -> FnEndpoint {
    FnEndpoint::new(id, move |inv: &Invocation<'_>| {
        Ok(
            Representation::new(ReprType::new(media), bytes.as_bytes().to_vec())
                .cacheable()
                .depends_on(inv.request.target.as_str()),
        )
    })
    .with_description(
        Description::new(id)
            .title(id)
            .summary(id)
            .verb(Verb::Source)
            .output(media),
    )
}

fn host() -> Kernel {
    let space = EndpointSpace::new()
        .bind(
            Exact::new("urn:iki:ui:lens:name:edited"),
            fixed("lens-edited", "application/json", LENS),
        )
        .bind(
            Exact::new("urn:iki:ui:lens:name:edited:query"),
            cell(BEFORE),
        )
        .bind(
            Exact::new("urn:iki:ui:lens:name:other"),
            fixed("lens-other", "application/json", LENS),
        )
        .bind(
            Exact::new("urn:iki:ui:lens:name:other:query"),
            fixed("lens-other-query", lens::QUERY, OTHER),
        );
    common::host_with(None, vec![Arc::new(space) as Arc<dyn Space>])
}

fn source(kernel: &Kernel, at: &str) -> Result<Vec<u8>, Error> {
    block_on(kernel.issue(
        Request::new(Verb::Source, Iri::parse(at).unwrap()),
        &Capability::root(),
    ))
    .map(|r| r.bytes)
}

fn result(kernel: &Kernel, name: &str) -> serde_json::Value {
    let at = format!("urn:iki:ui:lens:name:{name}:focus:{EXAMPLE_GRAPH}");
    serde_json::from_slice(&source(kernel, &at).unwrap()).unwrap()
}

fn prepared(kernel: &Kernel, name: &str) -> lens::Prepared {
    let at = format!("urn:iki:ui:lens:name:{name}:prepared");
    serde_json::from_slice(&source(kernel, &at).unwrap()).unwrap()
}

fn view(kernel: &Kernel, name: &str) -> Result<Vec<u8>, Error> {
    source(
        kernel,
        &format!("urn:iki:ui:view:lens:{name}:resource:{EXAMPLE_GRAPH}"),
    )
}

fn sink(kernel: &Kernel, at: &str, content: &str) {
    block_on(
        kernel.issue(
            Request::new(Verb::Sink, Iri::parse(at).unwrap())
                .with_arg("content", ArgRef::Inline(content.as_bytes().to_vec())),
            &Capability::root(),
        ),
    )
    .unwrap();
}

fn cached(kernel: &Kernel, at: &str) -> bool {
    kernel.is_cached(
        &Request::new(Verb::Source, Iri::parse(at).unwrap()),
        &Capability::root(),
    )
}

#[test]
fn an_edit_through_sink_re_prepares_exactly_the_lens_over_it() {
    let kernel = host();
    let base = lens::prepared_count();

    // Read each lens's result, prepared form and view, twice: each query is prepared once.
    for _ in 0..2 {
        assert_eq!(
            result(&kernel, "edited")["head"]["vars"],
            serde_json::json!(["focus", "node", "label"])
        );
        result(&kernel, "other");
        view(&kernel, "edited").unwrap();
        view(&kernel, "other").unwrap();
    }
    assert_eq!(prepared(&kernel, "edited").query, BEFORE);
    assert_eq!(prepared(&kernel, "other").form, lens::Form::Select);
    assert_eq!(
        lens::prepared_count() - base,
        2,
        "an unchanged query is prepared once"
    );
    let other_result = format!("urn:iki:ui:lens:name:other:focus:{EXAMPLE_GRAPH}");
    let edited_result = format!("urn:iki:ui:lens:name:edited:focus:{EXAMPLE_GRAPH}");
    for at in [
        "urn:iki:ui:lens:name:edited:prepared",
        "urn:iki:ui:lens:name:other:prepared",
        edited_result.as_str(),
        other_result.as_str(),
    ] {
        assert!(cached(&kernel, at), "{at} is cached");
    }

    // The edit, through the kernel: its Sink cuts the query's thread.
    sink(&kernel, "urn:iki:ui:lens:name:edited:query", AFTER);
    assert!(
        !cached(&kernel, "urn:iki:ui:lens:name:edited:prepared"),
        "the edit cut the prepared form"
    );
    assert!(!cached(&kernel, &edited_result), "and the result over it");
    assert!(
        cached(&kernel, "urn:iki:ui:lens:name:other:prepared") && cached(&kernel, &other_result),
        "and nothing else"
    );

    assert_eq!(
        result(&kernel, "edited")["head"]["vars"],
        serde_json::json!(["focus", "label"]),
        "the result is the edited query's"
    );
    assert_eq!(prepared(&kernel, "edited").query, AFTER);
    result(&kernel, "other");
    view(&kernel, "edited").unwrap();
    view(&kernel, "other").unwrap();
    assert_eq!(
        lens::prepared_count() - base,
        3,
        "only the edited query was prepared again"
    );

    // An edit to an update is refused where it is prepared, before anything runs.
    sink(
        &kernel,
        "urn:iki:ui:lens:name:edited:query",
        "DELETE WHERE { ?s ?p ?o }",
    );
    assert!(matches!(
        view(&kernel, "edited"),
        Err(Error::InvalidArgument { .. })
    ));
    view(&kernel, "other").unwrap();
}

#[test]
fn a_service_is_resolved_and_checked_against_the_lens() {
    let p = lens::prepare(
        "PREFIX ex: <urn:ex:>\nSELECT * WHERE { SERVICE ex:remote { ?s ?p ?o } SERVICE SILENT <urn:ok> { ?s ?p ?o } }",
    )
    .unwrap();
    assert_eq!(p.services, vec!["urn:ex:remote", "urn:ok"]);
    assert!(!p.limit);
    assert!(p.check(&["urn:ok".into()]).is_err());
    assert_eq!(
        p.check(&["urn:ok".into(), "urn:ex:remote".into()]).unwrap(),
        lens::Form::Select
    );
    // A SERVICE through a variable, or an undeclared prefix, names no IRI a lens may list.
    for bad in [
        "SELECT * WHERE { SERVICE ?where { ?s ?p ?o } }",
        "SELECT * WHERE { SERVICE nope:x { ?s ?p ?o } }",
    ] {
        assert!(
            matches!(lens::prepare(bad), Err(Error::InvalidArgument { .. })),
            "{bad}"
        );
    }
    assert!(lens::prepare("ASK { ?s ?p ?o } LIMIT 1").unwrap().limit);
}
