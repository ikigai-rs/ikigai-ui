//! The demo end to end, through its ikigai-web door (ledger #3356): the gallery answers, the
//! contact form is the one generated from the contract, a valid submission is stored, and an
//! invalid one is refused on its field.

use std::sync::Arc;

use ikigai_core::{Capability, Iri, Request, Verb};
use ikigai_ui_demo::{Store, SUBMISSION, SUBMISSION_ID, SUBMISSION_READ};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn call(addr: std::net::SocketAddr, method: &str, target: &str, body: &str) -> String {
    let request = format!(
        "{method} {target} HTTP/1.1\r\nHost: {addr}\r\nHX-Request: true\r\n\
         Content-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\
         Connection: close\r\n\r\n{body}",
        body.len()
    );
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut answer = String::new();
    stream.read_to_string(&mut answer).await.unwrap();
    answer
}

fn attribute<'a>(html: &'a str, name: &str) -> &'a str {
    let at = html
        .find(&format!(" {name}=\""))
        .unwrap_or_else(|| panic!("no {name} in {html}"));
    let rest = &html[at + name.len() + 3..];
    &rest[..rest.find('"').unwrap()]
}

#[tokio::test(flavor = "current_thread")]
async fn the_door_serves_the_gallery_and_a_working_contact_form() {
    let store: Store = Arc::default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(ikigai_ui_demo::serve(listener, store.clone()));

    // The gallery answers, at `/` and by name, in the shell.
    for target in ["/", "/page/gallery"] {
        let answer = call(addr, "GET", target, "").await;
        assert!(answer.starts_with("HTTP/1.1 200"), "{target}: {answer}");
        assert!(answer.contains("<title>Gallery</title>"), "{answer}");
        assert!(answer.contains("<h1>Gallery</h1>"), "{answer}");
    }
    // The shell's assets, where it links them.
    for target in ["/ui/tokens.css", "/ui/shell.js", "/ui/htmx"] {
        let answer = call(addr, "GET", target, "").await;
        assert!(answer.starts_with("HTTP/1.1 200"), "{target}: {answer}");
    }
    // The support page.
    let answer = call(addr, "GET", "/page/support", "").await;
    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
    assert!(answer.contains("<table"), "{answer}");

    // The contact form: generated from the contract, its fields the Sink's, never the carrier.
    let page = call(addr, "GET", "/page/contact", "").await;
    assert!(page.starts_with("HTTP/1.1 200"), "{page}");
    assert!(
        page.contains("generated from the service&#39;s model")
            || page.contains("generated from the service's model"),
        "{page}"
    );
    for field in ["name", "email", "topic", "message"] {
        assert!(
            page.contains(&format!("name=\"{field}\"")),
            "{field}: {page}"
        );
    }
    assert!(!page.contains("name=\"content\""), "{page}");
    let action = attribute(&page, "hx-post").to_string();
    assert_eq!(action, attribute(&page, "action"));

    // A valid submission, as a browser sends it (the body), is stored.
    let answer = call(
        addr,
        "POST",
        &action,
        "name=Ada+Lovelace&email=ada%40example.org&topic=question&message=Hello%21",
    )
    .await;
    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
    assert!(answer.contains(&format!("{SUBMISSION_ID}1")), "{answer}");
    {
        let held = store.lock().unwrap();
        assert_eq!(held.len(), 1);
        assert_eq!(held[0].name, "Ada Lovelace");
        assert_eq!(held[0].message, "Hello!");
    }

    // An invalid one is refused on its field, and nothing is stored.
    for (body, field) in [
        (
            "name=&email=ada%40example.org&topic=question&message=Hi",
            "name",
        ),
        (
            "name=Ada&email=not-an-address&topic=question&message=Hi",
            "email",
        ),
        (
            "name=Ada&email=ada%40example.org&topic=gossip&message=Hi",
            "topic",
        ),
        (
            "name=Ada&email=ada%40example.org&topic=question&message=",
            "message",
        ),
    ] {
        let answer = call(addr, "POST", &action, body).await;
        assert!(answer.starts_with("HTTP/1.1 400"), "{field}: {answer}");
        assert!(answer.contains(&format!("`{field}`")), "{field}: {answer}");
    }
    assert_eq!(store.lock().unwrap().len(), 1);

    // The submissions are not the door's to show.
    let answer = call(addr, "GET", &format!("/resolution/iri/{SUBMISSION}"), "").await;
    assert!(!answer.starts_with("HTTP/1.1 200"), "{answer}");

    // Through the kernel, with the read capability: the list and the one stored.
    let kernel = ikigai_ui_demo::kernel(ikigai_ui_demo::root(store.clone()));
    let cap = Capability::scoped(vec![SUBMISSION_READ.to_string()]);
    let one = kernel
        .issue(
            Request::new(
                Verb::Source,
                Iri::parse(format!("{SUBMISSION_ID}1")).unwrap(),
            ),
            &cap,
        )
        .await
        .unwrap();
    let one: serde_json::Value = serde_json::from_slice(&one.bytes).unwrap();
    assert_eq!(one["email"], "ada@example.org");
    let all = kernel
        .issue(
            Request::new(Verb::Source, Iri::parse(SUBMISSION).unwrap()),
            &cap,
        )
        .await
        .unwrap();
    let all: serde_json::Value = serde_json::from_slice(&all.bytes).unwrap();
    assert_eq!(all.as_array().unwrap().len(), 1);
}
