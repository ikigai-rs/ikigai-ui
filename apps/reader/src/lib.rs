//! **ikigai-reader** (a spike, ledger #3524): a public feed reader made of resources.
//!
//! Every screen is a composed screen (an outline) over resources the app's own kernel answers,
//! so the GPUI face draws it natively and the HTML face draws the same screens for free.
//!
//! | IRI | what |
//! | --- | --- |
//! | `urn:iki:ui:screen:name:feeds:source` | the feeds screen's outline |
//! | `urn:iki:ui:screen:name:feed-{layer}:source` | one feed's screen, an outline referencing it |
//! | `urn:iki:reader:feeds` | a UI node: the descriptor's public feeds, each a link to its screen |
//! | `urn:iki:reader:feed:{layer}` | a UI node: the feed's entries, newest first, Markdown as text kinds |
//!
//! The app is a STRANGER to the host: its [`Identity`] is its own fresh client certificate,
//! kept in its config home with owner-only permissions; it joins the public feeds through the
//! host's join lobby (layers' open join) and then reads them. Entry text is held in memory
//! only: nothing here writes, logs or prints an entry.
#![cfg(not(target_arch = "wasm32"))]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use ikigai_core::{
    ArgRef, Capability, EndpointSpace, Error, Exact, FnEndpoint, Invocation, Iri, Kernel, ReprType,
    Representation, Request, Time, UriTemplate, Verb,
};
use serde_json::{json, Value};

/// A host's descriptor of its public layers, as the edge publishes it at
/// `/.well-known/ikigai/layers.json`.
pub const WELL_KNOWN: &str = "/.well-known/ikigai/layers.json";
/// The model's JSON face.
const MODEL: &str = ikigai_ui::model::MEDIA_TYPE;
/// The feeds screen's outline.
pub const FEEDS_OUTLINE: &str = include_str!("../screens/feeds.outline");

/// One public feed a descriptor offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Feed {
    /// The layer's name: `urn:layer:{name}`.
    pub name: String,
    /// Its label, or its name.
    pub label: String,
}

/// What the app reads of a host's descriptor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Descriptor {
    /// The join lobby's say Sink.
    pub join: String,
    /// The public feeds.
    pub feeds: Vec<Feed>,
}

fn layer_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

impl Descriptor {
    /// Read `layers.json`. A layer whose name is not a layer name is refused, never shown.
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        let doc: Value = serde_json::from_slice(bytes).map_err(|e| format!("layers.json: {e}"))?;
        let join = doc["join"]
            .as_str()
            .filter(|j| {
                j.strip_prefix("urn:layer:")
                    .and_then(|r| r.strip_suffix(":say"))
                    .is_some_and(layer_name)
            })
            .ok_or("layers.json: no join lobby (`join`: urn:layer:{lobby}:say)")?
            .to_string();
        let mut feeds = Vec::new();
        for layer in doc["public_layers"]
            .as_array()
            .ok_or("layers.json: no `public_layers`")?
        {
            let name = layer["name"].as_str().unwrap_or_default();
            if !layer_name(name) {
                return Err(format!("layers.json: `{name}` is not a layer name"));
            }
            let label = layer["label"].as_str().unwrap_or(name).to_string();
            feeds.push(Feed {
                name: name.to_string(),
                label,
            });
        }
        Ok(Descriptor { join, feeds })
    }
}

/// The app's own identity: a client certificate it made itself, never anyone else's.
#[derive(Clone, Debug)]
pub struct Identity {
    /// The certificate, PEM (public).
    pub certificate: String,
    /// Its SHA-256 fingerprint, as the host names it.
    pub fingerprint: String,
    /// Where its private key is kept (owner-only); the key itself is never held here.
    pub key_path: PathBuf,
}

/// The config home: `$IKIGAI_READER_HOME`, else `~/.config/ikigai-reader`.
pub fn config_home() -> Option<PathBuf> {
    if let Some(home) = std::env::var_os("IKIGAI_READER_HOME") {
        return Some(home.into());
    }
    std::env::var_os("HOME").map(|h| Path::new(&h).join(".config/ikigai-reader"))
}

