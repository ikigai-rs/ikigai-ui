//! A form generated from a `urn:` contract submits through a real ikigai-web door (ledger
//! #3355): the door URL the html descriptor's submit rule gives the form is the route
//! `/resolution/iri/{+iri}` → `{+iri}`, and a POST to it reaches the resource's Sink.

mod common;

use std::sync::Arc;

use ikigai_ui::testkit::render;
use ikigai_web::{fixed_cap, EdgeConfig, Route, RouteTable};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test(flavor = "current_thread")]
async fn a_submit_through_the_door_reaches_the_sink() {
    let kernel = common::form_host(common::CONTACT_URN);
    let html = render::source(&kernel, "urn:iki:ui:html:screen:contact", &[]).unwrap();
    let action = common::attribute(&html, "hx-post").to_string();
    assert_eq!(action, common::attribute(&html, "action"));

    // The door route the submit rule names (ikigai_ui::target::SUBMIT).
    let route = Route {
        pattern: "/resolution/iri/{+iri}".into(),
        iri_template: "{+iri}".into(),
        redirect: None,
        cap: None,
        cors: None,
        csp: None,
    };
    let config = EdgeConfig {
        routes: RouteTable::new(vec![route]),
        ..EdgeConfig::default()
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let cap = fixed_cap(vec![common::CONTACT_WRITE.to_string()]);
    tokio::spawn(ikigai_web::serve_with_listener(
        Arc::new(kernel),
        cap,
        listener,
        config,
    ));

    let post = |target: String, body: &'static str| async move {
        let request = format!(
            "POST {target} HTTP/1.1\r\nHost: {addr}\r\nHX-Request: true\r\n\
             Content-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\
             Connection: close\r\n\r\n{body}",
            body.len()
        );
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut answer = String::new();
        stream.read_to_string(&mut answer).await.unwrap();
        answer
    };
    let fields = "name=Ada&email=mailto%3Aada%40example.org&message=Hello";

    // The fields as the query: the door forwards each as the Sink's argument of that name,
    // and the Sink of the resource the form names runs.
    let answer = post(format!("{action}?{fields}"), "").await;
    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
    assert!(answer.ends_with("sent"), "{answer}");
    assert_eq!(*common::SUNK.lock().unwrap(), ["Ada"]);

    // The fields as a browser sends them, in the body: ikigai-web 0.4.0 hands a Sink its body
    // as `content`, not as named arguments, so the same resource's contract refuses the
    // first required input by name, the refusal the shell lands on that field.
    let answer = post(action.clone(), fields).await;
    assert!(answer.starts_with("HTTP/1.1 400"), "{answer}");
    assert!(
        answer.ends_with("missing required argument `name`"),
        "{answer}"
    );
    assert_eq!(common::SUNK.lock().unwrap().len(), 1);
}
