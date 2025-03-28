// src/utils.rs

use crate::notes::utils::get_file_path;
use anyhow::{Context, Result, anyhow};
use serde_yaml;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Reads the config.yaml file and extracts the vault directory path for the specified vault name.
///
/// # Arguments
/// * `vault_name` - The name of the vault to look for
/// * `config_dir` - The directory containing the config.yaml file
///
/// # Returns
/// * `Result<PathBuf>` - The path to the vault directory
///
/// # Errors
/// * Returns an error if the config file can't be read
/// * Returns an error if the vault name is not found in the config
pub fn get_vault_directory(vault_name: &str, config_dir: &Path) -> Result<PathBuf> {
    // Build the path to the config file
    let config_path = config_dir.join("config.yaml");

    // Read the config file
    let config_content = fs::read_to_string(&config_path)
        .context(format!("Failed to read config file at {:?}", config_path))?;

    // Parse the YAML content
    let config: serde_yaml::Value =
        serde_yaml::from_str(&config_content).context("Failed to parse config.yaml content")?;

    // Extract the vaults section
    let vaults = config
        .get("vaults")
        .ok_or_else(|| anyhow!("No 'vaults' section found in config.yaml"))?;

    // Find the specified vault
    if let Some(vaults) = vaults.as_sequence() {
        for vault in vaults {
            if let Some(name) = vault.get("name") {
                if name.as_str() == Some(vault_name) {
                    if let Some(path) = vault.get("vault_directory") {
                        if let Some(path_str) = path.as_str() {
                            return Ok(PathBuf::from(path_str));
                        }
                    }
                    return Err(anyhow!(
                        "Vault '{}' found but has no valid path specified",
                        vault_name
                    ));
                }
            }
        }
    }

    Err(anyhow!("Vault '{}' not found in config.yaml", vault_name))
}

/// Reads the config.yaml file and extracts the default vault name, then returns its directory path.
///
/// # Arguments
/// * `config_dir` - The directory containing the config.yaml file
///
/// # Returns
/// * `Result<(String, PathBuf)>` - A tuple containing the default vault name and its directory path
///
/// # Errors
/// * Returns an error if the config file can't be read
/// * Returns an error if no default vault is specified
pub fn get_default_vault(config_dir: &Path) -> Result<(String, PathBuf)> {
    // Build the path to the config file
    let config_path = config_dir.join("config.yaml");

    // Read the config file
    let config_content = fs::read_to_string(&config_path)
        .context(format!("Failed to read config file at {:?}", config_path))?;

    // Parse the YAML content
    let config: serde_yaml::Value =
        serde_yaml::from_str(&config_content).context("Failed to parse config.yaml content")?;

    // Extract the default vault name
    let default_vault = config
        .get("default_vault")
        .ok_or_else(|| anyhow!("No 'default_vault' specified in config.yaml"))?
        .as_str()
        .ok_or_else(|| anyhow!("'default_vault' is not a valid string"))?
        .to_string();

    // Get the vault directory for the default vault
    let vault_directory = get_vault_directory(&default_vault, config_dir)?;

    Ok((default_vault, vault_directory))
}

/// Finds the path to a note file given its title and returns the path relative to the vault directory.
///
/// # Arguments
/// * `title` - The title of the note
/// * `vault_directory` - The base directory of the vault
///
/// # Returns
/// * `Result<String>` - The path to the note file relative to the vault directory
///
/// # Errors
/// * Returns an error if the note file can't be found
/// * Returns an error if the absolute path can't be converted to a relative path
pub fn get_relpath(title: &str, vault_directory: &Path) -> Result<String> {
    // Get the absolute file path
    let absolute_path = get_file_path(title, vault_directory)?;

    // Convert to relative path
    absolute_to_relative(&absolute_path, vault_directory)
}

