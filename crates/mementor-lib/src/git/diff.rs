use anyhow::Result;

use super::command::git;

/// The status of a file in a diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
}

/// A single line within a diff hunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffLine {
    Context(String),
    Added(String),
    Removed(String),
}

/// A contiguous hunk of changes in a file diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffHunk {
    pub header: String,
    pub lines: Vec<DiffLine>,
}

/// The diff for a single file, including parsed hunks and line statistics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDiff {
    pub path: String,
    pub status: FileStatus,
    pub additions: usize,
    pub deletions: usize,
    pub hunks: Vec<DiffHunk>,
}

/// Parse the unified diff output from `git diff-tree -p` into a list of
/// [`FileDiff`] values.
fn parse_diff_output(output: &str) -> Vec<FileDiff> {
    let mut diffs = Vec::new();
    let mut lines = output.lines().peekable();

    while let Some(line) = lines.peek() {
        if !line.starts_with("diff --git ") {
            lines.next();
            continue;
        }
        lines.next(); // consume the "diff --git" line

        // Collect header lines until we hit a hunk or another diff.
        let mut status = FileStatus::Modified;
        let mut path = String::new();
        let mut rename_to: Option<String> = None;

        while let Some(&header_line) = lines.peek() {
            if header_line.starts_with("@@") || header_line.starts_with("diff --git ") {
                break;
            }
            let header_line = lines.next().unwrap();

            if header_line.starts_with("new file mode") {
                status = FileStatus::Added;
            } else if header_line.starts_with("deleted file mode") {
                status = FileStatus::Deleted;
            } else if header_line.starts_with("rename from")
                || header_line.starts_with("similarity index")
            {
                status = FileStatus::Renamed;
            } else if let Some(to) = header_line.strip_prefix("rename to ") {
                status = FileStatus::Renamed;
                rename_to = Some(to.to_owned());
            } else if let Some(b_path) = header_line.strip_prefix("+++ b/") {
                b_path.clone_into(&mut path);
            } else if path.is_empty()
                && let Some(a_path) = header_line.strip_prefix("--- a/")
            {
                a_path.clone_into(&mut path);
            }
        }

        // For renames, prefer the destination path.
        if let Some(to) = rename_to {
            path = to;
        }

        // If we still have no path (e.g., binary file), try extracting from the
        // diff --git line would have been consumed already, so skip this entry.
        if path.is_empty() {
            continue;
        }

        // Parse hunks.
        let mut hunks = Vec::new();
        let mut additions: usize = 0;
        let mut deletions: usize = 0;

        while let Some(&hunk_line) = lines.peek() {
            if !hunk_line.starts_with("@@") {
                break;
            }
            let header = lines.next().unwrap().to_owned();
            let mut hunk_lines = Vec::new();

            while let Some(&content_line) = lines.peek() {
                if content_line.starts_with("@@") || content_line.starts_with("diff --git ") {
                    break;
                }
                let content_line = lines.next().unwrap();

                if let Some(added) = content_line.strip_prefix('+') {
                    hunk_lines.push(DiffLine::Added(added.to_owned()));
                    additions += 1;
                } else if let Some(removed) = content_line.strip_prefix('-') {
                    hunk_lines.push(DiffLine::Removed(removed.to_owned()));
                    deletions += 1;
                } else if let Some(ctx) = content_line.strip_prefix(' ') {
                    hunk_lines.push(DiffLine::Context(ctx.to_owned()));
                } else if content_line == r"\ No newline at end of file" {
                    // Ignore this marker.
                } else {
                    // Context line without leading space (empty line in diff).
                    hunk_lines.push(DiffLine::Context(content_line.to_owned()));
                }
            }

            hunks.push(DiffHunk {
                header,
                lines: hunk_lines,
            });
        }

        diffs.push(FileDiff {
            path,
            status,
            additions,
            deletions,
            hunks,
        });
    }

    diffs
}

