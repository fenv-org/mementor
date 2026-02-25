use anyhow::{Result, bail};

use super::command::git;

/// Metadata extracted from a single git commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitInfo {
    pub hash: String,
    pub short_hash: String,
    pub subject: String,
    pub author: String,
    pub date: String,
    /// The `Entire-Checkpoint` trailer value, if present.
    pub checkpoint_id: Option<String>,
}

/// Separator used between commit records in the formatted log output.
const RECORD_SEPARATOR: &str = "---";

/// Git log format string that produces one record per commit.
///
/// Fields (one per line):
/// 1. Full hash
/// 2. Short hash
/// 3. Subject
/// 4. Author name
/// 5. Author date (ISO-like)
/// 6. `Entire-Checkpoint` trailer value (empty if absent)
/// 7. Record separator (`---`)
const LOG_FORMAT: &str =
    "%H%n%h%n%s%n%an%n%ai%n%(trailers:key=Entire-Checkpoint,valueonly,separator=%x2C)%n---";

/// Parse the multi-line output from `git log --format=<LOG_FORMAT>` into
/// a list of [`CommitInfo`] values.
fn parse_log_output(output: &str) -> Result<Vec<CommitInfo>> {
    let mut commits = Vec::new();
    let mut lines = output.lines().peekable();

    while lines.peek().is_some() {
        // Skip empty lines between records.
        let hash = loop {
            match lines.next() {
                Some(line) if line.is_empty() || line == RECORD_SEPARATOR => {}
                Some(line) => break line,
                None => return Ok(commits),
            }
        };

        let short_hash = lines
            .next()
            .ok_or_else(|| anyhow::anyhow!("truncated log: missing short_hash"))?;
        let subject = lines
            .next()
            .ok_or_else(|| anyhow::anyhow!("truncated log: missing subject"))?;
        let author = lines
            .next()
            .ok_or_else(|| anyhow::anyhow!("truncated log: missing author"))?;
        let date = lines
            .next()
            .ok_or_else(|| anyhow::anyhow!("truncated log: missing date"))?;
        let trailer = lines
            .next()
            .ok_or_else(|| anyhow::anyhow!("truncated log: missing trailer"))?;

        // The separator line ("---") follows the trailer.
        let sep = lines.next();
        if sep != Some(RECORD_SEPARATOR) {
            bail!("expected record separator '---', got {sep:?} after commit {hash}");
        }

        let checkpoint_id = if trailer.is_empty() {
            None
        } else {
            Some(trailer.to_owned())
        };

        commits.push(CommitInfo {
            hash: hash.to_owned(),
            short_hash: short_hash.to_owned(),
            subject: subject.to_owned(),
            author: author.to_owned(),
            date: date.to_owned(),
            checkpoint_id,
        });
    }

    Ok(commits)
}

/// Retrieve the most recent commits on `branch`, including any
/// `Entire-Checkpoint` trailer values.
pub async fn log_with_checkpoints(branch: &str, limit: usize) -> Result<Vec<CommitInfo>> {
    let limit_arg = format!("-{limit}");
    let format_arg = format!("--format={LOG_FORMAT}");
    let output = git(&["log", &limit_arg, &format_arg, branch]).await?;
    parse_log_output(&output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_log_output() -> String {
        [
            "abc123def456789abcdef0123456789abcdef01",
            "abc123d",
            "add feature X",
            "Alice",
            "2026-02-20 10:30:00 +0900",
            "cp-001",
            "---",
            "def456789abcdef0123456789abcdef0123456789",
            "def4567",
            "fix bug Y",
            "Bob",
            "2026-02-19 09:00:00 +0900",
            "",
            "---",
        ]
        .join("\n")
    }

    #[test]
    fn parse_two_commits() {
        let output = sample_log_output();
        let commits = parse_log_output(&output).unwrap();

        assert_eq!(commits.len(), 2);

        assert_eq!(commits[0].short_hash, "abc123d");
        assert_eq!(commits[0].subject, "add feature X");
        assert_eq!(commits[0].author, "Alice");
        assert_eq!(commits[0].checkpoint_id.as_deref(), Some("cp-001"));

        assert_eq!(commits[1].short_hash, "def4567");
        assert_eq!(commits[1].subject, "fix bug Y");
        assert_eq!(commits[1].author, "Bob");
        assert!(commits[1].checkpoint_id.is_none());
    }

    #[test]
    fn parse_empty_output() {
        let commits = parse_log_output("").unwrap();
        assert!(commits.is_empty());
    }

    #[test]
    fn parse_single_commit_no_trailer() {
        let output = [
            "aaaa1111bbbb2222cccc3333dddd4444eeee5555",
            "aaaa111",
            "initial commit",
            "Charlie",
            "2026-01-01 00:00:00 +0000",
            "",
            "---",
        ]
        .join("\n");

        let commits = parse_log_output(&output).unwrap();
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].subject, "initial commit");
        assert!(commits[0].checkpoint_id.is_none());
    }

    #[test]
    fn parse_truncated_output_fails() {
        let output = [
            "aaaa1111bbbb2222cccc3333dddd4444eeee5555",
            "aaaa111",
            // missing subject, author, date, trailer, separator
        ]
        .join("\n");

        assert!(parse_log_output(&output).is_err());
    }

    #[test]
    fn parse_trailing_newlines() {
        let output = [
            "aaaa1111bbbb2222cccc3333dddd4444eeee5555",
            "aaaa111",
            "commit msg",
            "Dev",
            "2026-02-25 12:00:00 +0900",
            "",
            "---",
            "",
            "",
        ]
        .join("\n");

        let commits = parse_log_output(&output).unwrap();
        assert_eq!(commits.len(), 1);
    }
}
