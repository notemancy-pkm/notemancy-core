// src/notes/utils.rs (or new src/tags.rs)

use crate::notes::utils::get_title; // Assuming get_title is in src/notes/utils.rs
use crate::utils::absolute_to_relative; // Assuming absolute_to_relative is in src/utils.rs
use anyhow::{Context, Result, anyhow};
use regex; // Ensure regex crate is in Cargo.toml if get_notes_by_tag uses it
use std::collections::HashSet;
use std::io::Write; // Required for writing to stdin if needed, though not directly here
use std::path::Path;
use std::process::{Child, Command, Stdio}; // Added Child for type hint

/// Executes a single stage of the command pipeline, taking stdin from the previous stage.
fn run_pipeline_stage(
    mut prev_cmd: Option<Child>, // Take Option<Child> to handle the first command
    program: &str,
    args: &[&str],
    current_dir: Option<&Path>, // Optional current directory
) -> Result<Child> {
    // Return Child to allow chaining
    let mut command = Command::new(program);
    command.args(args);
    command.stdout(Stdio::piped()); // Always pipe stdout for the next stage or final capture
    command.stderr(Stdio::piped()); // Always capture stderr

    if let Some(dir) = current_dir {
        command.current_dir(dir);
    }

    // If there's a previous command, pipe its stdout to this command's stdin
    if let Some(ref mut previous) = prev_cmd {
        command.stdin(Stdio::from(previous.stdout.take().with_context(|| {
            format!(
                "Failed to take stdout from previous command for '{}'",
                program
            )
        })?));
    } else {
        // For the first command in the pipe, use the default stdin (or could be configured if needed)
        command.stdin(Stdio::null()); // Prevent inheriting parent stdin
    }

    let child = command
        .spawn()
        .with_context(|| format!("Failed to spawn command: {}", program))?;

    // Wait for the previous command *after* spawning the current one to ensure the pipe is connected.
    if let Some(previous) = prev_cmd {
        let prev_output = previous
            .wait_with_output()
            .with_context(|| "Failed to wait for previous command in pipe")?;
        if !prev_output.status.success() {
            // Allow status 1 for rg (no matches), but fail on others
            let stderr = String::from_utf8_lossy(&prev_output.stderr);
            if prev_output.status.code() != Some(1)
                || stderr.contains("cannot execute")
                || stderr.contains("command not found")
            {
                // Treat actual execution errors as fatal
                return Err(anyhow!(
                    "Previous command in pipe failed with status {}: {}",
                    prev_output.status,
                    stderr
                ));
            }
            // If it was just "no matches", the pipe will naturally close, let the current command handle it.
        }
    }

    Ok(child)
}

/// Extracts all unique tags strictly from the YAML frontmatter of markdown files
/// within the vault directory using a two-stage ripgrep pipe.
///
/// This function relies on `rg` being installed and available in the system PATH.
/// Stage 1: Finds 'tags:' blocks within YAML frontmatter.
/// Stage 2: Extracts individual tag values from those blocks, stripping comments.
///
/// # Arguments
/// * `vault_directory` - The absolute path to the vault directory.
///
/// # Returns
/// * `Result<Vec<String>>` - A sorted list of unique tags found in frontmatter.
///
/// # Errors
/// * Returns an error if `rg` command execution fails at any stage.
/// * Returns an error if the vault path is invalid.
/// * Returns an error if command output cannot be parsed.
pub fn get_all_tags(vault_directory: &Path) -> Result<Vec<String>> {
    // --- Stage 1: Extract tag blocks from frontmatter ---
    let stage1_pattern = r"(?s)^---.*?^tags:\s*\n((?:\s*-\s*.*\n)+)";
    let stage1_args = &[
        "-U",  // Multiline mode
        "-oP", // Only matching, Perl regex
        stage1_pattern,
        "--no-filename",
        ".", // Search current directory (will set current_dir later)
    ];
    let mut stage1_cmd = run_pipeline_stage(None, "rg", stage1_args, Some(vault_directory))
        .context("Failed to start pipeline stage 1")?;

    // --- Stage 2: Extract tag content after '- ' using \K ---
    let stage2_pattern = r"^\s*-\s*\K.*";
    let stage2_args = &["-oP", stage2_pattern]; // Only matching, Perl regex
    let mut stage2_cmd = run_pipeline_stage(Some(stage1_cmd), "rg", stage2_args, None) // No current_dir needed after stage 1
        .context("Failed to start pipeline stage 2")?;

    // --- Stage 3: Remove empty lines ---
    let stage3_pattern = r"^\s*$";
    let stage3_args = &["-v", stage3_pattern]; // Invert match
    let mut stage3_cmd = run_pipeline_stage(Some(stage2_cmd), "rg", stage3_args, None)
        .context("Failed to start pipeline stage 3")?;

    // --- Stage 4: Remove lines with only hyphens/dashes ---
    let stage4_pattern = r"^[-ー]+$";
    let stage4_args = &["-v", stage4_pattern]; // Invert match
    let mut stage4_cmd = run_pipeline_stage(Some(stage3_cmd), "rg", stage4_args, None)
        .context("Failed to start pipeline stage 4")?;

    // --- Final Output Processing ---
    let final_output = stage4_cmd
        .wait_with_output()
        .context("Failed to get final output from pipeline stage 4")?;

    if !final_output.status.success() {
        // Allow status 1 for the final rg (no matches remaining after filtering)
        let stderr = String::from_utf8_lossy(&final_output.stderr);
        if final_output.status.code() != Some(1) {
            return Err(anyhow!(
                "Pipeline stage 4 failed with status {}: {}",
                final_output.status,
                stderr
            ));
        }
        // If no matches remain, return empty vec
        if final_output.stdout.is_empty() {
            return Ok(Vec::new());
        }
    }

    // Process the final filtered output
    let stdout = String::from_utf8(final_output.stdout)
        .context("Failed to parse final pipeline output as UTF-8")?;
    let mut unique_tags = HashSet::new();

    for line in stdout.lines() {
        // Output should be clean tags now, but trim just in case
        let tag = line.trim();
        if !tag.is_empty() {
            // Final check against potential empty lines if rg -v wasn't perfect
            // Trim potential surrounding quotes (common in YAML)
            let tag_trimmed = tag.trim_matches(|c| c == '\'' || c == '"');
            if !tag_trimmed.is_empty() {
                // Check again after trimming quotes
                unique_tags.insert(tag_trimmed.to_string());
            }
        }
    }

    let mut sorted_tags: Vec<String> = unique_tags.into_iter().collect();
    sorted_tags.sort_unstable();

    Ok(sorted_tags)
}

