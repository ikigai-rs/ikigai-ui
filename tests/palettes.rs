//! D12: the palettes clear the contrast floor, the token stylesheet carries every palette
//! behind the right selector, and no color is written anywhere but the palette tables.

mod common;

use common::*;
use ikigai_ui::palette::{palette, PALETTES, TOKENS};
use ikigai_ui::testkit::{contrast, lint};

#[test]
fn every_palette_clears_the_contrast_floor() {
    let rows = contrast::measure(&PALETTES);
    for row in &rows {
        println!("{row}");
    }
    let failing: Vec<String> = rows
        .iter()
        .filter(|r| !r.passes())
        .map(|r| r.to_string())
        .collect();
    assert!(failing.is_empty(), "{}", failing.join("\n"));
    assert_eq!(rows.len(), PALETTES.len() * ikigai_ui::palette::PAIRS.len());
}

#[test]
fn every_palette_defines_every_token_in_order() {
    for p in PALETTES {
        let names: Vec<&str> = p.colors.iter().map(|(t, _)| *t).collect();
        assert_eq!(names, TOKENS, "{}", p.name);
        for (t, v) in p.colors {
            assert!(
                ikigai_a11y::Rgba::parse(v).is_ok() && v.len() == 7,
                "{}: --{t} is {v}, not #rrggbb",
                p.name
            );
        }
    }
}

#[test]
fn the_stylesheet_carries_each_palette_behind_its_selector() {
    let css = text(&host(None), ikigai_ui::TOKENS_CSS, &[]);
    for (selector, name) in [
        (":root {\n  color-scheme: light;", "light"),
        ("@media (prefers-color-scheme: dark) {\n  :root:not([data-theme]), :root[data-theme=\"auto\"] {\n    color-scheme: dark;", "dark"),
        ("@media (prefers-contrast: more) {\n  :root:not([data-theme]), :root[data-theme=\"auto\"] {\n    color-scheme: dark;", "high-contrast"),
        (":root[data-theme=\"light\"] {\n  color-scheme: light;", "light"),
        (":root[data-theme=\"dark\"] {\n  color-scheme: dark;", "dark"),
        (":root[data-theme=\"high-contrast\"] {\n  color-scheme: dark;", "high-contrast"),
    ] {
        let block = css
            .split_once(selector)
            .unwrap_or_else(|| panic!("`{selector}` in the stylesheet"))
            .1;
        let declarations = palette(name).unwrap().declarations();
        assert!(
            block.contains(&declarations),
            "`{selector}` is followed by the {name} palette"
        );
    }
    // The contrast setting comes after the dark scheme, so on a dark system that asks for more
    // contrast, high contrast wins.
    assert!(
        css.find("@media (prefers-contrast: more)").unwrap()
            > css.find("@media (prefers-color-scheme: dark)").unwrap()
    );
    assert!(!css.contains("light-dark("), "no light-dark() (D12)");
}

#[test]
fn no_artifact_writes_a_color_and_every_var_is_defined() {
    let kernel = host(None);
    let css = text(&kernel, ikigai_ui::TOKENS_CSS, &[]);
    for a in &ikigai_ui::ARTIFACTS {
        if a.iri == "urn:ui:htmx" {
            continue; // vendored as it is; it states no color (includeIndicatorStyles is off)
        }
        let source = std::str::from_utf8(a.bytes).unwrap();
        assert!(
            lint::hex_colors(source).is_empty(),
            "{}: {:?}",
            a.iri,
            lint::hex_colors(source)
        );
    }
    let undefined = lint::undefined_vars(&[&css]);
    assert!(
        undefined.is_empty(),
        "used and never defined: {undefined:?}"
    );
    // Every color in the composed stylesheet is a palette value.
    let values: Vec<String> = PALETTES
        .iter()
        .flat_map(|p| p.colors.iter().map(|(_, v)| v.to_string()))
        .collect();
    for hex in lint::hex_colors(&css) {
        assert!(
            values.contains(&hex),
            "{hex} in tokens.css is not a palette value"
        );
    }
}

#[test]
fn the_turtle_face_is_one_triple_per_token() {
    let kernel = host(None);
    for p in PALETTES {
        let iri = format!("urn:ui:palette:{}", p.name);
        let rep = issue(
            &kernel,
            ikigai_core::Verb::Source,
            &iri,
            &[("as", b"text/turtle")],
        )
        .unwrap();
        assert_eq!(rep.repr_type.media_type, "text/turtle");
        let ttl = String::from_utf8(rep.bytes).unwrap();
        assert_eq!(ttl.lines().count(), TOKENS.len());
        assert!(!ttl.contains("_:"), "no blank nodes");
        for (t, v) in p.colors {
            assert!(ttl.contains(&format!("<{iri}> <urn:ui:token:{t}> \"{v}\" .\n")));
        }
    }
    assert!(matches!(
        source(&kernel, "urn:ui:palette:chartreuse"),
        Err(ikigai_core::Error::NotFound(_))
    ));
}
