//! HTML snapshots: a page's bytes held to a file in the repository.
//!
//! A snapshot that does not exist yet is WRITTEN and the check fails, saying so: review the new
//! file and commit it. To accept a change, delete the file and run the test again. There is no
//! environment switch that rewrites snapshots, so a test run can never bless itself.

use std::path::Path;

/// What [`check`] found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The bytes match the file.
    Same,
    /// There was no file; it has now been written.
    Written,
    /// The bytes differ: the first differing line, 1-based, with both versions of it.
    Differs {
        /// The first line that differs.
        line: usize,
        /// That line in the file.
        want: String,
        /// That line in `actual`.
        got: String,
    },
}

/// Compare `actual` with `dir/name`, writing the file if it is missing.
pub fn check(dir: &Path, name: &str, actual: &str) -> std::io::Result<Outcome> {
    let file = dir.join(name);
    let want = match std::fs::read_to_string(&file) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir_all(dir)?;
            std::fs::write(&file, actual)?;
            return Ok(Outcome::Written);
        }
        Err(e) => return Err(e),
    };
    if want == actual {
        return Ok(Outcome::Same);
    }
    let (mut a, mut b) = (want.lines(), actual.lines());
    let mut line = 0;
    loop {
        line += 1;
        match (a.next(), b.next()) {
            (Some(x), Some(y)) if x == y => continue,
            (None, None) => {
                // Same lines, different line endings or a final newline.
                return Ok(Outcome::Differs {
                    line,
                    want: format!("{} bytes", want.len()),
                    got: format!("{} bytes", actual.len()),
                });
            }
            (x, y) => {
                return Ok(Outcome::Differs {
                    line,
                    want: x.unwrap_or("(end of file)").to_string(),
                    got: y.unwrap_or("(end of file)").to_string(),
                })
            }
        }
    }
}

/// [`check`], panicking with a message that says what to do unless the bytes match.
pub fn assert_snapshot(dir: &Path, name: &str, actual: &str) {
    match check(dir, name, actual) {
        Ok(Outcome::Same) => {}
        Ok(Outcome::Written) => panic!(
            "snapshot {} did not exist and has been written: review it and commit it",
            dir.join(name).display()
        ),
        Ok(Outcome::Differs { line, want, got }) => panic!(
            "snapshot {} differs at line {line}:\n  file: {want}\n  now:  {got}\n\
             (to accept the change, delete the file and run the test again)",
            dir.join(name).display()
        ),
        Err(e) => panic!("snapshot {}: {e}", dir.join(name).display()),
    }
}
