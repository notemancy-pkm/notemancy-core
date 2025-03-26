// src/notes/crud.rs

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Local};
use regex::Regex;
use std::fs::{File, create_dir_all};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Creates a new markdown note in the specified project directory within the vault.
///
/// # Arguments
/// * `title` - The title of the note
/// * `project` - The project path relative to the vault directory
/// * `vault_directory` - The absolute path to the vault directory
///
/// # Returns
/// * `Result<PathBuf>` - The path to the created note file on success
pub fn create_note(title: &str, project: &str, vault_directory: &Path) -> Result<PathBuf> {
    // Join vault directory with project path
    let project_dir = vault_directory.join(project);

    // Ensure project directory exists
    create_dir_all(&project_dir).context("Failed to create project directory")?;

    // Sanitize the title to create a valid filename
    let sanitized_title = sanitize_filename(title);

    // Make the filename unique
    let unique_filename = ensure_unique_filename(&sanitized_title, vault_directory)?;

    // Create the full path for the new note
    let note_path = project_dir.join(format!("{}.md", unique_filename));

    // Get current timestamp and formatted date
    let now = SystemTime::now();
    let timestamp = now
        .duration_since(UNIX_EPOCH)
        .context("Failed to get current timestamp")?
        .as_secs();

    let local_datetime: DateTime<Local> = now.into();
    let formatted_date = local_datetime.format("%Y-%m-%d").to_string();

    // Create YAML frontmatter
    let frontmatter = format!(
        "---\ntitle: {}\ndate: {}\ncreated_at: {}\nmodified_at: {}\n---\n\n",
        title, formatted_date, timestamp, timestamp
    );

    // Create the file and write the frontmatter
    let mut file = File::create(&note_path).context("Failed to create note file")?;

    file.write_all(frontmatter.as_bytes())
        .context("Failed to write frontmatter to note file")?;

    Ok(note_path)
}

/// Sanitizes a string to create a valid filename.
fn sanitize_filename(input: &str) -> String {
    // Replace spaces with hyphens
    let with_hyphens = input.trim().replace(' ', "-");

    // Replace invalid characters with hyphens
    let re = Regex::new(r"[<>:/\\|?*\n\r\t]").unwrap();
    let sanitized = re.replace_all(&with_hyphens, "-").to_string();

    // Replace multiple consecutive hyphens with a single one
    let re_multiple_hyphens = Regex::new(r"-+").unwrap();
    let sanitized = re_multiple_hyphens.replace_all(&sanitized, "-").to_string();

    // Remove leading and trailing hyphens
    let sanitized = sanitized.trim_matches('-').to_string();

    // Ensure filename is not empty after sanitization
    if sanitized.is_empty() {
        return "Untitled".to_string();
    }

    sanitized
}

/// Ensures the filename is unique within the vault directory.
/// If the filename already exists somewhere in the vault, appends a number to make it unique.
fn ensure_unique_filename(base_filename: &str, vault_directory: &Path) -> Result<String> {
    let mut unique_filename = base_filename.to_string();

    // First check if the base filename is already unique
    if !file_exists_in_vault(&unique_filename, vault_directory)? {
        return Ok(unique_filename);
    }

    // If not unique, try adding a number (001, 002, etc.)
    for i in 1..1000 {
        let numbered_filename = format!("{}-{:03}", base_filename, i);

        if !file_exists_in_vault(&numbered_filename, vault_directory)? {
            unique_filename = numbered_filename;
            break;
        }

        // If we've gone through 999 and still can't find a unique name, return an error
        if i == 999 {
            return Err(anyhow!(
                "Cannot create a unique filename after 999 attempts"
            ));
        }
    }

    Ok(unique_filename)
}

/// Checks if a file with the given name exists anywhere in the vault directory.
fn file_exists_in_vault(filename: &str, vault_directory: &Path) -> Result<bool> {
    let md_filename = format!("{}.md", filename);

    // Walk through all files in the vault directory and check if any match the filename
    let walker = walkdir::WalkDir::new(vault_directory)
        .follow_links(true)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file());

    for entry in walker {
        if let Some(entry_filename) = entry.file_name().to_str() {
            if entry_filename == md_filename {
                return Ok(true);
            }
        }
    }

    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("Test Title"), "Test-Title");
        assert_eq!(
            sanitize_filename("Test/Title:With?Invalid*Chars"),
            "Test-Title-With-Invalid-Chars"
        );
        assert_eq!(
            sanitize_filename("   Leading and trailing spaces   "),
            "Leading-and-trailing-spaces"
        );
        assert_eq!(
            sanitize_filename("---Title--with---many-hyphens----"),
            "Title-with-many-hyphens"
        );
        assert_eq!(sanitize_filename(""), "Untitled");
        assert_eq!(sanitize_filename("//////"), "Untitled");
    }

    #[test]
    fn test_create_note() -> Result<()> {
        // Create a temporary directory for testing
        let temp_dir = tempdir()?;
        let vault_directory = temp_dir.path();

        // Test creating a note
        let note_path = create_note("Test Note", "project/subproject", vault_directory)?;

        // Verify the file was created
        assert!(note_path.exists());

        // Verify the path is correct
        let expected_path = vault_directory.join("project/subproject/Test-Note.md");
        assert_eq!(note_path, expected_path);

        // Verify file contents
        let contents = fs::read_to_string(&note_path)?;
        assert!(contents.contains("title: Test Note"));
        assert!(contents.contains("date: "));
        assert!(contents.contains("created_at: "));
        assert!(contents.contains("modified_at: "));

        Ok(())
    }

    #[test]
    fn test_unique_filename() -> Result<()> {
        // Create a temporary directory for testing
        let temp_dir = tempdir()?;
        let vault_directory = temp_dir.path();

        // Create a first note
        let first_note_path = create_note("Duplicate", "project", vault_directory)?;
        assert!(first_note_path.exists());

        // Create a second note with the same title
        let second_note_path = create_note("Duplicate", "project", vault_directory)?;
        assert!(second_note_path.exists());

        // Verify the second note has a different path (should have -001 appended)
        assert_ne!(first_note_path, second_note_path);
        assert!(second_note_path.to_str().unwrap().contains("Duplicate-001"));

        // Create a third note with the same title
        let third_note_path = create_note("Duplicate", "project", vault_directory)?;
        assert!(third_note_path.exists());

        // Verify the third note has -002 appended
        assert_ne!(third_note_path, second_note_path);
        assert!(third_note_path.to_str().unwrap().contains("Duplicate-002"));

        Ok(())
    }

    #[test]
    fn test_recursive_uniqueness_check() -> Result<()> {
        // Create a temporary directory for testing
        let temp_dir = tempdir()?;
        let vault_directory = temp_dir.path();

        // Create a note in one project directory
        let first_note_path = create_note("Recursive", "project1", vault_directory)?;
        assert!(first_note_path.exists());

        // Create a note with the same title in another project directory
        let second_note_path = create_note("Recursive", "project2", vault_directory)?;
        assert!(second_note_path.exists());

        // Verify the second note has a different filename (should have -001 appended)
        assert_ne!(first_note_path.file_name(), second_note_path.file_name());
        assert!(second_note_path.to_str().unwrap().contains("Recursive-001"));

        Ok(())
    }
}
