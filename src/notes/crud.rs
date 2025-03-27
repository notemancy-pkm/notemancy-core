// src/notes/crud.rs

use crate::notes::utils::{check_unique_title, sanitize_title};
use anyhow::{Context, Result, anyhow};
use chrono::prelude::*;
use std::fs::{self, create_dir_all};
use std::path::{Path, PathBuf};

/// Creates a new markdown note with the given title in the specified project directory.
///
/// # Arguments
/// * `title` - The title of the note (will be sanitized for the filename)
/// * `vault_directory` - The base directory of the vault
/// * `project` - The sub-path within the vault where the note should be created
///
/// # Returns
/// * `Result<PathBuf>` - The path to the created note file
///
/// # Errors
/// * Returns an error if the title is not unique (a file with that name already exists)
/// * Returns an error if there is an issue creating the directory or file
///
/// # Examples
/// ```
/// use std::path::Path;
/// use notemancy_core::notes::crud::create_note;
///
/// let vault_dir = Path::new("/path/to/vault");
/// let result = create_note("My New Note", vault_dir, "personal/ideas");
/// ```
pub fn create_note(title: &str, vault_directory: &Path, project: &str) -> Result<PathBuf> {
    // Sanitize the title for use as a filename
    let sanitized_title = sanitize_title(title);

    // Check if the title is unique in the vault
    if !check_unique_title(title, vault_directory)? {
        return Err(anyhow!("A note with the title '{}' already exists", title));
    }

    // Create the full project path
    let project_path = vault_directory.join(project);

    // Create the project directory if it doesn't exist
    create_dir_all(&project_path).context("Failed to create project directory")?;

    // Create the full path to the note file
    let note_path = project_path.join(format!("{}.md", sanitized_title));

    // Get the current date and time for frontmatter
    let now = Local::now();
    let date_str = now.format("%Y-%m-%d").to_string();
    let timestamp_str = now.format("%Y-%m-%d %H:%M:%S").to_string();

    // Create the frontmatter content
    let content = format!(
        "---\ntitle: {}\ncreated_on: {}\nmodified_at: {}\n---\n\n",
        title, date_str, timestamp_str
    );

    // Write the content to the file
    fs::write(&note_path, content).context("Failed to write note file")?;

    Ok(note_path)
}

/// Reads a markdown note with the given title from the vault directory.
///
/// # Arguments
/// * `title` - The title of the note to read
/// * `vault_directory` - The base directory of the vault
/// * `frontmatter` - Whether to include frontmatter in the returned content (true)
///                   or strip it (false/None)
///
/// # Returns
/// * `Result<String>` - The content of the note, with or without frontmatter
///
/// # Errors
/// * Returns an error if the note is not found
/// * Returns an error if there is an issue reading the file
///
/// # Examples
/// ```
/// use std::path::Path;
/// use notemancy_core::notes::crud::read_note;
///
/// let vault_dir = Path::new("/path/to/vault");
///
/// // Read note with frontmatter
/// let content_with_frontmatter = read_note("My Note", vault_dir, true);
///
/// // Read note without frontmatter
/// let content_without_frontmatter = read_note("My Note", vault_dir, false);
/// ```
pub fn read_note(title: &str, vault_directory: &Path, frontmatter: bool) -> Result<String> {
    // Get the file path for the note
    let file_path = crate::notes::utils::get_file_path(title, vault_directory)?;

    // Read the content of the file
    let content = fs::read_to_string(&file_path)
        .context(format!("Failed to read note file: {}", file_path))?;

    if frontmatter {
        // Return the entire content if frontmatter is required
        Ok(content)
    } else {
        // Strip frontmatter if present and return only the content
        strip_frontmatter(&content)
    }
}

