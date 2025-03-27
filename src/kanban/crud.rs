// src/kanban/crud.rs

use anyhow::{Context, Result, anyhow};
use chrono::prelude::*;
use regex::Regex;
use std::collections::HashMap;
use std::fs::{self, File, create_dir_all};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

/// Represents a Kanban board's metadata
#[derive(Debug, Clone)]
pub struct KanbanBoard {
    pub name: String,
    pub date: String,
    pub description: String,
    pub columns: Vec<String>,
    pub tasks: HashMap<String, Vec<Task>>,
}

/// Represents a task in a Kanban board
#[derive(Debug, Clone)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub priority: Option<String>,
    pub tags: Vec<String>,
    pub created: Option<String>,
    pub column: String,
    pub metadata: HashMap<String, String>, // For any additional metadata
}

impl Task {
    /// Create a new task with the given title
    pub fn new(id: &str, title: &str, column: &str) -> Self {
        Task {
            id: id.to_string(),
            title: title.to_string(),
            priority: None,
            tags: Vec::new(),
            created: Some(Local::now().format("%Y-%m-%d").to_string()),
            column: column.to_string(),
            metadata: HashMap::new(),
        }
    }

    /// Convert a task to its string representation in TKF format
    pub fn to_string(&self) -> String {
        let mut result = format!("* [ID:{}] {}", self.id, self.title);

        // Add priority if present
        if let Some(priority) = &self.priority {
            result.push_str(&format!(" | Priority: {}", priority));
        }

        // Add tags if present
        if !self.tags.is_empty() {
            result.push_str(&format!(" | Tags: {}", self.tags.join(", ")));
        }

        // Add created date if present
        if let Some(created) = &self.created {
            result.push_str(&format!(" | Created: {}", created));
        }

        // Add any additional metadata
        for (key, value) in &self.metadata {
            if key != "Priority" && key != "Tags" && key != "Created" {
                result.push_str(&format!(" | {}: {}", key, value));
            }
        }

        result
    }

    /// Parse a task from a TKF format string
    pub fn from_string(line: &str, column: &str) -> Result<Self> {
        // Extract ID and title
        let id_regex = Regex::new(r"\[ID:([^\]]+)\]").unwrap();
        let id = match id_regex.captures(line) {
            Some(caps) => caps.get(1).unwrap().as_str().to_string(),
            None => return Err(anyhow!("No task ID found in line: {}", line)),
        };

        // Extract the title (everything between the ID and the first pipe, or to the end)
        let title_regex = Regex::new(r"\[ID:[^\]]+\]\s*([^|]*)").unwrap();
        let title = match title_regex.captures(line) {
            Some(caps) => caps.get(1).unwrap().as_str().trim().to_string(),
            None => return Err(anyhow!("No task title found in line: {}", line)),
        };

        // Initialize the task
        let mut task = Task {
            id,
            title,
            priority: None,
            tags: Vec::new(),
            created: None,
            column: column.to_string(),
            metadata: HashMap::new(),
        };

        // Extract metadata (everything after the first pipe)
        if let Some(metadata_str) = line
            .split('|')
            .skip(1)
            .collect::<Vec<&str>>()
            .join("|")
            .trim()
            .to_string()
            .into()
        {
            for item in metadata_str.split('|') {
                let parts: Vec<&str> = item.split(':').collect();
                if parts.len() >= 2 {
                    let key = parts[0].trim();
                    let value = parts[1..].join(":").trim().to_string();

                    match key.to_lowercase().as_str() {
                        "priority" => task.priority = Some(value),
                        "tags" => {
                            task.tags = value.split(',').map(|s| s.trim().to_string()).collect()
                        }
                        "created" => task.created = Some(value),
                        _ => {
                            task.metadata.insert(key.to_string(), value);
                        }
                    }
                }
            }
        }

        Ok(task)
    }
}

/// Create a new Kanban board with the specified name and columns
pub fn create_board(
    board_name: &str,
    columns: &[&str],
    description: &str,
    kanban_directory: &Path,
) -> Result<PathBuf> {
    // Create the kanban directory if it doesn't exist
    create_dir_all(kanban_directory).context("Failed to create kanban directory")?;

    // Create the board file path
    let board_file = kanban_directory.join(format!("{}.tkf", sanitize_filename(board_name)));

    // Check if the board already exists
    if board_file.exists() {
        return Err(anyhow!(
            "A board with the name '{}' already exists",
            board_name
        ));
    }

    // Get the current date
    let current_date = Local::now().format("%Y-%m-%d").to_string();

    // Create the board content
    let mut content = format!(
        "# TUI Kanban Board: {}\nDate: {}\nDescription: {}\n\n",
        board_name, current_date, description
    );

    // Add column sections
    for column in columns {
        content.push_str(&format!("== {} ==\n", column));
    }

    // Write the content to the file
    fs::write(&board_file, content).context("Failed to write board file")?;

    Ok(board_file)
}

