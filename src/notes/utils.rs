// src/notes/utils.rs

use anyhow::{Context, Result, anyhow};
use regex::Regex;
use serde_json::{Map as JsonMap, Value as JsonValue};
use serde_yaml;
use std::fs;
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

/// Parses and returns the YAML frontmatter from a markdown file as a JSON object.
///
/// This function searches for the file recursively in the vault directory, reads its
/// contents, and parses any YAML frontmatter present at the beginning of the file.
///
/// # Arguments
/// * `filename` - The name of the markdown file (with or without extension)
/// * `vault_directory` - The absolute path to the vault directory
///
/// # Returns
/// * `Result<JsonValue>` - The frontmatter as a JSON object on success
///
/// # Errors
/// * Returns an error if the file is not found or if frontmatter parsing fails
///
/// # Examples
/// ```
/// use std::path::Path;
/// use notemancy_core::notes::utils::get_frontmatter;
///
/// let vault_dir = Path::new("/path/to/vault");
/// let frontmatter = get_frontmatter("My Note", vault_dir);
/// // If successful, frontmatter will be a JSON object containing the parsed YAML frontmatter
/// ```
pub fn get_frontmatter(filename: &str, vault_directory: &Path) -> Result<JsonValue> {
    // Find the file
    let file_path = match get_file_path_alt(filename, vault_directory) {
        Ok(path) => path,
        Err(_) => {
            // Try with .md extension if not found
            let filename_with_ext =
                if !filename.ends_with(".md") && !filename.ends_with(".markdown") {
                    format!("{}.md", filename)
                } else {
                    filename.to_string()
                };

            let sanitized = sanitize_title(&filename_with_ext);
            get_file_path_alt(&sanitized, vault_directory)?
        }
    };

    // Read the file contents
    let content = fs::read_to_string(&file_path)
        .context(format!("Failed to read file at path: {:?}", file_path))?;

    // Check if the file has frontmatter (content between --- and ---)
    if !content.starts_with("---") {
        // Return empty object if no frontmatter
        return Ok(JsonValue::Object(JsonMap::new()));
    }

    // Find the end of frontmatter (the second ---)
    let end_index = match content[3..].find("---") {
        Some(idx) => idx + 3, // Add 3 to account for the first "---"
        None => return Ok(JsonValue::Object(JsonMap::new())), // No closing "---"
    };

    // Extract the frontmatter content
    let frontmatter_content = &content[3..end_index].trim();

    // Parse the YAML to JSON
    let yaml_value: serde_yaml::Value =
        serde_yaml::from_str(frontmatter_content).context("Failed to parse YAML frontmatter")?;

    // Convert YAML value to JSON value
    let json_value = yaml_to_json(yaml_value);

    Ok(json_value)
}

/// Converts serde_yaml::Value to serde_json::Value.
fn yaml_to_json(yaml: serde_yaml::Value) -> JsonValue {
    match yaml {
        serde_yaml::Value::Null => JsonValue::Null,
        serde_yaml::Value::Bool(b) => JsonValue::Bool(b),
        serde_yaml::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                JsonValue::Number(serde_json::Number::from(i))
            } else if let Some(f) = n.as_f64() {
                // Use from_f64 and handle non-finite values
                JsonValue::Number(
                    serde_json::Number::from_f64(f).unwrap_or(serde_json::Number::from(0)),
                )
            } else {
                JsonValue::Null // Fallback, should not happen
            }
        }
        serde_yaml::Value::String(s) => JsonValue::String(s),
        serde_yaml::Value::Sequence(seq) => {
            JsonValue::Array(seq.into_iter().map(yaml_to_json).collect())
        }
        serde_yaml::Value::Mapping(map) => {
            let mut json_map = JsonMap::new();
            for (k, v) in map {
                if let serde_yaml::Value::String(key) = k {
                    json_map.insert(key, yaml_to_json(v));
                } else {
                    // Convert non-string keys to strings (JSON requires string keys)
                    let key = match k {
                        serde_yaml::Value::Null => "null".to_string(),
                        serde_yaml::Value::Bool(b) => b.to_string(),
                        serde_yaml::Value::Number(n) => n.to_string(),
                        serde_yaml::Value::String(s) => s,
                        serde_yaml::Value::Sequence(_) | serde_yaml::Value::Mapping(_) => {
                            format!("{:?}", k)
                        }
                        _ => format!("{:?}", k),
                    };
                    json_map.insert(key, yaml_to_json(v));
                }
            }
            JsonValue::Object(json_map)
        }
        _ => JsonValue::Null,
    }
}

