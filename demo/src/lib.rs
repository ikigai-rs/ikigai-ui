//! `ikigai-ui-demo`: the UI demo host (ledger #3356). One kernel, one ikigai-web door:
//!
//! | name | kind | what it is |
//! |---|---|---|
//! | `urn:iki:uidemo:contact` | the service | a Sink: name, email, topic ([`TOPICS`]), message; validates, stores |
//! | `urn:iki:uidemo:submission` | data | a Source: every stored submission, as JSON |
//! | `urn:iki:uidemo:submission:id:{id}` | data | one submission, the demo's only stored state |
//! | `urn:iki:ui:screen:name:{screen}:source` | resource | a screen's outline ([`SCREENS`]): `contact` (the form, generated from the contract) and `write` (the same in short names) |
//! | `urn:iki:ui:screen:name:{screen}` | composition | the outline compiled by ikigai-ui ([`ikigai_ui::screen`]) |
//! | `urn:iki:uidemo:inbox` | data | a UI node: the latest submission's sender, readable only under [`SUBMISSION_READ`]; the contact screen references it, so a caller without the grant sees it withheld ([`INBOX_WITHHELD`]) |
//! | `urn:iki:uidemo:prefixes` | data | the demo's prefix table, `demo:` for `urn:iki:uidemo:` ([`PREFIXES`]) |
//! | `urn:iki:uidemo:example:rating` | data | a rating element node, which `write` references as `demo:example:rating` |
//! | `urn:iki:uidemo:page:{name}` | composition | the shell over a fragment ([`ikigai_ui::Pages`]): `gallery`, `elements`, `contact`, `support` |
//! | `urn:iki:uidemo:fragment:{name}` | composition | the page's body: a note and the screen it shows |
//! | `urn:iki:uidemo:page:lens:{lens}:resource:{iri}` | composition | a lensed view of `{iri}` in the shell ([`ikigai_ui::Pages::lenses`]); the door's `/lens/name/{lens}/resource/{iri}` |
//! | `urn:iki:ui:workspace:person:urn:person:demo:name:contacts` | data | the demo's one workspace atom ([`WORKSPACE`]), a fixture over the example lenses |
//! | `…:name:contacts:edit`, `…:revert`, `…:version:{n}`, `…:addable` | service, data | the workspace's edit and revert Sinks over an in-memory store of every version ([`Layouts`], ledger #3952), and each version |
//! | `urn:iki:uidemo:page:workspace:{iri}` | composition | a workspace in the shell ([`ikigai_ui::Pages::workspaces`]); the door's `/workspace/{iri}` ([`WORKSPACE_PATH`]) |
//!
//! The contact form is not written by hand: it is the `form` node ikigai-ui generates from the
//! contact resource's own contract (the same model its OpenAPI description is projected from),
//! rendered by the html target and submitted to the Sink that contract describes.

#![forbid(unsafe_code)]

use std::sync::{Arc, Mutex};

use ikigai_core::{
    ActionSpec, ArgRef, ArgSpec, AsyncFnEndpoint, Description, EndpointSpace, Error, Exact,
    Fallback, FnEndpoint, Invocation, InvokeFuture, Iri, Kernel, ReprType, Representation, Request,
    Space, UriTemplate, Verb,
};
use ikigai_fn::html::{form_pairs, html_text};
#[cfg(not(target_arch = "wasm32"))]
use ikigai_web::{fixed_cap, CapFn, EdgeConfig, Route, RouteTable};

/// The contact resource: its Sink takes a message.
pub const CONTACT: &str = "urn:iki:uidemo:contact";
/// Every stored submission.
pub const SUBMISSION: &str = "urn:iki:uidemo:submission";
/// One stored submission: `urn:iki:uidemo:submission:id:{id}`.
pub const SUBMISSION_ID: &str = "urn:iki:uidemo:submission:id:";
/// What a write to [`CONTACT`] requires; the door grants it.
pub const CONTACT_WRITE: &str = "urn:cap:uidemo:contact:write";
/// What reading the submissions requires; the door does NOT grant it (who wrote what is not
/// public, and sign-in comes later, #2169).
pub const SUBMISSION_READ: &str = "urn:cap:uidemo:submission:read";
/// The latest submission's sender, as a UI node: read only under [`SUBMISSION_READ`].
pub const INBOX: &str = "urn:iki:uidemo:inbox";
/// What the contact screen shows of [`INBOX`] to a caller without the grant: its reference's
/// fallback text (ledger #3361). The door grants no read, so every visitor sees this.
pub const INBOX_WITHHELD: &str =
    "Recent messages are withheld: reading them needs the submission read grant.";