/// Helper function to strip YAML frontmatter from markdown content.
///
/// Frontmatter is expected to start and end with "---" on its own line.
/// If no frontmatter is detected, the original content is returned unchanged.
fn strip_frontmatter(content: &str) -> Result<String> {
    // Check if the content starts with frontmatter delimiter
    if content.trim_start().starts_with("---") {
        // Find the end of the frontmatter (second occurrence of "---")
        let start_pos = content.find("---").unwrap(); // Safe because we checked it starts with ---
        let from_first_delimiter = &content[start_pos + 3..]; // Skip past first "---"

        if let Some(second_delimiter_pos) = from_first_delimiter.find("---") {
            // Calculate the absolute position of the second delimiter in the original string
            let end_pos = start_pos + 3 + second_delimiter_pos;

            // Find the next newline after the second "---"
            if let Some(newline_pos) = content[end_pos..].find('\n') {
                let content_start = end_pos + newline_pos + 1;
                return Ok(content[content_start..].trim_start().to_string());
            } else {
                // No content after frontmatter
                return Ok(String::new());
            }
        } else {
            // Could not find second delimiter, return original content
            return Ok(content.to_string());
        }
    } else {
        // No frontmatter, return the original content
        Ok(content.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_create_note_with_valid_title() -> Result<()> {
        // Create a temporary directory for the test vault
        let temp_dir = tempdir()?;
        let vault_dir = temp_dir.path();

        // Create a test project path
        let project = "test/project";

        // Create a note with a valid title
        let title = "Test Note";
        let note_path = create_note(title, vault_dir, project)?;

        // Verify the note was created at the correct path
        let expected_path = vault_dir.join(project).join("Test-Note.md");
        assert_eq!(note_path, expected_path);
        assert!(note_path.exists(), "Note file was not created");

        // Verify the content of the note
        let content = fs::read_to_string(&note_path)?;
        assert!(
            content.contains("title: Test Note"),
            "Frontmatter title is incorrect"
        );
        assert!(
            content.contains("created_on:"),
            "Frontmatter created_on is missing"
        );
        assert!(
            content.contains("modified_at:"),
            "Frontmatter modified_at is missing"
        );

        Ok(())
    }

    #[test]
    fn test_create_note_with_invalid_characters() -> Result<()> {
        // Create a temporary directory for the test vault
        let temp_dir = tempdir()?;
        let vault_dir = temp_dir.path();

        // Create a test project path
        let project = "test/project";

        // Create a note with a title containing invalid characters
        let title = "Test Note: With <invalid> chars?";
        let note_path = create_note(title, vault_dir, project)?;

        // Verify the note was created with sanitized filename
        let expected_path = vault_dir
            .join(project)
            .join("Test-Note-With-invalid-chars.md");
        assert_eq!(note_path, expected_path);
        assert!(note_path.exists(), "Note file was not created");

        // Verify the content uses the original title
        let content = fs::read_to_string(&note_path)?;
        assert!(
            content.contains("title: Test Note: With <invalid> chars?"),
            "Frontmatter should contain original title"
        );

        Ok(())
    }

    #[test]
    fn test_create_note_in_nonexistent_project() -> Result<()> {
        // Create a temporary directory for the test vault
        let temp_dir = tempdir()?;
        let vault_dir = temp_dir.path();

        // Create a deep nested project path that doesn't exist yet
        let project = "deep/nested/project/path";

        // Create a note, which should create all necessary directories
        let title = "Deep Note";
        let note_path = create_note(title, vault_dir, project)?;

        // Verify the project directories were created
        let project_dir = vault_dir.join(project);
        assert!(project_dir.exists(), "Project directory was not created");

        // Verify the note was created at the correct path
        let expected_path = project_dir.join("Deep-Note.md");
        assert_eq!(note_path, expected_path);
        assert!(note_path.exists(), "Note file was not created");

        Ok(())
    }

    #[test]
    fn test_create_note_with_duplicate_title() -> Result<()> {
        // Create a temporary directory for the test vault
        let temp_dir = tempdir()?;
        let vault_dir = temp_dir.path();

        // Create a test project path
        let project = "test/project";

        // Create a note
        let title = "Duplicate Note";
        let first_note = create_note(title, vault_dir, project)?;
        assert!(first_note.exists(), "First note was not created");

        // Try to create another note with the same title
        let result = create_note(title, vault_dir, project);

        // Verify that an error was returned
        assert!(result.is_err(), "Creating a duplicate note should fail");
        if let Err(err) = result {
            assert!(
                err.to_string().contains("already exists"),
                "Error message should mention duplicate title"
            );
        }

        Ok(())
    }

    #[test]
    fn test_strip_frontmatter() -> Result<()> {
        // Test with valid frontmatter
        let content =
            "---\ntitle: Test Note\ncreated_on: 2023-01-01\n---\n\nThis is the note content.";
        let stripped = strip_frontmatter(content)?;
        assert_eq!(stripped, "This is the note content.");

        // Test with no frontmatter
        let content_no_frontmatter = "This is a note without frontmatter.";
        let stripped = strip_frontmatter(content_no_frontmatter)?;
        assert_eq!(stripped, content_no_frontmatter);

        // Test with only frontmatter start delimiter
        let content_incomplete = "---\ntitle: Incomplete\n\nContent after incomplete frontmatter.";
        let stripped = strip_frontmatter(content_incomplete)?;
        assert_eq!(stripped, content_incomplete);

        // Test with leading whitespace before frontmatter
        let content_with_leading_whitespace = "  \n\n---\ntitle: Test\n---\n\nContent.";
        let stripped = strip_frontmatter(content_with_leading_whitespace)?;
        assert_eq!(stripped, "Content.");

        Ok(())
    }

    #[test]
    fn test_read_note_with_frontmatter() -> Result<()> {
        // Create a temporary directory for the test vault
        let temp_dir = tempdir()?;
        let vault_dir = temp_dir.path();

        // Create a test note
        let title = "Read Test Note";
        let _note_path = create_note(title, vault_dir, "test")?;

        // Read the note with frontmatter
        let content = read_note(title, vault_dir, true)?;

        // Verify the content includes frontmatter
        assert!(
            content.contains("---"),
            "Content should include frontmatter delimiters"
        );
        assert!(
            content.contains("title: Read Test Note"),
            "Content should include title in frontmatter"
        );

        Ok(())
    }

    #[test]
    fn test_read_note_without_frontmatter() -> Result<()> {
        // Create a temporary directory for the test vault
        let temp_dir = tempdir()?;
        let vault_dir = temp_dir.path();

        // Create a test note
        let title = "Read Test Note";
        let note_path = create_note(title, vault_dir, "test")?;

        // Add some content after the frontmatter
        let original_content = fs::read_to_string(&note_path)?;
        let new_content = format!("{}This is the actual note content.", original_content);
        fs::write(&note_path, new_content)?;

        // Read the note without frontmatter
        let content = read_note(title, vault_dir, false)?;

        // Verify the content excludes frontmatter
        assert!(
            !content.contains("---"),
            "Content should not include frontmatter delimiters"
        );
        assert!(
            !content.contains("title:"),
            "Content should not include frontmatter fields"
        );
        assert!(
            content.contains("This is the actual note content."),
            "Content should include the actual note"
        );

        Ok(())
    }

    #[test]
    fn test_read_note_not_found() -> Result<()> {
        // Create a temporary directory for the test vault
        let temp_dir = tempdir()?;
        let vault_dir = temp_dir.path();

        // Try to read a non-existent note
        let result = read_note("Non Existent Note", vault_dir, true);

        // Verify that an error was returned
        assert!(result.is_err(), "Reading a non-existent note should fail");

        Ok(())
    }

    #[test]
    fn test_read_note_with_modified_content() -> Result<()> {
        // Create a temporary directory for the test vault
        let temp_dir = tempdir()?;
        let vault_dir = temp_dir.path();

        // Create a test note with complex content
        let title = "Complex Content Note";
        let note_path = create_note(title, vault_dir, "test")?;

        // Create more complex content with multiple frontmatter-like sections
        let complex_content = r#"---
title: Complex Content Note
created_on: 2023-01-01
tags:
  - test
  - example
---

# Main Content

This is the main content of the note.

## Code Example
More text after the code block.

---

This is a horizontal rule, not a frontmatter delimiter.

---

End of note.
"#;

        fs::write(&note_path, complex_content)?;

        // Read with frontmatter
        let with_frontmatter = read_note(title, vault_dir, true)?;
        assert_eq!(with_frontmatter, complex_content);

        // Read without frontmatter
        let without_frontmatter = read_note(title, vault_dir, false)?;
        assert!(!without_frontmatter.contains("title: Complex Content Note"));
        assert!(without_frontmatter.contains("# Main Content"));
        assert!(without_frontmatter.contains("This is a horizontal rule"));

        Ok(())
    }
}
