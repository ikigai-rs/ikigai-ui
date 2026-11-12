//! Lenses (ledger #3518, stage 1 of #3517): what a view of a resource shows is driven by a
//! read-only SPARQL query, rendered by a NAMED TEMPLATE.
//!
//! | name | kind | what it is |
//! |---|---|---|
//! | `urn:iki:ui:lens:name:{lens}` | data (a module's or host's) | the lens, [`Lens`] as JSON: its query IRI, renderer, focus variable, bound, legend |
//! | `urn:iki:ui:lens:name:{lens}:query` | data (a module's or host's) | the lens's query, SPARQL text, its default query IRI |
//! | `urn:iki:ui:lens:prepare` | transreptor | a query (`content`, [`QUERY`]) to its prepared form, [`PREPARED`]: its words read, checked read-only, every SERVICE resolved to an IRI ([`prepare`]); pure, so the same text is prepared once |
//! | `urn:iki:ui:lens:name:{lens}:prepared` | data | the lens's query source prepared by `urn:iki:ui:lens:prepare` and its SERVICEs checked against the lens's list: CODE AS A RESOURCE, cached and golden-threaded from the source, so an edit to the query (a Sink to it) re-prepares exactly the lenses over it and cuts the results over them (ledger #3530) |
//! | `urn:iki:ui:lens:name:{lens}:focus:{iri}` | data | the lens's result for `{iri}`: the lens's PREPARED query ([`Prepared`]), `{iri}` bound to the focus variable as a TERM through `urn:sparql:*`'s `bindings=` (never spliced), run under the CALLER's capability |
//! | `urn:iki:ui:lens:type:{class}` | data (a module's or host's) | a JSON list of lens names for a type |
//! | `urn:iki:ui:lens:instance:{iri}` | data (a host's) | a JSON list of lens names for one resource |
//! | `urn:iki:ui:lens:dataset` | data (a host's) | a JSON list of lens names for the whole dataset |
//! | `urn:iki:ui:lens:resource:{iri}` | data | `{iri}`'s lenses in precedence: instance, dataset, each type, then `graph` ([`precedence`]) |
//! | `urn:iki:ui:renderer:name:{name}` | data | a renderer, a model template: `graph`, `table`, `list` are compiled in; a host's binding of the same name, placed first, wins |
//! | `urn:iki:ui:view:lens:{lens}:resource:{iri}` | data | the lensed view: the legend, a radio chooser among `{iri}`'s lenses, the renderer filled with the result |
//! | `urn:iki:ui:lens:title:lens:{lens}:resource:{iri}` | text | a lensed page's title: the legend and the resource; a family with [`crate::Pages::lenses`] serves `{base}page:lens:{lens}:resource:{iri}` in the shell, at the door's `/lens/name/{lens}/resource/{iri}` ([`html_href`]) |
//! | `urn:iki:ui:home` | data (a host's) | the home mapping, a JSON prefix table `[{"prefix", "href"}]`, longest prefix wins |
//! | `urn:iki:ui:home:resource:{iri}` | data | where `{iri}` links: its home by the table, else its own view ([`crate::view::href`]) |
//!
//! **A lens query is read-only.** SELECT, CONSTRUCT or ASK, nothing else; an UPDATE keyword
//! anywhere, or a SERVICE the lens does not list in `services`, is refused as
//! `InvalidArgument` before anything runs. The store's time and answer budgets apply as for any
//! caller, and the lens asks for at most `bound + 1` rows, so it knows when it is past its bound.
//!
//! **A withheld result is said in words.** When the caller may not read what the lens reads, the
//! lensed view says so in a `text`, never an empty renderer.
//!
//! **Templates repeat, boundedly.** A renderer is a model tree with three constructs beside
//! ordinary nodes, all data, nothing code ([`expand`]):
//!
//! - `{"repeat": "rows", "each": T}` — `T` once per result row, at most the lens's bound;
//!   with `"group": "v"`, once per distinct value of `?v` (first-seen order), and inside it
//!   `{"repeat": "group", "each": T}` repeats over that group's rows;
//! - `{"repeat": "vars", "each": T}` — once per projected variable (`{?var}` is its name);
//!   `{"repeat": "cells", "each": T}` — once per variable of the current row (`{?value}`);
//! - `{"repeat": "edges", "each": T}` — once per edge of a CONSTRUCT's result graph (`{?from}`,
//!   `{?edge}`, `{?to}`, as labels), every one, within the scene's [`crate::scene::MAX_EDGES`];
//! - `{"past": "words"}` — a `text` only when the result had more rows than the bound: what the
//!   renderer does past its bound is to show the first `bound` rows and say so.
//!
//! In any string, `{?v}` is the row's value of `?v`, `{home:?v}` its home link, and `{lens}`,
//! `{legend}`, `{focus}`, `{bound}`, `{shown}` the lens's.
//!
//! **The `graph` renderer draws the lens's own result** (ledger #3532). A CONSTRUCT lens's
//! result is its graph as JSON-LD; its rows are the graph's nodes (`?node`, `?label`, by label)
//! and its edges are the graph's. The renderer's scene is
//! `urn:iki:ui:scene:graph:urn:iki:ui:lens:name:{lens}:focus:{iri}`, the scene of that result
//! ([`crate::scene`]), laid out by the seeded layout and cached, golden-threaded from the
//! result; the selection beside it is the same nodes as links, and its replacement a table of
//! every edge. A SELECT lens drawn as a graph is its `?node`s, with no edges. A `link` whose value is not an IRI is its text.
//! A bound past [`MAX_BOUND`], or an expansion past [`crate::element::MAX_NODES`] nodes, is
//! refused, never unbounded.

use std::collections::BTreeMap;

use ikigai_core::{
    ArgRef, AsyncFnEndpoint, Description, EndpointSpace, Error, Exact, FnEndpoint, Invocation,
    InvokeFuture, Iri, ReprType, Representation, Request, Verb,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as Json};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::model::{self, Node};

/// A lens, by name.
pub const NAME: &str = "urn:iki:ui:lens:name:";
/// A type's lens names.
pub const TYPE: &str = "urn:iki:ui:lens:type:";
/// One resource's own lens names.
pub const INSTANCE: &str = "urn:iki:ui:lens:instance:";
/// The dataset's lens names.
pub const DATASET: &str = "urn:iki:ui:lens:dataset";
/// A resource's lenses, in precedence.
pub const RESOURCE: &str = "urn:iki:ui:lens:resource:";
/// A renderer, by name.
pub const RENDERER: &str = "urn:iki:ui:renderer:name:";
/// The lensed view.
pub const VIEW: &str = "urn:iki:ui:view:lens:";
/// The home mapping's prefix table.
pub const HOME_TABLE: &str = "urn:iki:ui:home";
/// A resource's home.
pub const HOME: &str = "urn:iki:ui:home:resource:";
/// The lens every resource has last.
pub const DEFAULT: &str = "graph";
/// The largest bound a lens may state.
pub const MAX_BOUND: usize = 1000;
/// The largest lens or query read, in bytes.
pub const MAX_BYTES: usize = 64 * 1024;
/// A lens's prepared query is its name plus this.
pub const PREPARED_SUFFIX: &str = ":prepared";
/// The lens query preparer, a transreptor from [`QUERY`] to [`PREPARED`].
pub const PREPARE: &str = "urn:iki:ui:lens:prepare";
/// A lens query's source media type.
pub const QUERY: &str = "application/sparql-query";
/// A prepared lens query's media type, [`Prepared`] as JSON.
pub const PREPARED: &str = "application/vnd.ikigai.ui.lens-query+json";
/// The SPARQL result media type.
pub const RESULTS: &str = "application/sparql-results+json";
/// A CONSTRUCT lens's result media type: the result graph, as node objects.
pub const GRAPH: &str = "application/ld+json";

const XSD_ANY_URI: &str = "http://www.w3.org/2001/XMLSchema#anyURI";

/// The lens every resource has last, compiled in: its JSON and its query.
pub const DEFAULT_LENS: (&str, &str) = (
    include_str!("../assets/lens/graph.json"),
    include_str!("../assets/lens/graph.rq"),
);

/// The example lenses over the contact graph ([`crate::scene::EXAMPLE_GRAPH`]), one per renderer:
/// (name, lens, query). Shipped by this crate, which owns that graph; a host places
/// [`examples`].
pub const EXAMPLES: [(&str, &str, &str); 3] = [
    (
        "contact-graph",
        include_str!("../assets/lens/contact-graph.json"),
        include_str!("../assets/lens/contact-graph.rq"),
    ),
    (
        "contact-table",
        include_str!("../assets/lens/contact-table.json"),
        include_str!("../assets/lens/contact-table.rq"),
    ),
    (
        "contact-list",
        include_str!("../assets/lens/contact-list.json"),
        include_str!("../assets/lens/contact-list.rq"),
    ),
];