/// The values `topic` takes.
pub const TOPICS: [&str; 3] = ["question", "feedback", "support"];
/// The demo's pages.
pub const PAGES: [(&str, &str); 4] = [
    ("gallery", "Gallery"),
    ("elements", "Elements"),
    ("contact", "Contact"),
    ("support", "Support"),
];
/// The demo's one workspace (ledger #3967): one fixed person, as there is no sign-in yet
/// (#2169). Its atom ([`WORKSPACE_ATOM`]) tiles the example lenses over the contact graph.
pub const WORKSPACE: &str = "urn:iki:ui:workspace:person:urn:person:demo:name:contacts";
/// The workspace atom: the contact list beside a tab list of the dashboard and the table, each
/// pane following `graph`.
pub const WORKSPACE_ATOM: &str = include_str!("../assets/workspace.json");
/// What the demo's workspace can open in a new pane (ledger #3953): its `addable` list, the
/// three example contact lenses over the selected graph.
pub const WORKSPACE_ADDABLE: &str = include_str!("../assets/addable.json");
/// Where the door serves [`WORKSPACE`]: the path its "Panes" list links to.
pub const WORKSPACE_PATH: &str =
    "/workspace/urn:iki:ui:workspace:person:urn:person:demo:name:contacts";
/// The page family's base.
pub const BASE: &str = "urn:iki:uidemo:";
/// What the contact page says it demonstrates.
pub const GENERATED_NOTE: &str = "This form is a client generated from the service's model, \
not written by hand. The model is the contact service's own contract, the one its OpenAPI \
description is projected from; the same generation works from an OpenAPI document or another \
model. Submitting it calls the Sink that contract describes.";

const HTML: &str = "text/html; charset=utf-8";
const JSON: &str = "application/json";
const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
const MAX_FIELD: usize = 200;
const MAX_MESSAGE: usize = 5000;

/// One accepted message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Submission {
    /// Its id, from 1.
    pub id: u64,
    /// Who wrote.
    pub name: String,
    /// Where to answer.
    pub email: String,
    /// One of [`TOPICS`].
    pub topic: String,
    /// What they said.
    pub message: String,
}

impl Submission {
    fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "@id": format!("{SUBMISSION_ID}{}", self.id),
            "id": self.id,
            "name": self.name,
            "email": self.email,
            "topic": self.topic,
            "message": self.message,
        })
    }
}

/// The demo's stored state: the submissions, in order.
pub type Store = Arc<Mutex<Vec<Submission>>>;

fn invalid(name: &str, detail: impl Into<String>) -> Error {
    Error::InvalidArgument {
        name: name.into(),
        detail: detail.into(),
    }
}

/// The fields of a call: named arguments, or the body (`content`) a form POST carries,
/// `application/x-www-form-urlencoded` or a JSON object.
fn fields(inv: &Invocation<'_>) -> Result<Vec<(String, String)>, Error> {
    let mut named = Vec::new();
    for name in ["name", "email", "topic", "message"] {
        if let Some(value) = inv.optional_str(name)? {
            named.push((name.to_string(), value.to_string()));
        }
    }
    const FORM: &str = "application/x-www-form-urlencoded";
    let media_type = inv
        .optional_str("content-type")?
        .map(|s| s.split_once(';').map_or(s, |(base, _)| base).trim())
        .unwrap_or(FORM);
    let json = media_type.eq_ignore_ascii_case(JSON);
    if !json && !media_type.eq_ignore_ascii_case(FORM) {
        return Err(invalid("content-type", format!("use {FORM} or {JSON}")));
    }
    let Some(body) = inv.optional_str("content")? else {
        return Ok(named);
    };
    if !named.is_empty() {
        return Err(invalid(
            "content",
            "the body is given twice: as `content` and as its fields; give one",
        ));
    }
    if json {
        let v: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(body).map_err(|e| invalid("content", format!("{e}")))?;
        return Ok(v
            .into_iter()
            .filter_map(|(k, v)| v.as_str().map(|s| (k, s.to_string())))
            .collect());
    }
    form_pairs(body).map_err(|e| invalid("content", e.to_string()))
}

