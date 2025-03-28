// src/workspaces/crud.rs

use crate::notes::utils::sanitize_title;
use crate::workspaces::utils::check_if_workspace_exists;
use anyhow::{Context, Result, anyhow};
use std::fs::{self, File, create_dir_all};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

/// Creates a new workspace with the given name and adds the file path to it.
///
/// # Arguments
/// * `vault_directory` - The base directory of the vault
/// * `workspace_name` - The name of the workspace to create
/// * `file_path` - The path to a markdown note to add to the workspace
///
/// # Returns
/// * `Result<PathBuf>` - The path to the created workspace file
///
/// # Errors
/// * Returns an error if the file path doesn't point to a valid markdown note
/// * Returns an error if the workspace file can't be created
pub fn create_workspace(
    vault_directory: &Path,
    workspace_name: &str,
    file_path: &str,
) -> Result<PathBuf> {
    // Validate the file path points to a valid markdown note
    validate_file_path(vault_directory, file_path)?;

    // Sanitize the workspace name
    let sanitized_name = sanitize_title(workspace_name);

    // Create the workspaces directory if it doesn't exist
    let workspaces_dir = vault_directory.join("workspaces");
    create_dir_all(&workspaces_dir).context("Failed to create workspaces directory")?;

    // Create the workspace file path
    let workspace_file_path = workspaces_dir.join(format!("{}.txt", sanitized_name));

    // Check if workspace file already exists
    if workspace_file_path.exists() {
        return Err(anyhow!(
            "A workspace with the name '{}' already exists",
            workspace_name
        ));
    }

    // Create and write to the workspace file
    let mut file = File::create(&workspace_file_path).context(format!(
        "Failed to create workspace file: {}",
        workspace_name
    ))?;

    writeln!(file, "{}", file_path).context("Failed to write to workspace file")?;

    Ok(workspace_file_path)
}

/// Appends a file path to an existing workspace or creates a new one if it doesn't exist.
///
/// # Arguments
/// * `vault_directory` - The base directory of the vault
/// * `workspace_name` - The name of the workspace
/// * `file_path` - The path to a markdown note to add to the workspace
///
/// # Returns
/// * `Result<PathBuf>` - The path to the workspace file
///
/// # Errors
/// * Returns an error if the file path doesn't point to a valid markdown note
/// * Returns an error if the workspace file can't be accessed or created
pub fn append_to_workspace(
    vault_directory: &Path,
    workspace_name: &str,
    file_path: &str,
) -> Result<PathBuf> {
    // Validate the file path points to a valid markdown note
    validate_file_path(vault_directory, file_path)?;

    // Check if workspace exists
    if !check_if_workspace_exists(vault_directory, workspace_name)? {
        // If workspace doesn't exist, create it with the file path
        return create_workspace(vault_directory, workspace_name, file_path);
    }

    // If workspace exists, get the sanitized name and file path
    let sanitized_name = sanitize_title(workspace_name);
    let workspace_file_path = vault_directory
        .join("workspaces")
        .join(format!("{}.txt", sanitized_name));

    // Check if the file path is already in the workspace
    if is_file_in_workspace(&workspace_file_path, file_path)? {
        return Err(anyhow!(
            "File path '{}' is already in workspace '{}'",
            file_path,
            workspace_name
        ));
    }

    // Append the file path to the workspace file
    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(&workspace_file_path)
        .context(format!(
            "Failed to open workspace file for appending: {}",
            workspace_name
        ))?;

    writeln!(file, "{}", file_path).context("Failed to append to workspace file")?;

    Ok(workspace_file_path)
}

