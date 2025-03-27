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

/// Updates a markdown note with new content while preserving the frontmatter.
///
/// # Arguments
/// * `title` - The title of the note to update
/// * `vault_directory` - The base directory of the vault
/// * `updated_content` - The new content to write to the note (without frontmatter)
///
/// # Returns
/// * `Result<()>` - Ok if the note was successfully updated
///
/// # Errors
/// * Returns an error if the note is not found
/// * Returns an error if there is an issue reading or writing the file
///
/// # Examples
/// ```
/// use std::path::Path;
/// use notemancy_core::notes::crud::update_note;
///
/// let vault_dir = Path::new("/path/to/vault");
/// let new_content = "# Updated Content\n\nThis is the new content of the note.";
/// let result = update_note("My Note", vault_dir, new_content);
/// ```
pub fn update_note(title: &str, vault_directory: &Path, updated_content: &str) -> Result<()> {
    // Get the file path for the note
    let file_path = crate::notes::utils::get_file_path(title, vault_directory)?;

    // Read the current content of the file
    let current_content = fs::read_to_string(&file_path)
        .context(format!("Failed to read note file: {}", file_path))?;

    // Extract the frontmatter if it exists
    let frontmatter = extract_frontmatter(&current_content)?;

    // Combine frontmatter with updated content
    let new_content = if let Some(frontmatter) = frontmatter {
        format!("{}\n\n{}", frontmatter, updated_content.trim())
    } else {
        // If no frontmatter exists, use the updated content as is
        updated_content.to_string()
    };

    // Update the modification timestamp in the frontmatter if it exists
    let new_content = update_modification_timestamp(&new_content)?;

    // Write the new content to the file
    fs::write(&file_path, new_content).context(format!(
        "Failed to write updated content to file: {}",
        file_path
    ))?;

    Ok(())
}

/// Deletes a markdown note with the given title from the vault directory.
///
/// # Arguments
/// * `title` - The title of the note to delete
/// * `vault_directory` - The base directory of the vault
///
/// # Returns
/// * `Result<()>` - Ok if the note was successfully deleted
///
/// # Errors
/// * Returns an error if the note is not found
/// * Returns an error if there is an issue deleting the file
///
/// # Examples
/// ```
/// use std::path::Path;
/// use notemancy_core::notes::crud::delete_note;
///
/// let vault_dir = Path::new("/path/to/vault");
/// let result = delete_note("My Note", vault_dir);
/// ```
pub fn delete_note(title: &str, vault_directory: &Path) -> Result<()> {
    // Get the file path for the note
    let file_path = crate::notes::utils::get_file_path(title, vault_directory)?;

    // Delete the file
    fs::remove_file(&file_path).context(format!("Failed to delete note file: {}", file_path))?;

    Ok(())
}

/// Helper function to extract frontmatter from content.
///
/// Returns Some(frontmatter) if frontmatter exists, None otherwise.
fn extract_frontmatter(content: &str) -> Result<Option<String>> {
    let trimmed = content.trim_start();

    // Check if content starts with frontmatter delimiter
    if trimmed.starts_with("---") {
        let start_pos = content.find("---").unwrap(); // Safe because we checked it starts with ---
        let from_first_delimiter = &content[start_pos + 3..]; // Skip past first "---"

        if let Some(second_delimiter_pos) = from_first_delimiter.find("---") {
            // Calculate the absolute position of the second delimiter in the original string
            let end_pos = start_pos + 3 + second_delimiter_pos + 3; // +3 to include the second delimiter

            // Extract the frontmatter including both delimiters
            let frontmatter = content[start_pos..end_pos].to_string();
            return Ok(Some(frontmatter));
        }
    }

    // No frontmatter found
    Ok(None)
}

/// Helper function to update the modification timestamp in frontmatter.
fn update_modification_timestamp(content: &str) -> Result<String> {
    let now = chrono::Local::now();
    let timestamp_str = now.format("%Y-%m-%d %H:%M:%S").to_string();

    // Check if content has frontmatter
    if let Some(frontmatter) = extract_frontmatter(content)? {
        // Check if frontmatter contains a modified_at field
        if frontmatter.contains("modified_at:") {
            // Use regex to replace the modified_at field value
            let re = regex::Regex::new(r"modified_at:.*(\r?\n|\r)")
                .context("Failed to create regex pattern")?;
            let updated = re.replace(content, &format!("modified_at: {}\n", timestamp_str));
            return Ok(updated.to_string());
        } else {
            // Add modified_at field at the end of frontmatter
            let before_second_delimiter = content.rfind("---").unwrap();
            let (start, end) = content.split_at(before_second_delimiter);
            return Ok(format!("{}modified_at: {}\n{}", start, timestamp_str, end));
        }
    }

    // No frontmatter or no modified_at field, return original content
    Ok(content.to_string())
}