/// The submission the fields describe, or `InvalidArgument` naming the first field that is
/// wrong.
pub fn validate(fields: &[(String, String)]) -> Result<Submission, Error> {
    let get = |k: &str| {
        fields
            .iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.trim().to_string())
            .unwrap_or_default()
    };
    let (name, email, topic, message) = (get("name"), get("email"), get("topic"), get("message"));
    if name.is_empty() {
        return Err(invalid("name", "say who you are"));
    }
    if name.chars().count() > MAX_FIELD {
        return Err(invalid("name", format!("at most {MAX_FIELD} characters")));
    }
    let email_ok = email.chars().count() <= MAX_FIELD
        && !email.chars().any(char::is_whitespace)
        && matches!(email.split_once('@'), Some((l, d)) if !l.is_empty() && d.contains('.') && !d.contains('@') && !d.starts_with('.') && !d.ends_with('.'));
    if !email_ok {
        return Err(invalid("email", "an address like ada@example.org"));
    }
    if !TOPICS.contains(&topic.as_str()) {
        return Err(invalid("topic", format!("one of {}", TOPICS.join(", "))));
    }
    if message.is_empty() {
        return Err(invalid("message", "say what you want to say"));
    }
    if message.chars().count() > MAX_MESSAGE {
        return Err(invalid(
            "message",
            format!("at most {MAX_MESSAGE} characters"),
        ));
    }
    Ok(Submission {
        id: 0,
        name,
        email,
        topic,
        message,
    })
}

fn contact_description() -> Description {
    let field = |name: &str, summary: &str| ArgSpec::new(name).summary(summary).class(XSD_STRING);
    Description::new("uidemo-contact")
        .title("Contact")
        .summary("The demo's contact service.")
        .verb(Verb::Sink)
        .verb(Verb::Meta)
        .action(
            ActionSpec::new(Verb::Sink)
                .summary("Send us a message.")
                .requires(CONTACT_WRITE)
                .input(field("name", "Who is writing."))
                .input(field("email", "Where to answer you."))
                .input(field("topic", "What it is about.").one_of(TOPICS))
                .input(
                    ArgSpec::new("message")
                        .summary("What you want to say.")
                        .class("https://schema.org/Text"),
                )
                // The body a form POST is: the fields above arrive inside it.
                .input(ArgSpec::new("content").class(XSD_STRING).summary(
                    "the submission itself: a urlencoded or JSON body carrying the fields above",
                ))
                .envelope("content")
                .output(HTML),
        )
}

fn contact(store: Store) -> FnEndpoint {
    FnEndpoint::new("uidemo-contact", move |inv: &Invocation<'_>| {
        let mut submission = validate(&fields(inv)?)?;
        let mut held = store
            .lock()
            .map_err(|_| Error::Endpoint("poisoned".into()))?;
        submission.id = held.len() as u64 + 1;
        let body = format!(
            "<p role=\"status\">Thank you, {}. Your message is stored as {}.</p>",
            html_text(&submission.name),
            html_text(&format!("{SUBMISSION_ID}{}", submission.id)),
        );
        held.push(submission);
        Ok(Representation::new(ReprType::new(HTML), body.into_bytes()))
    })
    .with_description(contact_description())
}

fn submissions(store: Store) -> FnEndpoint {
    FnEndpoint::new("uidemo-submission", move |_: &Invocation<'_>| {
        let held = store
            .lock()
            .map_err(|_| Error::Endpoint("poisoned".into()))?;
        let all: Vec<_> = held.iter().map(Submission::json).collect();
        Ok(Representation::new(
            ReprType::new(JSON),
            serde_json::to_vec(&all).map_err(|e| Error::Endpoint(e.to_string()))?,
        ))
    })
    .with_description(
        Description::new("uidemo-submission")
            .title("Submissions")
            .summary("Every stored submission, in order, as JSON.")
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .requires(SUBMISSION_READ)
            .output(JSON),
    )
}

fn submission(store: Store) -> FnEndpoint {
    FnEndpoint::new("uidemo-submission-id", move |inv: &Invocation<'_>| {
        let id = inv.bindings.get("id").unwrap_or_default();
        let held = store
            .lock()
            .map_err(|_| Error::Endpoint("poisoned".into()))?;
        let found = id
            .parse::<u64>()
            .ok()
            .and_then(|n| held.iter().find(|s| s.id == n))
            .ok_or_else(|| Error::NotFound(format!("no submission `{id}`")))?;
        Ok(Representation::new(
            ReprType::new(JSON),
            serde_json::to_vec(&found.json()).map_err(|e| Error::Endpoint(e.to_string()))?,
        ))
    })
    .with_description(
        Description::new("uidemo-submission-id")
            .title("A submission")
            .summary("One stored submission, as JSON.")
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .requires(SUBMISSION_READ)
            .input(ArgSpec::new("id").class(XSD_INTEGER).binding())
            .output(JSON),
    )
}

