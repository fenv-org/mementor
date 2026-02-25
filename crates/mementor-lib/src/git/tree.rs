use anyhow::{Result, bail};

use super::command::{git, git_bytes};

/// The type of object in a git tree entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectType {
    Blob,
    Tree,
}

/// A single entry from `git ls-tree`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    pub name: String,
    pub object_type: ObjectType,
    pub hash: String,
}

/// Parse a single line of `git ls-tree` output.
///
/// Expected format: `<mode> <type> <hash>\t<name>`
fn parse_ls_tree_line(line: &str) -> Result<TreeEntry> {
    let (meta, name) = line
        .split_once('\t')
        .ok_or_else(|| anyhow::anyhow!("missing tab in ls-tree line: {line}"))?;

    let parts: Vec<&str> = meta.split(' ').collect();
    if parts.len() != 3 {
        bail!(
            "expected 3 space-separated fields before tab, got {}: {line}",
            parts.len()
        );
    }

    let object_type = match parts[1] {
        "blob" => ObjectType::Blob,
        "tree" => ObjectType::Tree,
        other => bail!("unknown object type: {other}"),
    };

    Ok(TreeEntry {
        name: name.to_owned(),
        object_type,
        hash: parts[2].to_owned(),
    })
}

/// List entries in a tree at the given path on the specified branch.
pub async fn ls_tree(branch: &str, path: &str) -> Result<Vec<TreeEntry>> {
    let ref_path = if path.is_empty() {
        branch.to_owned()
    } else {
        format!("{branch}:{path}")
    };

    let output = git(&["ls-tree", &ref_path]).await?;
    output
        .lines()
        .filter(|line| !line.is_empty())
        .map(parse_ls_tree_line)
        .collect()
}

/// Read a blob as raw bytes from the given branch and path.
pub async fn show_blob(branch: &str, path: &str) -> Result<Vec<u8>> {
    git_bytes(&["show", &format!("{branch}:{path}")]).await
}

/// Read a blob as a UTF-8 string from the given branch and path.
pub async fn show_blob_str(branch: &str, path: &str) -> Result<String> {
    git(&["show", &format!("{branch}:{path}")]).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_blob_entry() {
        let line = "100644 blob abc123def456\tREADME.md";
        let entry = parse_ls_tree_line(line).unwrap();
        assert_eq!(entry.name, "README.md");
        assert_eq!(entry.object_type, ObjectType::Blob);
        assert_eq!(entry.hash, "abc123def456");
    }

    #[test]
    fn parse_tree_entry() {
        let line = "040000 tree deadbeef0123\tsrc";
        let entry = parse_ls_tree_line(line).unwrap();
        assert_eq!(entry.name, "src");
        assert_eq!(entry.object_type, ObjectType::Tree);
        assert_eq!(entry.hash, "deadbeef0123");
    }

    #[test]
    fn parse_name_with_spaces() {
        let line = "100644 blob aabbccdd\tpath with spaces/file.txt";
        let entry = parse_ls_tree_line(line).unwrap();
        assert_eq!(entry.name, "path with spaces/file.txt");
    }

    #[test]
    fn parse_missing_tab_fails() {
        let line = "100644 blob abc123 no-tab-here";
        assert!(parse_ls_tree_line(line).is_err());
    }

    #[test]
    fn parse_unknown_type_fails() {
        let line = "100644 commit abc123\tsome-ref";
        assert!(parse_ls_tree_line(line).is_err());
    }

    #[test]
    fn parse_multiple_lines() {
        let output = "100644 blob aaa111\tfile1.rs\n040000 tree bbb222\tsubdir\n";
        let entries: Vec<TreeEntry> = output
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| parse_ls_tree_line(l).unwrap())
            .collect();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "file1.rs");
        assert_eq!(entries[0].object_type, ObjectType::Blob);
        assert_eq!(entries[1].name, "subdir");
        assert_eq!(entries[1].object_type, ObjectType::Tree);
    }
}
