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
    let re = Regex::new(r"[<>:/\\|?*\n\r\t\.]").unwrap();
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

/// Converts serde_yaml::Value to serde_json::Value.
pub fn yaml_to_json(yaml: serde_yaml::Value) -> JsonValue {
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

/// Lists all markdown files in the vault directory.
///
/// This function uses the `fd` command to find all markdown files in the vault directory.
/// If `relative` is true, paths are returned relative to the vault directory.
/// If `relative` is false, absolute paths are returned.
///
/// # Arguments
/// * `vault_directory` - The absolute path to the vault directory
/// * `relative` - Whether to return relative paths (true) or absolute paths (false)
///
/// # Returns
/// * `Result<Vec<String>>` - A list of paths to markdown files
///
/// # Errors
/// * Returns an error if the `fd` command fails
///
/// # Examples
/// ```
/// use std::path::Path;
/// use notemancy_core::notes::utils::list_all_notes;
///
/// let vault_dir = Path::new("/path/to/vault");
/// // Get all markdown files with relative paths
/// let relative_paths = list_all_notes(vault_dir, true);
/// // Get all markdown files with absolute paths
/// let absolute_paths = list_all_notes(vault_dir, false);
/// ```
pub fn list_all_notes(vault_directory: &Path, relative: bool) -> Result<Vec<String>> {
    // Use fd to find all markdown files in the vault directory
    let output = Command::new("fd")
        .args(&[
            ".md$|.markdown$", // Match markdown file extensions
            vault_directory.to_str().unwrap_or("."),
            "--type",
            "f", // Only find files
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

    // Split by newlines to get all file paths
    let file_paths: Vec<String> = stdout
        .trim()
        .split('\n')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    // If relative paths are requested, convert absolute paths to relative
    if relative {
        let relative_paths = file_paths
            .into_iter()
            .map(|path| {
                let path_buf = PathBuf::from(&path);
                match path_buf.strip_prefix(vault_directory) {
                    Ok(rel_path) => rel_path.to_string_lossy().to_string(),
                    Err(_) => path, // Fallback to original path if stripping prefix fails
                }
            })
            .map(|p| p.trim_start_matches('/').to_string())
            .collect();

        Ok(relative_paths)
    } else {
        Ok(file_paths)
    }
}

/// Alternative implementation of list_all_notes without using the external fd command.
///
/// This is an alternative implementation that uses the walkdir crate to find all markdown files.
///
/// # Arguments
/// * `vault_directory` - The absolute path to the vault directory
/// * `relative` - Whether to return relative paths (true) or absolute paths (false)
///
/// # Returns
/// * `Result<Vec<String>>` - A list of paths to markdown files
pub fn list_all_notes_alt(vault_directory: &Path, relative: bool) -> Result<Vec<String>> {
    let walker = walkdir::WalkDir::new(vault_directory)
        .follow_links(true)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry.file_type().is_file() && {
                let file_name = entry.file_name().to_string_lossy();
                file_name.ends_with(".md") || file_name.ends_with(".markdown")
            }
        });

    let mut file_paths = Vec::new();

    for entry in walker {
        let path = entry.path();

        if relative {
            if let Ok(rel_path) = path.strip_prefix(vault_directory) {
                file_paths.push(rel_path.to_string_lossy().to_string());
            }
        } else {
            file_paths.push(path.to_string_lossy().to_string());
        }
    }

    Ok(file_paths)
}

/// Checks if a title is unique in the vault directory.
///
/// This function sanitizes the input title and checks if a file with that name
/// (with .md or .markdown extension) already exists in the vault directory.
///
/// # Arguments
/// * `title` - The title to check
/// * `vault_directory` - The absolute path to the vault directory
///
/// # Returns
/// * `Result<bool>` - True if the title is unique (no matching file exists),
///                   False if it is not unique (a file with that name already exists)
///
/// # Errors
/// * Returns an error if the file listing operation fails
///
/// # Examples
/// ```
/// use std::path::Path;
/// use notemancy_core::notes::utils::check_unique_title;
///
/// let vault_dir = Path::new("/path/to/vault");
/// // Check if a title "My Note" is unique in the vault
/// let is_unique = check_unique_title("My Note", vault_dir);
/// ```
pub fn check_unique_title(title: &str, vault_directory: &Path) -> Result<bool> {
    // Sanitize the title
    let sanitized_title = sanitize_title(title);

    // List all files in the vault directory
    let all_files = list_all_notes(vault_directory, true)?;

    // Check if any file matches the sanitized title
    for file_path in all_files {
        let path = PathBuf::from(&file_path);
        if let Some(file_stem) = path.file_stem() {
            if let Some(file_stem_str) = file_stem.to_str() {
                // Sanitize the file stem to ensure consistent comparison
                let sanitized_file_stem = sanitize_title(file_stem_str);
                if sanitized_file_stem == sanitized_title {
                    return Ok(false); // Not unique - a matching file exists
                }
            }
        }
    }

    // No matching file found, the title is unique
    Ok(true)
}

/// Gets the absolute file path for a markdown file with the given title using fd.
///
/// This function sanitizes the input title and searches for a file with that name
/// (with .md or .markdown extension) in the vault directory using the fd command.
///
/// # Arguments
/// * `title` - The title to search for
/// * `vault_directory` - The absolute path to the vault directory
///
/// # Returns
/// * `Result<String>` - The absolute path to the file if found
///
/// # Errors
/// * Returns an error if the fd command fails or if no matching file is found
///
/// # Examples
/// ```
/// use std::path::Path;
/// use notemancy_core::notes::utils::get_file_path;
///
/// let vault_dir = Path::new("/path/to/vault");
/// // Get the absolute path of a note with title "My Note"
/// let file_path = get_file_path("My Note", vault_dir);
/// ```
pub fn get_file_path(title: &str, vault_directory: &Path) -> Result<String> {
    // Build a regex pattern that matches a line starting with "title:" followed by the title.
    // This will match lines like "title: My Note" (allowing any whitespace after the colon).
    let rg_pattern = format!("^title:\\s*{}", regex_escape(title));

    // Use ripgrep to search for markdown files that contain the title in their frontmatter.
    let rg_output = Command::new("rg")
        .args(&[
            "--files-with-matches", // Only return the filenames with a match
            "--glob",
            "*.md", // Search markdown files
            "--glob",
            "*.markdown", // Also search .markdown files
            &rg_pattern,  // The pattern to search for
            vault_directory.to_str().unwrap_or("."),
        ])
        .output()
        .context("Failed to execute ripgrep command. Is 'rg' installed?")?;

    // If ripgrep was successful and returned a match, use that.
    if rg_output.status.success() {
        let rg_stdout =
            String::from_utf8(rg_output.stdout).context("Failed to parse ripgrep output")?;
        let rg_file_paths: Vec<&str> = rg_stdout
            .trim()
            .split('\n')
            .filter(|s| !s.is_empty())
            .collect();
        if !rg_file_paths.is_empty() {
            return Ok(rg_file_paths[0].to_string());
        }
    }

    // Fallback: search by sanitized filename using fd.
    let sanitized = sanitize_title(title);
    let fd_pattern = format!("^{}\\.(md|markdown)$", regex_escape(&sanitized));
    let fd_output = Command::new("fd")
        .args(&[
            &fd_pattern,
            vault_directory.to_str().unwrap_or("."),
            "--type",
            "f", // Only find files
        ])
        .output()
        .context("Failed to execute fd command")?;
    if !fd_output.status.success() {
        return Err(anyhow!(
            "fd command failed: {}",
            String::from_utf8_lossy(&fd_output.stderr)
        ));
    }
    let fd_stdout =
        String::from_utf8(fd_output.stdout).context("Failed to parse fd command output")?;
    let fd_file_paths: Vec<&str> = fd_stdout
        .trim()
        .split('\n')
        .filter(|s| !s.is_empty())
        .collect();
    if fd_file_paths.is_empty() {
        Err(anyhow!("No markdown file found with title: {}", title))
    } else {
        Ok(fd_file_paths[0].to_string())
    }
}

/// Extracts the title from a markdown file.
///
/// This function attempts to find the title from:
/// 1. The 'title' field in the YAML frontmatter if it exists
/// 2. Falls back to using the filename (without extension) if no frontmatter title is found
///
/// # Arguments
/// * `file_path` - Path to the markdown file
///
/// # Returns
/// * `Result<String>` - The extracted title
///
/// # Errors
/// * Returns an error if the file cannot be read
///
/// # Examples
/// ```
/// use std::path::Path;
/// use notemancy_core::notes::utils::get_title;
///
/// let file_path = Path::new("/path/to/vault/My-Note.md");
/// let title = get_title(file_path);
/// ```
pub fn get_title(file_path: &Path) -> Result<String> {
    // Read the file content
    let content = fs::read_to_string(file_path)
        .with_context(|| format!("Failed to read file: {}", file_path.display()))?;

    // Try to extract title from frontmatter
    if let Some(title) = extract_frontmatter_title(&content) {
        return Ok(title);
    }

    // Fall back to using the filename without extension
    if let Some(file_stem) = file_path.file_stem() {
        if let Some(file_stem_str) = file_stem.to_str() {
            // Convert hyphens back to spaces for a more natural title
            let title = file_stem_str.replace('-', " ");
            return Ok(title);
        }
    }

    // If even the filename can't be used, return an error
    Err(anyhow!(
        "Could not determine title for file: {}",
        file_path.display()
    ))
}

/// Helper function to extract title from YAML frontmatter.
fn extract_frontmatter_title(content: &str) -> Option<String> {
    // Check if the content starts with YAML frontmatter (---)
    if !content.starts_with("---") {
        return None;
    }

    // Find the end of frontmatter
    let end_marker = content[3..].find("---")?;
    let frontmatter = &content[3..3 + end_marker];

    // Look for a line starting with "title:" in the frontmatter
    for line in frontmatter.lines() {
        let line = line.trim();
        if line.starts_with("title:") {
            // Extract the title value (everything after "title:")
            let title = line[6..].trim().to_string();
            return Some(title);
        }
    }

    None
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

    #[test]
    fn test_list_all_notes_alt() -> Result<()> {
        // Create a temporary directory for testing
        let temp_dir = tempdir()?;
        let vault_directory = temp_dir.path();

        // Create test directories
        let project1_dir = vault_directory.join("project1");
        let project2_dir = vault_directory.join("project2");
        fs::create_dir_all(&project1_dir)?;
        fs::create_dir_all(&project2_dir)?;

        // Create test markdown files
        let test_file1 = project1_dir.join("Note1.md");
        let test_file2 = project1_dir.join("Note2.markdown");
        let test_file3 = project2_dir.join("Note3.md");
        fs::write(&test_file1, "content1")?;
        fs::write(&test_file2, "content2")?;
        fs::write(&test_file3, "content3")?;

        // Create a non-markdown file (should be ignored)
        let non_md_file = project1_dir.join("document.txt");
        fs::write(&non_md_file, "not a markdown file")?;

        // Test with absolute paths
        let absolute_paths = list_all_notes_alt(vault_directory, false)?;
        assert_eq!(absolute_paths.len(), 3);
        // Check that all paths are absolute
        for path in &absolute_paths {
            assert!(Path::new(path).is_absolute());
        }

        // Test with relative paths
        let relative_paths = list_all_notes_alt(vault_directory, true)?;
        assert_eq!(relative_paths.len(), 3);
        // Check that all paths are relative
        for path in &relative_paths {
            assert!(!Path::new(path).is_absolute());
        }

        Ok(())
    }

    #[test]
    // #[ignore] // Skip if fd is not available
    fn test_list_all_notes() -> Result<()> {
        // Check if fd is available
        match Command::new("fd").arg("--version").output() {
            Ok(_) => {} // fd is available
            Err(_) => {
                eprintln!("Skipping test_list_all_notes because fd command is not available");
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

        // Create test markdown files
        let test_file1 = project1_dir.join("Note1.md");
        let test_file2 = project1_dir.join("Note2.markdown");
        let test_file3 = project2_dir.join("Note3.md");
        fs::write(&test_file1, "content1")?;
        fs::write(&test_file2, "content2")?;
        fs::write(&test_file3, "content3")?;

        // Create a non-markdown file (should be ignored)
        let non_md_file = project1_dir.join("document.txt");
        fs::write(&non_md_file, "not a markdown file")?;

        // Test with absolute paths
        let absolute_paths = list_all_notes(vault_directory, false)?;
        assert_eq!(absolute_paths.len(), 3);
        // Check that all paths are absolute
        for path in &absolute_paths {
            assert!(Path::new(path).is_absolute());
        }

        // Test with relative paths
        let relative_paths = list_all_notes(vault_directory, true)?;
        assert_eq!(relative_paths.len(), 3);
        // Check that all paths are relative
        for path in &relative_paths {
            assert!(!Path::new(path).is_absolute());
        }

        Ok(())
    }

    #[test]
    fn test_check_unique_title() -> Result<()> {
        // Create a temporary directory for testing
        let temp_dir = tempdir()?;
        let vault_directory = temp_dir.path();

        // Create a test directory
        let project_dir = vault_directory.join("project");
        fs::create_dir_all(&project_dir)?;

        // Create a test file
        let test_filename = "Existing-Note.md";
        let test_filepath = project_dir.join(test_filename);
        fs::write(&test_filepath, "existing note content")?;

        // Test with a title that exists (should not be unique)
        let is_unique = check_unique_title("Existing Note", vault_directory)?;
        assert!(
            !is_unique,
            "Expected title 'Existing Note' to not be unique"
        );

        // Test with a title that doesn't exist (should be unique)
        let is_unique = check_unique_title("Non Existent Note", vault_directory)?;
        assert!(is_unique, "Expected title 'Non Existent Note' to be unique");

        // Test with a title that uses different formatting but sanitizes to the same result
        let is_unique = check_unique_title("Existing.Note", vault_directory)?;
        assert!(
            !is_unique,
            "Expected title 'Existing.Note' to not be unique after sanitization"
        );

        Ok(())
    }

    #[test]
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
        let project1_dir = vault_directory.join("project1");
        let project2_dir = vault_directory.join("project2");
        fs::create_dir_all(&project1_dir)?;
        fs::create_dir_all(&project2_dir)?;

        // Create test markdown files
        let test_file1 = project1_dir.join("My-Note.md");
        let test_file2 = project1_dir.join("Another-Note.markdown");
        let test_file3 = project2_dir.join("Special-Note.md");
        fs::write(&test_file1, "content1")?;
        fs::write(&test_file2, "content2")?;
        fs::write(&test_file3, "content3")?;

        // Create a non-markdown file (should be ignored)
        let non_md_file = project1_dir.join("document.txt");
        fs::write(&non_md_file, "not a markdown file")?;

        // Test finding a file with exact title
        let file_path = get_file_path("My Note", vault_directory);
        assert!(
            file_path.is_ok(),
            "Should find the file with title 'My Note'"
        );
        let file_path = file_path?;
        assert!(file_path.contains("My-Note.md"), "Should find My-Note.md");

        // Test finding a file with title that needs sanitization
        let file_path = get_file_path("Another.Note", vault_directory);
        assert!(
            file_path.is_ok(),
            "Should find the file with sanitized title 'Another-Note'"
        );
        let file_path = file_path?;
        assert!(
            file_path.contains("Another-Note.markdown"),
            "Should find Another-Note.markdown"
        );

        // Test finding a file in a subdirectory
        let file_path = get_file_path("Special Note", vault_directory);
        assert!(
            file_path.is_ok(),
            "Should find the file with title 'Special Note'"
        );
        let file_path = file_path?;
        assert!(
            file_path.contains("Special-Note.md"),
            "Should find Special-Note.md"
        );

        // Test with a title that doesn't exist
        let file_path = get_file_path("Non Existent Note", vault_directory);
        assert!(
            file_path.is_err(),
            "Should not find a file with title 'Non Existent Note'"
        );

        Ok(())
    }
    #[test]
    fn test_get_title_from_frontmatter() -> Result<()> {
        // Create a temporary file with frontmatter
        let temp_dir = tempdir()?;
        let file_path = temp_dir.path().join("Test-Note.md");

        let content = r#"---
title: Custom Frontmatter Title
created_on: 2023-05-15
modified_at: 2023-05-15 10:30:45
---

This is the content of the note."#;

        fs::write(&file_path, content)?;

        // Test getting title from frontmatter
        let title = get_title(&file_path)?;
        assert_eq!(title, "Custom Frontmatter Title");

        Ok(())
    }

    #[test]
    fn test_get_title_from_filename() -> Result<()> {
        // Create a temporary file without frontmatter
        let temp_dir = tempdir()?;
        let file_path = temp_dir.path().join("Filename-Based-Title.md");

        let content = "This note has no frontmatter, so the title should come from the filename.";
        fs::write(&file_path, content)?;

        // Test getting title from filename
        let title = get_title(&file_path)?;
        assert_eq!(title, "Filename Based Title");

        Ok(())
    }

    #[test]
    fn test_get_title_malformed_frontmatter() -> Result<()> {
        // Create a temporary file with malformed frontmatter (missing title)
        let temp_dir = tempdir()?;
        let file_path = temp_dir.path().join("Malformed-Frontmatter.md");

        let content = r#"---
created_on: 2023-05-15
modified_at: 2023-05-15 10:30:45
---

This frontmatter has no title field."#;

        fs::write(&file_path, content)?;

        // Test falling back to filename when frontmatter has no title
        let title = get_title(&file_path)?;
        assert_eq!(title, "Malformed Frontmatter");

        Ok(())
    }
}