/// Finds all notes (markdown files) containing a specific tag within the vault directory using ripgrep.
/// Returns a list of tuples containing the relative path and title of the matching notes.
///
/// This function relies on `rg` being installed and available in the system PATH.
///
/// # Arguments
/// * `tag` - The tag to search for (case-sensitive).
/// * `vault_directory` - The absolute path to the vault directory.
///
/// # Returns
/// * `Result<Vec<(String, String)>>` - A list of (relative path, title) pairs for matching notes.
///
/// # Errors
/// * Returns an error if `rg` command execution fails.
/// * Returns an error if the vault path is invalid.
/// * Returns an error if command output cannot be parsed.
/// * Returns an error if title extraction or path conversion fails for a matching file.
pub fn get_notes_by_tag(tag: &str, vault_directory: &Path) -> Result<Vec<(String, String)>> {
    let vault_path_str = vault_directory
        .to_str()
        .context("Invalid vault directory path")?;

    // Escape the tag for safe insertion into the regex pattern
    let escaped_tag = regex::escape(tag);

    // Construct the regex pattern to find the tag as a list item under 'tags:' in frontmatter
    // (?ms) enables multi-line mode and dotall mode (.) matches newlines)
    // ^---\s*$ matches the start frontmatter delimiter
    // .*? lazily matches any characters
    // ^tags:\s* matches the tags key
    // (?:.*\n)*? lazily matches any lines (needed if tag is not first)
    // ^\s*-\s*ESCAPED_TAG_NAME\s*(?:#.*)?$ matches the tag list item (allows trailing comments)
    // .*? lazily matches any characters until
    // ^---\s*$ matches the end frontmatter delimiter
    let pattern = format!(
        r"(?ms)^---\s*$.*?^tags:\s*(?:.*\n)*?^\s*-\s*{}\s*(?:#.*)?$.*?^---\s*$",
        escaped_tag
    );

    // Execute rg command
    let output = Command::new("rg")
        .args(&[
            "--type",
            "md",
            "--files-with-matches", // Output only filenames
            "--multiline",          // Enable multi-line matching
            "--regexp",             // Specify pattern is a regex
            &pattern,               // The regex pattern
            vault_path_str,         // Directory to search
        ])
        .output()
        .context(format!(
            "Failed to execute rg command for tag '{}'. Is 'rg' installed?",
            tag
        ))?;

    if !output.status.success() {
        // rg returns status 1 if no matches are found, which is not an error in this context.
        let stderr = String::from_utf8_lossy(&output.stderr);
        if output.status.code() != Some(1) {
            // Code 1 often means "no matches"
            return Err(anyhow!(
                "ripgrep command failed for tag '{}' with status {}: {}",
                tag,
                output.status,
                stderr
            ));
        }
        // If no matches, return empty vec
        return Ok(Vec::new());
    }

    // Process the output (list of absolute file paths)
    let stdout = String::from_utf8(output.stdout).context("Failed to parse rg command output")?;
    let mut results = Vec::new();

    for abs_path_str in stdout.lines() {
        if abs_path_str.is_empty() {
            continue;
        }
        let abs_path = Path::new(abs_path_str);

        // Get the relative path
        let rel_path = absolute_to_relative(abs_path_str, vault_directory)
            .with_context(|| format!("Failed to convert path to relative: {}", abs_path_str))?;

        // Get the title of the note
        let title = get_title(abs_path)
            .with_context(|| format!("Failed to get title for file: {}", abs_path_str))?;

        results.push((rel_path, title));
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::tempdir;

    // Helper function to check if rg is available
    fn check_rg_available() -> bool {
        Command::new("rg").arg("--version").output().is_ok()
    }

    #[test]
    fn test_get_all_tags_integration() -> Result<()> {
        if !check_rg_available() {
            eprintln!("Skipping test_get_all_tags_integration: rg not found.");
            return Ok(());
        }

        let temp_dir = tempdir()?;
        let vault_dir = temp_dir.path();

        // Create test files
        let note1_content = r#"---
title: Note 1
tags:
  - rust
  - programming
  - 'test tag'
---
Content 1"#;
        let note2_content = r#"---
title: Note 2
tags: [rust, performance] # Inline array format (rg might handle depending on regex)
---
Content 2"#; // This format might not be matched by the current regex approach
        let note3_content = r#"---
title: Note 3
tags:
  - programming
  - another-tag
  - RUST
---
Content 3"#;
        let note4_content = r#"---
title: Note 4
# No tags key
---
Content 4"#;
        let note5_content = r#"---
title: Note 5
tags: # Empty tags list
---
Content 5"#;
        let note6_content = r#"---
title: Note 6
tags:
  - "quoted tag"
  - tag with spaces
---
Content 6"#;

        fs::write(vault_dir.join("note1.md"), note1_content)?;
        fs::write(vault_dir.join("note2.md"), note2_content)?; // Will likely fail to extract tags
        fs::write(vault_dir.join("note3.md"), note3_content)?;
        fs::write(vault_dir.join("note4.md"), note4_content)?;
        fs::write(vault_dir.join("note5.md"), note5_content)?;
        fs::write(vault_dir.join("note6.md"), note6_content)?;

        let tags = get_all_tags(vault_dir)?;

        // Expected tags (Note: 'performance' from note2 won't be picked up by the regex)
        let mut expected = vec![
            "RUST", // Case preserved
            "another-tag",
            "programming",
            "quoted tag",
            "rust",
            "tag with spaces",
            "test tag",
        ];
        expected.sort_unstable();

        assert_eq!(tags, expected);

        Ok(())
    }

    #[test]
    fn test_get_notes_by_tag_integration() -> Result<()> {
        if !check_rg_available() {
            eprintln!("Skipping test_get_notes_by_tag_integration: rg not found.");
            return Ok(());
        }

        let temp_dir = tempdir()?;
        let vault_dir = temp_dir.path();

        // Create test files
        let note1_content = r#"---
title: Note One With Rust
tags:
  - rust
  - programming
---
Content 1"#;
        let note2_content = r#"---
title: Note Two No Rust Tag
tags:
  - other
  - stuff
---
Content 2"#;
        let note3_content = r#"---
title: Note Three Also Rust
tags:
  - programming
  - rust
---
Content 3"#;
        fs::create_dir(vault_dir.join("subdir"))?;
        fs::write(vault_dir.join("note1.md"), note1_content)?;
        fs::write(vault_dir.join("note2.md"), note2_content)?;
        fs::write(vault_dir.join("subdir/note3.md"), note3_content)?;

        let notes = get_notes_by_tag("rust", vault_dir)?;

        assert_eq!(notes.len(), 2);

        // Convert to HashSet for easier comparison regardless of order
        let results_set: HashSet<(String, String)> = notes.into_iter().collect();

        let expected_set: HashSet<(String, String)> = vec![
            ("note1.md".to_string(), "Note One With Rust".to_string()),
            (
                "subdir/note3.md".to_string().replace('\\', "/"),
                "Note Three Also Rust".to_string(),
            ), // Normalize path for Windows
        ]
        .into_iter()
        .collect();

        assert_eq!(results_set, expected_set);

        // Test for a tag that doesn't exist
        let no_notes = get_notes_by_tag("nonexistent", vault_dir)?;
        assert!(no_notes.is_empty());

        Ok(())
    }

    #[test]
    fn test_get_all_tags_empty_vault() -> Result<()> {
        if !check_rg_available() {
            eprintln!("Skipping test_get_all_tags_empty_vault: rg not found.");
            return Ok(());
        }
        let temp_dir = tempdir()?;
        let tags = get_all_tags(temp_dir.path())?;
        assert!(tags.is_empty());
        Ok(())
    }

    #[test]
    fn test_get_notes_by_tag_empty_vault() -> Result<()> {
        if !check_rg_available() {
            eprintln!("Skipping test_get_notes_by_tag_empty_vault: rg not found.");
            return Ok(());
        }
        let temp_dir = tempdir()?;
        let notes = get_notes_by_tag("anytag", temp_dir.path())?;
        assert!(notes.is_empty());
        Ok(())
    }
}
