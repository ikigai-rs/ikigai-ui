//! Reference nodes (ledger #3361): a screen names a resource, never holds its value.
//!
//! A `reference` node carries `resource` (an absolute IRI) and `fallback` (text), and takes no
//! children and no replacement. The target face ([`crate::target`]) resolves every reference
//! BEFORE custom elements expand and the descriptor projects: it issues `Source` on the
//! resource under the CALLER's capability, in the caller's scope, and splices the model it
//! answers in place of the node. A referenced model may itself hold references (a reference
//! can name another screen); they resolve in turn, bounded by [`MAX_DEPTH`].
//!
//! ## The withheld rung
//!
//! A reference the caller may not read (`Denied`) is WITHHELD: it renders as a `text` node
//! whose content is the reference's fallback text. The value never enters the composed
//! screen. `Invocation::issue` makes a result that swallowed a denial uncacheable, so no
//! cache entry readable under the narrow grant holds the value either; under a grant that
//! may read it, the answer is cached under that grant's fingerprint. Every other error is the
//! face's error.
//!
//! ## Bounds
//!
//! One face resolves at most [`MAX_REFERENCES`] references, nested at most [`MAX_DEPTH`]
//! deep; past either it is refused as `InvalidArgument`.

use std::future::Future;
use std::pin::Pin;

use ikigai_core::{Error, Invocation, Iri, Request, Verb};

use crate::model::{self, Node, Value};

/// The reference node's kind name. Structural, like a custom element: never a target kind.
pub const KIND: &str = "reference";
/// The property naming the referenced resource.
pub const RESOURCE: &str = "resource";
/// The property holding the fallback text, the withheld rung.
pub const FALLBACK: &str = "fallback";
/// The most references one face resolves.
pub const MAX_REFERENCES: usize = 64;
/// How deep references nest (a reference inside a referenced model is one deeper).
pub const MAX_DEPTH: usize = 4;

fn refuse(detail: impl Into<String>) -> Error {
    Error::InvalidArgument {
        name: "reference".into(),
        detail: detail.into(),
    }
}

/// Check a reference node's shape (called by [`model::validate`]).
pub(crate) fn validate(node: &Node) -> Result<(), String> {
    for p in node.properties.keys() {
        if p != RESOURCE && p != FALLBACK {
            return Err(format!("a reference has no property `{p}`"));
        }
    }
    match node.properties.get(RESOURCE) {
        Some(Value::Text(s)) if Iri::parse(s.as_str()).is_ok() => {}
        _ => return Err(format!("a reference needs `{RESOURCE}`, an absolute IRI")),
    }
    match node.properties.get(FALLBACK) {
        Some(Value::Text(s)) if !s.is_empty() => {}
        _ => return Err(format!("a reference needs `{FALLBACK}`, its fallback text")),
    }
    if !node.children.is_empty() || node.replacement.is_some() {
        return Err("a reference takes no children and no replacement".into());
    }
    Ok(())
}

/// The withheld rung: the reference's fallback text, and nothing of the value.
pub fn withheld(node: &Node) -> Node {
    Node {
        kind: "text".into(),
        hint: Default::default(),
        replacement: None,
        properties: [("content".into(), node.properties[FALLBACK].clone())].into(),
        children: Vec::new(),
    }
}

/// Resolve every reference in `node` under `inv`'s capability and scope.
pub async fn resolve(inv: &Invocation<'_>, node: &Node) -> Result<Node, Error> {
    let mut count = 0;
    walk(inv, node, 0, &mut count).await
}

type Walk<'a> = Pin<Box<dyn Future<Output = Result<Node, Error>> + Send + 'a>>;

fn walk<'a>(
    inv: &'a Invocation<'_>,
    node: &'a Node,
    depth: usize,
    count: &'a mut usize,
) -> Walk<'a> {
    Box::pin(async move {
        if node.kind != KIND {
            let mut out = node.clone();
            if let Some(replacement) = &node.replacement {
                out.replacement = Some(Box::new(walk(inv, replacement, depth, count).await?));
            }
            let mut children = Vec::with_capacity(node.children.len());
            for child in &node.children {
                children.push(walk(inv, child, depth, count).await?);
            }
            out.children = children;
            return Ok(out);
        }
        *count += 1;
        if *count > MAX_REFERENCES {
            return Err(refuse(format!("more than {MAX_REFERENCES} references")));
        }
        if depth >= MAX_DEPTH {
            return Err(refuse(format!("references nested past {MAX_DEPTH}")));
        }
        let Some(Value::Text(resource)) = node.properties.get(RESOURCE) else {
            return Err(refuse("a validated reference names its resource"));
        };
        let iri = Iri::parse(resource.as_str()).map_err(|e| refuse(e.to_string()))?;
        match inv.issue(Request::new(Verb::Source, iri)).await {
            Ok(rep) => {
                let referenced = model::parse(&rep.bytes)?;
                walk(inv, &referenced, depth + 1, count).await
            }
            Err(Error::Denied(_)) => Ok(withheld(node)),
            Err(e) => Err(e),
        }
    })
}
