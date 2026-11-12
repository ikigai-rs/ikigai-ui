//! The native face takes every color from the shared palettes (`urn:iki:ui:palette:{name}`),
//! so no color may be written in its source: no hex literal, no `rgb(0x…)`, no `hsla(…)`, and
//! none of GPUI's named colors. A color written here would escape the palettes' contrast floor
//! and would not follow the theme.

const SOURCES: [(&str, &str); 7] = [
    ("lib.rs", include_str!("../src/lib.rs")),
    ("main.rs", include_str!("../src/main.rs")),
    ("native.rs", include_str!("../src/native.rs")),
    ("input.rs", include_str!("../src/input.rs")),
    ("scene.rs", include_str!("../src/scene.rs")),
    ("theme.rs", include_str!("../src/theme.rs")),
    ("worker.rs", include_str!("../src/worker.rs")),
];

fn hex_run(s: &str) -> usize {
    s.bytes().take_while(u8::is_ascii_hexdigit).count()
}

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[test]
fn the_source_list_names_every_file_in_src() {
    let mut found: Vec<String> = std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/src"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.ends_with(".rs"))
        .collect();
    found.sort();
    let mut listed: Vec<String> = SOURCES.iter().map(|(n, _)| n.to_string()).collect();
    listed.sort();
    assert_eq!(found, listed, "add the new file to SOURCES");
}

#[test]
fn no_color_is_written_in_the_native_face() {
    let named = [
        "white",
        "black",
        "red",
        "green",
        "blue",
        "yellow",
        "hsla",
        "rgba",
        "opaque_grey",
    ];
    let mut found = Vec::new();
    for (file, source) in SOURCES {
        for (n, line) in source.lines().enumerate() {
            let at = format!("{file}:{}: {}", n + 1, line.trim());
            let bytes = line.as_bytes();
            for (i, _) in line.match_indices("0x") {
                if hex_run(&line[i + 2..]) >= 6 {
                    found.push(at.clone());
                }
            }
            for (i, _) in line.match_indices('#') {
                let run = hex_run(&line[i + 1..]);
                if (run == 3 || run == 6 || run == 8)
                    && !line[i + 1 + run..].starts_with(|c: char| c.is_ascii_alphanumeric())
                {
                    found.push(at.clone());
                }
            }
            if line.contains("rgb(0x") {
                found.push(at.clone());
            }
            for name in named {
                for (i, _) in line.match_indices(&format!("{name}(")) {
                    if i == 0 || !is_ident(bytes[i - 1]) {
                        found.push(at.clone());
                    }
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "colors written in gpui/src:\n{}",
        found.join("\n")
    );
}