/// Read a Kanban board from a file
pub fn read_board(board_name: &str, kanban_directory: &Path) -> Result<KanbanBoard> {
    let board_file = kanban_directory.join(format!("{}.tkf", sanitize_filename(board_name)));

    if !board_file.exists() {
        return Err(anyhow!("Board '{}' not found", board_name));
    }

    let file = File::open(&board_file).context("Failed to open board file")?;
    let reader = BufReader::new(file);

    let mut board = KanbanBoard {
        name: board_name.to_string(),
        date: String::new(),
        description: String::new(),
        columns: Vec::new(),
        tasks: HashMap::new(),
    };

    let mut current_column: Option<String> = None;
    let date_regex = Regex::new(r"^Date:\s*(.+)$").unwrap();
    let desc_regex = Regex::new(r"^Description:\s*(.+)$").unwrap();
    let column_regex = Regex::new(r"^==\s*([^=]+)\s*==$").unwrap();
    let task_regex = Regex::new(r"^\*\s+.*$").unwrap();

    for line in reader.lines() {
        let line = line.context("Failed to read line")?;
        let trimmed_line = line.trim();

        if trimmed_line.is_empty() || trimmed_line.starts_with('#') {
            continue;
        }

        if let Some(caps) = date_regex.captures(trimmed_line) {
            board.date = caps.get(1).unwrap().as_str().trim().to_string();
            continue;
        }

        if let Some(caps) = desc_regex.captures(trimmed_line) {
            board.description = caps.get(1).unwrap().as_str().trim().to_string();
            continue;
        }

        if let Some(caps) = column_regex.captures(trimmed_line) {
            let column_name = caps.get(1).unwrap().as_str().trim().to_string();
            board.columns.push(column_name.clone());
            board.tasks.insert(column_name.clone(), Vec::new());
            current_column = Some(column_name);
            continue;
        }

        if let Some(column) = &current_column {
            if task_regex.is_match(trimmed_line) {
                match Task::from_string(trimmed_line, column) {
                    Ok(task) => {
                        if let Some(tasks) = board.tasks.get_mut(column) {
                            tasks.push(task);
                        }
                    }
                    Err(e) => {
                        eprintln!("Warning: Failed to parse task '{}': {}", trimmed_line, e);
                    }
                }
            }
        }
    }

    Ok(board)
}

/// Save a Kanban board to a file
pub fn save_board(board: &KanbanBoard, kanban_directory: &Path) -> Result<PathBuf> {
    let board_file = kanban_directory.join(format!("{}.tkf", sanitize_filename(&board.name)));

    let mut content = format!(
        "# TUI Kanban Board: {}\nDate: {}\nDescription: {}\n\n",
        board.name, board.date, board.description
    );

    for column in &board.columns {
        content.push_str(&format!("== {} ==\n", column));
        if let Some(tasks) = board.tasks.get(column) {
            for task in tasks {
                content.push_str(&format!("{}\n", task.to_string()));
            }
        }
        content.push('\n');
    }

    fs::write(&board_file, content).context("Failed to write board file")?;

    Ok(board_file)
}

/// Delete a Kanban board file
pub fn delete_board(board_name: &str, kanban_directory: &Path) -> Result<()> {
    let board_file = kanban_directory.join(format!("{}.tkf", sanitize_filename(board_name)));

    if !board_file.exists() {
        return Err(anyhow!("Board '{}' not found", board_name));
    }

    fs::remove_file(&board_file).context("Failed to delete board file")?;

    Ok(())
}

/// List all Kanban boards in the directory
pub fn list_boards(kanban_directory: &Path) -> Result<Vec<String>> {
    if !kanban_directory.exists() {
        return Ok(Vec::new());
    }

    let mut boards = Vec::new();

    for entry in fs::read_dir(kanban_directory).context("Failed to read kanban directory")? {
        let entry = entry.context("Failed to read directory entry")?;
        let path = entry.path();

        if path.is_file() && path.extension().map_or(false, |ext| ext == "tkf") {
            if let Some(filename) = path.file_stem() {
                if let Some(name) = filename.to_str() {
                    boards.push(name.to_string());
                }
            }
        }
    }

    Ok(boards)
}

