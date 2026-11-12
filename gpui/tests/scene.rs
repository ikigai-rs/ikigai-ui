//! The `scene` feature's offscreen renderer (ledger #3516): the html face's own WGSL, drawn with
//! wgpu into a texture and read back. Deterministic enough to pixel-sample: a node's projected
//! centre is its role's color, the ground is `--bg`, and N / Shift+N / Escape move the selection
//! and its announcement. `measure_*` prints device, frame and readback times (run with
//! `--nocapture`). Skipped, saying so, where the machine has no GPU adapter.
#![cfg(all(feature = "scene", any(target_os = "macos", target_os = "linux")))]

use std::time::Instant;

use ikigai_ui::{palette::PALETTES, scene};
use ikigai_ui_gpui::{
    scene::{announce, pixel, Renderer, SceneColors, View},
    theme::Tokens,
};

fn gallery() -> scene::Scene {
    let g = scene::read_graph(scene::EXAMPLE_GRAPH_JSON_LD.as_bytes()).unwrap();
    scene::scene(scene::EXAMPLE_GRAPH, &g, &scene::layout(&g))
}

/// A graph at `MAX_NODES`, a ring with chords, laid out as the kernel lays one out.
fn largest() -> scene::Scene {
    let mut g = scene::Graph::default();
    let n = scene::MAX_NODES;
    for i in 0..n {
        g.nodes
            .insert(format!("urn:t:n{i:03}"), (format!("n{i}"), None));
    }
    for i in 0..n {
        for step in [1, 7, 31] {
            g.edges.push((
                format!("urn:t:n{i:03}"),
                "p".into(),
                format!("urn:t:n{:03}", (i + step) % n),
            ));
        }
    }
    scene::scene("urn:t:large", &g, &scene::layout(&g))
}

fn renderer() -> Option<Renderer> {
    match Renderer::new() {
        Ok(r) => Some(r),
        Err(e) => {
            eprintln!("SKIPPED: {e}");
            None
        }
    }
}

fn at(frame: &[u8], width: u32, (x, y): (u32, u32)) -> u32 {
    let i = ((y * width + x) * 4) as usize;
    let [b, g, r] = [frame[i], frame[i + 1], frame[i + 2]];
    (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
}

#[test]
fn the_shared_wgsl_is_the_html_renderers_and_takes_the_palette_by_role() {
    let w = scene::wgsl();
    for code in [&w.node, &w.edge] {
        assert!(code.contains("@vertex fn vs") && code.contains("@fragment fn fs"));
        assert!(code.contains("node: vec4<f32>, edge: vec4<f32>, chosen: vec4<f32>"));
        assert!(
            !code.contains("vec4(0."),
            "no color is written in the shader: {code}"
        );
    }
}

#[test]
fn a_fixed_datum_renders_its_palette_by_role_in_every_theme() {
    let Some(r) = renderer() else { return };
    let s = gallery();
    let prepared = r.prepare(&s);
    let (w, h) = (320, 240);
    for p in PALETTES {
        let colors = SceneColors::from_tokens(&Tokens::compiled(&p));
        let mut view = View::new(&s);
        let frame = r.frame(&s, &prepared, &view, &colors, w, h);
        assert_eq!(
            at(&frame, w, (0, 0)),
            colors.ground,
            "{}: the ground",
            p.name
        );
        let node = (0..s.nodes.len())
            .find_map(|i| pixel(&s, &view, i, w, h).map(|px| (i, px)))
            .expect("a node in frame");
        assert_eq!(
            at(&frame, w, node.1),
            colors.node,
            "{}: node {} at {:?}",
            p.name,
            node.0,
            node.1
        );
        // Select it: its centre takes the selection's color.
        while view.selected != Some(node.0) {
            view.key("n", false, s.nodes.len());
        }
        let frame = r.frame(&s, &prepared, &view, &colors, w, h);
        assert_eq!(
            at(&frame, w, node.1),
            colors.selected,
            "{}: the selection",
            p.name
        );
        // The same frame twice is the same bytes.
        assert_eq!(
            frame,
            r.frame(&s, &prepared, &view, &colors, w, h),
            "{}: deterministic",
            p.name
        );
    }
}

#[test]
fn keyboard_selection_changes_the_announced_status() {
    let s = gallery();
    let mut view = View::new(&s);
    let n = s.nodes.len();
    assert_eq!(announce(&s, view.selected), "No node selected.");
    assert!(view.key("n", false, n));
    let first = announce(&s, view.selected);
    assert!(
        first.starts_with(&format!("Selected {}", s.nodes[0].label)),
        "{first}"
    );
    assert!(view.key("n", true, n));
    assert_eq!(view.selected, Some(n - 1), "Shift+N goes back, wrapping");
    assert_ne!(announce(&s, view.selected), first);
    assert!(view.key("escape", false, n));
    assert_eq!(announce(&s, view.selected), "No node selected.");
    let before = view;
    assert!(view.key("left", false, n) && view.key("+", false, n));
    assert!(view.yaw < before.yaw && view.distance < before.distance);
    assert!(!view.key("q", false, n), "not the scene's key");
}

fn time<T>(f: impl FnOnce() -> T) -> (T, f64) {
    let t = Instant::now();
    let v = f();
    (v, t.elapsed().as_secs_f64() * 1000.0)
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

#[test]
fn measure_device_frame_and_readback() {
    let (r, device_ms) = time(Renderer::new);
    let Ok(r) = r else {
        eprintln!("SKIPPED: no adapter");
        return;
    };
    println!("device + pipelines: {device_ms:.1} ms");
    let colors = SceneColors::from_tokens(&Tokens::compiled(&ikigai_ui::palette::LIGHT));
    for (name, s) in [("gallery", gallery()), ("MAX_NODES", largest())] {
        let prepared = r.prepare(&s);
        for (w, h) in [(640u32, 360u32), (1280, 720)] {
            let mut view = View::new(&s);
            r.frame(&s, &prepared, &view, &colors, w, h); // warm
            let samples: Vec<f64> = (0..60)
                .map(|_| {
                    view.key("right", false, s.nodes.len());
                    time(|| r.frame(&s, &prepared, &view, &colors, w, h)).1
                })
                .collect();
            let worst = samples.iter().cloned().fold(0.0, f64::max);
            println!(
                "{name} ({} nodes, {} edges) {w}x{h}: frame + readback median {:.2} ms, worst {:.2} ms",
                s.nodes.len(),
                s.edges.len(),
                median(samples),
                worst
            );
        }
    }
}
