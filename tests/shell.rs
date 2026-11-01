//! D13: one status region. The shell template holds it (`#status`); the shell script never
//! makes another, so an in-place note is plain text the control points at, never a live region.
//! axe/shell-smoke.mjs drives the same rule in a browser.

mod common;

use common::*;

#[test]
fn the_shell_script_makes_no_live_region_of_its_own() {
    let js = text(&host(None), "urn:iki:ui:shell.js", &[]);
    let code: String = js
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !code.contains("\"role\", \"status\""),
        "the shell script sets role=status on something: an in-place note is plain text (D13)"
    );
    assert!(
        !code.contains("aria-live"),
        "no aria-live outside the shell's #status"
    );
    // The note is linked to its control instead, and the sentence goes to #status.
    assert!(code.contains("aria-describedby"));
}
