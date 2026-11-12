//! An embedded 3D scene (ledger #3514), first for RDF graphs.
//!
//! | name | what it is |
//! |---|---|
//! | `urn:iki:ui:layout:graph:{graph}` | 3D positions for `{graph}`'s nodes: a force-directed layout from a fixed seed ([`layout`]), cacheable and golden-threaded from the graph |
//! | `urn:iki:ui:scene:graph:{graph}` | the scene datum ([`SCENE_MEDIA_TYPE`]): `{graph}`'s nodes (labels, types, positions), its edges (labels) and a camera; data, never code |
//! | `urn:iki:ui:shape:scene` | the datum's SHACL shape, Turtle ([`SCENE_SHAPE`]); [`validate`] says the same in Rust |
//! | `urn:iki:ui:scene-renderer.js` | the html face's WebGPU renderer, a first-party script ([`RENDERER`]) |
//! | `urn:iki:ui:example:graph:contact` | a small RDF graph, JSON-LD: a contact service's contract as triples, the gallery's scene |
//!
//! **The graph is read under the caller's capability.** Both resources read `{graph}` through
//! the invocation, so a viewer sees only what they may read, and the kernel keys the layout
//! (and the datum) by what was read: an edit to the graph cuts both, and an unchanged graph is
//! never laid out twice.
//!
//! **What a graph is.** `{graph}` is Sourced in its own face, and again `as=application/ld+json`
//! when that face is not JSON (so a refused `as=` never leaves the layout uncacheable), and read as JSON-LD node objects (a top-level array, an `@graph`,
//! or one object): every object with an `@id` is a node, its first `@type` its type, its
//! `rdfs:label` (or `label`, `name`, `title`) its label, and every property whose value is
//! `{"@id": …}` an edge labelled by the property. Bounded by [`MAX_NODES`] and [`MAX_EDGES`].
//!
//! **Sealed.** The datum names nothing for a renderer to fetch: it is the whole scene. The
//! renderer reads only the datum the page's `scene` node names, with fixed shaders.

use std::collections::BTreeMap;

use ikigai_core::{
    ArgRef, ArgSpec, AsyncFnEndpoint, Description, Error, Invocation, InvokeFuture, Iri, ReprType,
    Representation, Request, Verb,
};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

/// The scene datum's media type.
pub const SCENE_MEDIA_TYPE: &str = "application/vnd.ikigai.ui.scene+json";
/// A graph's scene datum is this plus the graph's IRI.
pub const SCENE_PREFIX: &str = "urn:iki:ui:scene:graph:";
/// A graph's layout is this plus the graph's IRI.
pub const LAYOUT_PREFIX: &str = "urn:iki:ui:layout:graph:";
/// Where the datum's shape is served.
pub const SHAPE_IRI: &str = "urn:iki:ui:shape:scene";
/// The datum's SHACL shape, Turtle.
pub const SCENE_SHAPE: &str = include_str!("../assets/scene/scene.ttl");
/// The example graph's IRI.
pub const EXAMPLE_GRAPH: &str = "urn:iki:ui:example:graph:contact";
/// The example graph, JSON-LD.
pub const EXAMPLE_GRAPH_JSON_LD: &str = include_str!("../assets/scene/contact.jsonld");
/// The WebGPU renderer.
pub const RENDERER: crate::Artifact = crate::Artifact {
    iri: "urn:iki:ui:scene-renderer.js",
    id: "ui-scene-renderer-js",
    file: "scene-renderer.js",
    media_type: crate::JS,
    bytes: include_bytes!("../assets/scene-renderer.js"),
};
/// The most nodes a scene draws.
pub const MAX_NODES: usize = 512;
/// The most edges a scene draws.
pub const MAX_EDGES: usize = 4096;
/// The layout's fixed seed.
pub const SEED: u64 = 0x1c_16a1_3514;
/// The layout's iterations.
pub const ITERATIONS: usize = 200;

const JSON_LD: &str = "application/ld+json";
const LABELS: [&str; 5] = [
    "http://www.w3.org/2000/01/rdf-schema#label",
    "rdfs:label",
    "label",
    "name",
    "title",
];

/// A graph node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneNode {
    /// Its IRI.
    pub id: String,
    /// Its label, text.
    pub label: String,
    /// Its type's IRI, if it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    /// Where it is, in scene units (the layout fits a unit sphere).
    pub position: [f64; 3],
}

/// A graph edge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneEdge {
    /// The subject's IRI.
    pub from: String,
    /// The object's IRI.
    pub to: String,
    /// The predicate, as its label.
    pub label: String,
}

