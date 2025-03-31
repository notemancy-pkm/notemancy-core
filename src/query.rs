// src/query.rs

use crate::query_parser::{build_jq_expression, parse_query};
use anyhow::{Context, Result, anyhow};
use serde_json::Value;
use std::path::Path;
use std::process::Command;

/// Runs a DSL query against the YAML frontmatter of markdown notes in the vault.
///
/// The DSL query is parsed and converted into a jq expression (using our query_parser module).
/// For example, a DSL query like:
///
/// ```ignore
/// not tag = "CLI" and date >= "2025-03-01"
/// ```
///
/// will be converted into a jq filter that checks that the note’s tags array does not contain "CLI"
/// (by producing an expression like `(.tags | index("CLI") == null)`) and that the date field meets the condition.
///
/// Then, a shell pipeline is executed that:
/// 1. Uses `fd` to list all markdown files (glob "*.md") in the vault directory.
/// 2. For each file, uses `sed` to extract its YAML frontmatter.
/// 3. If frontmatter exists, pipes it into `yq` (which converts it to JSON) and uses `jq` to add a "path"
///    field (with any leading "./" removed).
/// 4. Aggregates all JSON objects into an array using `jq -s` and applies a final jq filter that selects
///    only those note objects matching the DSL query, mapping each note to only its `path` and `title`.
///
/// Returns a vector of `(relative_path, title)` pairs for the matched notes.
///
/// Note: This implementation requires that the following CLI tools be installed:
///   - fd
///   - sed
///   - yq (version that supports `eval -o=json`)
///   - jq
pub fn query_notes(vault_directory: &Path, query: &str) -> Result<Vec<(String, String)>> {
    // Parse the DSL query and build a jq expression.
    let ast = parse_query(query)?;
    let jq_expr = build_jq_expression(&ast);
    // Build a jq filter that selects note objects and maps them to {path, title}.
    let filter = format!(
        "map(select({})) | map({{path: .path, title: .title}})",
        jq_expr
    );
    let full_filter = format!("{{notes: {}}}", filter);

    // Build the shell pipeline.
    let cmd = format!(
        "fd --glob '*.md' . | while read file; do \
  clean=$(echo \"$file\" | sed 's|^\\./||'); \
  front=$(sed -n '/^---$/,/^---$/p' \"$file\"); \
  if [ -n \"$front\" ]; then \
    echo \"$front\" | yq eval -o=json - | jq --arg path \"$clean\" '. + {{path: $path}}'; \
  fi; \
done | jq -s '{}'",
        full_filter
    );

    let output = Command::new("sh")
        .arg("-c")
        .arg(&cmd)
        .current_dir(vault_directory)
        .output()
        .context("Failed to execute query pipeline")?;
    if !output.status.success() {
        return Err(anyhow!(
            "Query pipeline failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    // Parse the output JSON; expect an object with a "notes" array.
    let json: Value =
        serde_json::from_str(&stdout).context("Failed to parse JSON output from query pipeline")?;
    let mut results = Vec::new();
    if let Some(notes) = json.get("notes").and_then(|n| n.as_array()) {
        for note in notes {
            if let (Some(path), Some(title)) = (
                note.get("path").and_then(|v| v.as_str()),
                note.get("title").and_then(|v| v.as_str()),
            ) {
                results.push((path.to_string(), title.to_string()));
            }
        }
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use std::fs;
    use std::path::Path;
    use tempfile::tempdir;

    // Helper: Create a markdown note with YAML frontmatter.
    // The frontmatter includes "title", "date", and "tags" fields.
    fn create_markdown_note(
        dir: &Path,
        filename: &str,
        title: &str,
        date: &str,
        tags: &[&str],
    ) -> Result<()> {
        let file_path = dir.join(filename);
        let mut frontmatter = format!("---\ntitle: {}\ndate: {}\ntags:\n", title, date);
        for tag in tags {
            frontmatter.push_str(&format!("  - {}\n", tag));
        }
        frontmatter.push_str("---\n\nContent of the note.");
        fs::write(&file_path, frontmatter)?;
        Ok(())
    }

    #[test]
    fn test_query_notes_by_date_range() -> Result<()> {
        let temp_dir = tempdir()?;
        let vault_dir = temp_dir.path();
        // Create three notes with different dates.
        create_markdown_note(vault_dir, "note1.md", "Note One", "2025-03-01", &["CLI"])?;
        create_markdown_note(vault_dir, "note2.md", "Note Two", "2025-03-15", &["rust"])?;
        create_markdown_note(
            vault_dir,
            "note3.md",
            "Note Three",
            "2025-04-01",
            &["CLI", "rust"],
        )?;
        // Query for notes between 2025-03-01 and 2025-03-31.
        let query = r#"date >= "2025-03-01" and date <= "2025-03-31""#;
        let results = query_notes(vault_dir, query)?;
        let mut paths: Vec<_> = results.iter().map(|(p, _)| p.clone()).collect();
        paths.sort();
        assert_eq!(paths, vec!["note1.md".to_string(), "note2.md".to_string()]);
        Ok(())
    }

    #[test]
    fn test_query_notes_with_tag_not() -> Result<()> {
        let temp_dir = tempdir()?;
        let vault_dir = temp_dir.path();
        // Create two notes.
        create_markdown_note(
            vault_dir,
            "note1.md",
            "Note One",
            "2025-03-01",
            &["CLI", "rust"],
        )?;
        create_markdown_note(vault_dir, "note2.md", "Note Two", "2025-03-15", &["rust"])?;
        // Query for notes that do NOT have tag "CLI"
        let query = r#"not tag = "CLI""#;
        let results = query_notes(vault_dir, query)?;
        let paths: Vec<_> = results.iter().map(|(p, _)| p.clone()).collect();
        assert_eq!(paths, vec!["note2.md".to_string()]);
        Ok(())
    }

    #[test]
    fn test_query_notes_complex() -> Result<()> {
        let temp_dir = tempdir()?;
        let vault_dir = temp_dir.path();
        // Create four notes.
        create_markdown_note(vault_dir, "note1.md", "Note One", "2025-03-01", &["CLI"])?;
        create_markdown_note(vault_dir, "note2.md", "Note Two", "2025-03-15", &["rust"])?;
        create_markdown_note(
            vault_dir,
            "note3.md",
            "Note Three",
            "2025-03-20",
            &["CLI", "rust"],
        )?;
        create_markdown_note(vault_dir, "note4.md", "Note Four", "2025-04-05", &["CLI"])?;
        // Query: within March 2025 and either has tag "rust" or does NOT have tag "CLI".
        // Breakdown:
        // - Note1: has "CLI" so not(tag = "CLI") is false, and doesn't have "rust" => false.
        // - Note2: has "rust" => true.
        // - Note3: has "CLI" and "rust": tag="rust" is true, so overall true.
        // - Note4: outside date range.
        let query = r#"date >= "2025-03-01" and date <= "2025-03-31" and (tag = "rust" or not tag = "CLI")"#;
        let results = query_notes(vault_dir, query)?;
        let mut paths: Vec<_> = results.iter().map(|(p, _)| p.clone()).collect();
        paths.sort();
        // Expect note2.md and note3.md
        assert_eq!(paths, vec!["note2.md".to_string(), "note3.md".to_string()]);
        Ok(())
    }

    #[test]
    fn test_query_notes_or_condition() -> Result<()> {
        let temp_dir = tempdir()?;
        let vault_dir = temp_dir.path();
        // Create three notes with different tags.
        create_markdown_note(vault_dir, "note1.md", "Note One", "2025-03-01", &["CLI"])?;
        create_markdown_note(vault_dir, "note2.md", "Note Two", "2025-03-10", &["rust"])?;
        create_markdown_note(
            vault_dir,
            "note3.md",
            "Note Three",
            "2025-03-20",
            &["CLI", "rust"],
        )?;
        // Query for notes that have either tag "CLI" or tag "rust".
        let query = r#"tag = "CLI" or tag = "rust""#;
        let results = query_notes(vault_dir, query)?;
        let mut paths: Vec<_> = results.iter().map(|(p, _)| p.clone()).collect();
        paths.sort();
        // All three notes should match.
        assert_eq!(
            paths,
            vec![
                "note1.md".to_string(),
                "note2.md".to_string(),
                "note3.md".to_string()
            ]
        );
        Ok(())
    }
}