/// The compiled-in renderers, by name.
pub const RENDERERS: [(&str, &str); 3] = [
    ("graph", include_str!("../assets/renderer/graph.json")),
    ("table", include_str!("../assets/renderer/table.json")),
    ("list", include_str!("../assets/renderer/list.json")),
];

/// A lens's data.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lens {
    /// Its query's IRI; `{lens}:query` when absent.
    #[serde(default)]
    pub query: Option<String>,
    /// The renderer's name, `urn:iki:ui:renderer:name:{renderer}`.
    pub renderer: String,
    /// The variable the resource is bound to.
    #[serde(default = "focus")]
    pub focus: String,
    /// The most rows it renders.
    pub bound: usize,
    /// What it shows, in words.
    pub legend: String,
    /// The graphs it queries (`urn:sparql:*`'s `graph=`).
    #[serde(default)]
    pub graphs: Vec<String>,
    /// The SERVICE IRIs it may name.
    #[serde(default)]
    pub services: Vec<String>,
    /// Its facets: each a checkbox per allowed value, bound into the query as a term.
    #[serde(default)]
    pub facets: Vec<Facet>,
}

/// A facet (ledger #3531): a query parameter `name` whose value, one of `values`, is bound to
/// `?variable` as a TERM through `bindings=`; absent, the variable is unbound.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Facet {
    /// The query parameter it reads.
    pub name: String,
    /// What it is, in words.
    pub label: String,
    /// The variable its value binds; `name` when absent.
    #[serde(default)]
    pub variable: Option<String>,
    /// The values it allows.
    pub values: Vec<String>,
    /// `uri` binds an IRI; anything else (the default, `literal`) a plain literal.
    #[serde(default, rename = "type")]
    pub term: Option<String>,
}

impl Facet {
    /// The variable its value binds.
    pub fn variable(&self) -> &str {
        self.variable.as_deref().unwrap_or(&self.name)
    }

    fn term(&self, value: &str) -> Json {
        if self.term.as_deref() == Some("uri") {
            json!({"type": "uri", "value": value})
        } else {
            json!(value)
        }
    }
}

/// The most facets a lens declares, and the most values a facet allows.
pub const MAX_FACETS: usize = 8;
/// The most values one facet allows.
pub const MAX_VALUES: usize = 32;
/// Arguments a lensed view reads that are not facets.
const RESERVED: [&str; 4] = ["lens", "as", "theme", "bindings"];

fn variable_name(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// A facet selection: facet name → its chosen value, in name order.
pub type Selection = BTreeMap<String, String>;

impl Lens {
    /// `get`'s value for each facet, held to its allowed values: one outside them, or a
    /// parameter the lens does not declare, is `InvalidArgument`.
    pub fn select<'a>(
        &self,
        given: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Result<Selection, Error> {
        let mut out = Selection::new();
        for (name, value) in given {
            if RESERVED.contains(&name) {
                continue;
            }
            let facet = self
                .facets
                .iter()
                .find(|f| f.name == name)
                .ok_or_else(|| refuse(name, format!("the lens has no facet `{name}`")))?;
            if value.is_empty() {
                continue;
            }
            if !facet.values.iter().any(|v| v == value) {
                return Err(refuse(
                    name,
                    format!(
                        "`{value}` is not one of the facet's values ({})",
                        facet.values.join(", ")
                    ),
                ));
            }
            if out.insert(name.to_string(), value.to_string()).is_some() {
                return Err(refuse(name, "a facet binds one term: choose one value"));
            }
        }
        Ok(out)
    }
}

/// The facet arguments of a request, as (name, value).
fn given<'a>(inv: &'a Invocation<'_>) -> Result<Vec<(&'a str, &'a str)>, Error> {
    let mut out = Vec::new();
    for name in inv.request.args.keys() {
        if RESERVED.contains(&name.as_str()) {
            continue;
        }
        out.push((name.as_str(), inv.inline_str(name)?));
    }
    Ok(out)
}

fn focus() -> String {
    "focus".into()
}

fn refuse(name: &str, detail: impl std::fmt::Display) -> Error {
    Error::InvalidArgument {
        name: name.into(),
        detail: detail.to_string(),
    }
}

fn iri(s: &str) -> Result<Iri, Error> {
    Iri::parse(s).map_err(|e| refuse("iri", format!("`{s}` is not an IRI: {e}")))
}

impl Lens {
    /// A bounded lens.
    pub fn parse(bytes: &[u8], lens: &str) -> Result<Self, Error> {
        if bytes.len() > MAX_BYTES {
            return Err(refuse("lens", format!("{lens}: the lens is too large")));
        }
        let l: Self = serde_json::from_slice(bytes)
            .map_err(|e| refuse("lens", format!("{lens}: not a lens: {e}")))?;
        if l.bound == 0 || l.bound > MAX_BOUND {
            return Err(refuse(
                "lens",
                format!("{lens}: its bound {} is not in 1..={MAX_BOUND}", l.bound),
            ));
        }
        if l.facets.len() > MAX_FACETS {
            return Err(refuse(
                "lens",
                format!("{lens}: more than {MAX_FACETS} facets"),
            ));
        }
        for f in &l.facets {
            if !variable_name(&f.name)
                || !variable_name(f.variable())
                || f.variable() == l.focus
                || RESERVED.contains(&f.name.as_str())
                || f.values.is_empty()
                || f.values.len() > MAX_VALUES
            {
                return Err(refuse(
                    "lens",
                    format!(
                        "{lens}: facet `{}` needs a name and variable (letters, digits, `_`, not the focus) and 1..={MAX_VALUES} values",
                        f.name
                    ),
                ));
            }
        }
        if !variable_name(&l.focus) {
            return Err(refuse(
                "lens",
                format!("{lens}: `{}` is not a variable", l.focus),
            ));
        }
        Ok(l)
    }
}

/// A read-only query's form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Form {
    /// SELECT.
    Select,
    /// CONSTRUCT.
    Construct,
    /// ASK.
    Ask,
}

impl Form {
    /// Its `urn:sparql:*` endpoint.
    pub fn endpoint(self) -> &'static str {
        match self {
            Form::Select => "urn:sparql:select",
            Form::Construct => "urn:sparql:construct",
            Form::Ask => "urn:sparql:ask",
        }
    }
}

const UPDATE: [&str; 11] = [
    "INSERT", "DELETE", "LOAD", "CLEAR", "CREATE", "DROP", "COPY", "MOVE", "ADD", "WITH", "USING",
];

/// The words of a query, its strings and comments dropped, its IRIs one `<…>` word each.
fn words(q: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = q.chars().peekable();
    let flush = |cur: &mut String, out: &mut Vec<String>| {
        if !cur.is_empty() {
            out.push(std::mem::take(cur));
        }
    };
    while let Some(c) = chars.next() {
        match c {
            '#' => {
                flush(&mut cur, &mut out);
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            '"' | '\'' => {
                flush(&mut cur, &mut out);
                let mut escaped = false;
                for d in chars.by_ref() {
                    if escaped {
                        escaped = false;
                    } else if d == '\\' {
                        escaped = true;
                    } else if d == c {
                        break;
                    }
                }
                out.push("\"\"".into());
            }
            '<' if cur.is_empty() => {
                let mut i = String::from("<");
                let mut closed = false;
                while let Some(&d) = chars.peek() {
                    if d.is_whitespace() {
                        break;
                    }
                    chars.next();
                    i.push(d);
                    if d == '>' {
                        closed = true;
                        break;
                    }
                }
                if !closed {
                    out.push("<".into());
                    cur = i[1..].to_string();
                } else {
                    out.push(i);
                }
            }
            c if c.is_alphanumeric() || "_:?$-.".contains(c) => cur.push(c),
            _ => {
                flush(&mut cur, &mut out);
                if !c.is_whitespace() {
                    out.push(c.to_string());
                }
            }
        }
    }
    flush(&mut cur, &mut out);
    out
}

/// A lens query, prepared ([`prepare`]): what [`PREPARE`] answers, as [`PREPARED`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Prepared {
    /// SELECT, CONSTRUCT or ASK.
    pub form: Form,
    /// The query's text, as written.
    pub query: String,
    /// Every SERVICE it names, resolved to a full IRI (a prefixed name through the query's own
    /// PREFIX), in the order written, each once.
    pub services: Vec<String>,
    /// Whether it states its own LIMIT.
    pub limit: bool,
}

static PREPARED_COUNT: AtomicUsize = AtomicUsize::new(0);

/// How many times [`PREPARE`] has prepared a query in this process: the prepared form is cached
/// and golden-threaded from the query's source, so an unchanged query is prepared once.
pub fn prepared_count() -> usize {
    PREPARED_COUNT.load(Ordering::SeqCst)
}