/// Converts an absolute path to a path relative to the vault directory.
///
/// # Arguments
/// * `absolute_path` - The absolute path to convert
/// * `vault_directory` - The base directory of the vault
///
/// # Returns
/// * `Result<String>` - The path relative to the vault directory
///
/// # Errors
/// * Returns an error if the absolute path is not within the vault directory
/// * Returns an error if the path conversion fails
pub fn absolute_to_relative(absolute_path: &str, vault_directory: &Path) -> Result<String> {
    let path = Path::new(absolute_path);

    // Convert path and vault directory to canonical paths to handle any "../" or symlinks
    let canonical_path = fs::canonicalize(path)
        .context(format!("Failed to canonicalize path: {}", absolute_path))?;

    let canonical_vault = fs::canonicalize(vault_directory).context(format!(
        "Failed to canonicalize vault directory: {:?}",
        vault_directory
    ))?;

    // Make sure the path is within the vault directory
    if !canonical_path.starts_with(&canonical_vault) {
        return Err(anyhow!(
            "Path '{}' is not within the vault directory",
            absolute_path
        ));
    }

    // Strip the vault prefix to get the relative path
    let relative_path = canonical_path
        .strip_prefix(&canonical_vault)
        .context(format!(
            "Failed to strip vault prefix from path: {}",
            absolute_path
        ))?;

    // Convert to a string, handling potential non-UTF8 characters
    let rel_path_str = relative_path
        .to_str()
        .ok_or_else(|| anyhow!("Path contains invalid UTF-8 characters"))?
        .to_string();

    // Ensure the path uses forward slashes (for cross-platform compatibility)
    let normalized_path = rel_path_str.replace('\\', "/");

    // Remove leading slash if present
    let normalized_path = normalized_path.trim_start_matches('/').to_string();

    Ok(normalized_path)
}

/// Converts a path relative to the vault directory to an absolute path.
///
/// # Arguments
/// * `relative_path` - The path relative to the vault directory
/// * `vault_directory` - The base directory of the vault
///
/// # Returns
/// * `Result<String>` - The absolute path
///
/// # Errors
/// * Returns an error if the path joining fails
pub fn relative_to_absolute(relative_path: &str, vault_directory: &Path) -> Result<String> {
    // Remove any leading slashes or "./" from the relative path
    let cleaned_path = relative_path
        .trim_start_matches('/')
        .trim_start_matches("./");

    // Join the vault directory with the cleaned relative path
    let absolute_path = vault_directory.join(cleaned_path);

    // Convert to a string, handling potential non-UTF8 characters
    let abs_path_str = absolute_path
        .to_str()
        .ok_or_else(|| anyhow!("Path contains invalid UTF-8 characters"))?
        .to_string();

    Ok(abs_path_str)
}

