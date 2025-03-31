// src/tags.rs

use crate::notes::utils::get_title;
use anyhow::{Context, Result};
use regex::escape;
use std::path::Path;
use std::process::Command;

/// Returns a list of all unique tags found in the vault directory.
///
/// This function executes a shell command using ripgrep (rg) and sort.
/// It sets the current directory to the vault directory so that the dot (.)
/// in the command refers to the vault root.
///
/// The command used is:
///
///   rg -U -oP '(?s)^---.*?^tags:\s*\n((?:\s*-\s*.*\n)+)' --no-filename . \
///     | rg -oP '^\s*-\s*\K.*' \
///     | rg -v '^\s*$' \
///     | rg -v '^[-ー]+$' \
///     | sort -u
///
pub fn get_all_tags(vault_directory: &Path) -> Result<Vec<String>> {
    let command = "rg -U -oP '(?s)^---.*?^tags:\\s*\\n((?:\\s*-\\s*.*\\n)+)' --no-filename . | \
                   rg -oP '^\\s*-\\s*\\K.*' | \
                   rg -v '^\\s*$' | \
                   rg -v '^[-ー]+$' | \
                   sort -u";
    let output = Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(vault_directory)
        .output()
        .context("Failed to execute rg command for get_all_tags")?;
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "get_all_tags command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let tags: Vec<String> = stdout
        .lines()
        .map(|s| s.to_string())
        .filter(|s| !s.trim().is_empty())
        .collect();
    Ok(tags)
}

/// Returns a list of all notes (as tuples of relative path and title)
/// that have the given tag. This function builds a regex that matches YAML
/// frontmatter containing a list of tags in which one of the items exactly
/// matches the provided tag. Then it executes ripgrep to list matching markdown files.
/// For each file, it extracts the note title using get_title.
///
/// The command used is of the form:
///
///   rg -l -U -P '(?s)^---.*?\n\s*-\s*<TAG>\s*\n.*?^---' --glob '*.md'
///
pub fn get_notes_by_tag(tag: &str, vault_directory: &Path) -> Result<Vec<(String, String)>> {
    // Escape the tag for use in the regex pattern.
    let escaped_tag = escape(tag);
    let pattern = format!("(?s)^---.*?\\n\\s*-\\s*{}\\s*\\n.*?^---", escaped_tag);
    let command = format!("rg -l -U -P '{}' --glob '*.md'", pattern);
    let output = Command::new("sh")
        .arg("-c")
        .arg(&command)
        .current_dir(vault_directory)
        .output()
        .context("Failed to execute rg command for get_notes_by_tag")?;
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "get_notes_by_tag command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut results = Vec::new();
    for line in stdout.lines().filter(|s| !s.trim().is_empty()) {
        // The output is a file path relative to vault_directory.
        let rel_path = line.trim().to_string();
        let file_path = vault_directory.join(&rel_path);
        // Use get_title to extract the note title.
        let title = get_title(&file_path).unwrap_or_else(|_| String::from("<No Title>"));
        results.push((rel_path, title));
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::tempdir;

    // Helper function to create a markdown note with frontmatter.
    fn create_markdown_note(
        dir: &Path,
        filename: &str,
        title: &str,
        tags: &[&str],
    ) -> Result<PathBuf> {
        let file_path = dir.join(filename);
        let mut frontmatter = format!("---\ntitle: {}\ntags:\n", title);
        for tag in tags {
            frontmatter.push_str(&format!("  - {}\n", tag));
        }
        frontmatter.push_str("---\n\nContent of the note.");
        fs::write(&file_path, frontmatter)?;
        Ok(file_path)
    }

    #[test]
    fn test_get_all_tags() -> Result<()> {
        let temp_dir = tempdir()?;
        let vault_dir = temp_dir.path();

        // Create three notes with various tags.
        create_markdown_note(vault_dir, "note1.md", "Note One", &["CLI", "rust"])?;
        create_markdown_note(vault_dir, "note2.md", "Note Two", &["rust"])?;
        create_markdown_note(vault_dir, "note3.md", "Note Three", &["something else"])?;

        let mut tags = get_all_tags(vault_dir)?;
        tags.sort();
        // The sorted order is determined by lexicographic order: uppercase letters come before lowercase.
        // Therefore, the expected order is: ["CLI", "rust", "something else"]
        let expected = vec![
            "CLI".to_string(),
            "rust".to_string(),
            "something else".to_string(),
        ];
        assert_eq!(tags, expected);
        Ok(())
    }

    #[test]
    fn test_get_notes_by_tag() -> Result<()> {
        let temp_dir = tempdir()?;
        let vault_dir = temp_dir.path();

        // Create three notes.
        create_markdown_note(vault_dir, "note1.md", "Note One", &["CLI", "rust"])?;
        create_markdown_note(vault_dir, "note2.md", "Note Two", &["rust"])?;
        create_markdown_note(vault_dir, "note3.md", "Note Three", &["CLI"])?;

        // Search for notes with tag "CLI"
        let mut notes_cli = get_notes_by_tag("CLI", vault_dir)?;
        notes_cli.sort_by(|a, b| a.0.cmp(&b.0));
        let expected_cli = vec![
            (String::from("note1.md"), String::from("Note One")),
            (String::from("note3.md"), String::from("Note Three")),
        ];
        assert_eq!(notes_cli, expected_cli);

        // Search for notes with tag "rust"
        let mut notes_rust = get_notes_by_tag("rust", vault_dir)?;
        notes_rust.sort_by(|a, b| a.0.cmp(&b.0));
        let expected_rust = vec![
            (String::from("note1.md"), String::from("Note One")),
            (String::from("note2.md"), String::from("Note Two")),
        ];
        assert_eq!(notes_rust, expected_rust);

        Ok(())
    }
}