#[cfg(unix)]
fn owner_only(path: &Path, mode: u32) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(not(unix))]
fn owner_only(_: &Path, _: u32) -> Result<(), String> {
    Ok(())
}

impl Identity {
    /// The identity in `home`, made there on first use: `client.crt` and `client.key`, the
    /// directory 0700 and the key 0600. A key that others can read is refused, not used.
    pub fn load_or_create(home: &Path) -> Result<Self, String> {
        let (crt, key) = (home.join("client.crt"), home.join("client.key"));
        if !crt.exists() {
            std::fs::create_dir_all(home).map_err(|e| format!("{}: {e}", home.display()))?;
            owner_only(home, 0o700)?;
            let pair = rcgen::KeyPair::generate_for(&rcgen::PKCS_ED25519)
                .map_err(|e| format!("a key: {e}"))?;
            let params = rcgen::CertificateParams::new(vec!["ikigai-reader".to_string()])
                .map_err(|e| format!("a certificate: {e}"))?;
            let cert = params
                .self_signed(&pair)
                .map_err(|e| format!("a certificate: {e}"))?;
            // The key first, created owner-only, so it is never readable by others.
            write_new(&key, pair.serialize_pem().as_bytes(), 0o600)?;
            write_new(&crt, cert.pem().as_bytes(), 0o644)?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&key)
                .map_err(|e| format!("{}: {e}", key.display()))?
                .permissions()
                .mode();
            if mode & 0o077 != 0 {
                return Err(format!(
                    "{} is readable by others (mode {:o}): expected 0600",
                    key.display(),
                    mode & 0o777
                ));
            }
        }
        let certificate =
            std::fs::read_to_string(&crt).map_err(|e| format!("{}: {e}", crt.display()))?;
        let fingerprint = ikigai_layers::onboard::fingerprint_of_pem(&certificate)?;
        Ok(Identity {
            certificate,
            fingerprint,
            key_path: key,
        })
    }
}

fn write_new(path: &Path, bytes: &[u8], mode: u32) -> Result<(), String> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(mode);
    }
    let mut file = options
        .open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    file.write_all(bytes)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    owner_only(path, mode)
}

/// How the app reaches a layer host: one request, under whatever capability the host's door
/// mints for the app's certificate.
pub trait Door: Send + Sync {
    /// Issue `request` as the connection holding the app's certificate.
    fn issue(&self, request: Request) -> Result<Representation, Error>;
}

/// A layer host in this process, reached as its door would reach it: each request runs under
/// the capability [`ikigai_layers::Host::door_capability`] mints for the app's fingerprint at
/// that moment, so a join is live on the next request, as on a reconnect. Development and the
/// tests run against this, never the live edge.
pub struct LocalDoor {
    host: Arc<ikigai_layers::Host>,
    kernel: Kernel,
    door: String,
    fingerprint: String,
}

impl LocalDoor {
    /// A door named `door` over `host`, for the certificate `fingerprint`.
    pub fn new(host: Arc<ikigai_layers::Host>, door: &str, fingerprint: &str) -> Self {
        let root = Arc::new(ikigai_core::Fallback::new(vec![Arc::new(
            ikigai_layers::space(&host),
        )]));
        let kernel = Kernel::with_meta_renderer(root, Arc::new(ikigai_vocab::TurtleRenderer))
            .with_clock(Arc::new(ikigai_core::SystemClock));
        LocalDoor {
            host,
            kernel,
            door: door.to_string(),
            fingerprint: fingerprint.to_string(),
        }
    }
}

fn now() -> Time {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default();
    Time::from_millis(millis)
}

impl Door for LocalDoor {
    fn issue(&self, request: Request) -> Result<Representation, Error> {
        let capability = self
            .host
            .door_capability(&self.door, &self.fingerprint, None, now())
            .map_err(Error::Endpoint)?
            .ok_or_else(|| Error::Endpoint("the door refuses this certificate".into()))?;
        // A face resolves this from inside its own executor, so the host's work runs on a thread
        // of its own (as a door's connection would), never nested in the caller's executor.
        std::thread::scope(|s| {
            s.spawn(|| futures::executor::block_on(self.kernel.issue(request, &capability)))
                .join()
                .unwrap_or_else(|_| Err(Error::Endpoint("the local host panicked".into())))
        })
    }
}