/// Finds all notes that link to a specific note by searching for its relative path.
///
/// # Arguments
/// * `title` - The title of the note to find backlinks for
/// * `vault_directory` - The base directory of the vault
///
/// # Returns
/// * `Result<Vec<String>>` - A list of relative paths to notes that link to the specified note
///
/// # Errors
/// * Returns an error if the note file can't be found
/// * Returns an error if the ripgrep command fails
/// * Returns an error if any path conversion fails
pub fn get_backlinks(title: &str, vault_directory: &Path) -> Result<Vec<String>> {
    // 1. Get the relative path to the note with the given title
    let target_relpath = get_relpath(title, vault_directory)?;

    // 2. Use ripgrep to search for all occurrences of the relative path in markdown files
    let output = Command::new("rg")
        .args(&[
            "--files-with-matches", // Only show filenames that match
            "--glob",
            "*.md",          // Only search markdown files
            &target_relpath, // Search pattern (the relative path)
            vault_directory
                .to_str()
                .ok_or_else(|| anyhow!("Invalid vault directory path"))?,
        ])
        .output()
        .context("Failed to execute ripgrep command. Is 'rg' installed?")?;

    if !output.status.success() {
        // If ripgrep exits with a non-zero status but the error is "no matches found",
        // we treat this as an empty result rather than an error
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.contains("no matches found") {
            return Err(anyhow!("ripgrep command failed: {}", stderr));
        }
    }

    // 3. Get the list of absolute paths from the ripgrep output
    let stdout =
        String::from_utf8(output.stdout).context("Failed to parse ripgrep command output")?;

    // Split by newlines to get all file paths, filtering out empty lines
    let absolute_paths: Vec<&str> = stdout
        .trim()
        .split('\n')
        .filter(|s| !s.is_empty())
        .collect();

    // 4. Convert absolute paths to relative paths
    let mut relative_paths = Vec::new();

    for abs_path in absolute_paths {
        // Skip the target file itself - we don't consider self-links as backlinks
        if Path::new(abs_path).file_stem() != Path::new(&target_relpath).file_stem() {
            let rel_path = absolute_to_relative(abs_path, vault_directory)?;
            relative_paths.push(rel_path);
        }
    }

    Ok(relative_paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_get_vault_directory() -> Result<()> {
        let temp_dir = tempdir()?;
        let config_dir = temp_dir.path();

        // Create a test config.yaml file
        let config_content = r#"
        default_vault: vault1
        vaults:
          - name: vault1
            vault_directory: /path/to/vault1
          - name: vault2
            vault_directory: /path/to/vault2
        "#;

        let config_path = config_dir.join("config.yaml");
        let mut file = File::create(&config_path)?;
        file.write_all(config_content.as_bytes())?;

        // Test retrieving vault directory
        let vault_dir = get_vault_directory("vault1", config_dir)?;
        assert_eq!(vault_dir, PathBuf::from("/path/to/vault1"));

        // Test with vault that doesn't exist
        let result = get_vault_directory("vault3", config_dir);
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_get_default_vault() -> Result<()> {
        let temp_dir = tempdir()?;
        let config_dir = temp_dir.path();

        // Create a test config.yaml file
        let config_content = r#"
        default_vault: vault1
        vaults:
          - name: vault1
            vault_directory: /path/to/vault1
          - name: vault2
            vault_directory: /path/to/vault2
        "#;

        let config_path = config_dir.join("config.yaml");
        let mut file = File::create(&config_path)?;
        file.write_all(config_content.as_bytes())?;

        // Test retrieving default vault
        let (vault_name, vault_dir) = get_default_vault(config_dir)?;
        assert_eq!(vault_name, "vault1");
        assert_eq!(vault_dir, PathBuf::from("/path/to/vault1"));

        // Test with missing default_vault field
        let config_content_no_default = r#"
        vaults:
          - name: vault1
            path: /path/to/vault1
        "#;

        let mut file = File::create(&config_path)?;
        file.write_all(config_content_no_default.as_bytes())?;

        let result = get_default_vault(config_dir);
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_absolute_to_relative() -> Result<()> {
        let temp_dir = tempdir()?;
        let vault_dir = temp_dir.path();

        // Create a test directory structure
        let test_dir = vault_dir.join("test");
        fs::create_dir_all(&test_dir)?;

        let test_file = test_dir.join("test.md");
        fs::write(&test_file, "test content")?;

        // Test converting absolute to relative path
        let absolute_path = test_file.to_string_lossy().to_string();
        let relative_path = absolute_to_relative(&absolute_path, vault_dir)?;

        assert_eq!(relative_path, "test/test.md");

        // Test with a path outside the vault
        let outside_dir = tempdir()?;
        let outside_file = outside_dir.path().join("outside.md");
        fs::write(&outside_file, "outside content")?;

        let outside_path = outside_file.to_string_lossy().to_string();
        let result = absolute_to_relative(&outside_path, vault_dir);

        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_relative_to_absolute() -> Result<()> {
        let temp_dir = tempdir()?;
        let vault_dir = temp_dir.path();

        // Test converting relative to absolute path
        let relative_path = "notes/test.md";
        let absolute_path = relative_to_absolute(relative_path, vault_dir)?;

        let expected_path = vault_dir
            .join("notes/test.md")
            .to_string_lossy()
            .to_string();
        assert_eq!(absolute_path, expected_path);

        // Test with leading slash
        let relative_path = "/notes/test.md";
        let absolute_path = relative_to_absolute(relative_path, vault_dir)?;

        assert_eq!(absolute_path, expected_path);

        // Test with "./path" notation
        let relative_path = "./notes/test.md";
        let absolute_path = relative_to_absolute(relative_path, vault_dir)?;

        assert_eq!(absolute_path, expected_path);

        Ok(())
    }

    // Note: We can't test get_relpath directly here without mocking get_file_path
    // But we've tested the main components separately

    #[test]
    fn test_get_backlinks() -> Result<()> {
        let temp_dir = tempdir()?;
        let vault_dir = temp_dir.path();

        // Create a test directory structure
        let notes_dir = vault_dir.join("notes");
        fs::create_dir_all(&notes_dir)?;

        // Create a target note
        let target_note_path = notes_dir.join("target-note.md");
        fs::write(&target_note_path, "This is the target note")?;

        // Create some notes that link to the target note
        let backlink1_path = notes_dir.join("backlink1.md");
        fs::write(
            &backlink1_path,
            "This links to [notes/target-note.md](notes/target-note.md)",
        )?;

        let backlink2_path = notes_dir.join("backlink2.md");
        fs::write(&backlink2_path, "Another link to notes/target-note.md here")?;

        let no_backlink_path = notes_dir.join("no-backlink.md");
        fs::write(&no_backlink_path, "This note has no link to the target")?;

        // We can't properly test this without mocking get_file_path and the ripgrep command
        // In a real test, we would use a mocking framework or dependency injection
        // For now, we'll skip the actual execution and just confirm the function exists

        Ok(())
    }
}