fn inbox(store: Store) -> FnEndpoint {
    FnEndpoint::new("uidemo-inbox", move |_: &Invocation<'_>| {
        let held = store
            .lock()
            .map_err(|_| Error::Endpoint("poisoned".into()))?;
        let content = match held.last() {
            Some(s) => format!(
                "Latest message from {} ({}), about {}.",
                s.name, s.email, s.topic
            ),
            None => "No messages yet.".to_string(),
        };
        let node = serde_json::json!({"kind": "text", "properties": {"content": content}});
        Ok(Representation::new(
            ReprType::new(ikigai_ui::model::MEDIA_TYPE),
            serde_json::to_vec(&node).map_err(|e| Error::Endpoint(e.to_string()))?,
        ))
    })
    .with_description(
        Description::new("uidemo-inbox")
            .title("The inbox")
            .summary("The latest submission's sender, as a UI text node.")
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .requires(SUBMISSION_READ)
            .output(ikigai_ui::model::MEDIA_TYPE),
    )
}

/// The demo's workspace store (ledger #3952): every version of [`WORKSPACE`]'s atom, version
/// `n` at `n - 1`, never rewritten. It starts at [`WORKSPACE_ATOM`] as version 1. In memory: a
/// restart starts again from the fixture.
pub type Layouts = Arc<Mutex<Vec<Vec<u8>>>>;

/// What a write to the demo's workspace requires (design section 1): the person's own
/// partition. The door grants it, as the demo has one fixed person and no sign-in (#2169).
pub const WORKSPACE_WRITE: &str = "urn:cap:ui:workspace:write:person:urn:person:demo";

fn layouts() -> Layouts {
    Arc::new(Mutex::new(vec![WORKSPACE_ATOM.as_bytes().to_vec()]))
}

fn held(layouts: &Layouts) -> Result<std::sync::MutexGuard<'_, Vec<Vec<u8>>>, Error> {
    layouts
        .lock()
        .map_err(|_| Error::Endpoint("poisoned".into()))
}

/// One version as the store answers it: the atom, its type saying which version it is, never
/// cached (the next edit changes the bare name's answer).
fn version_rep(n: usize, bytes: &[u8]) -> Representation {
    Representation::new(
        ReprType::new(JSON).with_param("version", n.to_string()),
        bytes.to_vec(),
    )
}

fn workspace_atom(layouts: Layouts) -> FnEndpoint {
    FnEndpoint::new("uidemo-workspace", move |_: &Invocation<'_>| {
        let held = held(&layouts)?;
        Ok(version_rep(held.len(), held.last().expect("version 1")))
    })
    .with_description(
        Description::new("uidemo-workspace")
            .title("The demo's workspace")
            .summary(
                "A workspace atom (fixtures only): the contact list beside a tab list of the \
                 dashboard and the table, each pane following the selected graph. The latest \
                 version, its type saying which (application/json; version=n).",
            )
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .output(JSON),
    )
}

fn workspace_version(layouts: Layouts) -> FnEndpoint {
    FnEndpoint::new("uidemo-workspace-version", move |inv: &Invocation<'_>| {
        let n = inv.bindings.get("n").unwrap_or_default();
        let held = held(&layouts)?;
        let at = n
            .parse::<usize>()
            .ok()
            .filter(|n| (1..=held.len()).contains(n))
            .ok_or_else(|| Error::NotFound(format!("no version `{n}`")))?;
        Ok(version_rep(at, &held[at - 1]))
    })
    .with_description(
        Description::new("uidemo-workspace-version")
            .title("One version of the demo's workspace")
            .summary("One saved version of the workspace atom, never rewritten.")
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .input(ArgSpec::new("n").class(XSD_INTEGER).binding())
            .output(JSON),
    )
}

/// A Sink's form body, as pairs.
fn body_pairs(inv: &Invocation<'_>) -> Result<std::collections::BTreeMap<String, String>, Error> {
    let body = inv
        .inline_arg("content")
        .map_err(|_| invalid("content", "no body"))?;
    let body = std::str::from_utf8(body).map_err(|_| invalid("content", "not UTF-8"))?;
    Ok(form_pairs(body)
        .map_err(|e| invalid("content", e.to_string()))?
        .into_iter()
        .collect())
}

