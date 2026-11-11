//! Settings as resources (ledger #3361): a screen's target, theme, locale, density and viewer
//! are names, `urn:iki:ui:setting:name:{name}`, answered by the SPACE the screen runs in, never
//! passed to a face as an argument.
//!
//! [`crate::space`] answers every name of [`NAMES`] with its default ([`DEFAULTS`]), from the
//! root. A host that runs a screen in another binding builds a SCREEN SPACE ([`screen_space`],
//! a named space binding some of the names) and injects it for the request with
//! `Kernel::issue_in` ([`scope`]): the screen space sits ahead of the root, so it answers the
//! names it binds and the root answers the rest. Only the kernel's holder can place a corridor
//! ahead of the root (an endpoint can only narrow: `Invocation::confine`), which is why the host
//! injects it and a face never does.
//!
//! The face `urn:iki:ui:face:screen:{screen}` ([`crate::target`]) reads
//! [`TARGET`] from the space it runs in. Each screen space is named, and the name is part of
//! every cache key under it, so two bindings of the settings cache separately; within one,
//! every answer stays keyed on the caller's capability fingerprint.
//!
//! A value is text: nonempty, at most [`MAX_VALUE`] bytes, no control characters.

use std::sync::Arc;

use ikigai_core::{
    ArgSpec, Description, EndpointSpace, Error, Exact, FnEndpoint, Invocation, Iri, ReprType,
    Representation, Request, Scope, Verb,
};

/// A setting's name is this plus `{name}`.
pub const PREFIX: &str = "urn:iki:ui:setting:name:";
/// The face's target, a [`crate::target::NAMES`] name.
pub const TARGET: &str = "urn:iki:ui:setting:name:target";
/// The settings a screen runs under.
pub const NAMES: [&str; 5] = ["target", "theme", "locale", "density", "viewer"];
/// Each setting's default, the root's answer, in [`NAMES`]' order.
pub const DEFAULTS: [(&str, &str); 5] = [
    ("target", "html"),
    ("theme", "auto"),
    ("locale", "en"),
    ("density", "comfortable"),
    ("viewer", "anonymous"),
];
/// The longest setting value, in bytes.
pub const MAX_VALUE: usize = 256;
/// What a setting is served as.
pub const MEDIA_TYPE: &str = "text/plain; charset=utf-8";

fn invalid(detail: impl Into<String>) -> crate::ConfigError {
    crate::ConfigError(detail.into())
}

fn constant(id: &'static str, value: String) -> FnEndpoint {
    FnEndpoint::new(id, move |_: &Invocation<'_>| {
        Ok(Representation::new(ReprType::new(MEDIA_TYPE), value.clone().into_bytes()).cacheable())
    })
    .with_description(
        Description::new(id)
            .title("A UI setting")
            .summary("One setting a screen runs under, as text, bound by the host's screen space.")
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .output(MEDIA_TYPE),
    )
}

/// Whether `value` is a setting value.
pub fn value_ok(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_VALUE && !value.chars().any(char::is_control)
}

/// A screen space named `id` binding each `(name, value)` of `settings` at
/// `urn:iki:ui:setting:name:{name}`. A name not in [`NAMES`], a name given twice, an invalid
/// value or an `id` that is not an IRI is refused.
pub fn screen_space(
    id: &str,
    settings: &[(&str, &str)],
) -> Result<EndpointSpace, crate::ConfigError> {
    let id = Iri::parse(id).map_err(|e| invalid(format!("`{id}` is not an IRI: {e}")))?;
    let mut space = EndpointSpace::new().named(id);
    let mut seen = Vec::new();
    for (name, value) in settings {
        if !NAMES.contains(name) {
            return Err(invalid(format!(
                "`{name}` is not a setting ({})",
                NAMES.join(", ")
            )));
        }
        if seen.contains(name) {
            return Err(invalid(format!("`{name}` is set twice")));
        }
        if !value_ok(value) {
            return Err(invalid(format!("`{name}`: not a setting value")));
        }
        seen.push(*name);
        space = space.bind(
            Exact::new(format!("{PREFIX}{name}")),
            constant("ui-setting", value.to_string()),
        );
    }
    Ok(space)
}

/// The scope a host issues a face request in: [`screen_space`] injected ahead of the root.
pub fn scope(id: &str, settings: &[(&str, &str)]) -> Result<Scope, crate::ConfigError> {
    Ok(Scope::empty().with(Arc::new(screen_space(id, settings)?)))
}

/// Read setting `name` in the space `inv` runs in.
pub async fn read(inv: &Invocation<'_>, name: &str) -> Result<String, Error> {
    let iri = Iri::parse(format!("{PREFIX}{name}")).map_err(|e| Error::InvalidArgument {
        name: "setting".into(),
        detail: e.to_string(),
    })?;
    let rep = inv.issue(Request::new(Verb::Source, iri)).await?;
    let value = String::from_utf8(rep.bytes).map_err(|_| Error::InvalidArgument {
        name: "setting".into(),
        detail: format!("{name}: not UTF-8"),
    })?;
    let value = value.trim().to_string();
    if !value_ok(&value) {
        return Err(Error::InvalidArgument {
            name: "setting".into(),
            detail: format!("{name}: not a setting value"),
        });
    }
    Ok(value)
}

/// `urn:iki:ui:setting:name:{name}` in the root: the default.
pub(crate) fn default_endpoint() -> FnEndpoint {
    FnEndpoint::new("ui-setting-name", |inv: &Invocation<'_>| {
        let name = inv.bindings.get("name").unwrap_or_default();
        let (_, value) = DEFAULTS
            .iter()
            .find(|(n, _)| *n == name)
            .ok_or_else(|| Error::NotFound(format!("{PREFIX}{name}: not a setting")))?;
        Ok(Representation::new(ReprType::new(MEDIA_TYPE), value.as_bytes().to_vec()).cacheable())
    })
    .with_description(
        Description::new("ui-setting-name")
            .title("A UI setting's default")
            .summary(
                "A setting a screen runs under (target, theme, locale, density, viewer), its \
                 default as text. A host's screen space, injected ahead of the root, answers \
                 the names it binds instead.",
            )
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .input(
                ArgSpec::new("name")
                    .summary("the setting")
                    .class(crate::XSD_STRING)
                    .binding()
                    .one_of(NAMES),
            )
            .output(MEDIA_TYPE),
    )
}