/// Add a new task to a board
pub fn add_task(
    board_name: &str,
    title: &str,
    column: &str,
    priority: Option<&str>,
    tags: &[&str],
    kanban_directory: &Path,
) -> Result<Task> {
    let mut board = read_board(board_name, kanban_directory)?;

    // Check if the column exists
    if !board.columns.contains(&column.to_string()) {
        return Err(anyhow!(
            "Column '{}' not found in board '{}'",
            column,
            board_name
        ));
    }

    // Generate a new task ID
    let next_id = generate_next_id(&board)?;

    // Create the new task
    let mut task = Task::new(&next_id, title, column);

    // Set priority if provided
    if let Some(p) = priority {
        task.priority = Some(p.to_string());
    }

    // Add tags if provided
    task.tags = tags.iter().map(|&s| s.to_string()).collect();

    // Add the task to the board
    if let Some(tasks) = board.tasks.get_mut(column) {
        tasks.push(task.clone());
    }

    // Save the updated board
    save_board(&board, kanban_directory)?;

    Ok(task)
}

/// Get a task by ID
pub fn get_task(board_name: &str, task_id: &str, kanban_directory: &Path) -> Result<Task> {
    let board = read_board(board_name, kanban_directory)?;

    for (_, tasks) in &board.tasks {
        for task in tasks {
            if task.id == task_id {
                return Ok(task.clone());
            }
        }
    }

    Err(anyhow!(
        "Task with ID '{}' not found in board '{}'",
        task_id,
        board_name
    ))
}

/// Update an existing task
pub fn update_task(
    board_name: &str,
    task_id: &str,
    title: Option<&str>,
    column: Option<&str>,
    priority: Option<&str>,
    tags: Option<&[&str]>,
    kanban_directory: &Path,
) -> Result<Task> {
    let mut board = read_board(board_name, kanban_directory)?;

    // Find the task and remove it from its current column
    let mut found_task: Option<Task> = None;
    let mut current_column: Option<String> = None;

    for (col_name, tasks) in &mut board.tasks {
        let task_pos = tasks.iter().position(|t| t.id == task_id);
        if let Some(pos) = task_pos {
            found_task = Some(tasks.remove(pos));
            current_column = Some(col_name.clone());
            break;
        }
    }

    if found_task.is_none() || current_column.is_none() {
        return Err(anyhow!(
            "Task with ID '{}' not found in board '{}'",
            task_id,
            board_name
        ));
    }

    let mut task = found_task.unwrap();
    let current_col = current_column.unwrap();

    // Update the task properties if provided
    if let Some(t) = title {
        task.title = t.to_string();
    }

    if let Some(p) = priority {
        task.priority = Some(p.to_string());
    }

    if let Some(t) = tags {
        task.tags = t.iter().map(|&s| s.to_string()).collect();
    }

    // Determine the target column
    let target_column = match column {
        Some(col) => {
            if !board.columns.contains(&col.to_string()) {
                return Err(anyhow!(
                    "Column '{}' not found in board '{}'",
                    col,
                    board_name
                ));
            }
            col.to_string()
        }
        None => current_col.clone(),
    };

    // Update the task's column property
    task.column = target_column.clone();

    // Add the task to the target column
    if let Some(tasks) = board.tasks.get_mut(&target_column) {
        tasks.push(task.clone());
    } else {
        return Err(anyhow!(
            "Column '{}' not found in board '{}'",
            target_column,
            board_name
        ));
    }

    // Save the updated board
    save_board(&board, kanban_directory)?;

    Ok(task)
}

/// Move a task from one column to another
pub fn move_task(
    board_name: &str,
    task_id: &str,
    to_column: &str,
    kanban_directory: &Path,
) -> Result<Task> {
    update_task(
        board_name,
        task_id,
        None,
        Some(to_column),
        None,
        None,
        kanban_directory,
    )
}