/// Removes a file path from an existing workspace.
///
/// # Arguments
/// * `vault_directory` - The base directory of the vault
/// * `workspace_name` - The name of the workspace
/// * `file_path` - The path to a markdown note to remove from the workspace
///
/// # Returns
/// * `Result<PathBuf>` - The path to the updated workspace file
///
/// # Errors
/// * Returns an error if the workspace doesn't exist
/// * Returns an error if the file path is not found in the workspace
/// * Returns an error if the workspace file can't be updated
pub fn remove_from_workspace(
    vault_directory: &Path,
    workspace_name: &str,
    file_path: &str,
) -> Result<PathBuf> {
    // Check if workspace exists
    if !check_if_workspace_exists(vault_directory, workspace_name)? {
        return Err(anyhow!("Workspace '{}' does not exist", workspace_name));
    }

    // Get the sanitized name and file path
    let sanitized_name = sanitize_title(workspace_name);
    let workspace_file_path = vault_directory
        .join("workspaces")
        .join(format!("{}.txt", sanitized_name));

    // Read the existing file paths
    let file = File::open(&workspace_file_path)
        .context(format!("Failed to open workspace file: {}", workspace_name))?;
    let reader = BufReader::new(file);

    let mut file_paths: Vec<String> = Vec::new();
    let mut found = false;

    // Filter out the file path to remove
    for line in reader.lines() {
        let line = line.context("Failed to read line from workspace file")?;
        if line.trim() == file_path.trim() {
            found = true;
        } else {
            file_paths.push(line);
        }
    }

    if !found {
        return Err(anyhow!(
            "File path '{}' not found in workspace '{}'",
            file_path,
            workspace_name
        ));
    }

    // Write the updated file paths back to the workspace file
    let mut file = File::create(&workspace_file_path).context(format!(
        "Failed to update workspace file: {}",
        workspace_name
    ))?;

    for path in &file_paths {
        writeln!(file, "{}", path).context("Failed to write to workspace file")?;
    }

    Ok(workspace_file_path)
}

/// Deletes an existing workspace.
///
/// # Arguments
/// * `vault_directory` - The base directory of the vault
/// * `workspace_name` - The name of the workspace to delete
///
/// # Returns
/// * `Result<()>` - Ok if the workspace was successfully deleted
///
/// # Errors
/// * Returns an error if the workspace doesn't exist
/// * Returns an error if the workspace file can't be deleted
pub fn delete_workspace(vault_directory: &Path, workspace_name: &str) -> Result<()> {
    // Check if workspace exists
    if !check_if_workspace_exists(vault_directory, workspace_name)? {
        return Err(anyhow!("Workspace '{}' does not exist", workspace_name));
    }

    // Get the sanitized name and file path
    let sanitized_name = sanitize_title(workspace_name);
    let workspace_file_path = vault_directory
        .join("workspaces")
        .join(format!("{}.txt", sanitized_name));

    // Delete the workspace file
    fs::remove_file(&workspace_file_path).context(format!(
        "Failed to delete workspace file: {}",
        workspace_name
    ))?;

    Ok(())
}

/// Helper function to validate that a file path points to a valid markdown note in the vault.
fn validate_file_path(vault_directory: &Path, file_path: &str) -> Result<()> {
    let path = Path::new(file_path);

    // Check if the file exists within the vault directory
    if !path.is_absolute() {
        // If the path is relative, join it with the vault directory
        let full_path = vault_directory.join(path);
        if !full_path.exists() {
            return Err(anyhow!(
                "File path '{}' does not exist in the vault",
                file_path
            ));
        }

        // Check if the file is a markdown file
        if let Some(extension) = full_path.extension() {
            if extension != "md" && extension != "markdown" {
                return Err(anyhow!("File '{}' is not a markdown file", file_path));
            }
        } else {
            return Err(anyhow!("File '{}' has no extension", file_path));
        }
    } else {
        // If the path is absolute, check if it's within the vault
        let vault_canonical =
            fs::canonicalize(vault_directory).context("Failed to canonicalize vault directory")?;
        let file_canonical = fs::canonicalize(path)
            .context(format!("Failed to canonicalize file path: {}", file_path))?;

        if !file_canonical.starts_with(&vault_canonical) {
            return Err(anyhow!(
                "File path '{}' is not within the vault directory",
                file_path
            ));
        }

        // Check if the file is a markdown file
        if let Some(extension) = path.extension() {
            if extension != "md" && extension != "markdown" {
                return Err(anyhow!("File '{}' is not a markdown file", file_path));
            }
        } else {
            return Err(anyhow!("File '{}' has no extension", file_path));
        }
    }

    Ok(())
}

