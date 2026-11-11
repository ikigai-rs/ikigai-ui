//! Resource work happens on dedicated OS threads, including synchronous compositions.
use std::{
    collections::BTreeMap,
    sync::{
        mpsc::{self, Receiver, TryRecvError},
        Arc,
    },
};

use ikigai_core::{ArgRef, Capability, Error, Iri, Kernel, Request, Scope, Verb};
use ikigai_ui::model::Node;

use crate::{text, Snapshot};

/// Nonblocking result of resource work. Poll it from the UI's timer/task.
pub struct Pending<T>(Receiver<Result<T, Error>>);
impl<T> Pending<T> {
    /// `None` means still working. Never waits for a resource.
    pub fn poll(&self) -> Option<Result<T, Error>> {
        match self.0.try_recv() {
            Ok(value) => Some(value),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                Some(Err(Error::Endpoint("resource worker stopped".into())))
            }
        }
    }
}

fn spawn<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, Error> + Send + 'static,
) -> Pending<T> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(work());
    });
    Pending(receiver)
}

/// A host's kernel and authority, used only by background jobs.
#[derive(Clone)]
pub struct Worker {
    kernel: Arc<Kernel>,
    capability: Capability,
    scope: Scope,
}
impl Worker {
    /// Use the same authority and resolution corridor for loading and submitting.
    pub fn new(kernel: Arc<Kernel>, capability: Capability, scope: Scope) -> Self {
        Self {
            kernel,
            capability,
            scope,
        }
    }

    /// Resolve the GPUI target face completely before publishing a snapshot.
    pub fn load(&self, screen: &str) -> Pending<Snapshot> {
        let this = self.clone();
        let screen = screen.to_string();
        spawn(move || {
            // A screen name is one binding, not an arbitrary resource supplied by a node.
            if screen.is_empty()
                || !screen
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            {
                return Err(Error::InvalidArgument {
                    name: "screen".into(),
                    detail: "expected a screen name".into(),
                });
            }
            let iri = Iri::parse(format!("urn:iki:ui:face:target:gpui:screen:{screen}"))
                .map_err(|e| Error::Endpoint(e.to_string()))?;
            let rep = futures::executor::block_on(this.kernel.issue_in(
                Request::new(Verb::Source, iri),
                &this.capability,
                this.scope,
            ))?;
            Snapshot::from_face(&rep.bytes)
        })
    }

    /// Submit named values directly to the form's Sink, under the host's capability.
    /// HTML `action` URLs are never interpreted or executed by the native renderer.
    pub fn submit(&self, form: &Node, values: BTreeMap<String, String>) -> Pending<()> {
        let this = self.clone();
        let form = form.clone();
        spawn(move || {
            if form.kind != "form" || text(&form, "verb").unwrap_or("sink") != "sink" {
                return Err(Error::InvalidArgument {
                    name: "form".into(),
                    detail: "native forms require a Sink resource".into(),
                });
            }
            let resource = text(&form, "resource").ok_or_else(|| Error::InvalidArgument {
                name: "form".into(),
                detail: "this form has no Sink resource".into(),
            })?;
            let iri = Iri::parse(resource).map_err(|e| Error::InvalidArgument {
                name: "resource".into(),
                detail: e.to_string(),
            })?;
            let mut request = Request::new(Verb::Sink, iri);
            for (name, value) in values {
                request = request.with_arg(name, ArgRef::Inline(value.into_bytes()));
            }
            futures::executor::block_on(this.kernel.issue_in(
                request,
                &this.capability,
                this.scope,
            ))?;
            Ok(())
        })
    }
}

/// Presentation state for a completed submission. Field refusals retain their names.
#[derive(Default, Debug)]
pub struct Feedback {
    /// Errors keyed by the service's argument names.
    pub fields: BTreeMap<String, String>,
    /// A success message or a refusal not associated with a particular field.
    pub message: String,
}
impl Feedback {
    /// Replace earlier feedback, preserving the user's input values in the view.
    pub fn complete(&mut self, result: Result<(), Error>) {
        self.fields.clear();
        self.message.clear();
        match result {
            Ok(()) => self.message = "Submitted successfully.".into(),
            Err(Error::InvalidArgument { name, detail }) => {
                self.message = format!("Please check {name}: {detail}");
                self.fields.insert(name, detail);
            }
            Err(error) => self.message = error.to_string(),
        }
    }
}