/// One entry, in memory only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Its IRI, `urn:layer:{layer}:entry:{id}`.
    pub id: String,
    /// Its feed order.
    pub seq: u64,
    /// Its title, if it has one.
    pub title: Option<String>,
    /// Its Markdown body, if it has one.
    pub body: Option<String>,
    /// When the host stamped it.
    pub created: Option<String>,
}

fn request(verb: Verb, iri: &str, args: &[(&str, &str)]) -> Result<Request, Error> {
    let mut request = Request::new(
        verb,
        Iri::parse(iri).map_err(|e| Error::Endpoint(e.to_string()))?,
    );
    for (name, value) in args {
        request = request.with_arg(*name, ArgRef::Inline(value.as_bytes().to_vec()));
    }
    Ok(request)
}

/// A Turtle string literal.
fn literal(value: &str) -> String {
    let mut out = String::from("\"");
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn text_of(node: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|k| match &node[*k] {
        Value::String(s) => Some(s.clone()),
        Value::Object(o) => o.get("@value").and_then(Value::as_str).map(str::to_string),
        _ => None,
    })
}

/// The reader: a descriptor, the app's identity and a door.
pub struct Reader {
    door: Arc<dyn Door>,
    identity: Identity,
    descriptor: Descriptor,
    joined: Mutex<bool>,
}

impl Reader {
    /// A reader of `descriptor`'s feeds through `door`, as `identity`.
    pub fn new(door: Arc<dyn Door>, identity: Identity, descriptor: Descriptor) -> Self {
        Reader {
            door,
            identity,
            descriptor,
            joined: Mutex::new(false),
        }
    }

    /// The descriptor's public feeds.
    pub fn feeds(&self) -> &[Feed] {
        &self.descriptor.feeds
    }

    /// Join every public feed the descriptor offers, the way a stranger does: one join request
    /// in the lobby, stating the app's own certificate. Answers the layers granted.
    pub fn join(&self) -> Result<Vec<String>, Error> {
        let layers = self
            .descriptor
            .feeds
            .iter()
            .map(|f| format!("<urn:layer:{}>", f.name))
            .collect::<Vec<_>>()
            .join(", ");
        let fp = &self.identity.fingerprint;
        let turtle = format!(
            "@prefix onb: <https://ikigai-rs.dev/ns/onboard#> .\n<urn:iki:onboard:join:{fp}> a onb:JoinRequest ;\n  \
             onb:certificate {} ;\n  onb:fingerprint {} ;\n  onb:layer {layers} .\n",
            literal(&self.identity.certificate),
            literal(fp),
        );
        let answer = self.door.issue(request(
            Verb::Sink,
            &self.descriptor.join,
            &[("content", &turtle), ("as", "application/json")],
        )?)?;
        let doc: Value = serde_json::from_slice(&answer.bytes)
            .map_err(|e| Error::Endpoint(format!("the join's answer: {e}")))?;
        *self.joined.lock().unwrap() = true;
        let mut granted: Vec<String> = Vec::new();
        collect_layers(&doc, &mut granted);
        Ok(granted)
    }

    /// `layer`'s entries, newest first, joining first if the app has not joined yet.
    pub fn entries(&self, layer: &str) -> Result<Vec<Entry>, Error> {
        if !self.descriptor.feeds.iter().any(|f| f.name == layer) {
            return Err(Error::NotFound(format!("urn:layer:{layer}")));
        }
        if !*self.joined.lock().unwrap() {
            self.join()?;
        }
        let answer = self.door.issue(request(
            Verb::Source,
            &format!("urn:layer:{layer}:entries"),
            &[("as", "application/ld+json")],
        )?)?;
        let doc: Value = serde_json::from_slice(&answer.bytes)
            .map_err(|e| Error::Endpoint(format!("{layer}'s entries: {e}")))?;
        let mut entries: Vec<Entry> = doc["@graph"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|node| {
                let id = node["@id"].as_str()?.to_string();
                let seq = text_of(node, &["seq", "ik:seq"])
                    .and_then(|s| s.parse().ok())
                    .or_else(|| node["seq"].as_u64())?;
                Some(Entry {
                    id,
                    seq,
                    title: text_of(node, &["title", "dcterms:title"]),
                    body: text_of(node, &["body", "ik:body"]),
                    created: text_of(node, &["created", "dcterms:created"]),
                })
            })
            .collect();
        entries.sort_by(|a, b| b.seq.cmp(&a.seq));
        Ok(entries)
    }
}