/// `query` prepared, when it is read-only: SELECT, CONSTRUCT or ASK, no update keyword, every
/// SERVICE naming an IRI (written `<…>` or as a prefixed name the query declares). Anything
/// else is `InvalidArgument`. Which SERVICEs a lens may name is the lens's ([`Prepared::services`]
/// checked against [`Lens::services`]).
pub fn prepare(query: &str) -> Result<Prepared, Error> {
    if query.len() > MAX_BYTES {
        return Err(refuse("query", "the lens query is too large"));
    }
    let w = words(query);
    let mut i = 0;
    let mut form = None;
    let mut prefixes: BTreeMap<String, String> = BTreeMap::new();
    while i < w.len() {
        match w[i].to_ascii_uppercase().as_str() {
            "PREFIX" => {
                if let (Some(p), Some(ns)) = (w.get(i + 1), w.get(i + 2)) {
                    if let Some(ns) = ns.strip_prefix('<').and_then(|n| n.strip_suffix('>')) {
                        prefixes.insert(p.clone(), ns.to_string());
                    }
                }
                i += 3
            }
            "BASE" => i += 2,
            "SELECT" => {
                form = Some(Form::Select);
                break;
            }
            "CONSTRUCT" => {
                form = Some(Form::Construct);
                break;
            }
            "ASK" => {
                form = Some(Form::Ask);
                break;
            }
            other => {
                return Err(refuse(
                    "query",
                    format!("a lens query is SELECT, CONSTRUCT or ASK, not `{other}`"),
                ))
            }
        }
    }
    let form = form.ok_or_else(|| refuse("query", "a lens query is SELECT, CONSTRUCT or ASK"))?;
    let mut services: Vec<String> = Vec::new();
    let mut limit = false;
    for (j, word) in w.iter().enumerate() {
        let up = word.to_ascii_uppercase();
        if UPDATE.contains(&up.as_str()) {
            return Err(refuse(
                "query",
                format!("a lens query is read-only: `{up}` is an update"),
            ));
        }
        if up == "LIMIT" {
            limit = true;
        }
        if up == "SERVICE" {
            let mut k = j + 1;
            if w.get(k).is_some_and(|s| s.eq_ignore_ascii_case("SILENT")) {
                k += 1;
            }
            let named = w.get(k).map(String::as_str).unwrap_or("(none)");
            let resolved =
                if let Some(i) = named.strip_prefix('<').and_then(|n| n.strip_suffix('>')) {
                    Some(i.to_string())
                } else {
                    named.find(':').and_then(|at| {
                        prefixes
                            .get(&named[..=at])
                            .map(|ns| format!("{ns}{}", &named[at + 1..]))
                    })
                };
            let Some(resolved) = resolved else {
                return Err(refuse(
                    "query",
                    format!("SERVICE {named} names no IRI a lens may list"),
                ));
            };
            if !services.contains(&resolved) {
                services.push(resolved);
            }
        }
    }
    Ok(Prepared {
        form,
        query: query.to_string(),
        services,
        limit,
    })
}

impl Prepared {
    /// Its form, when every SERVICE it names is one `allowed` lists; else `InvalidArgument`.
    pub fn check(&self, allowed: &[String]) -> Result<Form, Error> {
        if let Some(s) = self.services.iter().find(|s| !allowed.contains(s)) {
            return Err(refuse(
                "query",
                format!("SERVICE {s} is not one the lens lists"),
            ));
        }
        Ok(self.form)
    }
}

/// `query`'s form when it is read-only ([`prepare`]) and names no SERVICE but those `services`
/// lists. Anything else is `InvalidArgument`.
pub fn read_only(query: &str, services: &[String]) -> Result<Form, Error> {
    prepare(query)?.check(services)
}

/// One result row: variable → value.
pub type Row = BTreeMap<String, String>;

/// A result read: its variables, its rows, whether it was past the bound.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Rows {
    /// The projected variables.
    pub vars: Vec<String>,
    /// The rows, at most the bound.
    pub rows: Vec<Row>,
    /// Whether there were more.
    pub past: bool,
}

/// A SPARQL results JSON document as rows, the first `bound` kept.
pub fn rows(bytes: &[u8], bound: usize) -> Result<Rows, Error> {
    let doc: Json = serde_json::from_slice(bytes)
        .map_err(|e| refuse("result", format!("not SPARQL results JSON: {e}")))?;
    let vars = doc["head"]["vars"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    if let Some(b) = doc.get("boolean").and_then(Json::as_bool) {
        let mut r = Row::new();
        r.insert("answer".into(), if b { "yes" } else { "no" }.into());
        return Ok(Rows {
            vars: vec!["answer".into()],
            rows: vec![r],
            past: false,
        });
    }
    let all = doc["results"]["bindings"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let past = all.len() > bound;
    let rows = all
        .iter()
        .take(bound)
        .map(|b| {
            b.as_object()
                .map(|o| {
                    o.iter()
                        .filter_map(|(k, v)| Some((k.clone(), v["value"].as_str()?.to_string())))
                        .collect()
                })
                .unwrap_or_default()
        })
        .collect();
    Ok(Rows { vars, rows, past })
}

/// The home mapping: (prefix, href), longest prefix first.
#[derive(Debug, Clone, Default)]
pub struct Home(Vec<(String, String)>);

impl Home {
    /// A prefix table, `[{"prefix": …, "href": …}]`.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let t: Vec<BTreeMap<String, String>> = serde_json::from_slice(bytes)
            .map_err(|e| refuse("home", format!("not a prefix table: {e}")))?;
        let mut v: Vec<(String, String)> = t
            .into_iter()
            .filter_map(|mut e| Some((e.remove("prefix")?, e.remove("href")?)))
            .collect();
        v.sort_by_key(|(p, _)| std::cmp::Reverse(p.len()));
        Ok(Home(v))
    }

    /// Where `iri` links: its home, else its own view. `None` when it is not an IRI.
    pub fn href(&self, iri: &str) -> Option<String> {
        if !iri.contains(':') || Iri::parse(iri).is_err() {
            return None;
        }
        for (prefix, href) in &self.0 {
            if let Some(rest) = iri.strip_prefix(prefix.as_str()) {
                return Some(format!(
                    "{href}{}",
                    crate::view::href(rest).trim_start_matches("/ui/view/resource/")
                ));
            }
        }
        Some(crate::view::href(iri))
    }
}

/// What a template is filled with.
pub struct Fill<'a> {
    /// The lens's name.
    pub lens: &'a str,
    /// The lens's legend.
    pub legend: &'a str,
    /// The resource.
    pub focus: &'a str,
    /// The lens's bound.
    pub bound: usize,
    /// The result.
    pub rows: &'a Rows,
    /// The result graph's edges (`?from`, `?edge`, `?to`, as labels): a CONSTRUCT's; a
    /// SELECT names none.
    pub edges: &'a Rows,
    /// The home mapping.
    pub home: &'a Home,
}

fn sub(s: &str, fill: &Fill<'_>, row: Option<&Row>) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let Some(close) = rest[open..].find('}') else {
            out.push_str(&rest[open..]);
            return out;
        };
        let key = &rest[open + 1..open + close];
        let value = |v: &str| row.and_then(|r| r.get(v)).cloned().unwrap_or_default();
        let got = match key {
            "lens" => Some(fill.lens.to_string()),
            "legend" => Some(fill.legend.to_string()),
            "focus" => Some(fill.focus.to_string()),
            "bound" => Some(fill.bound.to_string()),
            "shown" => Some(fill.rows.rows.len().to_string()),
            k if k.starts_with("home:?") => {
                Some(fill.home.href(&value(&k[6..])).unwrap_or_default())
            }
            k if k.starts_with('?') => Some(value(&k[1..])),
            _ => None,
        };
        match got {
            Some(v) => out.push_str(&v),
            None => out.push_str(&rest[open..=open + close]),
        }
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    out
}

/// A `link` with no href (its value is not an IRI) becomes its text; an empty label its value.
fn tidy(mut n: Json) -> Json {
    if n["kind"] == "link" && n["properties"]["href"].as_str().unwrap_or("").is_empty() {
        let label = n["properties"]["label"].as_str().unwrap_or("").to_string();
        n = json!({"kind": "text", "properties": {"content": if label.is_empty() { "(none)".into() } else { label }}});
    }
    for key in ["label", "content", "caption"] {
        if n["properties"][key].as_str() == Some("") {
            n["properties"][key] = json!("(none)");
        }
    }
    n
}

