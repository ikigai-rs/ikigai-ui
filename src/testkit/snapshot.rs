//! HTML snapshots: a page's bytes held to a file in the repository.
//!
//! A snapshot that does not exist yet is WRITTEN and the check fails, saying so: review the new
//! file and commit it. To accept a change, delete the file and run the test again.
//! Use [`assert_snapshots`] for a batch: every file is checked before one combined failure.
//! There is no environment switch that rewrites snapshots, so a test run can never bless itself.

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
// ambient: fs: test kit, never linked by a host: the calling repo's own snapshot file
// (read, and written when missing; the `testkit` feature is for a dev-dependency).
#[allow(clippy::disallowed_methods)]
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
/// For multiple pages, use [`assert_snapshots`] to check them all before failing.
pub fn assert_snapshot(dir: &Path, name: &str, actual: &str) {
    assert_snapshots(dir, [(name, actual)]);
}

/// Check every `(name, actual)` pair, then panic once listing all failures.
///
/// Missing files are all written, changed files are preserved, and I/O errors are
/// collected alongside mismatches. Review and commit new files; to accept changes,
/// delete the affected files and run again. No environment switch blesses snapshots.
///
/// ```no_run
/// use ikigai_ui::testkit::snapshot::assert_snapshots;
/// use std::path::Path;
///
/// assert_snapshots(Path::new("tests/snapshots"), [
///     ("home.html", "<h1>Home</h1>"),
///     ("about.html", "<h1>About</h1>"),
/// ]);
/// ```
pub fn assert_snapshots<N: AsRef<str>, A: AsRef<str>>(
    dir: &Path,
    snapshots: impl IntoIterator<Item = (N, A)>,
) {
    let mut failures = Vec::new();
    for (name, actual) in snapshots {
        let name = name.as_ref();
        let file = dir.join(name);
        match check(dir, name, actual.as_ref()) {
            Ok(Outcome::Same) => {}
            Ok(Outcome::Written) => failures.push(format!(
                "snapshot {} did not exist and has been written: review it and commit it",
                file.display()
            )),
            Ok(Outcome::Differs { line, want, got }) => failures.push(format!(
                "snapshot {} differs at line {line}:\n  file: {want}\n  now:  {got}\n\
                 (to accept the change, delete the file and run the test again)",
                file.display()
            )),
            Err(e) => failures.push(format!("snapshot {}: {e}", file.display())),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_batch_writes_every_missing_snapshot_and_reports_every_failure() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("target/snapshot-batch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("changed.html"), "old\n").unwrap();
        std::fs::write(dir.join("same.html"), "same\n").unwrap();
        // An I/O error must not prevent later snapshots from being checked either.
        std::fs::create_dir(dir.join("unreadable.html")).unwrap();
        let pages = [
            ("first.html", "first\n"),
            ("changed.html", "new\n"),
            ("unreadable.html", "error\n"),
            ("last.html", "last\n"),
            ("same.html", "same\n"),
        ];
        let failure = std::panic::catch_unwind(|| {
            assert_snapshots(&dir, pages);
        })
        .expect_err("new snapshots must fail even after writing them");
        assert_eq!(
            std::fs::read_to_string(dir.join("first.html")).unwrap(),
            "first\n"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("last.html")).unwrap(),
            "last\n"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("changed.html")).unwrap(),
            "old\n"
        );
        let message = failure.downcast_ref::<String>().unwrap();
        for name in ["first.html", "changed.html", "unreadable.html", "last.html"] {
            assert!(message.contains(name), "{message}");
        }
        assert!(message.contains("line 1"), "{message}");
        assert!(!message.contains("same.html"), "{message}");
        std::fs::remove_dir(dir.join("unreadable.html")).unwrap();
        std::fs::remove_file(dir.join("changed.html")).unwrap();
        assert!(
            std::panic::catch_unwind(|| assert_snapshot(&dir, "changed.html", "new\n")).is_err()
        );
        assert_snapshots(
            &dir,
            pages
                .into_iter()
                .filter(|(name, _)| *name != "unreadable.html"),
        );
        assert_snapshots(&dir, std::iter::empty::<(&str, &str)>());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
