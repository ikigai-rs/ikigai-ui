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
        "urn:iki:ui:scene-renderer.js",
        ikigai_ui::TOKENS_CSS,
    ] {
        let rep = common::source(&kernel, iri)?;
        let path = format!("/{}", iri.trim_start_matches("urn:iki:").replace(':', "/"));
        files.push((path, rep.repr_type.media_type, rep.bytes));
    }
    // The demo family's own stylesheet, which every demo page links from its head.
    files.push((
        common::DEMO_CSS_PATH.to_string(),
        "text/css".to_string(),
        common::DEMO_CSS.as_bytes().to_vec(),
    ));
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
    // The gallery through the html target's templates (ledger #3355), in the demo shell, so axe
    // covers every kind, a table with header cells and a form; and the custom elements (ledger
    // #3358), expanded by the html target, the same way.
    let about = ikigai_ui::testkit::render::source(&kernel, "urn:demo:page:about", &[])?;
    let (head, rest) = about
        .split_once("<main id=\"main\" tabindex=\"-1\">")
        .expect("the shell's main");
    let (_, tail) = rest.split_once("</main>").expect("the shell's main");
    for (screen, title) in [("gallery", "Gallery"), ("elements", "Elements")] {
        let body = ikigai_ui::testkit::render::source(
            &kernel,
            &format!("urn:iki:ui:html:screen:{screen}"),
            &[],
        )?;
        let page = format!(
            "{head}<main id=\"main\" tabindex=\"-1\">\n<h1>{title}</h1>\n{body}\n</main>{tail}"
        );
        files.push((
            format!("/{screen}.html"),
            common::HTML.to_string(),
            page.into_bytes(),
        ));
    }
    // A lensed view as a page in the shell (ledger #3538), the graph lens over the example
    // graph, in light and dark, so axe covers the lens chooser, the facets' form and a scene.
    for theme in ["light", "dark"] {
        let page = ikigai_ui::testkit::render::source(
            &kernel,
            &format!(
                "urn:demo:page:lens:contact-graph:resource:{}",
                ikigai_ui::scene::EXAMPLE_GRAPH
            ),
            &[("theme", theme)],
        )?;
        files.push((
            format!("/lens-contact-graph-{theme}.html"),
            common::HTML.to_string(),
            page.into_bytes(),
        ));
    }
    // The lens's scene datum, at the door URL its figure names, as the gallery's below.
    let lens_datum = format!(
        "{}urn:iki:ui:lens:name:contact-graph:focus:{}",
        ikigai_ui::scene::SCENE_PREFIX,
        ikigai_ui::scene::EXAMPLE_GRAPH
    );
    let rep = common::source(&kernel, &lens_datum)?;
    files.push((
        format!("/resolution/iri/{lens_datum}"),
        rep.repr_type.media_type,
        rep.bytes,
    ));
    // The gallery scene's datum, at the door URL its figure names (decoded, as the server reads
    // it), so the renderer's one fetch is answered as a door would answer it.
    let datum = format!(
        "{}{}",
        ikigai_ui::scene::SCENE_PREFIX,
        ikigai_ui::scene::EXAMPLE_GRAPH
    );
    let rep = common::source(&kernel, &datum)?;
    files.push((
        format!("/resolution/iri/{datum}"),
        rep.repr_type.media_type,
        rep.bytes,
    ));
    // The image example's source: a 1x1 grey PNG, so the page loads with no 404.
    const PNG: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 0,
        0, 0, 0, 58, 126, 155, 85, 0, 0, 0, 10, 73, 68, 65, 84, 120, 156, 99, 104, 0, 0, 0, 130, 0,
        129, 119, 205, 114, 182, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    files.push((
        "/img/lighthouse.png".to_string(),
        "image/png".to_string(),
        PNG.to_vec(),
    ));
    let borrowed: Vec<(&str, &str, &[u8])> = files
        .iter()
        .map(|(p, t, b)| (p.as_str(), t.as_str(), b.as_slice()))
        .collect();
    ikigai_ui::testkit::site::write(&dir, &borrowed)?;
    println!("wrote {} files to {}", files.len(), dir.display());
    Ok(())
}