fn walk(
    t: &Json,
    fill: &Fill<'_>,
    row: Option<&Row>,
    group: &[&Row],
    count: &mut usize,
) -> Result<Vec<Json>, Error> {
    if *count > crate::element::MAX_NODES {
        return Err(refuse(
            "renderer",
            format!("the expansion passes {} nodes", crate::element::MAX_NODES),
        ));
    }
    match t {
        Json::String(s) => Ok(vec![Json::String(sub(s, fill, row))]),
        Json::Array(a) => {
            let mut out = Vec::new();
            for x in a {
                out.extend(walk(x, fill, row, group, count)?);
            }
            Ok(vec![Json::Array(out)])
        }
        Json::Object(o) if o.contains_key("past") => {
            if !fill.rows.past {
                return Ok(Vec::new());
            }
            *count += 1;
            let words = sub(o["past"].as_str().unwrap_or(""), fill, row);
            Ok(vec![
                json!({"kind": "text", "properties": {"content": words}}),
            ])
        }
        Json::Object(o) if o.contains_key("repeat") => {
            let each = o
                .get("each")
                .ok_or_else(|| refuse("renderer", "a repeat has no `each`"))?;
            let owned: Vec<Row>;
            let over: Vec<(&Row, Vec<&Row>)> = match o["repeat"].as_str() {
                Some("rows") => match o.get("group").and_then(Json::as_str) {
                    None => fill.rows.rows.iter().map(|r| (r, vec![r])).collect(),
                    Some(var) => {
                        let mut groups: Vec<(&Row, Vec<&Row>)> = Vec::new();
                        for r in &fill.rows.rows {
                            match groups.iter_mut().find(|(g, _)| g.get(var) == r.get(var)) {
                                Some((_, members)) => members.push(r),
                                None => groups.push((r, vec![r])),
                            }
                        }
                        groups
                    }
                },
                Some("group") => group.iter().map(|r| (*r, vec![*r])).collect(),
                Some("edges") => {
                    // Every edge, never the first few: the replacement says what the picture
                    // draws. The scene's own bound holds them.
                    if fill.edges.rows.len() > crate::scene::MAX_EDGES {
                        return Err(refuse(
                            "renderer",
                            "an edge repeat passes the scene's bound",
                        ));
                    }
                    fill.edges.rows.iter().map(|r| (r, vec![r])).collect()
                }
                Some("vars") => {
                    owned = fill
                        .rows
                        .vars
                        .iter()
                        .map(|v| Row::from([("var".to_string(), v.clone())]))
                        .collect();
                    owned.iter().map(|r| (r, vec![r])).collect()
                }
                Some("cells") => {
                    owned = fill
                        .rows
                        .vars
                        .iter()
                        .map(|v| {
                            let value = row.and_then(|r| r.get(v)).cloned().unwrap_or_default();
                            Row::from([("value".to_string(), value)])
                        })
                        .collect();
                    owned.iter().map(|r| (r, vec![r])).collect()
                }
                other => {
                    return Err(refuse(
                        "renderer",
                        format!("`repeat` is rows, group, edges, vars or cells, not {other:?}"),
                    ))
                }
            };
            if o["repeat"] != "edges" && over.len() > fill.bound.max(fill.rows.vars.len()) {
                return Err(refuse("renderer", "a repeat passes the lens's bound"));
            }
            let mut out = Vec::new();
            for (r, members) in over {
                out.extend(walk(each, fill, Some(r), &members, count)?);
            }
            Ok(out)
        }
        Json::Object(o) => {
            let mut n = serde_json::Map::new();
            for (k, v) in o {
                let mut got = walk(v, fill, row, group, count)?;
                n.insert(k.clone(), got.pop().unwrap_or(Json::Null));
            }
            if n.contains_key("kind") {
                *count += 1;
                Ok(vec![tidy(Json::Object(n))])
            } else {
                Ok(vec![Json::Object(n)])
            }
        }
        v => Ok(vec![v.clone()]),
    }
}

/// A renderer template filled with a result: a model node, validated.
pub fn expand(template: &Json, fill: &Fill<'_>) -> Result<Node, Error> {
    if fill.bound > MAX_BOUND {
        return Err(refuse("renderer", format!("a bound past {MAX_BOUND}")));
    }
    let mut count = 0;
    let mut out = walk(template, fill, None, &[], &mut count)?;
    let one = out
        .pop()
        .filter(|_| out.is_empty())
        .ok_or_else(|| refuse("renderer", "a renderer is one node"))?;
    let node: Node = serde_json::from_value(one)
        .map_err(|e| refuse("renderer", format!("not a model node: {e}")))?;
    model::validate(&node)?;
    Ok(node)
}

/// The precedence of a resource's lenses: instance, dataset, each type, then [`DEFAULT`];
/// each name once, at its first place.
pub fn precedence(lists: &[Vec<String>]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for name in lists
        .iter()
        .flatten()
        .chain(std::iter::once(&DEFAULT.to_string()))
    {
        if !out.contains(name) {
            out.push(name.clone());
        }
    }
    out
}

async fn optional(inv: &Invocation<'_>, at: &str) -> Result<Option<Vec<u8>>, Error> {
    match inv.issue(Request::new(Verb::Source, iri(at)?)).await {
        Ok(r) => Ok(Some(r.bytes)),
        Err(Error::NotFound(_) | Error::Unresolved(_)) => Ok(None),
        Err(e) => Err(e),
    }
}

fn names(bytes: Option<Vec<u8>>, at: &str) -> Result<Vec<String>, Error> {
    match bytes {
        None => Ok(Vec::new()),
        Some(b) => serde_json::from_slice(&b)
            .map_err(|e| refuse("lens", format!("{at}: not a list of lens names: {e}"))),
    }
}

async fn read_lens(inv: &Invocation<'_>, lens: &str) -> Result<Lens, Error> {
    let at = format!("{NAME}{lens}");
    let rep = inv.issue(Request::new(Verb::Source, iri(&at)?)).await?;
    Lens::parse(&rep.bytes, &at)
}

async fn lenses_of(inv: &Invocation<'_>, resource: &str) -> Result<Vec<String>, Error> {
    let mut lists = vec![
        names(
            optional(inv, &format!("{INSTANCE}{resource}")).await?,
            resource,
        )?,
        names(optional(inv, DATASET).await?, DATASET)?,
    ];
    // Its types, from its own data; a resource the caller may not read has none here.
    let types: Vec<String> = match inv.issue(Request::new(Verb::Source, iri(resource)?)).await {
        Ok(r) => match serde_json::from_slice::<Json>(&r.bytes).ok() {
            Some(d) => match &d["@type"] {
                Json::String(s) => vec![s.clone()],
                Json::Array(a) => a
                    .iter()
                    .filter_map(|t| t.as_str().map(String::from))
                    .collect(),
                _ => Vec::new(),
            },
            None => Vec::new(),
        },
        Err(_) => Vec::new(),
    };
    for class in types {
        let at = format!("{TYPE}{class}");
        lists.push(names(optional(inv, &at).await?, &at)?);
    }
    Ok(precedence(&lists))
}

async fn result(
    inv: &Invocation<'_>,
    name: &str,
    lens: &Lens,
    resource: &str,
    chosen: &Selection,
) -> Result<Representation, Error> {
    let prepared = read_prepared(inv, name).await?;
    let form = prepared.form;
    // The lens asks for one row past its bound, so it knows it is past it. The store REFUSES an
    // answer past `max_rows=` rather than cutting it, so the bound is a LIMIT the lens appends to
    // its own query (a query with its own LIMIT keeps it, inside the store's answer budget).
    let text = if form == Form::Select && !prepared.limit {
        format!("{}\nLIMIT {}", prepared.query, lens.bound + 1)
    } else {
        prepared.query
    };
    let mut bindings = json!({ lens.focus.clone(): {"type": "uri", "value": resource} });
    // Each chosen facet value is a TERM bound to its variable, never text in the query.
    for f in &lens.facets {
        if let Some(v) = chosen.get(&f.name) {
            bindings[f.variable()] = f.term(v);
        }
    }
    let mut request = Request::new(Verb::Source, iri(form.endpoint())?)
        .with_arg("query", ArgRef::Inline(text.into_bytes()))
        .with_arg(
            "bindings",
            ArgRef::Inline(bindings.to_string().into_bytes()),
        );
    if form == Form::Construct {
        // The result graph as node objects: what the `graph` renderer's scene reads.
        request = request.with_arg("as", ArgRef::Inline(GRAPH.into()));
    }
    if !lens.graphs.is_empty() {
        // `{focus}` names the resource itself as a source: a graph argument, resolved through
        // the kernel under the caller's capability, never text in the query.
        let graphs: Vec<String> = lens
            .graphs
            .iter()
            .map(|g| {
                if g == "{focus}" {
                    resource.to_string()
                } else {
                    g.clone()
                }
            })
            .collect();
        for g in &graphs {
            iri(g)?;
        }
        request = request.with_arg("graph", ArgRef::Inline(graphs.join(" ").into_bytes()));
    }
    inv.issue(request).await
}

async fn read_prepared(inv: &Invocation<'_>, name: &str) -> Result<Prepared, Error> {
    let at = format!("{NAME}{name}{PREPARED_SUFFIX}");
    let rep = inv.issue(Request::new(Verb::Source, iri(&at)?)).await?;
    serde_json::from_slice(&rep.bytes)
        .map_err(|e| Error::Endpoint(format!("{at}: not a prepared query: {e}")))
}