/// The camera.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Camera {
    /// Where it is.
    pub position: [f64; 3],
    /// What it looks at.
    pub target: [f64; 3],
    /// Its vertical field of view, degrees.
    pub fov: f64,
}

/// The scene datum: a renderer-neutral scene, data only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scene {
    /// The graph it shows.
    pub graph: String,
    /// The nodes, by IRI order.
    pub nodes: Vec<SceneNode>,
    /// The edges, in order.
    pub edges: Vec<SceneEdge>,
    /// The camera.
    pub camera: Camera,
}

/// A graph as read: nodes (IRI, label, type) and edges, both ordered.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Graph {
    /// IRI → (label, type).
    pub nodes: BTreeMap<String, (String, Option<String>)>,
    /// (from, predicate, to).
    pub edges: Vec<(String, String, String)>,
}

fn refuse(detail: impl Into<String>) -> Error {
    Error::InvalidArgument {
        name: "graph".into(),
        detail: detail.into(),
    }
}

fn short(iri: &str) -> String {
    iri.rsplit(['#', '/', ':'])
        .find(|s| !s.is_empty())
        .unwrap_or(iri)
        .to_string()
}

fn text(v: &Json) -> Option<String> {
    match v {
        Json::String(s) if !s.is_empty() => Some(s.clone()),
        Json::Object(o) => o.get("@value").and_then(text),
        Json::Array(a) => a.iter().find_map(text),
        _ => None,
    }
}

fn ids(v: &Json) -> Vec<String> {
    match v {
        Json::Object(o) => match o.get("@id") {
            Some(Json::String(s)) => vec![s.clone()],
            _ => Vec::new(),
        },
        Json::Array(a) => a.iter().flat_map(ids).collect(),
        _ => Vec::new(),
    }
}

