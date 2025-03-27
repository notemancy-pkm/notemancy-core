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
}