/// `if-version` against the latest: an edit made against an older version is refused.
fn check_version(
    args: &std::collections::BTreeMap<String, String>,
    latest: usize,
) -> Result<(), Error> {
    match args.get("if-version").map(|v| v.trim().parse::<usize>()) {
        Some(Ok(n)) if n == latest => Ok(()),
        Some(Ok(_)) => Err(invalid(
            "if-version",
            "This workspace changed in another window. Showing the latest; your last change \
             was not saved.",
        )),
        _ => Err(invalid(
            "if-version",
            "the edit says no version it was made against",
        )),
    }
}

fn workspace_edit(layouts: Layouts) -> FnEndpoint {
    FnEndpoint::new("uidemo-workspace-edit", move |inv: &Invocation<'_>| {
        let args = body_pairs(inv)?;
        let mut held = held(&layouts)?;
        check_version(&args, held.len())?;
        let latest = ikigai_ui::workspace::validate(held.last().expect("version 1"))?;
        let list = ikigai_ui::workspace::addable(WORKSPACE_ADDABLE.as_bytes())?;
        let edited = ikigai_ui::workspace::apply_with(&latest, &args, &list)?;
        held.push(ikigai_ui::workspace::atom(&edited.workspace));
        let body = ikigai_ui::workspace::edited_html(
            WORKSPACE,
            edited.pane.as_deref(),
            &edited.said,
        );
        Ok(Representation::new(ReprType::new(HTML), body.into_bytes()))
    })
    .with_description(edit_description(
        "uidemo-workspace-edit",
        "Edit the workspace's layout",
        "Applies one edit (resize, move, reorder, collapse, expand, close or add) to the latest version and writes the next \
         one; refused, naming why, when if-version is not the latest or the result breaks a \
         bound.",
    ))
}

fn workspace_revert(layouts: Layouts) -> FnEndpoint {
    FnEndpoint::new("uidemo-workspace-revert", move |inv: &Invocation<'_>| {
        let args = body_pairs(inv)?;
        let mut held = held(&layouts)?;
        let to = args
            .get("to-version")
            .and_then(|v| v.trim().parse::<usize>().ok())
            .filter(|n| (1..=held.len()).contains(n))
            .ok_or_else(|| invalid("to-version", "not a version of this workspace"))?;
        let bytes = held[to - 1].clone();
        held.push(bytes);
        let body = ikigai_ui::workspace::edited_html(
            WORKSPACE,
            None,
            &format!("Reverted to version {to}, saved as version {}.", held.len()),
        );
        Ok(Representation::new(ReprType::new(HTML), body.into_bytes()))
    })
    .with_description(edit_description(
        "uidemo-workspace-revert",
        "Revert the workspace",
        "Writes the next version with the layout of an earlier one, to-version. History is \
         never rewritten.",
    ))
}

/// `{WORKSPACE}:addable`: the demo's [`WORKSPACE_ADDABLE`] list, as JSON.
fn workspace_addable() -> FnEndpoint {
    FnEndpoint::new("uidemo-workspace-addable", |_: &Invocation<'_>| {
        Ok(Representation::new(
            ReprType::new(JSON),
            WORKSPACE_ADDABLE.as_bytes().to_vec(),
        ))
    })
    .with_description(
        Description::new("uidemo-workspace-addable")
            .title("What the demo's workspace can open")
            .summary(
                "The workspace's addable list (fixtures only): the example contact lenses over \
                 the selected graph. The workspace screen offers only those the viewer can read.",
            )
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .output(JSON),
    )
}

fn edit_description(id: &str, title: &str, summary: &str) -> Description {
    Description::new(id)
        .title(title)
        .summary(summary)
        .verb(Verb::Sink)
        .verb(Verb::Meta)
        .action(
            ActionSpec::new(Verb::Sink)
                .summary(summary)
                .requires(WORKSPACE_WRITE)
                .input(ArgSpec::new("content").class(XSD_STRING).summary(
                    "the edit: a urlencoded body with op, if-version and the op's own arguments",
                ))
                .envelope("content")
                .output(HTML),
        )
}

