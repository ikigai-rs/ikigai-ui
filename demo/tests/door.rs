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

    // The contact screen references the inbox; the door grants no read, so after a stored
    // submission a visitor still sees the withheld rung and nothing of the sender (ledger #3361).
    let page = call(addr, "GET", "/page/contact", "").await;
    assert!(page.starts_with("HTTP/1.1 200"), "{page}");
    assert!(page.contains("Recent messages are withheld"), "{page}");
    assert!(
        !page.contains("Ada Lovelace") && !page.contains("ada@example.org"),
        "{page}"
    );

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

    // Under the read grant the same screen shows the inbox in place of its reference.
    let face = kernel
        .issue(
            Request::new(
                Verb::Source,
                Iri::parse("urn:iki:ui:face:target:html:screen:contact").unwrap(),
            ),
            &cap,
        )
        .await
        .unwrap();
    let face = String::from_utf8(face.bytes).unwrap();
    assert!(face.contains("Latest message from Ada Lovelace"), "{face}");
    assert!(!face.contains("withheld"), "{face}");
}

/// A lensed view opens through the door as a full page in the shell (ledger #3538): at the
/// path its resource's view's "Lenses" form names, with the theme, the status region and the
/// scene renderer, so the graph lens's canvas draws; its own form's lens= and facets pick
/// another; and the fragment route still answers for an htmx swap.
#[tokio::test(flavor = "current_thread")]
async fn the_door_serves_a_lensed_view_as_a_page() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(ikigai_ui_demo::serve(listener, Store::default()));
    let graph = ikigai_ui::scene::EXAMPLE_GRAPH;

    // Where the resource's view's "Lenses" form opens it (tests/lens.rs checks the form).
    let href = ikigai_ui::lens::html_href("contact-graph", graph, &Default::default());
    assert_eq!(
        href,
        "/lens/name/contact-graph/resource/urn%3Aiki%3Aui%3Aexample%3Agraph%3Acontact"
    );

    let page = call(addr, "GET", &format!("{href}?theme=dark"), "").await;
    assert!(page.starts_with("HTTP/1.1 200"), "{page}");
    for shell in [
        "data-theme=\"dark\"",
        "<title>",
        "href=\"/ui/tokens.css\"",
        "src=\"/ui/shell.js\"",
        "src=\"/ui/scene-renderer.js\"",
        "<div id=\"status\" role=\"status\"",
        "<main id=\"main\"",
        "<a href=\"/page/gallery\">Gallery</a>",
        "<canvas",
    ] {
        assert!(page.contains(shell), "{shell} not in {page}");
    }
    // Unencoded, as a person types it, the same page.
    let typed = call(
        addr,
        "GET",
        &format!("/lens/name/contact-graph/resource/{graph}"),
        "",
    )
    .await;
    assert!(typed.starts_with("HTTP/1.1 200"), "{typed}");
    assert!(typed.contains("src=\"/ui/scene-renderer.js\""), "{typed}");
    // The view's form submits to the page it is on.
    let list = call(addr, "GET", &format!("{href}?lens=contact-list"), "").await;
    assert!(list.starts_with("HTTP/1.1 200"), "{list}");
    assert!(list.contains("type=\"checkbox\""), "{list}");
    // The fragment, for htmx: no shell.
    let fragment = call(
        addr,
        "GET",
        &ikigai_ui::lens::fragment_href("contact-graph", graph, &Default::default()),
        "",
    )
    .await;
    assert!(fragment.starts_with("HTTP/1.1 200"), "{fragment}");
    assert!(!fragment.contains("<main"), "{fragment}");
}

/// The demo's workspace opens through the door as a page in the shell (ledger #3967), at the
/// path its own "Panes" list links to: the tab list, the splitters, the panes filled from the
/// example lenses, the Layout form; and `select.graph=` moves the following panes.
#[tokio::test(flavor = "current_thread")]
async fn the_door_serves_the_workspace_as_a_page() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(ikigai_ui_demo::serve(listener, Store::default()));

    let page = call(addr, "GET", ikigai_ui_demo::WORKSPACE_PATH, "").await;
    assert!(page.starts_with("HTTP/1.1 200"), "{page}");
    for part in [
        "<title>Contacts workspace</title>",
        "href=\"/ui/base.css\"",
        "src=\"/ui/shell.js\"",
        "<div id=\"status\" role=\"status\"",
        "<h1>Contacts workspace</h1>",
        "<nav aria-label=\"Panes\">",
        &format!("href=\"{}#pane-table\"", ikigai_ui_demo::WORKSPACE_PATH),
        "<div role=\"tablist\"",
        "role=\"separator\"",
        "id=\"pane-table\"",
        "aria-label=\"Tabs: Dashboard, Table\"",
        "name=\"if-version\"",
    ] {
        assert!(page.contains(part), "{part} not in {page}");
    }
    assert!(
        !page.contains("is not shown under your capability"),
        "{page}"
    );
    // Percent-encoded, as one segment: the same page.
    let encoded = ikigai_ui_demo::WORKSPACE_PATH.replacen(
        ikigai_ui_demo::WORKSPACE,
        &ikigai_ui_demo::WORKSPACE.replace(':', "%3A"),
        1,
    );
    let same = call(addr, "GET", &encoded, "").await;
    assert!(same.starts_with("HTTP/1.1 200"), "{same}");
    assert!(same.contains("<h1>Contacts workspace</h1>"), "{same}");
    // Not a workspace: refused, not a page.
    let refused = call(addr, "GET", "/workspace/urn:iki:uidemo:contact", "").await;
    assert!(!refused.starts_with("HTTP/1.1 200"), "{refused}");
}

