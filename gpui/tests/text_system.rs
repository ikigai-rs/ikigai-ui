//! GPUI's macOS text system is its `font-kit` feature, and this crate turns GPUI's default
//! features off. Without `font-kit`, GPUI installs `NoopTextSystem` and every native window draws
//! its boxes with no text at all. The window tests cannot see that: they run on GPUI's test
//! platform, whose text system is its own. So the manifest line itself is what this pins.

#[test]
fn the_macos_gpui_dependency_keeps_its_text_system() {
    let manifest = include_str!("../Cargo.toml");
    let table = manifest
        .split("[target.'cfg(target_os = \"macos\")'.dependencies]")
        .nth(1)
        .expect("a macOS dependency table");
    let table = table.split("\n[").next().unwrap_or(table);
    let gpui = table
        .lines()
        .find(|line| line.starts_with("gpui "))
        .expect("gpui in the macOS dependency table");
    assert!(
        gpui.contains("\"font-kit\""),
        "the macOS gpui dependency must enable `font-kit`, or windows draw no text: {gpui}"
    );
}
