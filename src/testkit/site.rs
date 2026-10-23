//! A static site for axe: rendered resources written as files at their URL paths, with the
//! media type each one is served with recorded in `_types.json`, so a plain file server can
//! serve `/ui/htmx` (no extension) as JavaScript. `.github/workflows/axe.yml` serves such a
//! directory and checks every `.html` file in it.

use std::collections::BTreeMap;
use std::path::Path;

/// Write each `(url path, media type, bytes)` under `dir`, and `dir/_types.json`.
///
/// A path is relative to the site root, `/`-separated, and may not climb out of it.
pub fn write(dir: &Path, files: &[(&str, &str, &[u8])]) -> std::io::Result<()> {
    let mut types = BTreeMap::new();
    for (path, media_type, bytes) in files {
        let rel = path.trim_start_matches('/');
        if rel.is_empty()
            || rel
                .split('/')
                .any(|s| s.is_empty() || s == "." || s == "..")
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("`{path}` is not a path inside the site"),
            ));
        }
        let file = dir.join(rel);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&file, bytes)?;
        types.insert(format!("/{rel}"), media_type.to_string());
    }
    let json = serde_json::to_string_pretty(&types).map_err(std::io::Error::other)?;
    std::fs::write(dir.join("_types.json"), json + "\n")
}
