//! Reference nodes (ledger #3361): a screen names a resource, never holds its value.
//!
//! A `reference` node carries `resource` (an absolute IRI) and `fallback` (text), and takes no
//! children and no replacement; its shape is [`model::STRUCTURES`]' (`urn:iki:ui:structure:name:reference`).
//! The target face ([`crate::target::compose`]) resolves every reference BEFORE custom elements
//! expand and the descriptor projects: it issues `Source` on the resource under the CALLER's
//! capability, in the caller's scope, and splices the model it answers in place of the node.
//! A referenced model may itself hold references (a reference can name another screen); they
//! resolve in turn, bounded by [`MAX_DEPTH`]. References an element's expansion introduces
//! resolve in the next round, the same way. Inside a region ([`crate::region`]) references
//! resolve in the region's narrowed space, never the screen's.
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

use ikigai_core::{Error, Invocation, Iri};

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

/// How many references one face has resolved so far, across its rounds and regions.
#[derive(Debug, Default)]
pub struct Budget {
    resolved: usize,
}

/// Resolve every reference in `node` under `inv`'s capability and scope. A region's interior is
/// left as it is: it resolves in the region's own narrowed space ([`crate::region`]).
pub async fn resolve(inv: &Invocation<'_>, node: &Node) -> Result<Node, Error> {
    resolve_in(inv, node, &mut Budget::default()).await
}

/// [`resolve`], counting against `budget`.
pub async fn resolve_in(
    inv: &Invocation<'_>,
    node: &Node,
    budget: &mut Budget,
) -> Result<Node, Error> {
    resolve_within(inv, node, budget, &crate::region::Reach::default()).await
}

/// [`resolve_in`] inside a region: every read goes through `reach` ([`crate::region::Reach`]).
pub async fn resolve_within(
    inv: &Invocation<'_>,
    node: &Node,
    budget: &mut Budget,
    reach: &crate::region::Reach,
) -> Result<Node, Error> {
    walk(inv, reach, node, 0, &mut budget.resolved).await
}

/// Whether `node` holds a reference outside every region.
pub fn any(node: &Node) -> bool {
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if n.kind == KIND {
            return true;
        }
        if n.kind != crate::region::KIND {
            stack.extend(n.children.iter());
            stack.extend(n.replacement.as_deref());
        }
    }
    false
}

type Walk<'a> = Pin<Box<dyn Future<Output = Result<Node, Error>> + Send + 'a>>;

fn walk<'a>(
    inv: &'a Invocation<'_>,
    reach: &'a crate::region::Reach,
    node: &'a Node,
    depth: usize,
    count: &'a mut usize,
) -> Walk<'a> {
    Box::pin(async move {
        if node.kind == crate::region::KIND {
            return Ok(node.clone());
        }
        if node.kind != KIND {
            let mut out = node.clone();
            if let Some(replacement) = &node.replacement {
                out.replacement =
                    Some(Box::new(walk(inv, reach, replacement, depth, count).await?));
            }
            let mut children = Vec::with_capacity(node.children.len());
            for child in &node.children {
                children.push(walk(inv, reach, child, depth, count).await?);
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
        match reach.source(inv, iri).await {
            Ok(rep) => {
                let referenced = model::parse(&rep.bytes)?;
                walk(inv, reach, &referenced, depth + 1, count).await
            }
            Err(Error::Denied(_)) => Ok(withheld(node)),
            Err(e) => Err(e),
        }
    })
}
