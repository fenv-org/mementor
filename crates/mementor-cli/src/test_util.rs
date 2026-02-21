use std::path::PathBuf;

use mementor_lib::context::MementorContext;
use mementor_lib::db::driver::DatabaseDriver;
use mementor_lib::runtime::Runtime;

// Re-export shared helpers from mementor-test-util.
pub use mementor_test_util::model::model_dir;
pub use mementor_test_util::transcript::{make_entry, make_pr_link_entry, write_transcript};

/// Create a [`Runtime`] with an in-memory database and a tempdir-based context.
///
/// The `name` must be unique per test to prevent cross-test DB collisions.
/// The caller must hold the returned [`tempfile::TempDir`] to keep the
/// temporary directory alive for the duration of the test.
pub fn runtime_in_memory(name: &str) -> (tempfile::TempDir, Runtime) {
    let tmp = tempfile::tempdir().unwrap();
    // Create a bare .git directory so that the tempdir looks like a git repo.
    std::fs::create_dir(tmp.path().join(".git")).unwrap();
    let ctx = MementorContext::new(tmp.path().to_path_buf()).unwrap();
    let db = DatabaseDriver::in_memory(name).unwrap();
    let runtime = Runtime { context: ctx, db };
    (tmp, runtime)
}

/// Create a [`Runtime`] where [`DatabaseDriver::is_ready`] returns `false`.
///
/// Uses a file-backed driver pointed at a nonexistent path, simulating a
/// project where mementor has not been enabled.
pub fn runtime_not_enabled() -> (tempfile::TempDir, Runtime) {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = MementorContext::new(tmp.path().to_path_buf()).unwrap();
    let db = DatabaseDriver::file(PathBuf::from("/nonexistent/mementor.db"));
    let runtime = Runtime { context: ctx, db };
    (tmp, runtime)
}

/// Strip margin markers from a multi-line string (Kotlin-style `trimMargin`).
///
/// Each line is scanned for the first `|` character after optional leading
/// whitespace. Everything before and including the `|` is removed. Lines that
/// do not contain a leading `|` are dropped.
///
/// Use `\|` to include a literal `|` in the output.
///
/// Prefer the [`trim_margin!`] macro which wraps `format!` for convenience.
pub fn _trim_margin(s: &str) -> String {
    s.lines()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            trimmed
                .strip_prefix('|')
                .map(|rest| rest.replace("\\|", "|"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Build a margin-trimmed string with `format!`-style interpolation.
///
/// Each line must start with optional whitespace followed by `|`. The `|` and
/// all preceding whitespace are stripped. Use `\|` for a literal pipe character.
///
/// # Example
///
/// ```ignore
/// let name = "world";
/// let s = trim_margin!(
///     "|Hello, {name}!
///      |  indented line
///      |done
///      |"
/// );
/// assert_eq!(s, "Hello, world!\n  indented line\ndone\n");
/// ```
macro_rules! trim_margin {
    ($fmt:literal $(, $arg:expr)* $(,)?) => {
        $crate::test_util::_trim_margin(&format!($fmt $(, $arg)*))
    };
}
pub(crate) use trim_margin;

#[cfg(test)]
mod tests {
    use super::_trim_margin;

    #[test]
    fn trim_margin_basic() {
        let result = _trim_margin(
            "|line one
             |  line two
             |line three",
        );
        assert_eq!(result, "line one\n  line two\nline three");
    }

    #[test]
    fn trim_margin_trailing_newline() {
        let result = _trim_margin(
            "|hello
             |",
        );
        assert_eq!(result, "hello\n");
    }

    #[test]
    fn trim_margin_escaped_pipe() {
        let result = _trim_margin(
            "|a \\| b
             |c",
        );
        assert_eq!(result, "a | b\nc");
    }

    #[test]
    fn trim_margin_skips_lines_without_pipe() {
        let result = _trim_margin(
            "no pipe here
             |has pipe",
        );
        assert_eq!(result, "has pipe");
    }

    #[test]
    fn trim_margin_with_format() {
        let name = "world";
        let s = trim_margin!(
            "|Hello, {name}!
             |  indented
             |"
        );
        assert_eq!(s, "Hello, world!\n  indented\n");
    }
}