/// Delete a task from the board
pub fn delete_task(board_name: &str, task_id: &str, kanban_directory: &Path) -> Result<()> {
    let mut board = read_board(board_name, kanban_directory)?;
    let mut task_found = false;

    for (_, tasks) in &mut board.tasks {
        let task_pos = tasks.iter().position(|t| t.id == task_id);
        if let Some(pos) = task_pos {
            tasks.remove(pos);
            task_found = true;
            break;
        }
    }

    if !task_found {
        return Err(anyhow!(
            "Task with ID '{}' not found in board '{}'",
            task_id,
            board_name
        ));
    }

    save_board(&board, kanban_directory)?;

    Ok(())
}

/// Add a new column to a board
pub fn add_column(
    board_name: &str,
    column_name: &str,
    kanban_directory: &Path,
) -> Result<KanbanBoard> {
    let mut board = read_board(board_name, kanban_directory)?;

    if board.columns.contains(&column_name.to_string()) {
        return Err(anyhow!(
            "Column '{}' already exists in board '{}'",
            column_name,
            board_name
        ));
    }

    board.columns.push(column_name.to_string());
    board.tasks.insert(column_name.to_string(), Vec::new());

    save_board(&board, kanban_directory)?;

    Ok(board)
}

/// Remove a column from a board (optionally moving tasks to another column)
pub fn remove_column(
    board_name: &str,
    column_name: &str,
    move_tasks_to: Option<&str>,
    kanban_directory: &Path,
) -> Result<KanbanBoard> {
    let mut board = read_board(board_name, kanban_directory)?;

    if !board.columns.contains(&column_name.to_string()) {
        return Err(anyhow!(
            "Column '{}' not found in board '{}'",
            column_name,
            board_name
        ));
    }

    // Check if there are tasks in the column
    let tasks_to_move = if let Some(tasks) = board.tasks.get(column_name) {
        tasks.clone()
    } else {
        Vec::new()
    };

    // If there are tasks and a target column is specified, move them
    if !tasks_to_move.is_empty() {
        if let Some(target_col) = move_tasks_to {
            if !board.columns.contains(&target_col.to_string()) {
                return Err(anyhow!(
                    "Target column '{}' not found in board '{}'",
                    target_col,
                    board_name
                ));
            }

            // Move tasks to the target column
            if let Some(target_tasks) = board.tasks.get_mut(target_col) {
                for mut task in tasks_to_move {
                    task.column = target_col.to_string();
                    target_tasks.push(task);
                }
            }
        } else if !tasks_to_move.is_empty() {
            return Err(anyhow!(
                "Column '{}' contains tasks. Specify a target column to move them or delete them first",
                column_name
            ));
        }
    }

    // Remove the column from the list and tasks map
    board.columns.retain(|c| c != column_name);
    board.tasks.remove(column_name);

    save_board(&board, kanban_directory)?;

    Ok(board)
}

/// Sanitize a string for use as a filename
fn sanitize_filename(name: &str) -> String {
    let name = name.trim();
    let re = Regex::new(r"[<>:/\\|?*\n\r\t\.]").unwrap();

    let sanitized = re.replace_all(name, "_").to_string();

    if sanitized.is_empty() {
        return "untitled".to_string();
    }

    sanitized
}

