pub mod utils;

/*
I want to reimplement crud.rs

create_note(title, vault_directory, project) should sanitize the title and use that as the file name. Join the project path string to the vault_directory to get the final folder where you will create the .md file with the sanitized title as the filename.

Before creating the file, check if the title is unique
*/
