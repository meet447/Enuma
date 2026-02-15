# AnimeCLI

A super fast, lightweight, terminal-based anime watcher written in Rust.

## Features
- Search for anime.
- Browse episodes.
- Stream episodes directly in `mpv`.

## Requirements
- Rust (to build)
- `mpv` player (must be in your PATH)

## Usage
1. Build the project:
   ```bash
   cargo build --release
   ```
2. Run the binary:
   ```bash
   ./target/release/Enuma
   ```
3. Controls:
   - **Type**: Search query.
   - **Enter**: Search / Select / Play.
   - **Up/Down**: Navigate lists.
   - **Esc**: Go back/Quit.

## Note
Ensure you have `mpv` installed, or the player will not launch.
# Enuma