/// Generate the next task ID for a board
fn generate_next_id(board: &KanbanBoard) -> Result<String> {
    let mut max_id = 0;

    for (_, tasks) in &board.tasks {
        for task in tasks {
            if let Ok(id_num) = task.id.parse::<usize>() {
                if id_num > max_id {
                    max_id = id_num;
                }
            }
        }
    }

    Ok((max_id + 1).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_create_and_read_board() -> Result<()> {
        let temp_dir = tempdir()?;
        let board_name = "Test Board";
        let columns = vec!["To Do", "In Progress", "Done"];
        let description = "Test board description";

        let board_path = create_board(board_name, &columns, description, temp_dir.path())?;
        assert!(board_path.exists());

        let board = read_board(board_name, temp_dir.path())?;
        assert_eq!(board.name, board_name);
        assert_eq!(board.description, description);
        assert_eq!(board.columns, columns);

        Ok(())
    }

    #[test]
    fn test_add_and_get_task() -> Result<()> {
        let temp_dir = tempdir()?;
        let board_name = "Task Test Board";
        let columns = vec!["To Do", "In Progress", "Done"];
        let description = "Board for testing tasks";

        create_board(board_name, &columns, description, temp_dir.path())?;

        let task = add_task(
            board_name,
            "Test Task",
            "To Do",
            Some("High"),
            &["test", "example"],
            temp_dir.path(),
        )?;

        let retrieved_task = get_task(board_name, &task.id, temp_dir.path())?;
        assert_eq!(retrieved_task.title, "Test Task");
        assert_eq!(retrieved_task.priority, Some("High".to_string()));
        assert_eq!(retrieved_task.tags, vec!["test", "example"]);
        assert_eq!(retrieved_task.column, "To Do");

        Ok(())
    }

    #[test]
    fn test_update_task() -> Result<()> {
        let temp_dir = tempdir()?;
        let board_name = "Update Test Board";
        let columns = vec!["To Do", "In Progress", "Done"];

        create_board(board_name, &columns, "Test description", temp_dir.path())?;

        let task = add_task(
            board_name,
            "Original Task",
            "To Do",
            Some("Medium"),
            &["original"],
            temp_dir.path(),
        )?;

        let updated_task = update_task(
            board_name,
            &task.id,
            Some("Updated Task"),
            Some("In Progress"),
            Some("High"),
            Some(&["updated", "important"]),
            temp_dir.path(),
        )?;

        assert_eq!(updated_task.title, "Updated Task");
        assert_eq!(updated_task.column, "In Progress");
        assert_eq!(updated_task.priority, Some("High".to_string()));
        assert_eq!(updated_task.tags, vec!["updated", "important"]);

        Ok(())
    }

    #[test]
    fn test_move_task() -> Result<()> {
        let temp_dir = tempdir()?;
        let board_name = "Move Test Board";
        let columns = vec!["To Do", "In Progress", "Done"];

        create_board(board_name, &columns, "Test description", temp_dir.path())?;

        let task = add_task(
            board_name,
            "Task to Move",
            "To Do",
            None,
            &[],
            temp_dir.path(),
        )?;

        let moved_task = move_task(board_name, &task.id, "In Progress", temp_dir.path())?;

        assert_eq!(moved_task.column, "In Progress");

        Ok(())
    }

    #[test]
    fn test_delete_task() -> Result<()> {
        let temp_dir = tempdir()?;
        let board_name = "Delete Test Board";
        let columns = vec!["To Do", "In Progress", "Done"];

        create_board(board_name, &columns, "Test description", temp_dir.path())?;

        let task = add_task(
            board_name,
            "Task to Delete",
            "To Do",
            None,
            &[],
            temp_dir.path(),
        )?;

        delete_task(board_name, &task.id, temp_dir.path())?;

        // Verify the task no longer exists
        let result = get_task(board_name, &task.id, temp_dir.path());
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_task_from_string() -> Result<()> {
        let task_str =
            "* [ID:42] Sample task | Priority: High | Tags: test, example | Created: 2025-03-20";
        let task = Task::from_string(task_str, "To Do")?;

        assert_eq!(task.id, "42");
        assert_eq!(task.title, "Sample task");
        assert_eq!(task.priority, Some("High".to_string()));
        assert_eq!(task.tags, vec!["test", "example"]);
        assert_eq!(task.created, Some("2025-03-20".to_string()));
        assert_eq!(task.column, "To Do");

        Ok(())
    }

    #[test]
    fn test_add_and_remove_column() -> Result<()> {
        let temp_dir = tempdir()?;
        let board_name = "Column Test Board";
        let columns = vec!["To Do", "Done"];

        create_board(board_name, &columns, "Test description", temp_dir.path())?;

        // Add a new column
        let updated_board = add_column(board_name, "In Progress", temp_dir.path())?;
        assert_eq!(updated_board.columns, vec!["To Do", "Done", "In Progress"]);

        // Remove a column
        let final_board = remove_column(board_name, "In Progress", None, temp_dir.path())?;
        assert_eq!(final_board.columns, vec!["To Do", "Done"]);

        Ok(())
    }

    #[test]
    fn test_delete_board() -> Result<()> {
        let temp_dir = tempdir()?;
        let board_name = "Board to Delete";
        let columns = vec!["To Do", "In Progress", "Done"];

        create_board(board_name, &columns, "Test description", temp_dir.path())?;
        assert!(
            temp_dir
                .path()
                .join(format!("{}.tkf", sanitize_filename(board_name)))
                .exists()
        );

        delete_board(board_name, temp_dir.path())?;
        assert!(
            !temp_dir
                .path()
                .join(format!("{}.tkf", sanitize_filename(board_name)))
                .exists()
        );

        Ok(())
    }
}
