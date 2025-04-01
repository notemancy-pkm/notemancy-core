// src/query_native.rs

use crate::query_parser::{Expr, parse_query};
use anyhow::{Context, Result, anyhow};
use rayon::prelude::*;
use serde_yaml::Value as YamlValue;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Struct to hold note metadata extracted from the YAML frontmatter.
#[derive(Debug)]
pub struct NoteMetadata {
    pub path: String,
    pub title: String,
    pub date: Option<String>,
    pub tags: Vec<String>,
}

/// Extracts the YAML frontmatter as a String from the given content.
/// The frontmatter is defined as the text between the first two lines that consist solely of "---".
fn extract_frontmatter_string(content: &str) -> Option<String> {
    let mut lines = content.lines();
    if let Some(first_line) = lines.next() {
        if first_line.trim() == "---" {
            let mut frontmatter_lines = Vec::new();
            for line in lines {
                if line.trim() == "---" {
                    return Some(frontmatter_lines.join("\n"));
                }
                frontmatter_lines.push(line);
            }
        }
    }
    None
}

/// Loads all markdown files (with extensions "md" or "markdown") from the given vault directory,
/// extracts their YAML frontmatter, and returns a vector of NoteMetadata.
pub fn load_notes(vault_directory: &Path) -> Result<Vec<NoteMetadata>> {
    let mut notes = Vec::new();
    // Walk through the directory recursively using WalkDir.
    for entry in WalkDir::new(vault_directory)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() {
            // Check for markdown file extension.
            if let Some(ext) = path.extension() {
                if ext != "md" && ext != "markdown" {
                    continue;
                }
            } else {
                continue;
            }
            // Compute the relative path from the vault.
            let relative_path = path
                .strip_prefix(vault_directory)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string();
            // Read the file content.
            let content = fs::read_to_string(path)
                .with_context(|| format!("Failed to read file {:?}", path))?;
            // Extract frontmatter.
            if let Some(front_str) = extract_frontmatter_string(&content) {
                // Parse the frontmatter YAML.
                let yaml: YamlValue = serde_yaml::from_str(&front_str)
                    .with_context(|| format!("Failed to parse YAML in file {:?}", path))?;
                if let YamlValue::Mapping(map) = yaml {
                    // Get the title field, falling back to the file stem if missing.
                    let title = if let Some(YamlValue::String(t)) =
                        map.get(&YamlValue::String("title".to_string()))
                    {
                        t.clone()
                    } else {
                        path.file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string()
                    };
                    // Get the date field.
                    let date = if let Some(YamlValue::String(d)) =
                        map.get(&YamlValue::String("date".to_string()))
                    {
                        Some(d.clone())
                    } else {
                        None
                    };
                    // Get the tags field as a Vec<String>.
                    let tags = if let Some(YamlValue::Sequence(seq)) =
                        map.get(&YamlValue::String("tags".to_string()))
                    {
                        seq.iter()
                            .filter_map(|v| {
                                if let YamlValue::String(s) = v {
                                    Some(s.clone())
                                } else {
                                    None
                                }
                            })
                            .collect()
                    } else {
                        Vec::new()
                    };
                    notes.push(NoteMetadata {
                        path: relative_path,
                        title,
                        date,
                        tags,
                    });
                }
            }
        }
    }
    Ok(notes)
}

/// Evaluates the DSL AST expression on a NoteMetadata record.
/// Supported fields include "tag", "date", and "title". Date comparisons assume ISO 8601 strings.
fn evaluate_expr(note: &NoteMetadata, expr: &Expr) -> bool {
    match expr {
        Expr::Condition { field, op, value } => match field.to_lowercase().as_str() {
            "tag" => {
                if op == "=" {
                    note.tags.iter().any(|t| t == value)
                } else if op == "!=" {
                    !note.tags.iter().any(|t| t == value)
                } else {
                    false
                }
            }
            "date" => {
                if let Some(note_date) = &note.date {
                    match op.as_str() {
                        "=" => note_date == value,
                        "!=" => note_date != value,
                        ">=" => note_date >= value,
                        "<=" => note_date <= value,
                        ">" => note_date > value,
                        "<" => note_date < value,
                        _ => false,
                    }
                } else {
                    false
                }
            }
            "title" => match op.as_str() {
                "=" => note.title == *value,
                "!=" => note.title != *value,
                _ => false,
            },
            _ => false,
        },
        Expr::And(lhs, rhs) => evaluate_expr(note, lhs) && evaluate_expr(note, rhs),
        Expr::Or(lhs, rhs) => evaluate_expr(note, lhs) || evaluate_expr(note, rhs),
        Expr::Not(inner) => !evaluate_expr(note, inner),
    }
}

/// Executes a native query on the vault directory using our DSL parser and evaluator.
/// It loads all note metadata natively and uses Rayon to filter them in parallel.
/// Returns a vector of (relative_path, title) pairs for notes that match the query.
pub fn query_notes_native(vault_directory: &Path, query: &str) -> Result<Vec<(String, String)>> {
    let ast = parse_query(query)?;
    let notes = load_notes(vault_directory)?;
    let results: Vec<(String, String)> = notes
        .into_par_iter()
        .filter(|note| evaluate_expr(note, &ast))
        .map(|note| (note.path, note.title))
        .collect();
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query; // shell-based query module
    use anyhow::Result;
    use std::fs;
    use std::path::Path;
    use tempfile::tempdir;

    // Helper: Create a markdown note with YAML frontmatter.
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
    fn test_query_native_vs_shell() -> Result<()> {
        let temp_dir = tempdir()?;
        let vault_dir = temp_dir.path();
        // Create four notes.
        create_markdown_note(vault_dir, "note1.md", "Note One", "2025-03-01", &["CLI"])?;
        create_markdown_note(vault_dir, "note2.md", "Note Two", "2025-03-10", &["rust"])?;
        create_markdown_note(
            vault_dir,
            "note3.md",
            "Note Three",
            "2025-03-20",
            &["CLI", "rust"],
        )?;
        create_markdown_note(vault_dir, "note4.md", "Note Four", "2025-04-05", &["CLI"])?;

        // Query: notes in March that do NOT have tag "CLI".
        let query = r#"date >= "2025-03-01" and date <= "2025-03-31" and not tag = "CLI""#;
        let shell_results = query::query_notes(vault_dir, query)?;
        let native_results = query_notes_native(vault_dir, query)?;
        // Sort results by file path.
        let mut shell_paths: Vec<_> = shell_results.into_iter().map(|(p, _)| p).collect();
        let mut native_paths: Vec<_> = native_results.into_iter().map(|(p, _)| p).collect();
        shell_paths.sort();
        native_paths.sort();
        assert_eq!(shell_paths, native_paths);
        Ok(())
    }

    #[test]
    fn test_complex_query_native() -> Result<()> {
        let temp_dir = tempdir()?;
        let vault_dir = temp_dir.path();
        // Create notes with various dates and tags.
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
        // Query for notes in March that either have tag "rust" or do NOT have tag "CLI".
        let query = r#"date >= "2025-03-01" and date <= "2025-03-31" and (tag = "rust" or not tag = "CLI")"#;
        let results = query_notes_native(vault_dir, query)?;
        let mut paths: Vec<_> = results.into_iter().map(|(p, _)| p).collect();
        paths.sort();
        // Expected: note2.md and note3.md.
        assert_eq!(paths, vec!["note2.md".to_string(), "note3.md".to_string()]);
        Ok(())
    }
}