/// The service: the contact Sink and the submissions, over `store`; and the workspace atom.
pub fn service(store: Store) -> EndpointSpace {
    let layouts = layouts();
    EndpointSpace::new()
        .bind(Exact::new(WORKSPACE), workspace_atom(layouts.clone()))
        .bind(
            Exact::new(format!("{WORKSPACE}:edit")),
            workspace_edit(layouts.clone()),
        )
        .bind(
            Exact::new(format!("{WORKSPACE}:addable")),
            workspace_addable(),
        )
        .bind(
            Exact::new(format!("{WORKSPACE}:revert")),
            workspace_revert(layouts.clone()),
        )
        .bind(
            UriTemplate::parse(format!("{WORKSPACE}:version:{{n}}")).expect("a template"),
            workspace_version(layouts),
        )
        .bind(Exact::new(CONTACT), contact(store.clone()))
        .bind(Exact::new(INBOX), inbox(store.clone()))
        .bind(Exact::new(SUBMISSION), submissions(store.clone()))
        .bind(
            UriTemplate::parse(format!("{SUBMISSION_ID}{{id}}")).expect("a template"),
            submission(store),
        )
}

fn html(body: Vec<u8>) -> Representation {
    Representation::new(ReprType::new(HTML), body)
}

fn page_name(inv: &Invocation<'_>) -> Result<(&'static str, &'static str), Error> {
    let name = inv.bindings.get("name").unwrap_or_default();
    PAGES
        .iter()
        .copied()
        .find(|(n, _)| *n == name)
        .ok_or_else(|| Error::NotFound(format!("no demo page `{name}`")))
}

fn by_name(id: &str, summary: &str) -> Description {
    Description::new(id)
        .title(id)
        .summary(summary)
        .verb(Verb::Source)
        .verb(Verb::Meta)
        .input(ArgSpec::new("name").class(XSD_STRING).binding())
        .output(HTML)
}

/// What a page's fragment shows: a note, and the resource (and its `as=`) it splices.
fn fragment_parts(name: &str) -> (&'static str, &'static str, Option<&'static str>) {
    match name {
        "gallery" => (
            "<h1>Gallery</h1>\n<p>Every UI node kind, rendered by the html target's templates.</p>",
            "urn:iki:ui:html:screen:gallery",
            None,
        ),
        "elements" => (
            "<h1>Elements</h1>\n<p>Custom elements, each a composition of core kinds: expanded by the html target, or native where it claims one (disclosure and meter).</p>",
            "urn:iki:ui:html:screen:elements",
            None,
        ),
        "contact" => ("<h1>Contact</h1>", "urn:iki:ui:html:screen:contact", None),
        _ => (
            "<h1>Support</h1>\n<p>What each target renders of each node kind.</p>",
            ikigai_ui::target::SUPPORT,
            Some("text/html"),
        ),
    }
}

/// What the page splices of `inner`: all of it, except the support page, whose resource is a
/// whole document (`<!doctype html>` to `</html>`, its own `<h1>`) and goes inside this page's
/// `<main>` as the body's content after that heading.
fn support_body<'a>(name: &str, inner: &'a [u8]) -> &'a [u8] {
    if name != "support" {
        return inner;
    }
    let text = std::str::from_utf8(inner).unwrap_or_default();
    let body = text
        .split_once("<body>")
        .and_then(|(_, rest)| rest.rsplit_once("</body>"))
        .map(|(body, _)| body)
        .unwrap_or(text);
    let body = body
        .strip_prefix("<h1>")
        .and_then(|rest| rest.split_once("</h1>"))
        .map(|(_, rest)| rest)
        .unwrap_or(body);
    body.as_bytes()
}

/// The demo's one image: the gallery's image example names `/img/lighthouse.jpg`.
const LIGHTHOUSE: &[u8] = include_bytes!("../assets/lighthouse.jpg");