/// Helper function to check if a file path is already in a workspace.
fn is_file_in_workspace(workspace_file_path: &Path, file_path: &str) -> Result<bool> {
    let file = File::open(workspace_file_path).context(format!(
        "Failed to open workspace file: {:?}",
        workspace_file_path
    ))?;
    let reader = BufReader::new(file);

    for line in reader.lines() {
        let line = line.context("Failed to read line from workspace file")?;
        if line.trim() == file_path.trim() {
            return Ok(true);
        }
    }

    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    // Helper function to create a test note file
    fn create_test_note(dir: &Path, filename: &str) -> Result<PathBuf> {
        let note_path = dir.join(filename);
        fs::write(&note_path, "Test note content").context("Failed to write test note")?;
        Ok(note_path)
    }

    #[test]
    fn test_create_workspace() -> Result<()> {
        let temp_dir = tempdir()?;
        let vault_dir = temp_dir.path();

        // Create a test note
        let note_path = create_test_note(vault_dir, "test-note.md")?;
        let note_path_str = note_path.to_string_lossy();

        // Create a workspace
        let workspace_path = create_workspace(vault_dir, "Test Workspace", &note_path_str)?;

        // Verify the workspace file exists
        assert!(workspace_path.exists());

        // Verify the content of the workspace file
        let content = fs::read_to_string(&workspace_path)?;
        assert!(content.contains(&*note_path_str));

        Ok(())
    }

    #[test]
    fn test_append_to_workspace() -> Result<()> {
        let temp_dir = tempdir()?;
        let vault_dir = temp_dir.path();

        // Create two test notes
        let note1_path = create_test_note(vault_dir, "note1.md")?;
        let note2_path = create_test_note(vault_dir, "note2.md")?;

        let note1_path_str = note1_path.to_string_lossy();
        let note2_path_str = note2_path.to_string_lossy();

        // Create a workspace with the first note
        let workspace_path = create_workspace(vault_dir, "Append Test", &note1_path_str)?;

        // Append the second note
        let updated_path = append_to_workspace(vault_dir, "Append Test", &note2_path_str)?;
        assert_eq!(workspace_path, updated_path);

        // Verify both notes are in the workspace
        let content = fs::read_to_string(&workspace_path)?;
        assert!(content.contains(&*note1_path_str));
        assert!(content.contains(&*note2_path_str));

        Ok(())
    }

    #[test]
    fn test_remove_from_workspace() -> Result<()> {
        let temp_dir = tempdir()?;
        let vault_dir = temp_dir.path();

        // Create two test notes
        let note1_path = create_test_note(vault_dir, "remove1.md")?;
        let note2_path = create_test_note(vault_dir, "remove2.md")?;

        let note1_path_str = note1_path.to_string_lossy();
        let note2_path_str = note2_path.to_string_lossy();

        // Create a workspace with both notes
        let workspace_name = "Remove Test";
        create_workspace(vault_dir, workspace_name, &note1_path_str)?;
        append_to_workspace(vault_dir, workspace_name, &note2_path_str)?;

        // Remove the first note
        let workspace_path = remove_from_workspace(vault_dir, workspace_name, &note1_path_str)?;

        // Verify only the second note remains
        let content = fs::read_to_string(&workspace_path)?;
        assert!(!content.contains(&*note1_path_str));
        assert!(content.contains(&*note2_path_str));

        Ok(())
    }

    #[test]
    fn test_delete_workspace() -> Result<()> {
        let temp_dir = tempdir()?;
        let vault_dir = temp_dir.path();

        // Create a test note
        let note_path = create_test_note(vault_dir, "delete-test.md")?;
        let note_path_str = note_path.to_string_lossy();

        // Create a workspace
        let workspace_name = "Delete Test";
        let workspace_path = create_workspace(vault_dir, workspace_name, &note_path_str)?;

        // Verify the workspace file exists
        assert!(workspace_path.exists());

        // Delete the workspace
        delete_workspace(vault_dir, workspace_name)?;

        // Verify the workspace file no longer exists
        assert!(!workspace_path.exists());

        Ok(())
    }

    #[test]
    fn test_validate_file_path() -> Result<()> {
        let temp_dir = tempdir()?;
        let vault_dir = temp_dir.path();

        // Create a valid markdown note
        let valid_note = create_test_note(vault_dir, "valid.md")?;
        let valid_note_str = valid_note.to_string_lossy();

        // Create a non-markdown file
        let invalid_file = vault_dir.join("invalid.txt");
        fs::write(&invalid_file, "Not a markdown file")?;
        let invalid_file_str = invalid_file.to_string_lossy();

        // Test with valid markdown file
        assert!(validate_file_path(vault_dir, &valid_note_str).is_ok());

        // Test with non-markdown file
        assert!(validate_file_path(vault_dir, &invalid_file_str).is_err());

        // Test with non-existent file
        let nonexistent = vault_dir
            .join("nonexistent.md")
            .to_string_lossy()
            .to_string();
        assert!(validate_file_path(vault_dir, &nonexistent).is_err());

        Ok(())
    }
}