/// Extracts the title from a markdown file.
///
/// This function searches for the file recursively in the vault directory, and then:
/// 1. If the file has YAML frontmatter with a 'title' field, returns that value
/// 2. If no title is found in frontmatter, returns the filename without extension
///
/// # Arguments
/// * `filename` - The name of the markdown file (with or without extension)
/// * `vault_directory` - The absolute path to the vault directory
///
/// # Returns
/// * `Result<String>` - The extracted title on success
///
/// # Errors
/// * Returns an error if the file is not found
///
/// # Examples
/// ```
/// use std::path::Path;
/// use notemancy_core::notes::utils::get_title;
///
/// let vault_dir = Path::new("/path/to/vault");
/// let title = get_title("My-Note.md", vault_dir);
/// // title would be either the frontmatter title or "My-Note"
/// ```
pub fn get_title(filename: &str, vault_directory: &Path) -> Result<String> {
    // Try to get frontmatter
    let frontmatter = get_frontmatter(filename, vault_directory)?;

    // Check if frontmatter has a title field
    if let JsonValue::Object(map) = &frontmatter {
        if let Some(JsonValue::String(title)) = map.get("title") {
            return Ok(title.clone());
        }
    }

    // If no title in frontmatter, use filename without extension
    let file_path = match get_file_path_alt(filename, vault_directory) {
        Ok(path) => path,
        Err(_) => {
            // Try with .md extension if not found
            let filename_with_ext =
                if !filename.ends_with(".md") && !filename.ends_with(".markdown") {
                    format!("{}.md", filename)
                } else {
                    filename.to_string()
                };

            let sanitized = sanitize_title(&filename_with_ext);
            get_file_path_alt(&sanitized, vault_directory)?
        }
    };

    let file_stem = file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow!("Failed to extract filename from path"))?;

    Ok(file_stem.to_string())
}

