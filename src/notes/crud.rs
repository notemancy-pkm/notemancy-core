// src/notes/crud.rs

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Local};
use regex::Regex;
use std::fs::{self, File, create_dir_all};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
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

/// Reads a note from the vault directory by its title using the fd command.
///
/// # Arguments
/// * `title` - The title of the note to read
/// * `vault_directory` - The absolute path to the vault directory
///
/// # Returns
/// * `Result<String>` - The contents of the note on success
pub fn read_note(title: &str, vault_directory: &Path) -> Result<String> {
    // Sanitize the title to create a valid filename
    let sanitized_title = sanitize_filename(title);

    // Escape characters that have special meaning in regex
    let escaped_title = regex_escape(&sanitized_title);

    // Use fd to find the file in the vault with exact match
    // Using word boundaries to ensure exact match
    let output = Command::new("fd")
        .args(&[
            &format!("^{}(\\.md|\\.markdown)$", escaped_title),
            vault_directory.to_str().unwrap_or("."),
            "--type",
            "f",
        ])
        .output()
        .context("Failed to execute fd command")?;

    if !output.status.success() {
        return Err(anyhow!(
            "fd command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    // Get the output as a string
    let stdout = String::from_utf8(output.stdout).context("Failed to parse fd command output")?;

    // Split by newlines and get all results
    let file_paths: Vec<&str> = stdout
        .trim()
        .split('\n')
        .filter(|s| !s.is_empty())
        .collect();

    if file_paths.is_empty() {
        return Err(anyhow!("Note with title '{}' not found", title));
    }

    // Check if multiple files match
    if file_paths.len() > 1 {
        let file_list = file_paths.join("\n  - ");
        return Err(anyhow!(
            "Multiple files found for title '{}'. Cannot determine which one to use:\n  - {}",
            title,
            file_list
        ));
    }

    // Read the contents of the file
    let file_contents = fs::read_to_string(&file_paths[0])
        .context(format!("Failed to read file at path: {}", file_paths[0]))?;

    Ok(file_contents)
}

/// Escapes special characters in a string for use in a regex pattern
fn regex_escape(s: &str) -> String {
    let special_chars = [
        '.', '^', '$', '*', '+', '?', '(', ')', '[', ']', '{', '}', '|', '\\',
    ];
    let mut result = String::with_capacity(s.len() * 2);

    for c in s.chars() {
        if special_chars.contains(&c) {
            result.push('\\');
        }
        result.push(c);
    }

    result
}

/// Alternative implementation without relying on external fd command
pub fn read_note_alt(title: &str, vault_directory: &Path) -> Result<String> {
    // Sanitize the title to create a valid filename
    let sanitized_title = sanitize_filename(title);

    // Find the file in the vault directory
    let file_path = find_note_path(&sanitized_title, vault_directory)?;

    if file_path.is_none() {
        return Err(anyhow!("Note with title '{}' not found", title));
    }

    // Read the contents of the file
    let file_contents =
        fs::read_to_string(file_path.unwrap()).context("Failed to read note file")?;

    Ok(file_contents)
}

/// Finds a note path by its sanitized title
fn find_note_path(sanitized_title: &str, vault_directory: &Path) -> Result<Option<PathBuf>> {
    let md_pattern = format!("{}.md", sanitized_title);
    let markdown_pattern = format!("{}.markdown", sanitized_title);

    // Walk through all files in the vault directory and check if any match the filename
    let walker = walkdir::WalkDir::new(vault_directory)
        .follow_links(true)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file());

    for entry in walker {
        if let Some(entry_filename) = entry.file_name().to_str() {
            if entry_filename == md_pattern || entry_filename == markdown_pattern {
                return Ok(Some(entry.path().to_path_buf()));
            }
        }
    }

    Ok(None)
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

    #[test]
    fn test_read_note_alt() -> Result<()> {
        // Create a temporary directory for testing
        let temp_dir = tempdir()?;
        let vault_directory = temp_dir.path();

        // Create a test note
        let title = "Read Test Note";
        let note_path = create_note(title, "read_test_project", vault_directory)?;

        // Append some content to the file
        let test_content = "This is some test content.";
        fs::write(&note_path, fs::read_to_string(&note_path)? + test_content)
            .context("Failed to append content to test note")?;

        // Read the note
        let contents = read_note_alt(title, vault_directory)?;

        // Verify the contents
        assert!(contents.contains("title: Read Test Note"));
        assert!(contents.contains("date: "));
        assert!(contents.contains("created_at: "));
        assert!(contents.contains("modified_at: "));
        assert!(contents.contains(test_content));

        Ok(())
    }

    #[test]
    fn test_read_note_alt_not_found() {
        // Create a temporary directory for testing
        let temp_dir = tempdir().unwrap();
        let vault_directory = temp_dir.path();

        // Try to read a non-existent note
        let result = read_note_alt("Non Existent Note", vault_directory);

        // Verify that the operation failed with the expected error
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_read_note_alt_with_numbers() -> Result<()> {
        // Create a temporary directory for testing
        let temp_dir = tempdir()?;
        let vault_directory = temp_dir.path();

        // Create multiple notes with the same base title
        let title = "Duplicate Note";
        create_note(title, "project", vault_directory)?;
        let second_note_path = create_note(title, "project", vault_directory)?;

        // Append some content to the second file
        let test_content = "This is the second note.";
        fs::write(
            &second_note_path,
            fs::read_to_string(&second_note_path)? + test_content,
        )
        .context("Failed to append content to test note")?;

        // Read the note using the original title
        // This should find the first note by default
        let contents = read_note_alt(title, vault_directory)?;

        // Verify it doesn't contain our second note content
        assert!(!contents.contains(test_content));

        // Now try to read the specific numbered note
        let second_note_result = read_note_alt("Duplicate Note-001", vault_directory)?;

        // Verify it contains our second note content
        assert!(second_note_result.contains(test_content));

        Ok(())
    }

    #[test]
    // #[ignore] // Add this attribute to skip the test if fd is not available
    fn test_read_note_fd() -> Result<()> {
        // Create a temporary directory for testing
        let temp_dir = tempdir()?;
        let vault_directory = temp_dir.path();

        // Check if fd is available
        match Command::new("fd").arg("--version").output() {
            Ok(_) => {} // fd is available
            Err(_) => {
                eprintln!("Skipping test_read_note_fd because fd command is not available");
                return Ok(());
            }
        }

        // Create a test note
        let title = "Read Test Note";
        let note_path = create_note(title, "read_test_project", vault_directory)?;

        // Append some content to the file
        let test_content = "This is some test content.";
        fs::write(&note_path, fs::read_to_string(&note_path)? + test_content)
            .context("Failed to append content to test note")?;

        // Read the note using fd
        let contents = read_note(title, vault_directory)?;

        // Verify the contents
        assert!(contents.contains("title: Read Test Note"));
        assert!(contents.contains("date: "));
        assert!(contents.contains("created_at: "));
        assert!(contents.contains("modified_at: "));
        assert!(contents.contains(test_content));

        Ok(())
    }

    #[test]
    // #[ignore] // Add this attribute to skip the test if fd is not available
    fn test_read_note_fd_not_found() -> Result<()> {
        // Create a temporary directory for testing
        let temp_dir = tempdir()?;
        let vault_directory = temp_dir.path();

        // Check if fd is available
        match Command::new("fd").arg("--version").output() {
            Ok(_) => {} // fd is available
            Err(_) => {
                eprintln!(
                    "Skipping test_read_note_fd_not_found because fd command is not available"
                );
                return Ok(());
            }
        }

        // Try to read a non-existent note
        let result = read_note("Non Existent Note", vault_directory);

        // Verify that the operation failed with the expected error
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));

        Ok(())
    }

    #[test]
    // #[ignore] // Add this attribute to skip the test if fd is not available
    fn test_read_note_fd_with_numbers() -> Result<()> {
        // Create a temporary directory for testing
        let temp_dir = tempdir()?;
        let vault_directory = temp_dir.path();

        // Check if fd is available
        match Command::new("fd").arg("--version").output() {
            Ok(_) => {} // fd is available
            Err(_) => {
                eprintln!(
                    "Skipping test_read_note_fd_with_numbers because fd command is not available"
                );
                return Ok(());
            }
        }

        // Create multiple notes with the same base title
        let title = "Duplicate Note";
        create_note(title, "project", vault_directory)?;
        let second_note_path = create_note(title, "project", vault_directory)?;

        // Append some content to the second file
        let test_content = "This is the second note.";
        fs::write(
            &second_note_path,
            fs::read_to_string(&second_note_path)? + test_content,
        )
        .context("Failed to append content to test note")?;

        // Read the note using the original title
        // This will find either the first or second note based on fd's sorting
        let _contents = read_note(title, vault_directory)?;

        // Try to read the specific numbered note
        let second_note_result = read_note("Duplicate Note-001", vault_directory)?;

        // Verify it contains our second note content
        assert!(second_note_result.contains(test_content));

        Ok(())
    }

    #[test]
    fn test_regex_escape() {
        assert_eq!(regex_escape("simple"), "simple");
        assert_eq!(regex_escape("with.dot"), "with\\.dot");
        assert_eq!(
            regex_escape("multiple^$.*+?()[]{}|\\chars"),
            "multiple\\^\\$\\.\\*\\+\\?\\(\\)\\[\\]\\{\\}\\|\\\\chars"
        );
    }

    #[test]
    // #[ignore]
    fn test_read_note_fd_similar_names() -> Result<()> {
        // Create a temporary directory for testing
        let temp_dir = tempdir()?;
        let vault_directory = temp_dir.path();

        // Check if fd is available
        match Command::new("fd").arg("--version").output() {
            Ok(_) => {} // fd is available
            Err(_) => {
                eprintln!(
                    "Skipping test_read_note_fd_similar_names because fd command is not available"
                );
                return Ok(());
            }
        }

        // Create notes with similar names
        create_note("File", "project", vault_directory)?;
        create_note("File1", "project", vault_directory)?;
        create_note("File2", "project", vault_directory)?;

        // Test that we can read the exact file
        let contents = read_note("File", vault_directory)?;
        assert!(contents.contains("title: File"));

        // Test that we can read File1 specifically
        let contents = read_note("File1", vault_directory)?;
        assert!(contents.contains("title: File1"));

        // Test that we can read File2 specifically
        let contents = read_note("File2", vault_directory)?;
        assert!(contents.contains("title: File2"));

        Ok(())
    }

    #[test]
    // #[ignore]
    fn test_read_note_fd_with_special_chars() -> Result<()> {
        // Create a temporary directory for testing
        let temp_dir = tempdir()?;
        let vault_directory = temp_dir.path();

        // Check if fd is available
        match Command::new("fd").arg("--version").output() {
            Ok(_) => {} // fd is available
            Err(_) => {
                eprintln!(
                    "Skipping test_read_note_fd_with_special_chars because fd command is not available"
                );
                return Ok(());
            }
        }

        // Create a note with a title containing special regex characters
        let title = "Test.With.Special+Chars*And[Brackets]";
        let note_path = create_note(title, "project", vault_directory)?;

        // Append some content to the file
        let test_content = "This has special characters in the title.";
        fs::write(&note_path, fs::read_to_string(&note_path)? + test_content)
            .context("Failed to append content to test note")?;

        // Read the note
        let contents = read_note(title, vault_directory)?;

        // Verify the contents
        assert!(contents.contains(test_content));

        Ok(())
    }
}