/// The demo pages' own names: a header, titles and fragments under [`BASE`].
pub fn site() -> EndpointSpace {
    let header = PAGES
        .iter()
        .map(|(n, t)| format!("<a href=\"/page/{n}\">{t}</a>"))
        .collect::<Vec<_>>()
        .join(" ");
    let header = format!("<nav aria-label=\"Site\">{header}</nav>");
    EndpointSpace::new()
        .bind(
            Exact::new(format!("{BASE}header")),
            FnEndpoint::new("uidemo-header", move |_: &Invocation<'_>| {
                Ok(html(header.clone().into_bytes()).cacheable())
            })
            .with_description(
                Description::new("uidemo-header")
                    .title("uidemo-header")
                    .summary("The demo's header: its pages.")
                    .verb(Verb::Source)
                    .verb(Verb::Meta)
                    .output(HTML),
            ),
        )
        .bind(
            Exact::new(format!("{BASE}img:lighthouse.jpg")),
            FnEndpoint::new("uidemo-img", |_: &Invocation<'_>| {
                Ok(
                    Representation::new(ReprType::new("image/jpeg"), LIGHTHOUSE.to_vec())
                        .cacheable(),
                )
            })
            .with_description(
                Description::new("uidemo-img")
                    .title("uidemo-img")
                    .summary("The gallery's image example: a lighthouse at dusk.")
                    .verb(Verb::Source)
                    .verb(Verb::Meta)
                    .output("image/jpeg"),
            ),
        )
        .bind(
            UriTemplate::parse(format!("{BASE}title:{{name}}")).expect("a template"),
            FnEndpoint::new("uidemo-title", |inv: &Invocation<'_>| {
                Ok(html(page_name(inv)?.1.as_bytes().to_vec()).cacheable())
            })
            .with_description(by_name("uidemo-title", "A demo page's title, as text.")),
        )
        .bind(
            UriTemplate::parse(format!("{BASE}fragment:{{name}}")).expect("a template"),
            AsyncFnEndpoint::new(
                "uidemo-fragment",
                |inv: &Invocation<'_>| -> InvokeFuture<'_> {
                    Box::pin(async move {
                        let (name, _) = page_name(inv)?;
                        let (head, iri, face) = fragment_parts(name);
                        let mut request =
                            Request::new(Verb::Source, Iri::parse(iri).expect("an IRI"));
                        if let Some(face) = face {
                            request = request.with_arg("as", ArgRef::Inline(face.into()));
                        }
                        let inner = inv.issue(request).await?;
                        let mut body = head.to_string();
                        if name == "contact" {
                            body.push_str(&format!("\n<p>{}</p>", html_text(GENERATED_NOTE)));
                        }
                        body.push('\n');
                        let mut bytes = body.into_bytes();
                        bytes.extend_from_slice(support_body(name, &inner.bytes));
                        Ok(html(bytes))
                    })
                },
            )
            .with_description(by_name(
                "uidemo-fragment",
                "A demo page's fragment: a note and the screen it shows.",
            )),
        )
}

/// The demo's prefix table: `demo:` for `urn:iki:uidemo:`.
pub const PREFIXES: &str = "urn:iki:uidemo:prefixes";
/// Where the demo's screens and table are overridden from: a host binds its screens directory
/// here (nothing is bound in this demo, so the compiled-in copies answer).
pub const DIR: &str = "urn:iki:uidemo:dir:";

/// The demo's screen outlines and its prefix table: files under [`DIR`], else compiled in.
pub const SCREENS: [ikigai_ui::Artifact; 3] = [
    ikigai_ui::Artifact {
        iri: "urn:iki:ui:screen:name:contact:source",
        id: "uidemo-screen-contact-source",
        file: "screens/contact.outline",
        media_type: "text/plain; charset=utf-8",
        bytes: include_bytes!("../screens/contact.outline"),
    },
    ikigai_ui::Artifact {
        iri: "urn:iki:ui:screen:name:write:source",
        id: "uidemo-screen-write-source",
        file: "screens/write.outline",
        media_type: "text/plain; charset=utf-8",
        bytes: include_bytes!("../screens/write.outline"),
    },
    ikigai_ui::Artifact {
        iri: PREFIXES,
        id: "uidemo-prefixes",
        file: "prefixes.ttl",
        media_type: "text/turtle",
        bytes: include_bytes!("../assets/prefixes.ttl"),
    },
];

/// The screens' sources, the prefix table and the rating node `write` references.
pub fn screens() -> EndpointSpace {
    let rating = ikigai_ui::element::EXAMPLES[0];
    ikigai_ui::artifacts(&SCREENS, DIR)
        .expect("the demo's screens are a valid set")
        .bind(
            Exact::new(format!("{BASE}example:rating")),
            FnEndpoint::new("uidemo-example-rating", move |_: &Invocation<'_>| {
                Ok(Representation::new(
                    ReprType::new(ikigai_ui::model::MEDIA_TYPE),
                    rating.as_bytes().to_vec(),
                )
                .cacheable())
            })
            .with_description(
                Description::new("uidemo-example-rating")
                    .title("A rating")
                    .summary("A rating element node, unexpanded.")
                    .verb(Verb::Source)
                    .verb(Verb::Meta)
                    .output(ikigai_ui::model::MEDIA_TYPE),
            ),
        )
}