/// A graph from JSON-LD node objects, bounded.
pub fn read_graph(bytes: &[u8]) -> Result<Graph, Error> {
    crate::model::bound(bytes)?;
    let doc: Json =
        serde_json::from_slice(bytes).map_err(|e| refuse(format!("not JSON-LD: {e}")))?;
    let objects: Vec<&Json> = match &doc {
        Json::Array(a) => a.iter().collect(),
        Json::Object(o) => match o.get("@graph") {
            Some(Json::Array(a)) => a.iter().collect(),
            _ => vec![&doc],
        },
        _ => return Err(refuse("not JSON-LD node objects")),
    };
    let mut graph = Graph::default();
    for object in objects {
        let Json::Object(o) = object else { continue };
        let Some(Json::String(id)) = o.get("@id") else {
            continue;
        };
        let label = LABELS
            .iter()
            .find_map(|k| o.get(*k).and_then(text))
            .unwrap_or_else(|| short(id));
        let r#type = match o.get("@type") {
            Some(Json::String(t)) => Some(t.clone()),
            Some(Json::Array(a)) => a.iter().find_map(|t| t.as_str().map(String::from)),
            _ => None,
        };
        graph.nodes.insert(id.clone(), (label, r#type));
        for (key, value) in o {
            if key.starts_with('@') {
                continue;
            }
            for to in ids(value) {
                graph.edges.push((id.clone(), key.clone(), to));
            }
        }
    }
    for (_, _, to) in &graph.edges {
        if !graph.nodes.contains_key(to) {
            graph.nodes.insert(to.clone(), (short(to), None));
        }
        if graph.nodes.len() > MAX_NODES {
            break;
        }
    }
    if graph.nodes.len() > MAX_NODES {
        return Err(refuse(format!("more than {MAX_NODES} nodes")));
    }
    if graph.edges.len() > MAX_EDGES {
        return Err(refuse(format!("more than {MAX_EDGES} edges")));
    }
    Ok(graph)
}

/// FNV-1a, so a node's starting place depends on its IRI and the seed only: adding a node
/// does not move where the others start.
fn hash(seed: u64, s: &str) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325 ^ seed;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

fn round(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

/// A force-directed 3D layout (Fruchterman–Reingold) of `graph` from [`SEED`]: the same graph
/// gives the same positions, rounded to thousandths and fitted to the unit sphere.
pub fn layout(graph: &Graph) -> BTreeMap<String, [f64; 3]> {
    let ids: Vec<&String> = graph.nodes.keys().collect();
    let n = ids.len();
    let index: BTreeMap<&str, usize> = ids
        .iter()
        .enumerate()
        .map(|(i, s)| (s.as_str(), i))
        .collect();
    let mut p: Vec<[f64; 3]> = ids
        .iter()
        .map(|id| {
            let h = hash(SEED, id);
            let c = |shift: u32| ((h >> shift) & 0xffff) as f64 / 65535.0 - 0.5;
            [c(0), c(16), c(32)]
        })
        .collect();
    let edges: Vec<(usize, usize)> = graph
        .edges
        .iter()
        .filter_map(|(a, _, b)| Some((*index.get(a.as_str())?, *index.get(b.as_str())?)))
        .filter(|(a, b)| a != b)
        .collect();
    let k = if n == 0 { 1.0 } else { (1.0 / n as f64).cbrt() };
    let mut t = 0.1;
    for _ in 0..ITERATIONS {
        let mut d = vec![[0.0f64; 3]; n];
        for i in 0..n {
            for j in (i + 1)..n {
                let v = [p[i][0] - p[j][0], p[i][1] - p[j][1], p[i][2] - p[j][2]];
                let dist = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
                let f = k * k / dist;
                for (a, va) in v.iter().enumerate() {
                    d[i][a] += va / dist * f;
                    d[j][a] -= va / dist * f;
                }
            }
        }
        for &(i, j) in &edges {
            let v = [p[i][0] - p[j][0], p[i][1] - p[j][1], p[i][2] - p[j][2]];
            let dist = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
            let f = dist * dist / k;
            for (a, va) in v.iter().enumerate() {
                d[i][a] -= va / dist * f;
                d[j][a] += va / dist * f;
            }
        }
        for i in 0..n {
            let len = (d[i][0] * d[i][0] + d[i][1] * d[i][1] + d[i][2] * d[i][2])
                .sqrt()
                .max(1e-9);
            let step = len.min(t);
            for a in 0..3 {
                p[i][a] += d[i][a] / len * step;
            }
        }
        t *= 0.98;
    }
    // Centre, then fit the unit sphere.
    if n > 0 {
        let mut c = [0.0; 3];
        for q in &p {
            for a in 0..3 {
                c[a] += q[a] / n as f64;
            }
        }
        let r = p
            .iter()
            .map(|q| ((q[0] - c[0]).powi(2) + (q[1] - c[1]).powi(2) + (q[2] - c[2]).powi(2)).sqrt())
            .fold(0.0, f64::max)
            .max(1e-9);
        for q in &mut p {
            for a in 0..3 {
                q[a] = round((q[a] - c[a]) / r);
            }
        }
    }
    ids.into_iter().cloned().zip(p).collect()
}

/// The scene for `graph`, from its positions.
pub fn scene(iri: &str, graph: &Graph, positions: &BTreeMap<String, [f64; 3]>) -> Scene {
    Scene {
        graph: iri.to_string(),
        nodes: graph
            .nodes
            .iter()
            .map(|(id, (label, r#type))| SceneNode {
                id: id.clone(),
                label: label.clone(),
                r#type: r#type.clone(),
                position: positions.get(id).copied().unwrap_or([0.0; 3]),
            })
            .collect(),
        edges: graph
            .edges
            .iter()
            .map(|(from, p, to)| SceneEdge {
                from: from.clone(),
                to: to.clone(),
                label: short(p),
            })
            .collect(),
        camera: Camera {
            position: [0.0, 0.0, 3.0],
            target: [0.0, 0.0, 0.0],
            fov: 50.0,
        },
    }
}

/// Validate a scene datum: what [`SCENE_SHAPE`] says, in Rust. Bounded, every label text, every
/// position finite, every edge between nodes the scene has, the camera's field of view in
/// (0, 180).
pub fn validate(scene: &Scene) -> Result<(), Error> {
    let bad = |d: String| Error::InvalidArgument {
        name: "scene".into(),
        detail: d,
    };
    Iri::parse(scene.graph.as_str()).map_err(|e| bad(format!("graph: {e}")))?;
    if scene.nodes.len() > MAX_NODES || scene.edges.len() > MAX_EDGES {
        return Err(bad("past the scene's bounds".into()));
    }
    let finite = |p: &[f64; 3]| p.iter().all(|x| x.is_finite());
    let mut ids = std::collections::BTreeSet::new();
    for (i, n) in scene.nodes.iter().enumerate() {
        if n.id.is_empty() || n.label.is_empty() || !finite(&n.position) {
            return Err(bad(format!(
                "/nodes/{i}: needs an id, a label and a finite position"
            )));
        }
        if !ids.insert(n.id.as_str()) {
            return Err(bad(format!("/nodes/{i}: {} twice", n.id)));
        }
    }
    for (i, e) in scene.edges.iter().enumerate() {
        if e.label.is_empty() || !ids.contains(e.from.as_str()) || !ids.contains(e.to.as_str()) {
            return Err(bad(format!(
                "/edges/{i}: needs a label and two of the scene's nodes"
            )));
        }
    }
    let c = &scene.camera;
    if !finite(&c.position)
        || !finite(&c.target)
        || c.fov.is_nan()
        || c.fov <= 0.0
        || c.fov >= 180.0
    {
        return Err(bad(
            "/camera: needs finite places and a field of view in (0, 180)".into(),
        ));
    }
    Ok(())
}

fn graph_iri(inv: &Invocation<'_>) -> Result<Iri, Error> {
    let graph = inv.bindings.get("graph").unwrap_or_default();
    Iri::parse(graph.to_string()).map_err(|e| refuse(e.to_string()))
}

/// `{graph}` read under the caller's capability.
async fn source_graph(inv: &Invocation<'_>) -> Result<(String, Graph), Error> {
    let at = graph_iri(inv)?;
    // Its own face first: a graph that already is JSON(-LD) is read as it is, so no refused
    // `as=` (a file endpoint serves only its file's type) leaves the layout uncacheable. Any
    // other face (Turtle, say) is asked for as JSON-LD, through the host's transreptors.
    let own = inv.issue(Request::new(Verb::Source, at.clone())).await?;
    let mt = own.repr_type.media_type.as_str();
    let rep = if mt == JSON_LD || mt == "application/json" || mt.ends_with("+json") {
        own
    } else {
        inv.issue(
            Request::new(Verb::Source, at.clone()).with_arg("as", ArgRef::Inline(JSON_LD.into())),
        )
        .await?
    };
    Ok((at.as_str().to_string(), read_graph(&rep.bytes)?))
}

fn json(media_type: &str, value: &impl Serialize) -> Representation {
    let bytes = serde_json::to_vec_pretty(value).expect("serializes");
    Representation::new(ReprType::new(media_type), bytes).cacheable()
}

fn graph_arg() -> ArgSpec {
    ArgSpec::new("graph")
        .summary("the graph's IRI, read under the caller's capability")
        .class(crate::XSD_STRING)
        .binding()
}

pub(crate) fn layout_endpoint() -> AsyncFnEndpoint {
    AsyncFnEndpoint::new(
        "ui-layout-graph",
        |inv: &Invocation<'_>| -> InvokeFuture<'_> {
            Box::pin(async move {
                let (iri, graph) = source_graph(inv).await?;
                let positions = layout(&graph);
                Ok(json(
                    "application/json",
                    &serde_json::json!({ "graph": iri, "seed": SEED, "positions": positions }),
                ))
            })
        },
    )
    .with_description(
        Description::new("ui-layout-graph")
            .title("A graph's 3D layout")
            .summary(
                "3D positions for a graph's nodes: a force-directed layout from a fixed seed, \
                 fitted to the unit sphere. The graph is read under the caller's capability; \
                 the layout is cacheable and golden-threaded from what was read.",
            )
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .input(graph_arg())
            .output("application/json"),
    )
}

pub(crate) fn scene_endpoint() -> AsyncFnEndpoint {
    AsyncFnEndpoint::new(
        "ui-scene-graph",
        |inv: &Invocation<'_>| -> InvokeFuture<'_> {
            Box::pin(async move {
                let (iri, graph) = source_graph(inv).await?;
                let at = Iri::parse(format!("{LAYOUT_PREFIX}{iri}"))
                    .map_err(|e| Error::Endpoint(e.to_string()))?;
                let rep = inv.issue(Request::new(Verb::Source, at)).await?;
                #[derive(Deserialize)]
                struct Layout {
                    positions: BTreeMap<String, [f64; 3]>,
                }
                let laid: Layout = serde_json::from_slice(&rep.bytes)
                    .map_err(|e| Error::Endpoint(format!("layout: {e}")))?;
                let scene = scene(&iri, &graph, &laid.positions);
                validate(&scene)?;
                Ok(json(SCENE_MEDIA_TYPE, &scene))
            })
        },
    )
    .with_description(
        Description::new("ui-scene-graph")
            .title("A graph as a 3D scene datum")
            .summary(
                "A renderer-neutral scene: the graph's nodes (labels, types, positions from \
                 urn:iki:ui:layout:graph:{graph}), its edges (labels) and a camera. Data, never \
                 code; the graph is read under the caller's capability.",
            )
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .input(graph_arg())
            .output(SCENE_MEDIA_TYPE),
    )
}

pub(crate) fn fixed_endpoint(
    id: &'static str,
    media_type: &'static str,
    bytes: &'static str,
) -> ikigai_core::FnEndpoint {
    ikigai_core::FnEndpoint::new(id, move |_: &Invocation<'_>| {
        Ok(Representation::new(ReprType::new(media_type), bytes.as_bytes().to_vec()).cacheable())
    })
    .with_description(
        Description::new(id)
            .title(id)
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .output(media_type),
    )
}