/// A CONSTRUCT's result graph as rows: its nodes (`?node`, `?label`), by label, the first
/// `bound` kept; and every edge (`?from`, `?edge`, `?to`, as labels), in the graph's order.
pub fn graph_rows(bytes: &[u8], bound: usize) -> Result<(Rows, Rows), Error> {
    let graph = crate::scene::read_graph(bytes)?;
    let mut nodes: Vec<(&String, &String)> = graph
        .nodes
        .iter()
        .map(|(id, (label, _))| (label, id))
        .collect();
    nodes.sort();
    let label = |id: &str| {
        graph
            .nodes
            .get(id)
            .map(|(l, _)| l.clone())
            .unwrap_or_default()
    };
    let all: Vec<Row> = nodes
        .iter()
        .map(|(label, id)| {
            Row::from([
                ("node".to_string(), (*id).clone()),
                ("label".to_string(), (*label).clone()),
            ])
        })
        .collect();
    let past = all.len() > bound;
    let rows = Rows {
        vars: vec!["node".into(), "label".into()],
        rows: all.into_iter().take(bound).collect(),
        past,
    };
    let edges = Rows {
        vars: vec!["from".into(), "edge".into(), "to".into()],
        rows: graph
            .edges
            .iter()
            .map(|(from, p, to)| {
                Row::from([
                    ("from".to_string(), label(from)),
                    ("edge".to_string(), crate::scene::short(p)),
                    ("to".to_string(), label(to)),
                ])
            })
            .collect(),
        past: false,
    };
    Ok((rows, edges))
}

fn words_node(content: impl Into<String>) -> Node {
    serde_json::from_value(json!({"kind": "text", "properties": {"content": content.into()}}))
        .expect("a text node")
}

/// The lensed view of `resource` through lens `name`, its facets `chosen`.
pub async fn lensed(
    inv: &Invocation<'_>,
    name: &str,
    resource: &str,
    chosen: &Selection,
) -> Result<Node, Error> {
    let lens = read_lens(inv, name).await?;
    let chosen = lens.select(chosen.iter().map(|(k, v)| (k.as_str(), v.as_str())))?;
    let choices = lenses_of(inv, resource).await?;
    let home = match optional(inv, HOME_TABLE).await? {
        Some(b) => Home::parse(&b)?,
        None => Home::default(),
    };
    let renderer_at = format!("{RENDERER}{}", lens.renderer);
    let template: Json = serde_json::from_slice(
        &inv.issue(Request::new(Verb::Source, iri(&renderer_at)?))
            .await?
            .bytes,
    )
    .map_err(|e| refuse("renderer", format!("{renderer_at}: {e}")))?;
    // Each facet combination is its own request, so its own cached result.
    let mut request = Request::new(
        Verb::Source,
        iri(&format!("{NAME}{name}:focus:{resource}"))?,
    );
    for (k, v) in &chosen {
        request = request.with_arg(k.clone(), ArgRef::Inline(v.clone().into_bytes()));
    }
    let body = match inv.issue(request).await {
        Ok(r) => {
            let (mut rows, edges) = if r.repr_type.media_type.as_str() == GRAPH {
                graph_rows(&r.bytes, lens.bound)?
            } else {
                (rows(&r.bytes, lens.bound)?, Rows::default())
            };
            // The focus is the resource the view is about, said in the heading, not a column.
            rows.vars.retain(|v| *v != lens.focus);
            let fill = Fill {
                lens: name,
                legend: &lens.legend,
                focus: resource,
                bound: lens.bound,
                rows: &rows,
                edges: &edges,
                home: &home,
            };
            if rows.rows.is_empty() {
                words_node(format!(
                    "{}: the query found nothing for {resource}.",
                    lens.legend
                ))
            } else {
                expand(&template, &fill)?
            }
        }
        Err(Error::Denied(_)) => words_node(format!(
            "{}: withheld. You may not read what this lens reads for {resource}.",
            lens.legend
        )),
        Err(e) => return Err(e),
    };
    let mut controls = vec![chooser_node(&choices, name)];
    for f in &lens.facets {
        for v in &f.values {
            let mut field = json!({"kind": "field", "properties": {
                "label": format!("{}: {v}", f.label), "name": f.name, "input": "checkbox", "value": v}});
            if chosen.get(&f.name) == Some(v) {
                field["hint"] = json!({ CHECKED: "checked" });
            }
            controls.push(field);
        }
    }
    controls.push(json!({"kind": "button", "properties": {"label": "Show"}}));
    let mut children = vec![
        json!({"kind": "heading", "properties": {"content": lens.legend, "level": 2}}),
        // Before the form, never against its button: a link under the button would crowd
        // its target (WCAG 2.2 SC 2.5.8, axe target-size; ledger #3538).
        json!({"kind": "link", "properties": {"label": "The resource's own view", "href": crate::view::href(resource)}}),
        // A GET form with no action submits to the page it is on: the lens and the facets
        // arrive as query parameters, and the page's route reads them.
        json!({"kind": "form", "properties": {"label": "Lens and facets", "method": "get"}, "children": controls}),
    ];
    if !chosen.is_empty() {
        let said: Vec<String> = lens
            .facets
            .iter()
            .filter_map(|f| chosen.get(&f.name).map(|v| format!("{}: {v}", f.label)))
            .collect();
        children.push(json!({"kind": "text", "properties": {"content": format!("Showing only {}.", said.join("; "))}}));
    }
    let view = json!({"kind": "group", "properties": {"label": lens.legend}, "children": children});
    let mut view: Node = serde_json::from_value(view).map_err(|e| refuse("lens", e))?;
    view.children.push(body);
    model::validate(&view)?;
    Ok(view)
}

/// The html face's hint that a checkbox starts checked.
pub const CHECKED: &str = "urn:iki:ui:hint:html:checked";

fn chooser_node(choices: &[String], chosen: &str) -> Json {
    let options: Vec<Json> = choices
        .iter()
        .map(|c| json!({"kind": "option", "properties": {"label": c, "value": c}}))
        .collect();
    // Radio buttons where the face claims the hint (ledger #3537), else its select.
    json!({"kind": "choice", "properties": {"label": "Lens", "name": "lens", "value": chosen},
        "hint": { model::RADIO: "radio" }, "children": options})
}

/// The lenses a resource declares (instance, dataset, types), without the default; empty
/// when it declares none.
pub async fn declared(inv: &Invocation<'_>, resource: &str) -> Result<Vec<String>, Error> {
    let all = lenses_of(inv, resource).await?;
    Ok(if all.len() == 1 { Vec::new() } else { all })
}

/// The lens chooser a resource's own view offers when it declares lenses: a GET form to the
/// html route of its first lens, whose `lens=` picks any of them.
pub async fn chooser(inv: &Invocation<'_>, resource: &str) -> Result<Option<Node>, Error> {
    let names = declared(inv, resource).await?;
    let Some(first) = names.first() else {
        return Ok(None);
    };
    let form = json!({"kind": "form", "properties": {
        "label": "Lenses", "method": "get", "action": html_href(first, resource, &Selection::new()),
        "description": "See this resource through one of its lenses."}, "children": [
        chooser_node(&names, first),
        {"kind": "button", "properties": {"label": "Open"}},
    ]});
    Ok(Some(
        serde_json::from_value(form).map_err(|e| refuse("lens", e))?,
    ))
}

/// The html screen name of a lensed view: `lens:{lens}[:facets:{k=v&…}]:resource:{iri}`,
/// the facet values percent-encoded.
pub fn screen_name(lens: &str, resource: &str, chosen: &Selection) -> String {
    if chosen.is_empty() {
        return format!("lens:{lens}:resource:{resource}");
    }
    let facets: Vec<String> = chosen
        .iter()
        .map(|(k, v)| format!("{k}={}", pct(v)))
        .collect();
    format!(
        "lens:{lens}:facets:{}:resource:{resource}",
        facets.join("&")
    )
}

/// Where a browser opens a lensed view as a PAGE, in the shell (ledger #3538):
/// `/lens/name/{lens}/resource/{iri}`, the IRI percent-encoded as one segment, and the chosen
/// facets (with `lens=`, so they are read) as the query. A host's door routes it onto its page
/// family's `{base}page:lens:{lens}:resource:{+iri}` ([`crate::Pages::lenses`]).
pub fn html_href(lens: &str, resource: &str, chosen: &Selection) -> String {
    let mut out = format!("{PAGE_PATH}{}/resource/{}", pct(lens), pct(resource));
    if !chosen.is_empty() {
        out.push_str(&format!("?lens={}", pct(lens)));
        for (k, v) in chosen {
            out.push_str(&format!("&{}={}", pct(k), pct(v)));
        }
    }
    out
}

/// The door path a lensed page is served at, before its lens's name: `/lens/name/`.
pub const PAGE_PATH: &str = "/lens/name/";

/// Where htmx swaps a lensed view in as a FRAGMENT, no shell: the html face of its screen,
/// `/ui/html:screen:lens:…` (the door's `/ui/{+rest}` route).
pub fn fragment_href(lens: &str, resource: &str, chosen: &Selection) -> String {
    let name = screen_name(lens, resource, chosen);
    let mut out = String::from("/ui/html:screen:");
    for b in name.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~:@!$'()*+,;=".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn lens_name_ok(lens: &str) -> bool {
    !lens.is_empty()
        && lens
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
}

