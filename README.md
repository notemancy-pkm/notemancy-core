# Notemancy-Core

Notemancy-Core is a lightweight library designed to help you manage your notes easily. It provides basic operations to create, read, update, and delete notes, and even offers simple AI features to generate text embeddings for improved note organization.

## Features

- **CRUD Operations:**  
  Easily create, read, update, and delete markdown notes. Each note includes a YAML frontmatter with details like title and date.

- **Configuration Management:**  
  Read your vault settings from a YAML file (`config.yaml`). This file defines where your notes (or vaults) are stored.

- **Utilities:**  
  List all your notes, extract titles, and work with your note files efficiently.

- **AI Integration:**  
  Generate sentence embeddings using a pre-trained transformer model. This helps with note searching and organization by understanding the content better.

## How It Works

Notemancy-Core uses a file system-based approach. Each note is a markdown file stored in a designated vault directory. The configuration file tells the library where these vaults are located. When you create a note, the library writes a markdown file with a title and the current date in its frontmatter. The AI module leverages a sentence transformer model to generate text embeddings.

## Getting Started

1. **Installation:**  
   Add Notemancy-Core to your `Cargo.toml` dependencies:
   ```toml
   [dependencies]
   notemancy-core = "0.1.0"

2. Usage Examples:

- **Creating a Note:**

```rust
use notemancy_core::crud::create_note;
create_note("main", "projects/ideas", "My First Note")?;
```

- **Reading a Note:**

```rust
use notemancy_core::crud::read_note;
let content = read_note("main", "projects/ideas/my-first-note.md", false)?;
println!("{}", content);

```

- **Listing Notes:**

```rust
use notemancy_core::utils::list_notes;
let notes = list_notes("main")?;
for note in notes {
    println!("{}: {}", note.relpath, note.title);
}
```

- **Generating Sentence Embeddings:**

```rust
use notemancy_core::sentence_transformer::generate_embedding;
let embeddings = generate_embedding("Hello, world!")?;
println!("{:?}", embeddings);
```
## Contributing

Contributions are always welcome! If you want to improve Notemancy-Core, here's how you can help:

- Fork the repository.
- Create a branch for your feature or bug fix.
- Commit your changes with clear messages.
- Submit a pull request describing your changes.

Please follow the existing code style and add tests when you contribute new features or fixes.

### License

This project is licensed under the MIT License. See the LICENSE file for details.

