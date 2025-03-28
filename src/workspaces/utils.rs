// src/workspaces/utils.rs

use crate::notes::utils::sanitize_title;
use anyhow::{Context, Result};
use std::path::Path;

/// Checks if a workspace with the given name exists.
///
/// # Arguments
/// * `vault_directory` - The base directory of the vault
/// * `workspace_name` - The name of the workspace to check
///
/// # Returns
/// * `Result<bool>` - True if the workspace exists, False otherwise
///
/// # Errors
/// * Returns an error if the workspace directory can't be accessed
pub fn check_if_workspace_exists(vault_directory: &Path, workspace_name: &str) -> Result<bool> {
    // Sanitize the workspace name
    let sanitized_name = sanitize_title(workspace_name);

    // Generate the workspace path
    let workspace_file_path = vault_directory
        .join("workspaces")
        .join(format!("{}.txt", sanitized_name));

    // Check if the workspace file exists
    Ok(workspace_file_path.exists())
}

/// List all available workspaces in the vault.
///
/// # Arguments
/// * `vault_directory` - The base directory of the vault
///
/// # Returns
/// * `Result<Vec<String>>` - A list of workspace names
///
/// # Errors
/// * Returns an error if the workspace directory can't be accessed
pub fn list_workspaces(vault_directory: &Path) -> Result<Vec<String>> {
    let workspaces_dir = vault_directory.join("workspaces");

    // If the workspaces directory doesn't exist, return an empty list
    if !workspaces_dir.exists() {
        return Ok(Vec::new());
    }

    let mut workspaces = Vec::new();

    // Read the directory entries
    for entry in
        std::fs::read_dir(&workspaces_dir).context("Failed to read workspaces directory")?
    {
        let entry = entry.context("Failed to access directory entry")?;
        let path = entry.path();

        // Check if it's a txt file
        if path.is_file() && path.extension().map_or(false, |ext| ext == "txt") {
            if let Some(file_stem) = path.file_stem() {
                if let Some(workspace_name) = file_stem.to_str() {
                    workspaces.push(workspace_name.to_string());
                }
            }
        }
    }

    Ok(workspaces)
}

/// Get the list of file paths in a workspace.
///
/// # Arguments
/// * `vault_directory` - The base directory of the vault
/// * `workspace_name` - The name of the workspace
///
/// # Returns
/// * `Result<Vec<String>>` - A list of file paths in the workspace
///
/// # Errors
/// * Returns an error if the workspace doesn't exist
/// * Returns an error if the workspace file can't be read
pub fn get_workspace_files(vault_directory: &Path, workspace_name: &str) -> Result<Vec<String>> {
    // Check if the workspace exists
    if !check_if_workspace_exists(vault_directory, workspace_name)? {
        return Err(anyhow::anyhow!(
            "Workspace '{}' does not exist",
            workspace_name
        ));
    }

    // Get the workspace file path
    let sanitized_name = sanitize_title(workspace_name);
    let workspace_file_path = vault_directory
        .join("workspaces")
        .join(format!("{}.txt", sanitized_name));

    // Read the file paths from the workspace file
    let content = std::fs::read_to_string(&workspace_file_path)
        .context(format!("Failed to read workspace file: {}", workspace_name))?;

    // Split the content by lines and collect non-empty lines
    let file_paths: Vec<String> = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_string())
        .collect();

    Ok(file_paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_check_if_workspace_exists() -> Result<()> {
        let temp_dir = tempdir()?;
        let vault_dir = temp_dir.path();

        // Create workspaces directory
        let workspaces_dir = vault_dir.join("workspaces");
        fs::create_dir_all(&workspaces_dir)?;

        // Create a test workspace file
        let workspace_name = "Test Workspace";
        let sanitized_name = sanitize_title(workspace_name);
        let workspace_file = workspaces_dir.join(format!("{}.txt", sanitized_name));

        // Initially the workspace should not exist
        assert!(!check_if_workspace_exists(vault_dir, workspace_name)?);

        // Create the workspace file
        File::create(&workspace_file)?;

        // Now the workspace should exist
        assert!(check_if_workspace_exists(vault_dir, workspace_name)?);

        Ok(())
    }

    #[test]
    fn test_list_workspaces() -> Result<()> {
        let temp_dir = tempdir()?;
        let vault_dir = temp_dir.path();

        // Create workspaces directory
        let workspaces_dir = vault_dir.join("workspaces");
        fs::create_dir_all(&workspaces_dir)?;

        // Initially there should be no workspaces
        let workspaces = list_workspaces(vault_dir)?;
        assert!(workspaces.is_empty());

        // Create some test workspace files
        let workspace_names = ["First Workspace", "Second Workspace", "Third-Workspace"];

        for name in &workspace_names {
            let sanitized_name = sanitize_title(name);
            let workspace_file = workspaces_dir.join(format!("{}.txt", sanitized_name));
            File::create(&workspace_file)?;
        }

        // Create a non-txt file (should be ignored)
        let non_txt_file = workspaces_dir.join("not-a-workspace.md");
        File::create(&non_txt_file)?;

        // Now there should be 3 workspaces
        let workspaces = list_workspaces(vault_dir)?;
        assert_eq!(workspaces.len(), 3);

        // Check if all workspace names are in the list (as sanitized names)
        for name in &workspace_names {
            let sanitized_name = sanitize_title(name);
            assert!(workspaces.contains(&sanitized_name));
        }

        Ok(())
    }

    #[test]
    fn test_get_workspace_files() -> Result<()> {
        let temp_dir = tempdir()?;
        let vault_dir = temp_dir.path();

        // Create workspaces directory
        let workspaces_dir = vault_dir.join("workspaces");
        fs::create_dir_all(&workspaces_dir)?;

        // Create a test workspace file with some paths
        let workspace_name = "Files Test";
        let sanitized_name = sanitize_title(workspace_name);
        let workspace_file = workspaces_dir.join(format!("{}.txt", sanitized_name));

        let test_paths = [
            "path/to/file1.md",
            "path/to/file2.md",
            "another/path/file3.md",
        ];

        let mut file = File::create(&workspace_file)?;
        for path in &test_paths {
            writeln!(file, "{}", path)?;
        }

        // Get the files in the workspace
        let files = get_workspace_files(vault_dir, workspace_name)?;

        // Check if all test paths are in the list
        assert_eq!(files.len(), test_paths.len());
        for path in &test_paths {
            assert!(files.contains(&path.to_string()));
        }

        // Test with non-existent workspace
        let result = get_workspace_files(vault_dir, "Non Existent");
        assert!(result.is_err());

        Ok(())
    }
}