/// Returns the relative path of a file from the vault directory.
///
/// This function searches for the file recursively in the vault directory and
/// returns its path relative to the vault directory without a leading slash.
///
/// # Arguments
/// * `filename` - The name of the markdown file (with or without extension)
/// * `vault_directory` - The absolute path to the vault directory
///
/// # Returns
/// * `Result<String>` - The relative path as a string without leading slash
///
/// # Errors
/// * Returns an error if the file is not found or if path calculation fails
///
/// # Examples
/// ```
/// use std::path::Path;
/// use notemancy_core::notes::utils::get_relpath;
///
/// let vault_dir = Path::new("/path/to/vault");
/// let rel_path = get_relpath("My Note", vault_dir);
/// // If successful, rel_path might be something like "project/My-Note.md"
/// ```
pub fn get_relpath(filename: &str, vault_directory: &Path) -> Result<String> {
    // Find the file
    let file_path = match get_file_path_alt(filename, vault_directory) {
        Ok(path) => path,
        Err(_) => {
            // Try with .md extension if not found
            let filename_with_ext =
                if !filename.ends_with(".md") && !filename.ends_with(".markdown") {
                    format!("{}.md", filename)
                } else {
                    filename.to_string()
                };

            let sanitized = sanitize_title(&filename_with_ext);
            get_file_path_alt(&sanitized, vault_directory)?
        }
    };

    // Calculate relative path
    let rel_path = file_path
        .strip_prefix(vault_directory)
        .context("Failed to calculate relative path")?;

    // Convert to string and remove leading slash if present
    let path_str = rel_path.to_string_lossy().to_string();
    let path_str = path_str.trim_start_matches('/').to_string();

    Ok(path_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
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

    // Helper function to create a test note file with frontmatter
    fn create_test_note_with_frontmatter(
        dir: &Path,
        filename: &str,
        frontmatter: &str,
        content: &str,
    ) -> Result<PathBuf> {
        let filepath = dir.join(filename);
        let full_content = format!("---\n{}---\n\n{}", frontmatter, content);
        fs::write(&filepath, full_content)?;
        Ok(filepath)
    }

    #[test]
    fn test_get_frontmatter() -> Result<()> {
        // Create a temporary directory for testing
        let temp_dir = tempdir()?;
        let vault_directory = temp_dir.path();

        // Create a subdirectory
        let project_dir = vault_directory.join("project");
        fs::create_dir_all(&project_dir)?;

        // Create a test note with frontmatter
        let frontmatter = r#"title: Test Frontmatter
date: 2025-03-26
tags:
  - test
  - frontmatter
nested:
  key1: value1
  key2: 42
  list:
    - item1
    - item2
"#;
        let content = "This is the content of the note.";
        let _test_filepath = create_test_note_with_frontmatter(
            &project_dir,
            "Test-Frontmatter.md",
            frontmatter,
            content,
        )?;

        // Test getting frontmatter
        let frontmatter_json = get_frontmatter("Test Frontmatter", vault_directory)?;

        // Verify the frontmatter was correctly parsed
        assert!(frontmatter_json.is_object());
        let obj = frontmatter_json.as_object().unwrap();

        // Check basic fields
        assert_eq!(
            obj.get("title").unwrap().as_str().unwrap(),
            "Test Frontmatter"
        );
        assert_eq!(obj.get("date").unwrap().as_str().unwrap(), "2025-03-26");

        // Check tags array
        let tags = obj.get("tags").unwrap().as_array().unwrap();
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0].as_str().unwrap(), "test");
        assert_eq!(tags[1].as_str().unwrap(), "frontmatter");

        // Check nested object
        let nested = obj.get("nested").unwrap().as_object().unwrap();
        assert_eq!(nested.get("key1").unwrap().as_str().unwrap(), "value1");
        assert_eq!(nested.get("key2").unwrap().as_i64().unwrap(), 42);

        // Check nested list
        let nested_list = nested.get("list").unwrap().as_array().unwrap();
        assert_eq!(nested_list.len(), 2);
        assert_eq!(nested_list[0].as_str().unwrap(), "item1");
        assert_eq!(nested_list[1].as_str().unwrap(), "item2");

        Ok(())
    }

    #[test]
    fn test_get_frontmatter_no_frontmatter() -> Result<()> {
        // Create a temporary directory for testing
        let temp_dir = tempdir()?;
        let vault_directory = temp_dir.path();

        // Create a subdirectory
        let project_dir = vault_directory.join("project");
        fs::create_dir_all(&project_dir)?;

        // Create a test note without frontmatter
        let test_filepath = project_dir.join("No-Frontmatter.md");
        fs::write(&test_filepath, "This is a note without frontmatter.")?;

        // Test getting frontmatter
        let frontmatter_json = get_frontmatter("No Frontmatter", vault_directory)?;

        // Verify an empty object is returned
        assert!(frontmatter_json.is_object());
        assert!(frontmatter_json.as_object().unwrap().is_empty());

        Ok(())
    }

    #[test]
    fn test_get_frontmatter_invalid() -> Result<()> {
        // Create a temporary directory for testing
        let temp_dir = tempdir()?;
        let vault_directory = temp_dir.path();

        // Create a subdirectory
        let project_dir = vault_directory.join("project");
        fs::create_dir_all(&project_dir)?;

        // Create a test note with invalid frontmatter
        let test_filepath = project_dir.join("Invalid-Frontmatter.md");
        fs::write(
            &test_filepath,
            "---\ntitle: Test\ninvalid:yaml:format\n---\n\nContent.",
        )?;

        // Test getting frontmatter (should fail)
        let result = get_frontmatter("Invalid Frontmatter", vault_directory);
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_get_title_from_frontmatter() -> Result<()> {
        // Create a temporary directory for testing
        let temp_dir = tempdir()?;
        let vault_directory = temp_dir.path();

        // Create a subdirectory
        let project_dir = vault_directory.join("project");
        fs::create_dir_all(&project_dir)?;

        // Create a test note with frontmatter containing a title
        let frontmatter = "title: Custom Title\ndate: 2025-03-26\n";
        let content = "This is the content of the note.";
        let _test_filepath = create_test_note_with_frontmatter(
            &project_dir,
            "Different-Filename.md",
            frontmatter,
            content,
        )?;

        // Test getting title
        let title = get_title("Different Filename", vault_directory)?;

        // Verify the title from frontmatter is returned
        assert_eq!(title, "Custom Title");

        Ok(())
    }

    #[test]
    fn test_get_title_from_filename() -> Result<()> {
        // Create a temporary directory for testing
        let temp_dir = tempdir()?;
        let vault_directory = temp_dir.path();

        // Create a subdirectory
        let project_dir = vault_directory.join("project");
        fs::create_dir_all(&project_dir)?;

        // Create a test note without frontmatter
        let test_filepath = project_dir.join("Filename-Title.md");
        fs::write(&test_filepath, "This is a note without frontmatter.")?;

        // Test getting title
        let title = get_title("Filename Title", vault_directory)?;

        // Verify the filename is returned as title
        assert_eq!(title, "Filename-Title");

        Ok(())
    }

    #[test]
    fn test_get_title_frontmatter_no_title() -> Result<()> {
        // Create a temporary directory for testing
        let temp_dir = tempdir()?;
        let vault_directory = temp_dir.path();

        // Create a subdirectory
        let project_dir = vault_directory.join("project");
        fs::create_dir_all(&project_dir)?;

        // Create a test note with frontmatter but no title
        let frontmatter = "date: 2025-03-26\ntags: [test]\n";
        let content = "This is the content of the note.";
        let _test_filepath = create_test_note_with_frontmatter(
            &project_dir,
            "No-Title-In-Frontmatter.md",
            frontmatter,
            content,
        )?;

        // Test getting title
        let title = get_title("No Title In Frontmatter", vault_directory)?;

        // Verify the filename is returned as title
        assert_eq!(title, "No-Title-In-Frontmatter");

        Ok(())
    }

    #[test]
    fn test_get_relpath() -> Result<()> {
        // Create a temporary directory for testing
        let temp_dir = tempdir()?;
        let vault_directory = temp_dir.path();

        // Create nested subdirectories
        let nested_dir = vault_directory.join("project/subproject/deep");
        fs::create_dir_all(&nested_dir)?;

        // Create a test note
        let test_filepath = nested_dir.join("Deep-Note.md");
        fs::write(&test_filepath, "This is a deeply nested note.")?;

        // Test getting relative path
        let relpath = get_relpath("Deep Note", vault_directory)?;

        // Verify the relative path
        assert_eq!(relpath, "project/subproject/deep/Deep-Note.md");

        Ok(())
    }

    #[test]
    fn test_get_relpath_root() -> Result<()> {
        // Create a temporary directory for testing
        let temp_dir = tempdir()?;
        let vault_directory = temp_dir.path();

        // Create a test note in the root directory
        let test_filepath = vault_directory.join("Root-Note.md");
        fs::write(&test_filepath, "This is a note in the root directory.")?;

        // Test getting relative path
        let relpath = get_relpath("Root Note", vault_directory)?;

        // Verify the relative path (should not have leading slash)
        assert_eq!(relpath, "Root-Note.md");

        Ok(())
    }

    #[test]
    fn test_yaml_to_json() {
        // Test conversion of scalar values
        let yaml_null = serde_yaml::Value::Null;
        assert!(yaml_to_json(yaml_null).is_null());

        let yaml_bool = serde_yaml::Value::Bool(true);
        assert!(yaml_to_json(yaml_bool).as_bool().unwrap());

        let yaml_int = serde_yaml::Value::Number(serde_yaml::Number::from(42));
        assert_eq!(yaml_to_json(yaml_int).as_i64().unwrap(), 42);

        let yaml_float = serde_yaml::Value::Number(serde_yaml::Number::from(3.14));
        assert!(yaml_to_json(yaml_float).as_f64().unwrap() - 3.14 < 0.001);

        let yaml_string = serde_yaml::Value::String("test".to_string());
        assert_eq!(yaml_to_json(yaml_string).as_str().unwrap(), "test");

        // Test conversion of sequence
        let yaml_seq = serde_yaml::Value::Sequence(vec![
            serde_yaml::Value::String("item1".to_string()),
            serde_yaml::Value::Number(serde_yaml::Number::from(2)),
        ]);
        let json_seq = yaml_to_json(yaml_seq);
        let json_array = json_seq.as_array().unwrap();
        assert_eq!(json_array.len(), 2);
        assert_eq!(json_array[0].as_str().unwrap(), "item1");
        assert_eq!(json_array[1].as_i64().unwrap(), 2);

        // Test conversion of mapping
        let mut yaml_mapping = serde_yaml::Mapping::new();
        yaml_mapping.insert(
            serde_yaml::Value::String("key".to_string()),
            serde_yaml::Value::String("value".to_string()),
        );
        yaml_mapping.insert(
            serde_yaml::Value::Number(serde_yaml::Number::from(1)),
            serde_yaml::Value::Bool(true),
        );

        let yaml_map = serde_yaml::Value::Mapping(yaml_mapping);
        let json_map = yaml_to_json(yaml_map);
        let json_obj = json_map.as_object().unwrap();

        assert_eq!(json_obj.get("key").unwrap().as_str().unwrap(), "value");
        assert_eq!(json_obj.get("1").unwrap().as_bool().unwrap(), true);
    }
}
