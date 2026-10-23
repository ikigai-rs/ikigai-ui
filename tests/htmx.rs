//! D7: one vendored htmx, its version and digest recorded and held, and the shell's
//! `integrity=` value the digest of exactly these bytes.

use base64::Engine;
use sha2::{Digest, Sha256, Sha384};

fn htmx() -> &'static [u8] {
    ikigai_ui::artifact("urn:ui:htmx").unwrap().bytes
}

#[test]
fn the_vendored_bytes_are_upstreams_release() {
    let digest: String = Sha256::digest(htmx())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(digest, ikigai_ui::HTMX_SHA256);
    // The release names itself in its own bytes.
    let text = std::str::from_utf8(htmx()).unwrap();
    assert!(
        text.contains(&format!("version:\"{}\"", ikigai_ui::HTMX_VERSION)),
        "htmx {} names its version",
        ikigai_ui::HTMX_VERSION
    );
}

#[test]
fn the_shell_carries_the_integrity_of_these_bytes() {
    let sri = format!(
        "sha384-{}",
        base64::engine::general_purpose::STANDARD.encode(Sha384::digest(htmx()))
    );
    assert_eq!(sri, ikigai_ui::HTMX_INTEGRITY);
    let shell = std::str::from_utf8(ikigai_ui::artifact(ikigai_ui::SHELL).unwrap().bytes).unwrap();
    assert!(
        shell.contains(&format!(
            "<script src=\"/ui/htmx\" integrity=\"{}\" defer></script>",
            ikigai_ui::HTMX_INTEGRITY
        )),
        "the shell loads htmx with its integrity"
    );
}

#[test]
fn the_shell_configures_htmx_once() {
    let shell = std::str::from_utf8(ikigai_ui::artifact(ikigai_ui::SHELL).unwrap().bytes).unwrap();
    let config = shell
        .lines()
        .find(|l| l.contains("name=\"htmx-config\""))
        .expect("an htmx-config meta");
    for setting in [
        "\"allowEval\":false",
        "\"allowScriptTags\":false",
        "\"includeIndicatorStyles\":false",
        "\"refreshOnHistoryMiss\":true",
    ] {
        assert!(config.contains(setting), "{setting} in {config}");
    }
    assert!(!shell.contains("hx-boost"), "no global boost (D7)");
}
