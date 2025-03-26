// src/notes/utils.rs

use anyhow::{Context, Result, anyhow};
use regex::Regex;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Sanitizes a title string to ensure it's a valid filename for Windows, Linux, and macOS.
///
/// # Arguments
/// * `title` - The title string to sanitize
///
/// # Returns
/// * `String` - The sanitized title that can be used as a valid filename
///
/// # Examples
/// ```
/// use notemancy_core::notes::utils::sanitize_title;
///
/// let sanitized = sanitize_title("My Note: With <invalid> chars?");
/// assert_eq!(sanitized, "My-Note-With-invalid-chars");
/// ```
pub fn sanitize_title(title: &str) -> String {
    // Replace spaces with hyphens
    let with_hyphens = title.trim().replace(' ', "-");

    // Replace invalid characters with hyphens
    // This covers invalid chars for Windows, Linux, and macOS
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

/// Escapes special characters in a string for use in a regex pattern.
///
/// # Arguments
/// * `s` - The string to escape
///
/// # Returns
/// * `String` - The escaped string
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

/// Finds the file path for a note with the given title in the vault directory.
///
/// This function uses the `fd` command to find the file path. The `fd` command
/// must be installed on the system.
///
/// # Arguments
/// * `title` - The title of the note to find
/// * `vault_directory` - The absolute path to the vault directory
///
/// # Returns
/// * `Result<PathBuf>` - The absolute path to the note file on success
///
/// # Errors
/// * Returns an error if the `fd` command fails or if the note is not found
///
/// # Examples
/// ```
/// use std::path::Path;
/// use notemancy_core::notes::utils::get_file_path;
///
/// let vault_dir = Path::new("/path/to/vault");
/// let file_path = get_file_path("My Note", vault_dir);
/// // If successful, file_path will be the absolute path to the note file
/// ```
pub fn get_file_path(title: &str, vault_directory: &Path) -> Result<PathBuf> {
    // Sanitize the title to create a valid filename
    let sanitized_title = sanitize_title(title);

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

    // Convert the file path to a PathBuf
    let path = PathBuf::from(file_paths[0]);

    Ok(path)
}

/// Finds the file path for a note without relying on the external fd command.
///
/// This is an alternative implementation that uses the walkdir crate to find the file.
///
/// # Arguments
/// * `title` - The title of the note to find
/// * `vault_directory` - The absolute path to the vault directory
///
/// # Returns
/// * `Result<PathBuf>` - The absolute path to the note file on success
///
/// # Errors
/// * Returns an error if the note is not found
pub fn get_file_path_alt(title: &str, vault_directory: &Path) -> Result<PathBuf> {
    // Sanitize the title to create a valid filename
    let sanitized_title = sanitize_title(title);

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
                return Ok(entry.path().to_path_buf());
            }
        }
    }

    Err(anyhow!("Note with title '{}' not found", title))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_sanitize_title() {
        assert_eq!(sanitize_title("Test Title"), "Test-Title");
        assert_eq!(
            sanitize_title("Test/Title:With?Invalid*Chars"),
            "Test-Title-With-Invalid-Chars"
        );
        assert_eq!(
            sanitize_title("   Leading and trailing spaces   "),
            "Leading-and-trailing-spaces"
        );
        assert_eq!(
            sanitize_title("---Title--with---many-hyphens----"),
            "Title-with-many-hyphens"
        );
        assert_eq!(sanitize_title(""), "Untitled");
        assert_eq!(sanitize_title("//////"), "Untitled");
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
    fn test_get_file_path_alt() -> Result<()> {
        // Create a temporary directory for testing
        let temp_dir = tempdir()?;
        let vault_directory = temp_dir.path();

        // Create test directories
        let project_dir = vault_directory.join("project");
        fs::create_dir_all(&project_dir)?;

        // Create a test file
        let test_filename = "Test-Note.md";
        let test_filepath = project_dir.join(test_filename);
        fs::write(&test_filepath, "test content")?;

        // Test finding the file
        let found_path = get_file_path_alt("Test Note", vault_directory)?;

        // Verify the paths match
        assert_eq!(found_path, test_filepath);

        Ok(())
    }

    #[test]
    fn test_get_file_path_alt_not_found() {
        // Create a temporary directory for testing
        let temp_dir = tempdir().unwrap();
        let vault_directory = temp_dir.path();

        // Try to find a non-existent file
        let result = get_file_path_alt("Non Existent Note", vault_directory);

        // Verify the operation failed with the expected error
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_get_file_path_alt_with_special_chars() -> Result<()> {
        // Create a temporary directory for testing
        let temp_dir = tempdir()?;
        let vault_directory = temp_dir.path();

        // Create test directories
        let project_dir = vault_directory.join("project");
        fs::create_dir_all(&project_dir)?;

        // Create a test file with a name containing special characters
        let title = "Test.With.Special+Chars*And[Brackets]";
        let sanitized = sanitize_title(title);
        let test_filename = format!("{}.md", sanitized);
        let test_filepath = project_dir.join(test_filename);
        fs::write(&test_filepath, "test content")?;

        // Test finding the file
        let found_path = get_file_path_alt(title, vault_directory)?;

        // Verify the paths match
        assert_eq!(found_path, test_filepath);

        Ok(())
    }

    #[test]
    // #[ignore] // Add this attribute to skip the test if fd is not available
    fn test_get_file_path() -> Result<()> {
        // Check if fd is available
        match Command::new("fd").arg("--version").output() {
            Ok(_) => {} // fd is available
            Err(_) => {
                eprintln!("Skipping test_get_file_path because fd command is not available");
                return Ok(());
            }
        }

        // Create a temporary directory for testing
        let temp_dir = tempdir()?;
        let vault_directory = temp_dir.path();

        // Create test directories
        let project_dir = vault_directory.join("project");
        fs::create_dir_all(&project_dir)?;

        // Create a test file
        let test_filename = "Test-Note.md";
        let test_filepath = project_dir.join(test_filename);
        fs::write(&test_filepath, "test content")?;

        // Test finding the file
        let found_path = get_file_path("Test Note", vault_directory)?;

        // Verify the paths match
        assert_eq!(found_path, test_filepath);

        Ok(())
    }

    #[test]
    // #[ignore] // Add this attribute to skip the test if fd is not available
    fn test_get_file_path_not_found() -> Result<()> {
        // Check if fd is available
        match Command::new("fd").arg("--version").output() {
            Ok(_) => {} // fd is available
            Err(_) => {
                eprintln!(
                    "Skipping test_get_file_path_not_found because fd command is not available"
                );
                return Ok(());
            }
        }

        // Create a temporary directory for testing
        let temp_dir = tempdir()?;
        let vault_directory = temp_dir.path();

        // Try to find a non-existent file
        let result = get_file_path("Non Existent Note", vault_directory);

        // Verify the operation failed with the expected error
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));

        Ok(())
    }

    #[test]
    // #[ignore] // Add this attribute to skip the test if fd is not available
    fn test_get_file_path_with_special_chars() -> Result<()> {
        // Check if fd is available
        match Command::new("fd").arg("--version").output() {
            Ok(_) => {} // fd is available
            Err(_) => {
                eprintln!(
                    "Skipping test_get_file_path_with_special_chars because fd command is not available"
                );
                return Ok(());
            }
        }

        // Create a temporary directory for testing
        let temp_dir = tempdir()?;
        let vault_directory = temp_dir.path();

        // Create test directories
        let project_dir = vault_directory.join("project");
        fs::create_dir_all(&project_dir)?;

        // Create a test file with a name containing special characters
        let title = "Test.With.Special+Chars*And[Brackets]";
        let sanitized = sanitize_title(title);
        let test_filename = format!("{}.md", sanitized);
        let test_filepath = project_dir.join(test_filename);
        fs::write(&test_filepath, "test content")?;

        // Test finding the file
        let found_path = get_file_path(title, vault_directory)?;

        // Verify the paths match
        assert_eq!(found_path, test_filepath);

        Ok(())
    }

    #[test]
    // #[ignore] // Add this attribute to skip the test if fd is not available
    fn test_get_file_path_multiple_matches() -> Result<()> {
        // Check if fd is available
        match Command::new("fd").arg("--version").output() {
            Ok(_) => {} // fd is available
            Err(_) => {
                eprintln!(
                    "Skipping test_get_file_path_multiple_matches because fd command is not available"
                );
                return Ok(());
            }
        }

        // Create a temporary directory for testing
        let temp_dir = tempdir()?;
        let vault_directory = temp_dir.path();

        // Create test directories
        let project1_dir = vault_directory.join("project1");
        let project2_dir = vault_directory.join("project2");
        fs::create_dir_all(&project1_dir)?;
        fs::create_dir_all(&project2_dir)?;

        // Create multiple files with the same name
        let test_filename = "Duplicate-Note.md";
        let test_filepath1 = project1_dir.join(test_filename);
        let test_filepath2 = project2_dir.join(test_filename);
        fs::write(&test_filepath1, "test content 1")?;
        fs::write(&test_filepath2, "test content 2")?;

        // Test finding the file (should fail with multiple matches)
        let result = get_file_path("Duplicate Note", vault_directory);

        // Verify the operation failed with the expected error
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Multiple files found")
        );

        Ok(())
    }
}