fn collect_layers(doc: &Value, out: &mut Vec<String>) {
    match doc {
        Value::String(s) if s.starts_with("urn:layer:") => out.push(s.clone()),
        Value::Array(a) => a.iter().for_each(|v| collect_layers(v, out)),
        Value::Object(o) => o.values().for_each(|v| collect_layers(v, out)),
        _ => {}
    }
}

fn node(kind: &str, properties: Value, children: Vec<Value>) -> Value {
    let mut n = json!({ "kind": kind, "properties": properties });
    if !children.is_empty() {
        n["children"] = Value::Array(children);
    }
    n
}

/// `markdown` as the model's text kinds: headings (one level below the entry's own), text,
/// lists of text and links to `https:` only. Raw HTML is dropped, never passed through.
pub fn markdown(markdown: &str) -> Vec<Value> {
    use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};
    let mut out: Vec<Value> = Vec::new();
    let mut lists: Vec<Vec<Value>> = Vec::new();
    let mut text = String::new();
    let mut links: Vec<(String, String)> = Vec::new();
    let mut link: Option<(String, String)> = None;
    let mut heading: Option<u8> = None;
    let push = |out: &mut Vec<Value>, lists: &mut Vec<Vec<Value>>, v: Value| match lists.last_mut()
    {
        Some(list) => list.push(v),
        None => out.push(v),
    };
    for event in Parser::new(markdown) {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                heading = Some(match level {
                    HeadingLevel::H1 => 3,
                    HeadingLevel::H2 => 4,
                    HeadingLevel::H3 => 5,
                    _ => 6,
                })
            }
            Event::Start(Tag::List(_)) => lists.push(Vec::new()),
            Event::End(TagEnd::List(_)) => {
                let items = lists.pop().unwrap_or_default();
                push(
                    &mut out,
                    &mut lists,
                    node("list", json!({ "label": "List" }), items),
                );
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                link = Some((dest_url.to_string(), String::new()))
            }
            Event::End(TagEnd::Link) => {
                if let Some((href, label)) = link.take() {
                    links.push((href, label));
                }
            }
            Event::Text(t) | Event::Code(t) => {
                if let Some((_, label)) = link.as_mut() {
                    label.push_str(&t);
                }
                text.push_str(&t);
            }
            Event::SoftBreak | Event::HardBreak => text.push(' '),
            Event::End(TagEnd::Heading(_)) => {
                let level = heading.take().unwrap_or(3);
                let content = std::mem::take(&mut text);
                push(
                    &mut out,
                    &mut lists,
                    node(
                        "heading",
                        json!({ "content": content, "level": level }),
                        vec![],
                    ),
                );
            }
            Event::End(
                TagEnd::Paragraph | TagEnd::Item | TagEnd::CodeBlock | TagEnd::BlockQuote(_),
            ) => {
                let content = std::mem::take(&mut text);
                if !content.trim().is_empty() {
                    push(
                        &mut out,
                        &mut lists,
                        node("text", json!({ "content": content.trim() }), vec![]),
                    );
                }
                for (href, label) in links.drain(..) {
                    if href.starts_with("https://") {
                        let label = if label.is_empty() {
                            href.clone()
                        } else {
                            label
                        };
                        push(
                            &mut out,
                            &mut lists,
                            node("link", json!({ "label": label, "href": href }), vec![]),
                        );
                    }
                }
            }
            // Raw HTML, inline or a block, never reaches a face.
            Event::Html(_) | Event::InlineHtml(_) => {}
            _ => {}
        }
    }
    let rest = text.trim();
    if !rest.is_empty() {
        out.push(node("text", json!({ "content": rest }), vec![]));
    }
    out
}

/// The screen name of a feed.
pub fn feed_screen(layer: &str) -> String {
    format!("feed-{layer}")
}