/// The corridor the demo's short names resolve in: ikigai-ui's default table (`ui:`) and the
/// demo's own ([`PREFIXES`], `demo:`), each an ikigai-fn prefix space read through `kernel` and
/// checked by `prefix_check`. The host injects it with `Kernel::issue_in` around the requests
/// that read a screen written in short names.
#[cfg(not(target_arch = "wasm32"))]
pub async fn prefixes(kernel: &Kernel) -> Result<ikigai_core::Scope, ikigai_core::DynamicRefusal> {
    let mut scope = ikigai_core::Scope::empty();
    for table in [ikigai_ui::screen::PREFIXES, PREFIXES] {
        let space = ikigai_ui::screen::prefix_space(table);
        space
            .refresh(kernel, &ikigai_core::Capability::root())
            .await?;
        scope = scope.with(Arc::new(space));
    }
    Ok(scope)
}

/// The root space: ikigai-ui, the page family, the site, the service, the screens' sources,
/// and last ikigai-ui's screens, each compiled from its source.
pub fn root(store: Store) -> Arc<dyn Space> {
    let family = ikigai_ui::Pages::new("uidemo", BASE)
        .lenses(true)
        .workspaces(true)
        // The demo has no look of its own, so it takes ikigai-ui's.
        .stylesheet("/ui/base.css")
        .build()
        .expect("a valid family");
    #[allow(unused_mut)]
    let mut spaces: Vec<Arc<dyn Space>> = vec![
        Arc::new(ikigai_ui::space()),
        Arc::new(family),
        Arc::new(site()),
        Arc::new(service(store)),
        Arc::new(screens()),
        // The example lenses over the contact graph, and the SPARQL store they run on (#3518).
        Arc::new(ikigai_ui::lens::examples()),
        Arc::new(ikigai_sparql::space()),
    ];
    // What the SHACL and asserted-vs-derived lenses join (#3521).
    #[cfg(not(target_arch = "wasm32"))]
    spaces.extend([
        Arc::new(ikigai_shacl::space()) as Arc<dyn Space>,
        Arc::new(ikigai_reason::space()),
    ]);
    spaces.push(Arc::new(ikigai_ui::screen::screens()));
    Arc::new(Fallback::new(spaces))
}

/// A kernel over [`root`], with the Meta renderer that reads a contract as JSON.
pub fn kernel(root: Arc<dyn Space>) -> Kernel {
    Kernel::with_meta_renderer(root, Arc::new(ikigai_vocab::TurtleRenderer))
}

#[cfg(not(target_arch = "wasm32"))]
fn route(pattern: &str, iri_template: &str) -> Route {
    Route {
        pattern: pattern.into(),
        iri_template: iri_template.into(),
        redirect: None,
        cap: None,
        cors: None,
        csp: None,
    }
}

#[cfg(not(target_arch = "wasm32"))]
/// The door's routes: `/` and `/page/{name}` to the pages, `/lens/name/{lens}/resource/{+iri}`
/// to a lensed view's page ([`ikigai_ui::lens::html_href`]), `/workspace/{+iri}` to a
/// workspace's page ([`WORKSPACE_PATH`]), `/ui/{…}` to ikigai-ui's
/// artifacts (where the shell links them), and the html target's submit rule
/// (`/resolution/iri/{+iri}`, [`ikigai_ui::target::SUBMIT`]).
pub fn door() -> EdgeConfig {
    EdgeConfig {
        routes: RouteTable::new(vec![
            route("/", &format!("{BASE}page:gallery")),
            route("/page/{name}", &format!("{BASE}page:{{name}}")),
            // A lensed view as a page in the shell (ledger #3538), the IRI one segment.
            route(
                "/lens/name/{lens}/resource/{+iri}",
                &format!("{BASE}page:lens:{{lens}}:resource:{{+iri}}"),
            ),
            // A workspace as a page in the shell (ledger #3967), where its "Panes" list links.
            route(
                "/workspace/{+iri}",
                &format!("{BASE}page:workspace:{{+iri}}"),
            ),
            route("/ui/{+rest}", "urn:iki:ui:{+rest}"),
            route("/img/{name}", &format!("{BASE}img:{{name}}")),
            route("/resolution/iri/{+iri}", "{+iri}"),
        ]),
        ..EdgeConfig::default()
    }
}

#[cfg(not(target_arch = "wasm32"))]
/// What the door grants every request: a write to the contact service and to the demo
/// person's workspaces, nothing more.
pub fn cap() -> CapFn {
    fixed_cap(vec![CONTACT_WRITE.to_string(), WORKSPACE_WRITE.to_string()])
}

#[cfg(not(target_arch = "wasm32"))]
/// Serve the demo on `listener` until it fails.
pub async fn serve(listener: tokio::net::TcpListener, store: Store) -> std::io::Result<()> {
    ikigai_web::serve_with_listener(Arc::new(kernel(root(store))), cap(), listener, door()).await
}