/// The panes' ids in the page's order: the DOM's reading order.
fn pane_order(page: &str) -> Vec<&str> {
    page.match_indices("<section class=\"ui-group ui-pane\" id=\"pane-")
        .map(|(at, m)| {
            let rest = &page[at + m.len()..];
            &rest[..rest.find('"').unwrap()]
        })
        .collect()
}

/// Every rearrangement is a write to the atom (ledger #3952, design section 5A): a `move` and a
/// `reorder` posted to the edit Sink, as the "Pane" menu's forms post them with no script, each
/// write the next version, answer the one sentence with a link back to the pane, and a second
/// viewer's next read shows the new layout, its DOM order and its "Panes" list matching. An edit
/// made against an old version is refused and writes nothing; a revert writes the first layout
/// again as a new version.
#[tokio::test(flavor = "current_thread")]
async fn a_rearrangement_is_a_write_a_second_viewer_sees() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(ikigai_ui_demo::serve(listener, Store::default()));
    let edit = format!(
        "/resolution/iri/{}",
        format!("{}:edit", ikigai_ui_demo::WORKSPACE).replace(':', "%3A")
    );
    let first = call(addr, "GET", ikigai_ui_demo::WORKSPACE_PATH, "").await;
    assert_eq!(pane_order(&first), ["list", "dashboard", "table"]);
    assert!(first.contains("name=\"if-version\" value=\"1\""), "{first}");

    let answer = call(
        addr,
        "POST",
        &edit,
        "if-version=1&op=move&pane=table&to=list&place=below",
    )
    .await;
    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
    assert!(
        answer.contains("<p role=\"status\" data-ui-said>Moved Table below Contacts.</p>"),
        "{answer}"
    );
    assert!(
        answer.contains(&format!(
            "href=\"{}#pane-table\"",
            ikigai_ui_demo::WORKSPACE_PATH
        )),
        "{answer}"
    );

    // Another viewer, a new connection: the new layout, at version 2.
    let second = call(addr, "GET", ikigai_ui_demo::WORKSPACE_PATH, "").await;
    assert!(
        second.contains("aria-label=\"Stacked: Contacts, Table\""),
        "{second}"
    );
    assert!(
        !second.contains("aria-label=\"Tabs:"),
        "a tab group of one is the pane alone"
    );
    assert_eq!(pane_order(&second), ["list", "table", "dashboard"]);
    let listed: Vec<&str> = second
        .match_indices("#pane-")
        .map(|(at, _)| {
            let rest = &second[at + 6..];
            &rest[..rest.find('"').unwrap()]
        })
        .take(3)
        .collect();
    assert_eq!(
        listed,
        ["list", "table", "dashboard"],
        "the Panes list matches"
    );
    assert!(second.contains("name=\"if-version\" value=\"2\""));

    // Made against version 1 again: refused, and nothing is written.
    let stale = call(
        addr,
        "POST",
        &edit,
        "if-version=1&op=move&pane=dashboard&to=list&place=tabs",
    )
    .await;
    assert!(!stale.starts_with("HTTP/1.1 200"), "{stale}");
    assert!(
        stale.contains("This workspace changed in another window."),
        "{stale}"
    );
    let still = call(addr, "GET", ikigai_ui_demo::WORKSPACE_PATH, "").await;
    assert_eq!(pane_order(&still), ["list", "table", "dashboard"]);

    // Into tabs, then a reorder: version 4.
    let tabs = call(
        addr,
        "POST",
        &edit,
        "if-version=2&op=move&pane=dashboard&to=list&place=tabs",
    )
    .await;
    assert!(
        tabs.contains("Put Dashboard in tabs with Contacts."),
        "{tabs}"
    );
    let reorder = call(
        addr,
        "POST",
        &edit,
        "if-version=3&op=reorder&pane=dashboard&index=0",
    )
    .await;
    assert!(
        reorder.contains("Moved Dashboard to tab 1 of 2."),
        "{reorder}"
    );
    let fourth = call(addr, "GET", ikigai_ui_demo::WORKSPACE_PATH, "").await;
    assert_eq!(pane_order(&fourth), ["dashboard", "list", "table"]);
    assert!(
        fourth.contains("aria-label=\"Tabs: Dashboard, Contacts\""),
        "{fourth}"
    );

    // The first version, written again as version 5.
    let revert = format!(
        "/resolution/iri/{}",
        format!("{}:revert", ikigai_ui_demo::WORKSPACE).replace(':', "%3A")
    );
    let back = call(addr, "POST", &revert, "to-version=1").await;
    assert!(
        back.contains("Reverted to version 1, saved as version 5."),
        "{back}"
    );
    let fifth = call(addr, "GET", ikigai_ui_demo::WORKSPACE_PATH, "").await;
    assert_eq!(pane_order(&fifth), ["list", "dashboard", "table"]);
    assert!(fifth.contains("name=\"if-version\" value=\"5\""));
}