/// The UI node `urn:iki:reader:feeds` answers: a list of links to the feeds' screens.
pub fn feeds_node(reader: &Reader) -> Value {
    let links = reader
        .feeds()
        .iter()
        .map(|f| {
            node(
                "link",
                json!({ "label": f.label, "href": format!("/page/{}", feed_screen(&f.name)) }),
                vec![],
            )
        })
        .collect::<Vec<_>>();
    if links.is_empty() {
        return node(
            "status",
            json!({ "content": "This host offers no public feeds." }),
            vec![],
        );
    }
    node("list", json!({ "label": "Public feeds" }), links)
}

/// The UI node `urn:iki:reader:feed:{layer}` answers: its entries, newest first, each a group
/// under its title, its Markdown as text kinds.
pub fn feed_node(reader: &Reader, layer: &str) -> Result<Value, Error> {
    let entries = reader.entries(layer)?;
    if entries.is_empty() {
        return Ok(node(
            "status",
            json!({ "content": "No entries yet." }),
            vec![],
        ));
    }
    let groups = entries
        .iter()
        .map(|e| {
            let title = e
                .title
                .clone()
                .unwrap_or_else(|| format!("Entry {}", e.seq));
            let mut children = vec![node(
                "heading",
                json!({ "content": title, "level": 2 }),
                vec![],
            )];
            if let Some(created) = &e.created {
                children.push(node("text", json!({ "content": created }), vec![]));
            }
            if let Some(body) = &e.body {
                children.extend(markdown(body));
            }
            node("group", json!({ "label": title }), children)
        })
        .collect();
    Ok(node(
        "list",
        json!({ "label": "Entries, newest first" }),
        groups,
    ))
}

fn model(value: &Value) -> Representation {
    Representation::new(
        ReprType::new(MODEL),
        serde_json::to_vec(value).expect("JSON"),
    )
}

fn text_repr(text: String) -> Representation {
    Representation::new(ReprType::new(ikigai_ui::screen::OUTLINE), text.into_bytes())
}

/// The app's own resources (the table at the top of this crate), answered by `reader`.
/// A host places this before ikigai-ui's spaces.
pub fn space(reader: Arc<Reader>) -> EndpointSpace {
    let feeds = reader.clone();
    let feed = reader.clone();
    let source = reader;
    EndpointSpace::new()
        .bind(
            Exact::new("urn:iki:ui:screen:name:feeds:source"),
            FnEndpoint::new("reader-feeds-source", |_: &Invocation<'_>| {
                Ok(text_repr(FEEDS_OUTLINE.to_string()))
            }),
        )
        .bind(
            Exact::new("urn:iki:reader:feeds"),
            FnEndpoint::new("reader-feeds", move |_: &Invocation<'_>| Ok(model(&feeds_node(&feeds)))),
        )
        .bind(
            UriTemplate::parse("urn:iki:reader:feed:{layer}").expect("a template"),
            FnEndpoint::new("reader-feed", move |inv: &Invocation<'_>| {
                let layer = inv.bindings.get("layer").unwrap_or_default();
                Ok(model(&feed_node(&feed, layer)?))
            }),
        )
        .bind(
            UriTemplate::parse("urn:iki:ui:screen:name:feed-{layer}:source").expect("a template"),
            FnEndpoint::new("reader-feed-source", move |inv: &Invocation<'_>| {
                let layer = inv.bindings.get("layer").unwrap_or_default();
                let feed = source
                    .feeds()
                    .iter()
                    .find(|f| f.name == layer)
                    .ok_or_else(|| Error::NotFound(format!("feed {layer}")))?;
                Ok(text_repr(format!(
                    "# One feed: its entries, newest first.\ngroup label={label}\n  nav label=\"Back\"\n    link label=\"All feeds\" href=/page/feeds\n  heading content={label} level=1\n  reference resource=urn:iki:reader:feed:{layer} fallback=\"This feed could not be read.\"\n",
                    label = quote(&feed.label),
                )))
            }),
        )
}

fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace(['"', '\n', '\\'], " "))
}

/// The app's capability over its own kernel: the root, attenuated to nothing it writes.
pub fn capability() -> Capability {
    Capability::root()
}