/// Appends content to an existing markdown note.
///
/// # Arguments
/// * `title` - The title of the note to append to
/// * `vault_directory` - The base directory of the vault
/// * `content` - The content to append to the note
///
/// # Returns
/// * `Result<()>` - Ok if the content was successfully appended
///
/// # Errors
/// * Returns an error if the note is not found
/// * Returns an error if there is an issue reading or writing the file
///
/// # Examples
/// ```
/// use std::path::Path;
/// use notemancy_core::notes::crud::append_to_note;
///
/// let vault_dir = Path::new("/path/to/vault");
/// let content_to_append = "\n\n## New Section\nThis is additional content.";
/// let result = append_to_note("My Note", vault_dir, content_to_append);
/// ```
pub fn append_to_note(title: &str, vault_directory: &Path, content: &str) -> Result<()> {
    // Get the file path for the note
    let file_path = crate::notes::utils::get_file_path(title, vault_directory)?;

    // Read the current content of the file
    let current_content = fs::read_to_string(&file_path)
        .context(format!("Failed to read note file: {}", file_path))?;

    // Create the new content by appending the provided content
    let new_content = format!("{}{}", current_content, content);

    // Update the modification timestamp in the frontmatter
    let new_content = update_modification_timestamp(&new_content)?;

    // Write the updated content to the file
    fs::write(&file_path, new_content).context(format!(
        "Failed to write updated content to file: {}",
        file_path
    ))?;

    Ok(())
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

    #[test]
    fn test_extract_frontmatter() -> Result<()> {
        // Test with valid frontmatter
        let content =
            "---\ntitle: Test Note\ncreated_on: 2023-01-01\n---\n\nThis is the note content.";
        let frontmatter = extract_frontmatter(content)?;
        assert!(frontmatter.is_some());
        assert_eq!(
            frontmatter.unwrap(),
            "---\ntitle: Test Note\ncreated_on: 2023-01-01\n---"
        );

        // Test with no frontmatter
        let content_no_frontmatter = "This is a note without frontmatter.";
        let frontmatter = extract_frontmatter(content_no_frontmatter)?;
        assert!(frontmatter.is_none());

        // Test with only frontmatter start delimiter
        let content_incomplete = "---\ntitle: Incomplete\n\nContent after incomplete frontmatter.";
        let frontmatter = extract_frontmatter(content_incomplete)?;
        assert!(frontmatter.is_none());

        Ok(())
    }

    #[test]
    fn test_update_modification_timestamp() -> Result<()> {
        // Test updating existing timestamp
        let content = "---\ntitle: Test Note\ncreated_on: 2023-01-01\nmodified_at: 2023-01-01 12:00:00\n---\n\nContent.";
        let updated = update_modification_timestamp(content)?;
        assert!(updated.contains("modified_at: "));
        assert!(!updated.contains("modified_at: 2023-01-01 12:00:00"));

        // Test adding timestamp when it doesn't exist
        let content_no_timestamp = "---\ntitle: Test Note\ncreated_on: 2023-01-01\n---\n\nContent.";
        let updated = update_modification_timestamp(content_no_timestamp)?;
        assert!(updated.contains("modified_at: "));

        // Test with no frontmatter
        let content_no_frontmatter = "This is a note without frontmatter.";
        let updated = update_modification_timestamp(content_no_frontmatter)?;
        assert_eq!(updated, content_no_frontmatter);

        Ok(())
    }

    #[test]
    fn test_update_note() -> Result<()> {
        // Create a temporary directory for the test vault
        let temp_dir = tempdir()?;
        let vault_dir = temp_dir.path();

        // Create a test note
        let title = "Update Test Note";
        let note_path = create_note(title, vault_dir, "test")?;

        // Initial content
        let initial_content = fs::read_to_string(&note_path)?;
        assert!(initial_content.contains("---\ntitle: Update Test Note"));

        // Update the note with new content
        let new_content = "# Updated Content\n\nThis is the updated content.";
        update_note(title, vault_dir, new_content)?;

        // Verify the note was updated correctly
        let updated_content = fs::read_to_string(&note_path)?;
        assert!(updated_content.contains("---\ntitle: Update Test Note"));
        assert!(updated_content.contains("# Updated Content"));
        assert!(updated_content.contains("This is the updated content."));

        // Verify the timestamp was updated
        assert!(updated_content.contains("modified_at: "));

        Ok(())
    }

    #[test]
    fn test_update_note_preserves_frontmatter() -> Result<()> {
        // Create a temporary directory for the test vault
        let temp_dir = tempdir()?;
        let vault_dir = temp_dir.path();

        // Create a test note with custom frontmatter
        let title = "Frontmatter Preservation Test";
        let note_path = create_note(title, vault_dir, "test")?;

        // Add custom fields to frontmatter
        let custom_frontmatter = format!(
            "---\ntitle: {}\ncreated_on: 2023-01-01\ntags:\n  - test\n  - example\npriority: high\n---\n\nInitial content.",
            title
        );
        fs::write(&note_path, custom_frontmatter)?;

        // Update the note
        let new_content = "This content should replace only what comes after frontmatter.";
        update_note(title, vault_dir, new_content)?;

        // Verify frontmatter is preserved
        let updated_content = fs::read_to_string(&note_path)?;
        assert!(updated_content.contains("tags:"));
        assert!(updated_content.contains("- test"));
        assert!(updated_content.contains("- example"));
        assert!(updated_content.contains("priority: high"));

        // Verify content is updated
        assert!(updated_content.contains(new_content));
        assert!(!updated_content.contains("Initial content."));

        Ok(())
    }

    #[test]
    fn test_delete_note() -> Result<()> {
        // Create a temporary directory for the test vault
        let temp_dir = tempdir()?;
        let vault_dir = temp_dir.path();

        // Create a test note
        let title = "Delete Test Note";
        let note_path = create_note(title, vault_dir, "test")?;

        // Verify the note exists
        assert!(note_path.exists(), "Note should exist before deletion");

        // Delete the note
        delete_note(title, vault_dir)?;

        // Verify the note no longer exists
        assert!(!note_path.exists(), "Note should not exist after deletion");

        Ok(())
    }

    #[test]
    fn test_delete_note_not_found() -> Result<()> {
        // Create a temporary directory for the test vault
        let temp_dir = tempdir()?;
        let vault_dir = temp_dir.path();

        // Try to delete a non-existent note
        let result = delete_note("Non Existent Note", vault_dir);

        // Verify that an error was returned
        assert!(result.is_err(), "Deleting a non-existent note should fail");

        Ok(())
    }

    #[test]
    fn test_update_note_not_found() -> Result<()> {
        // Create a temporary directory for the test vault
        let temp_dir = tempdir()?;
        let vault_dir = temp_dir.path();

        // Try to update a non-existent note
        let result = update_note("Non Existent Note", vault_dir, "Updated content");

        // Verify that an error was returned
        assert!(result.is_err(), "Updating a non-existent note should fail");

        Ok(())
    }

    #[test]
    fn test_append_to_note() -> Result<()> {
        // Create a temporary directory for the test vault
        let temp_dir = tempdir()?;
        let vault_dir = temp_dir.path();

        // Create a test note
        let title = "Append Test Note";
        let note_path = create_note(title, vault_dir, "test")?;

        // Initial content should only have frontmatter
        let initial_content = fs::read_to_string(&note_path)?;
        assert!(initial_content.contains("---\ntitle: Append Test Note"));
        assert!(!initial_content.contains("Initial content"));

        // Append content to the note
        let content_to_append = "# Initial content\nThis is the first append.";
        append_to_note(title, vault_dir, content_to_append)?;

        // Verify content was appended
        let updated_content = fs::read_to_string(&note_path)?;
        assert!(updated_content.contains("---\ntitle: Append Test Note"));
        assert!(updated_content.contains("# Initial content"));
        assert!(updated_content.contains("This is the first append."));

        // Append more content
        let more_content = "\n\n## Second section\nThis is the second append.";
        append_to_note(title, vault_dir, more_content)?;

        // Verify all content is present
        let final_content = fs::read_to_string(&note_path)?;
        assert!(final_content.contains("---\ntitle: Append Test Note"));
        assert!(final_content.contains("# Initial content"));
        assert!(final_content.contains("This is the first append."));
        assert!(final_content.contains("## Second section"));
        assert!(final_content.contains("This is the second append."));

        // Verify modification timestamp was updated
        assert!(final_content.contains("modified_at:"));

        Ok(())
    }

    #[test]
    fn test_append_to_nonexistent_note() -> Result<()> {
        // Create a temporary directory for the test vault
        let temp_dir = tempdir()?;
        let vault_dir = temp_dir.path();

        // Try to append to a non-existent note
        let result = append_to_note("Non Existent Note", vault_dir, "Some content");

        // Verify that an error was returned
        assert!(
            result.is_err(),
            "Appending to a non-existent note should fail"
        );

        Ok(())
    }
}