/// The screen a lensed PAGE shows: its name (`lens:{lens}[:facets:{…}]:resource:{iri}`, the
/// IRI as the door gave it, percent-encoded or not) read, the IRI decoded and checked (an
/// absolute IRI with no `?`, `#`, `%` or space, so it stays one name), and the request's `lens=`
/// and facets folded in ([`fold`]). Anything else is refused with `InvalidArgument` before
/// anything is read.
pub fn page_screen(inv: &Invocation<'_>, name: &str) -> Result<String, Error> {
    let (lens, chosen, resource) = parse_screen(name).ok_or_else(|| {
        refuse(
            "name",
            format!("`{name}` is not lens:{{lens}}:resource:{{iri}}"),
        )
    })??;
    if !lens_name_ok(&lens) {
        return Err(refuse("lens", format!("`{lens}` is not a lens name")));
    }
    let resource = unpct(&resource)?;
    if resource
        .chars()
        .any(|c| c.is_whitespace() || matches!(c, '?' | '#' | '%' | '<' | '>' | '"'))
    {
        return Err(refuse(
            "iri",
            format!("`{resource}`: a lensed page's resource carries no ? # % or space"),
        ));
    }
    iri(&resource)?;
    fold(inv, &screen_name(&lens, &resource, &chosen))
}

/// `urn:iki:ui:lens:title:lens:{lens}[:facets:{…}]:resource:{iri}`: a lensed page's title,
/// the lens's legend and the resource (its name when the lens cannot be read).
pub const TITLE: &str = "urn:iki:ui:lens:title:";

struct LensTitle;

impl ikigai_core::Grammar for LensTitle {
    fn match_iri(&self, iri: &Iri) -> Option<ikigai_core::Bindings> {
        let name = iri.as_str().strip_prefix(TITLE)?;
        let (lens, _, resource) = parse_screen(name)?.ok()?;
        let mut b = ikigai_core::Bindings::new();
        b.insert("lens", lens.as_str());
        b.insert("iri", resource.as_str());
        Some(b)
    }

    fn pattern(&self) -> String {
        format!("{TITLE}lens:{{lens}}:resource:{{iri}}")
    }
}

fn pct(v: &str) -> String {
    let mut out = String::new();
    for b in v.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn unpct(v: &str) -> Result<String, Error> {
    let b = v.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let h = v
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
                .ok_or_else(|| refuse("facets", "a bad percent escape"))?;
            out.push(h);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| refuse("facets", "not UTF-8"))
}

/// A lensed screen name as a compose template expands it (`{screen}`, its `:` escaped),
/// unescaped once; any other name as it is.
pub fn unescape_screen(name: &str) -> String {
    if name.starts_with("lens%3A") || name.starts_with("lens%3a") {
        unpct(name).unwrap_or_else(|_| name.to_string())
    } else {
        name.to_string()
    }
}

/// A lensed screen name parsed: (lens, facets, resource). `None` when it is not one.
pub fn parse_screen(name: &str) -> Option<Result<(String, Selection, String), Error>> {
    let rest = name.strip_prefix("lens:")?;
    let (lens, rest) = rest.split_once(':')?;
    let (facets, resource) = match rest.strip_prefix("facets:") {
        Some(r) => r.split_once(":resource:")?,
        None => ("", rest.strip_prefix("resource:")?),
    };
    if lens.is_empty() || resource.is_empty() {
        return None;
    }
    let mut chosen = Selection::new();
    for pair in facets.split('&').filter(|p| !p.is_empty()) {
        let (k, v) = match pair.split_once('=') {
            Some(kv) => kv,
            None => return Some(Err(refuse("facets", format!("`{pair}` is not name=value")))),
        };
        match unpct(v) {
            Ok(v) => {
                chosen.insert(k.to_string(), v);
            }
            Err(e) => return Some(Err(e)),
        }
    }
    Some(Ok((lens.to_string(), chosen, resource.to_string())))
}

/// The screen a browser's request names: a lensed screen with the request's `lens=` and facet
/// parameters folded in (the form sends `lens` always, so with it the facets are exactly the
/// request's), else `screen` itself.
pub fn fold(inv: &Invocation<'_>, screen: &str) -> Result<String, Error> {
    let screen = &unescape_screen(screen);
    let Some(parsed) = parse_screen(screen) else {
        return Ok(screen.to_string());
    };
    let (mut lens, mut chosen, resource) = parsed?;
    if inv.request.args.contains_key("lens") {
        lens = inv.inline_str("lens")?.to_string();
        if lens.is_empty()
            || !lens
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        {
            return Err(refuse("lens", format!("`{lens}` is not a lens name")));
        }
        chosen = given(inv)?
            .into_iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
    }
    Ok(screen_name(&lens, &resource, &chosen))
}

/// `urn:iki:ui:screen:name:lens:{lens}[:facets:{…}]:resource:{iri}`: the lensed view as a
/// screen, so the html face renders it.
struct LensScreen;

impl ikigai_core::Grammar for LensScreen {
    fn match_iri(&self, iri: &Iri) -> Option<ikigai_core::Bindings> {
        let name = iri.as_str().strip_prefix(crate::screen::SCREEN_PREFIX)?;
        let (lens, _, resource) = parse_screen(name)?.ok()?;
        let mut b = ikigai_core::Bindings::new();
        b.insert("name", name);
        b.insert("lens", lens.as_str());
        b.insert("iri", resource.as_str());
        Some(b)
    }

    fn pattern(&self) -> String {
        format!(
            "{}lens:{{lens}}:resource:{{iri}}",
            crate::screen::SCREEN_PREFIX
        )
    }
}

/// The lensed screens, in [`crate::space`].
pub(crate) fn bind_screens(space: EndpointSpace) -> EndpointSpace {
    space.bind(
        LensTitle,
        AsyncFnEndpoint::new("ui-lens-title", |inv: &Invocation<'_>| -> InvokeFuture<'_> {
            Box::pin(async move {
                let lens = inv.bindings.get("lens").unwrap_or_default().to_string();
                let resource = inv.bindings.get("iri").unwrap_or_default().to_string();
                let legend = match read_lens(inv, &lens).await {
                    Ok(l) => l.legend,
                    Err(_) => lens,
                };
                Ok(Representation::new(
                    ReprType::new(crate::HTML),
                    format!("{legend}: {resource}").into_bytes(),
                )
                .cacheable())
            })
        })
        .with_description(
            Description::new("ui-lens-title")
                .input(ikigai_core::ArgSpec::new("lens").summary("the lens's name").class("http://www.w3.org/2001/XMLSchema#string").binding().example("contact-list"))
                .input(ikigai_core::ArgSpec::new("iri").summary("the resource").class(XSD_ANY_URI).binding().example(crate::scene::EXAMPLE_GRAPH))
                .title("A lensed page's title")
                .summary("The title a lensed page's shell shows: the lens's legend and the resource, as text.")
                .verb(Verb::Source)
                .output(crate::HTML),
        ),
    )
    .bind(
        LensScreen,
        AsyncFnEndpoint::new("ui-screen-lens", |inv: &Invocation<'_>| -> InvokeFuture<'_> {
            Box::pin(async move {
                let name = inv.bindings.get("name").unwrap_or_default().to_string();
                let (lens, chosen, resource) =
                    parse_screen(&name).ok_or_else(|| Error::NotFound(name.clone()))??;
                iri(&resource)?;
                let n = lensed(inv, &lens, &resource, &chosen).await?;
                crate::model_face(inv, &n)
            })
        })
        .with_description(
            Description::new("ui-screen-lens")
                .input(ikigai_core::ArgSpec::new("lens").summary("the lens's name").class("http://www.w3.org/2001/XMLSchema#string").binding().example("contact-list"))
                .input(ikigai_core::ArgSpec::new("iri").summary("the resource").class(XSD_ANY_URI).binding().example(crate::scene::EXAMPLE_GRAPH))
                .title("A lensed view as a screen")
                .summary("The lensed view, named as a screen so the html face renders it: urn:iki:ui:html:screen:lens:{lens}:resource:{iri} opens it in a browser, and its form's lens= and facet parameters pick another.")
                .verb(Verb::Source)
                .output(model::MEDIA_TYPE)
                .output(model::JSON_LD),
        ),
    )
}

fn data(bytes: Vec<u8>, media: &str) -> Representation {
    Representation::new(ReprType::new(media), bytes).cacheable()
}

