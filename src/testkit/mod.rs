//! The UI test kit (UI conventions D18), for a UI repo's own tests.
//!
//! Enable it in a dev-dependency: `ikigai-ui = { version = "…", features = ["testkit"] }`.
//! Each part is a plain function that returns what it found, so a repo asserts on it in the
//! way its tests read best, and every failure is reported at once, not just the first.
//!
//! - [`snapshot`]: HTML snapshot files, so a migration proves it kept the page.
//! - [`vectors`]: FND-H's escaping and link vectors (ikigai-fn's file, verbatim, pinned by
//!   SHA-256), for checking any escaper, an XSL port's or a script's, against the contract.
//! - [`contrast`]: the contrast floor for every palette, measured with ikigai-a11y.
//! - [`lint`]: no hex color outside the palette tables, and every `var(--x)` used is defined.
//! - [`render`]: issue a Source and read the answer as text, the line every snapshot test needs.
//! - [`site`]: write rendered pages and their assets as a static site, for axe.
//!
//! axe is not a Rust check: it is the reusable workflow `.github/workflows/axe.yml` in this
//! repository, which a UI repo calls from its CI over a directory [`site`] wrote.

pub mod contrast;
pub mod lint;
pub mod site;
pub mod snapshot;
pub mod vectors;

/// Issue a Source for `iri` with `args` under root, and return the answer as UTF-8 text.
pub mod render {
    use ikigai_core::{ArgRef, Capability, Iri, Kernel, Request, Result, Verb};

    /// The text of `iri` with `args`, issued under [`Capability::root`].
    pub fn source(kernel: &Kernel, iri: &str, args: &[(&str, &str)]) -> Result<String> {
        source_as(kernel, iri, args, &Capability::root())
    }

    /// The text of `iri` with `args`, issued under `capability`.
    pub fn source_as(
        kernel: &Kernel,
        iri: &str,
        args: &[(&str, &str)],
        capability: &Capability,
    ) -> Result<String> {
        let target = Iri::parse(iri).map_err(|e| ikigai_core::Error::InvalidArgument {
            name: "iri".to_string(),
            detail: format!("`{iri}` is not an IRI: {e}"),
        })?;
        let mut request = Request::new(Verb::Source, target);
        for (k, v) in args {
            request = request.with_arg(*k, ArgRef::Inline(v.as_bytes().to_vec()));
        }
        let rep = futures_executor::block_on(kernel.issue(request, capability))?;
        String::from_utf8(rep.bytes)
            .map_err(|_| ikigai_core::Error::Endpoint(format!("{iri}: the answer is not UTF-8")))
    }
}