/// Get the diff for a single commit.
///
/// Uses `git diff-tree -p` to produce a unified diff of the commit's changes
/// against its parent.
pub async fn diff_commit(hash: &str) -> Result<Vec<FileDiff>> {
    let output = git(&["diff-tree", "-p", "--no-commit-id", "-M", hash]).await?;
    Ok(parse_diff_output(&output))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_diff() -> &'static str {
        "\
diff --git a/src/main.rs b/src/main.rs
index aaa..bbb 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,4 @@
 fn main() {
-    println!(\"hello\");
+    println!(\"hello world\");
+    println!(\"goodbye\");
 }"
    }

    #[test]
    fn parse_modified_file() {
        let diffs = parse_diff_output(sample_diff());
        assert_eq!(diffs.len(), 1);

        let d = &diffs[0];
        assert_eq!(d.path, "src/main.rs");
        assert_eq!(d.status, FileStatus::Modified);
        assert_eq!(d.additions, 2);
        assert_eq!(d.deletions, 1);
        assert_eq!(d.hunks.len(), 1);
        assert!(d.hunks[0].header.starts_with("@@ -1,3 +1,4 @@"));
    }

    #[test]
    fn parse_added_file() {
        let output = "\
diff --git a/new_file.txt b/new_file.txt
new file mode 100644
index 0000000..abc1234
--- /dev/null
+++ b/new_file.txt
@@ -0,0 +1,2 @@
+line one
+line two";

        let diffs = parse_diff_output(output);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].path, "new_file.txt");
        assert_eq!(diffs[0].status, FileStatus::Added);
        assert_eq!(diffs[0].additions, 2);
        assert_eq!(diffs[0].deletions, 0);
    }

    #[test]
    fn parse_deleted_file() {
        let output = "\
diff --git a/old_file.txt b/old_file.txt
deleted file mode 100644
index abc1234..0000000
--- a/old_file.txt
+++ /dev/null
@@ -1,3 +0,0 @@
-line a
-line b
-line c";

        let diffs = parse_diff_output(output);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].path, "old_file.txt");
        assert_eq!(diffs[0].status, FileStatus::Deleted);
        assert_eq!(diffs[0].additions, 0);
        assert_eq!(diffs[0].deletions, 3);
    }

    #[test]
    fn parse_renamed_file() {
        let output = "\
diff --git a/old_name.rs b/new_name.rs
similarity index 90%
rename from old_name.rs
rename to new_name.rs
index aaa..bbb 100644
--- a/old_name.rs
+++ b/new_name.rs
@@ -1,2 +1,2 @@
-fn old() {}
+fn new() {}";

        let diffs = parse_diff_output(output);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].path, "new_name.rs");
        assert_eq!(diffs[0].status, FileStatus::Renamed);
        assert_eq!(diffs[0].additions, 1);
        assert_eq!(diffs[0].deletions, 1);
    }

    #[test]
    fn parse_multiple_files() {
        let output = "\
diff --git a/a.rs b/a.rs
index aaa..bbb 100644
--- a/a.rs
+++ b/a.rs
@@ -1 +1 @@
-old a
+new a
diff --git a/b.rs b/b.rs
new file mode 100644
index 0000000..ccc
--- /dev/null
+++ b/b.rs
@@ -0,0 +1 @@
+new b";

        let diffs = parse_diff_output(output);
        assert_eq!(diffs.len(), 2);
        assert_eq!(diffs[0].path, "a.rs");
        assert_eq!(diffs[0].status, FileStatus::Modified);
        assert_eq!(diffs[1].path, "b.rs");
        assert_eq!(diffs[1].status, FileStatus::Added);
    }

    #[test]
    fn parse_multiple_hunks() {
        let output = "\
diff --git a/multi.rs b/multi.rs
index aaa..bbb 100644
--- a/multi.rs
+++ b/multi.rs
@@ -1,3 +1,3 @@
 fn a() {
-    old_a();
+    new_a();
 }
@@ -10,3 +10,3 @@
 fn b() {
-    old_b();
+    new_b();
 }";

        let diffs = parse_diff_output(output);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].hunks.len(), 2);
        assert!(diffs[0].hunks[0].header.contains("-1,3"));
        assert!(diffs[0].hunks[1].header.contains("-10,3"));
    }

    #[test]
    fn parse_empty_output() {
        let diffs = parse_diff_output("");
        assert!(diffs.is_empty());
    }

    #[test]
    fn diff_line_variants() {
        let hunk_lines = vec![
            DiffLine::Context("context".into()),
            DiffLine::Added("added".into()),
            DiffLine::Removed("removed".into()),
        ];
        assert_eq!(hunk_lines.len(), 3);
    }
}