/// `urn:iki:ui:lens:name:{lens}:focus:{iri}`.
pub(crate) fn result_endpoint() -> AsyncFnEndpoint {
    AsyncFnEndpoint::new("ui-lens-result", |inv: &Invocation<'_>| -> InvokeFuture<'_> {
        Box::pin(async move {
            let name = inv.bindings.get("lens").unwrap_or_default().to_string();
            let resource = inv.bindings.get("iri").unwrap_or_default().to_string();
            iri(&resource)?;
            let lens = read_lens(inv, &name).await?;
            let chosen = lens.select(given(inv)?)?;
            result(inv, &name, &lens, &resource, &chosen).await
        })
    })
    .with_description(
        Description::new("ui-lens-result").input(ikigai_core::ArgSpec::new("lens").summary("the lens's name").class("http://www.w3.org/2001/XMLSchema#string").binding().example("graph")).input(ikigai_core::ArgSpec::new("iri").summary("the resource").class(XSD_ANY_URI).binding().example(crate::scene::EXAMPLE_GRAPH))
            .title("A lens's result for a resource")
            .summary("The lens's read-only query run for the resource, bound as a term to its focus variable, under the caller's capability. A SELECT or ASK answers with SPARQL results JSON; a CONSTRUCT with its result graph as JSON-LD (application/ld+json), the face the graph renderer's scene reads.")
            .verb(Verb::Source)
            .output(RESULTS),
    )
}

/// [`PREPARE`]: a lens query (`content`, SPARQL) to its prepared form, [`PREPARED`].
pub(crate) fn prepare_endpoint() -> AsyncFnEndpoint {
    AsyncFnEndpoint::new(
        "ui-lens-prepare",
        |inv: &Invocation<'_>| -> InvokeFuture<'_> {
            Box::pin(async move {
                let text = std::str::from_utf8(inv.inline_arg("content")?)
                    .map_err(|_| refuse("content", "a lens query is UTF-8"))?;
                let prepared = prepare(text)?;
                PREPARED_COUNT.fetch_add(1, Ordering::SeqCst);
                Ok(data(
                    serde_json::to_vec(&prepared).expect("a prepared query"),
                    PREPARED,
                ))
            })
        },
    )
    .with_description(
        Description::new("ui-lens-prepare")
            .title("The lens query preparer")
            .summary(
                "A lens's SPARQL query prepared: its words read, checked read-only (SELECT, \
                 CONSTRUCT or ASK, no update keyword), and every SERVICE it names resolved to \
                 an IRI. Pure: the same text is prepared once and cached.",
            )
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .transreptor([QUERY], [PREPARED])
            .lossy()
            .input(
                ikigai_core::ArgSpec::new("content")
                    .summary("the query, SPARQL text")
                    .class("http://www.w3.org/2001/XMLSchema#string"),
            )
            .output(PREPARED),
    )
}

/// `urn:iki:ui:lens:name:{lens}:prepared`.
pub(crate) fn prepared_endpoint() -> AsyncFnEndpoint {
    AsyncFnEndpoint::new("ui-lens-prepared", |inv: &Invocation<'_>| -> InvokeFuture<'_> {
        Box::pin(async move {
            let name = inv.bindings.get("lens").unwrap_or_default().to_string();
            if name.is_empty() || name.contains(':') {
                return Err(Error::NotFound(inv.request.target.as_str().to_string()));
            }
            let lens = read_lens(inv, &name).await?;
            let query_at = lens
                .query
                .clone()
                .unwrap_or_else(|| format!("{NAME}{name}:query"));
            let source = inv
                .issue(Request::new(Verb::Source, iri(&query_at)?))
                .await?;
            let source_expiry = source.expiry;
            if source.bytes.len() > MAX_BYTES {
                return Err(refuse("query", format!("{query_at}: the lens query is too large")));
            }
            let prepared = inv
                .issue(
                    Request::new(Verb::Source, iri(PREPARE)?)
                        .with_arg("as", ArgRef::Inline(PREPARED.into()))
                        .with_arg("content", ArgRef::Inline(source.bytes)),
                )
                .await
                .map_err(|e| match e {
                    Error::InvalidArgument { name: n, detail } => {
                        refuse(&n, format!("{query_at}: {detail}"))
                    }
                    other => other,
                })?;
            let p: Prepared = serde_json::from_slice(&prepared.bytes)
                .map_err(|e| Error::Endpoint(format!("{PREPARE}: {e}")))?;
            p.check(&lens.services)
                .map_err(|e| match e {
                    Error::InvalidArgument { name: n, detail } => {
                        refuse(&n, format!("{query_at}: {detail}"))
                    }
                    other => other,
                })?;
            let mut rep = data(prepared.bytes, PREPARED);
            // No fresher than its source and its preparation.
            rep.expiry = rep
                .expiry
                .most_restrictive(source_expiry)
                .most_restrictive(prepared.expiry);
            Ok(rep)
        })
    })
    .with_description(
        Description::new("ui-lens-prepared")
            .input(ikigai_core::ArgSpec::new("lens").summary("the lens's name").class("http://www.w3.org/2001/XMLSchema#string").binding().example("graph"))
            .title("A lens's query, prepared")
            .summary("The lens's query source (urn:iki:ui:lens:name:{lens}:query, or the IRI its query names) prepared by urn:iki:ui:lens:prepare, its SERVICEs checked against the lens's list. Cached and golden-threaded from the source: an edit to the query re-prepares exactly the lenses over it.")
            .verb(Verb::Source)
            .output(PREPARED),
    )
}

/// `urn:iki:ui:lens:resource:{iri}`.
pub(crate) fn resource_endpoint() -> AsyncFnEndpoint {
    AsyncFnEndpoint::new("ui-lens-resource", |inv: &Invocation<'_>| -> InvokeFuture<'_> {
        Box::pin(async move {
            let resource = inv.bindings.get("iri").unwrap_or_default().to_string();
            iri(&resource)?;
            let names = lenses_of(inv, &resource).await?;
            Ok(data(serde_json::to_vec(&names).expect("names"), "application/json"))
        })
    })
    .with_description(
        Description::new("ui-lens-resource").input(ikigai_core::ArgSpec::new("iri").summary("the resource").class(XSD_ANY_URI).binding().example(crate::scene::EXAMPLE_GRAPH))
            .title("A resource's lenses")
            .summary("Its lens names in precedence: the instance's, the dataset's, each type's, then graph.")
            .verb(Verb::Source)
            .output("application/json"),
    )
}

/// `urn:iki:ui:renderer:name:{name}`: the compiled-in renderers.
pub(crate) fn renderer_endpoint() -> AsyncFnEndpoint {
    AsyncFnEndpoint::new("ui-renderer", |inv: &Invocation<'_>| -> InvokeFuture<'_> {
        Box::pin(async move {
            let name = inv.bindings.get("name").unwrap_or_default().to_string();
            let (_, t) = RENDERERS
                .iter()
                .find(|(n, _)| *n == name)
                .ok_or_else(|| Error::NotFound(format!("{RENDERER}{name}")))?;
            Ok(data(t.as_bytes().to_vec(), "application/json"))
        })
    })
    .with_description(
        Description::new("ui-renderer").input(ikigai_core::ArgSpec::new("name").summary("the renderer's name").class("http://www.w3.org/2001/XMLSchema#string").binding().example("graph"))
            .title("A renderer, a named template")
            .summary("A model template that repeats, boundedly, over a lens's result rows: graph, table, list.")
            .verb(Verb::Source)
            .output("application/json"),
    )
}

/// `urn:iki:ui:home:resource:{iri}`.
pub(crate) fn home_endpoint() -> AsyncFnEndpoint {
    AsyncFnEndpoint::new("ui-home", |inv: &Invocation<'_>| -> InvokeFuture<'_> {
        Box::pin(async move {
            let resource = inv.bindings.get("iri").unwrap_or_default().to_string();
            let home = match optional(inv, HOME_TABLE).await? {
                Some(b) => Home::parse(&b)?,
                None => Home::default(),
            };
            let href = home
                .href(&resource)
                .ok_or_else(|| refuse("iri", format!("`{resource}` is not an IRI")))?;
            Ok(data(href.into_bytes(), "text/plain"))
        })
    })
    .with_description(
        Description::new("ui-home")
            .input(
                ikigai_core::ArgSpec::new("iri")
                    .summary("the resource")
                    .class(XSD_ANY_URI)
                    .binding()
                    .example(crate::scene::EXAMPLE_GRAPH),
            )
            .title("Where a resource links")
            .summary("The home mapping's longest matching prefix, else the resource's own view.")
            .verb(Verb::Source)
            .output("text/plain"),
    )
}

