//! A native walker over immutable, resolved GPUI faces.
//!
//! Install [`descriptor_space`] before the base UI space so the GPUI face retains
//! forms and inputs. [`worker::Worker`] resolves faces and submits to Sinks on OS
//! threads. The macOS [`native::Screen`] only reads snapshots and local input state.
#![forbid(unsafe_code)]

use std::sync::Arc;

use ikigai_core::{
    Description, EndpointSpace, Exact, FnEndpoint, Invocation, ReprType, Representation, Verb,
};
use ikigai_ui::{
    model::{self, Node, Value},
    target::Descriptor,
};

/// The descriptor implemented by this renderer, including interactive forms.
pub const DESCRIPTOR: &[u8] = include_bytes!("../assets/gpui.json");

/// Override the base library's prospective GPUI descriptor in a host's fallback space.
pub fn descriptor_space() -> EndpointSpace {
    EndpointSpace::new().bind(
        Exact::new("urn:iki:ui:target:name:gpui"),
        FnEndpoint::new("ui-gpui-descriptor", |_: &Invocation<'_>| {
            Ok(
                Representation::new(ReprType::new("application/json"), DESCRIPTOR.to_vec())
                    .cacheable(),
            )
        })
        .with_description(
            Description::new("ui-gpui-descriptor")
                .title("Native GPUI target")
                .summary("The native renderer's kind coverage.")
                .verb(Verb::Source)
                .verb(Verb::Meta)
                .output("application/json"),
        ),
    )
}

/// A resolved, validated tree. Its root cannot be mutated through the snapshot API.
#[derive(Clone, Debug)]
pub struct Snapshot(Arc<Node>);

impl Snapshot {
    /// Validate an already resolved GPUI face. References, custom elements, and
    /// unsupported kinds are refused rather than resolved from a render callback.
    pub fn from_face(bytes: &[u8]) -> Result<Self, ikigai_core::Error> {
        let node = model::parse(bytes)?;
        let descriptor = Descriptor::parse(DESCRIPTOR, "gpui")?;
        let mut stack = vec![&node];
        while let Some(node) = stack.pop() {
            if !descriptor.renders.contains(&node.kind) || node.replacement.is_some() {
                return Err(ikigai_core::Error::InvalidArgument {
                    name: "snapshot".into(),
                    detail: format!("{} is not a resolved native GPUI node", node.kind),
                });
            }
            stack.extend(&node.children);
        }
        Ok(Self(Arc::new(node)))
    }

    /// The immutable root; no kernel or resource resolver is exposed to the walker.
    pub fn root(&self) -> &Node {
        &self.0
    }
}

/// A textual property, if present.
pub fn text<'a>(node: &'a Node, property: &str) -> Option<&'a str> {
    match node.properties.get(property) {
        Some(Value::Text(value)) => Some(value),
        _ => None,
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod native;
#[cfg(not(target_arch = "wasm32"))]
pub mod worker;
