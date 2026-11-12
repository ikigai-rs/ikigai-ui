//! A native walker over immutable, resolved GPUI faces.
//!
//! Install [`descriptor_space`] before the base UI space so the GPUI face retains
//! forms and inputs. [`worker::Worker`] resolves faces and submits to Sinks on OS
//! threads. The native [`native::Screen`] (macOS, Linux and FreeBSD, Windows) only reads snapshots and local input state.
#![forbid(unsafe_code)]

use std::sync::Arc;

use ikigai_core::{
    Description, EndpointSpace, Exact, FnEndpoint, Invocation, ReprType, Representation, Verb,
};
use ikigai_ui::{
    model::{self, Node, Value},
    target::Descriptor,
};

/// The descriptor implemented by this renderer, including interactive forms. With the `scene`
/// feature (ledger #3516) it also claims `scene`, drawn offscreen and painted as an image beside
/// its replacement; without it, `scene` falls back to its replacement as before.
#[cfg(not(feature = "scene"))]
pub const DESCRIPTOR: &[u8] = include_bytes!("../assets/gpui.json");
/// The descriptor implemented by this renderer, including interactive forms. With the `scene`
/// feature (ledger #3516) it also claims `scene`, drawn offscreen and painted as an image beside
/// its replacement; without it, `scene` falls back to its replacement as before.
#[cfg(feature = "scene")]
pub const DESCRIPTOR: &[u8] = include_bytes!("../assets/gpui-scene.json");

/// The disclosure element (ledger #3512), which this renderer draws natively: a button that
/// holds its open state in the window and shows or hides its body.
pub const DISCLOSURE: &str = "urn:iki:ui:element:name:disclosure";

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
    /// Validate an already resolved GPUI face. References, custom elements the descriptor
    /// does not claim (`elements`), and unsupported kinds are refused rather than resolved
    /// from a render callback. A replacement is refused too, except a natively drawn
    /// `scene`'s: its accessible form, drawn beside the image (and validated the same way).
    pub fn from_face(bytes: &[u8]) -> Result<Self, ikigai_core::Error> {
        let node = model::parse(bytes)?;
        let descriptor = Descriptor::parse(DESCRIPTOR, "gpui")?;
        let mut stack = vec![&node];
        while let Some(node) = stack.pop() {
            let native =
                descriptor.renders.contains(&node.kind) || descriptor.elements.contains(&node.kind);
            let kept = node.kind == "scene" && native;
            if !native || (node.replacement.is_some() && !kept) {
                return Err(ikigai_core::Error::InvalidArgument {
                    name: "snapshot".into(),
                    detail: format!("{} is not a resolved native GPUI node", node.kind),
                });
            }
            stack.extend(&node.children);
            stack.extend(node.replacement.as_deref());
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

#[cfg(any(
    target_os = "macos",
    target_os = "linux",
    target_os = "freebsd",
    target_os = "windows"
))]
pub mod native;
#[cfg(all(
    feature = "scene",
    any(
        target_os = "macos",
        target_os = "linux",
        target_os = "freebsd",
        target_os = "windows"
    )
))]
pub mod scene;
pub mod theme;
#[cfg(not(target_arch = "wasm32"))]
pub mod worker;