/// `urn:iki:ui:view:lens:{lens}:resource:{iri}`.
pub(crate) fn view_endpoint() -> AsyncFnEndpoint {
    AsyncFnEndpoint::new("ui-view-lens", |inv: &Invocation<'_>| -> InvokeFuture<'_> {
        Box::pin(async move {
            let name = inv.bindings.get("lens").unwrap_or_default().to_string();
            let resource = inv.bindings.get("iri").unwrap_or_default().to_string();
            iri(&resource)?;
            let chosen: Selection = given(inv)?
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            let n = lensed(inv, &name, &resource, &chosen).await?;
            crate::model_face(inv, &n)
        })
    })
    .with_description(
        Description::new("ui-view-lens").input(ikigai_core::ArgSpec::new("lens").summary("the lens's name").class("http://www.w3.org/2001/XMLSchema#string").binding().example("graph")).input(ikigai_core::ArgSpec::new("iri").summary("the resource").class(XSD_ANY_URI).binding().example(crate::scene::EXAMPLE_GRAPH))
            .title("A resource through a lens")
            .summary("The lens's legend, a chooser among the resource's lenses, and its renderer filled with the lens's result.")
            .verb(Verb::Source)
            .output(model::MEDIA_TYPE)
            .output(model::JSON_LD),
    )
}

fn fixed(id: &str, title: &str, media: &'static str, bytes: &'static str) -> FnEndpoint {
    FnEndpoint::new(id, move |_: &Invocation<'_>| {
        Ok(data(bytes.as_bytes().to_vec(), media))
    })
    .with_description(
        Description::new(id)
            .title(title)
            .summary(title)
            .verb(Verb::Source)
            .output(media),
    )
}

fn bind_lens(
    space: EndpointSpace,
    name: &str,
    lens: &'static str,
    query: &'static str,
) -> EndpointSpace {
    space
        .bind(
            Exact::new(format!("{NAME}{name}")),
            fixed(
                &format!("ui-lens-{name}"),
                "A lens",
                "application/json",
                lens,
            ),
        )
        .bind(
            Exact::new(format!("{NAME}{name}:query")),
            fixed(
                &format!("ui-lens-{name}-query"),
                "A lens's query",
                "application/sparql-query",
                query,
            ),
        )
}

/// `urn:iki:ui:lens:name:graph`: the default lens, in [`crate::space`].
pub(crate) fn default_lens() -> FnEndpoint {
    fixed(
        "ui-lens-graph",
        "The default lens",
        "application/json",
        DEFAULT_LENS.0,
    )
}

/// `urn:iki:ui:lens:name:graph:query`: the default lens's query.
pub(crate) fn default_query() -> FnEndpoint {
    fixed(
        "ui-lens-graph-query",
        "The default lens's query",
        "application/sparql-query",
        DEFAULT_LENS.1,
    )
}

/// The example lenses ([`EXAMPLES`]) and the contact graph's instance list, which names them.
/// A host places it AFTER its own lenses, so a host's binding of the same name wins.
pub fn examples() -> EndpointSpace {
    let mut space = EndpointSpace::new();
    for (name, lens, query) in EXAMPLES {
        space = bind_lens(space, name, lens, query);
    }
    space.bind(
        Exact::new(format!("{INSTANCE}{}", crate::scene::EXAMPLE_GRAPH)),
        fixed(
            "ui-lens-instance-contact",
            "The contact graph's lenses",
            "application/json",
            include_str!("../assets/lens/contact-instance.json"),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lens_query_is_read_only() {
        assert_eq!(
            read_only("PREFIX ex: <urn:ex:> SELECT * WHERE { ?focus ?p ?o }", &[]).unwrap(),
            Form::Select
        );
        assert_eq!(
            read_only("CONSTRUCT { ?focus ?p ?o } WHERE { ?focus ?p ?o }", &[]).unwrap(),
            Form::Construct
        );
        assert_eq!(read_only("ASK { ?focus a ?t }", &[]).unwrap(), Form::Ask);
        for bad in [
            "INSERT DATA { <urn:a> <urn:b> <urn:c> }",
            "SELECT * WHERE { ?s ?p ?o } ; DELETE WHERE { ?s ?p ?o }",
            "DESCRIBE ?focus",
            "SELECT * WHERE { SERVICE <http://evil.example/sparql> { ?s ?p ?o } }",
            "PREFIX x: <urn:x:> DROP ALL",
        ] {
            assert!(
                matches!(read_only(bad, &[]), Err(Error::InvalidArgument { .. })),
                "{bad}"
            );
        }
        // Words in strings, comments, IRIs and prefixed names are not keywords.
        read_only(
            "SELECT * WHERE { ?s <urn:delete> \"insert\" . ?s ex:drop ?o } # DELETE",
            &[],
        )
        .unwrap();
        // A listed SERVICE is allowed.
        read_only(
            "SELECT * WHERE { SERVICE <urn:ok> { ?s ?p ?o } }",
            &["urn:ok".into()],
        )
        .unwrap();
    }

    fn sample(n: usize, bound: usize) -> Rows {
        let mut doc =
            json!({"head": {"vars": ["item", "label", "group"]}, "results": {"bindings": []}});
        for i in 0..n {
            doc["results"]["bindings"]
                .as_array_mut()
                .unwrap()
                .push(json!({
                    "item": {"type": "uri", "value": format!("urn:ex:item:{i}")},
                    "label": {"type": "literal", "value": format!("Item {i}")},
                    "group": {"type": "literal", "value": if i % 2 == 0 { "even" } else { "odd" }},
                }));
        }
        rows(doc.to_string().as_bytes(), bound).unwrap()
    }

    fn render(name: &str, rows: &Rows, bound: usize) -> Node {
        let t: Json =
            serde_json::from_str(RENDERERS.iter().find(|r| r.0 == name).unwrap().1).unwrap();
        let home = Home::parse(br#"[{"prefix": "urn:ex:", "href": "/ex/"}]"#).unwrap();
        expand(
            &t,
            &Fill {
                lens: "items",
                legend: "Items",
                focus: "urn:ex:root",
                bound,
                rows,
                edges: &Rows::default(),
                home: &home,
            },
        )
        .unwrap()
    }

    fn all(n: &Node, kind: &str) -> Vec<Node> {
        let mut out: Vec<Node> = n.children.iter().flat_map(|c| all(c, kind)).collect();
        if n.kind == kind {
            out.insert(0, n.clone());
        }
        out
    }

    #[test]
    fn the_list_renderer_groups_and_links_home() {
        let n = render("list", &sample(4, 10), 10);
        let lists = all(&n, "list");
        assert_eq!(lists.len(), 3, "the outer list and one per group");
        let links = all(&n, "link");
        assert_eq!(links.len(), 4);
        assert_eq!(
            links[0].properties["href"],
            model::Value::Text("/ex/item:0".into())
        );
        assert!(all(&n, "text").is_empty(), "nothing past the bound");
    }

    #[test]
    fn past_its_bound_a_renderer_shows_the_first_rows_and_says_so() {
        let r = sample(7, 3);
        assert!(r.past);
        let n = render("table", &r, 3);
        assert_eq!(
            all(&n, "group").len(),
            1 + 1 + 3,
            "the renderer, a header and three rows"
        );
        let words = all(&n, "text");
        let said = format!("{:?}", words.last().unwrap().properties["content"]);
        assert!(said.contains("first 3"), "{said}");
    }

    #[test]
    fn the_graph_renderer_is_a_scene_with_the_selection_beside_it() {
        let n = render("graph", &sample(2, 5), 5);
        let scene = &all(&n, "scene")[0];
        assert!(scene.replacement.is_some());
        assert_eq!(
            all(&n, "link").len(),
            0,
            "rows name ?node, the sample has none: text, not links"
        );
    }

    #[test]
    fn a_repeat_past_the_bound_is_refused() {
        let r = sample(5, 100);
        let t = json!({"kind": "list", "properties": {"label": "x"}, "children": [
            {"repeat": "rows", "each": {"kind": "text", "properties": {"content": "{?label}"}}}]});
        let home = Home::default();
        let e = expand(
            &t,
            &Fill {
                lens: "items",
                legend: "x",
                focus: "urn:a",
                bound: 2,
                rows: &r,
                edges: &Rows::default(),
                home: &home,
            },
        );
        assert!(matches!(e, Err(Error::InvalidArgument { .. })));
        let e = expand(
            &t,
            &Fill {
                lens: "items",
                legend: "x",
                focus: "urn:a",
                bound: MAX_BOUND + 1,
                rows: &r,
                edges: &Rows::default(),
                home: &home,
            },
        );
        assert!(matches!(e, Err(Error::InvalidArgument { .. })));
    }

    #[test]
    fn precedence_is_instance_dataset_types_then_graph() {
        let p = precedence(&[
            vec!["mine".into()],
            vec!["all".into(), "mine".into()],
            vec!["typed".into()],
        ]);
        assert_eq!(p, ["mine", "all", "typed", "graph"]);
    }

    #[test]
    fn a_lens_states_a_bound() {
        assert!(Lens::parse(br#"{"renderer": "table", "bound": 0, "legend": "x"}"#, "l").is_err());
        assert!(Lens::parse(
            br#"{"renderer": "table", "bound": 5000, "legend": "x"}"#,
            "l"
        )
        .is_err());
        let l = Lens::parse(br#"{"renderer": "table", "bound": 50, "legend": "x"}"#, "l").unwrap();
        assert_eq!(l.focus, "focus");
    }
}
