//! Write the demo pages and the UI artifacts as a static site for axe:
//! `cargo run --example axe_site -- <dir>`. CI's axe job runs this, then
//! `.github/workflows/axe.yml` serves the directory and checks every page in it.

#[path = "../tests/common/mod.rs"]
mod common;

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("usage: cargo run --example axe_site -- <dir>")?,
    );
    let kernel = common::host(None);
    let mut files: Vec<(String, String, Vec<u8>)> = Vec::new();
    // The assets, at the paths the shell links them by (the door's route-table entry:
    // /ui/x is urn:iki:ui:x).
    for iri in [
        "urn:iki:ui:htmx",
        "urn:iki:ui:shell.js",
        ikigai_ui::TOKENS_CSS,
    ] {
        let rep = common::source(&kernel, iri)?;
        let path = format!("/{}", iri.trim_start_matches("urn:iki:").replace(':', "/"));
        files.push((path, rep.repr_type.media_type, rep.bytes));
    }
    // Every demo page in every theme.
    for (name, _, _) in common::PAGES {
        for theme in ikigai_ui::THEMES {
            let page = ikigai_ui::testkit::render::source(
                &kernel,
                &format!("urn:demo:page:{name}"),
                &[("theme", theme)],
            )?;
            files.push((
                format!("/{name}-{theme}.html"),
                common::HTML.to_string(),
                page.into_bytes(),
            ));
        }
    }
    let borrowed: Vec<(&str, &str, &[u8])> = files
        .iter()
        .map(|(p, t, b)| (p.as_str(), t.as_str(), b.as_slice()))
        .collect();
    ikigai_ui::testkit::site::write(&dir, &borrowed)?;
    println!("wrote {} files to {}", files.len(), dir.display());
    Ok(())
}
