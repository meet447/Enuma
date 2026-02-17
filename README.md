# AnimeCLI

A super fast, lightweight, terminal-based anime watcher written in Rust.

## Features
- Search for anime.
- Browse episodes.
- Stream episodes directly in `mpv`.

## Requirements
- `mpv` player (must be in your PATH)

## Installation

### Quick Install (Recommended)

**macOS/Linux:**
```bash
curl -sSL https://raw.githubusercontent.com/meet447/Enuma/main/install.sh | bash
```

**Windows (PowerShell):**
```powershell
iwr -useb https://raw.githubusercontent.com/meet447/Enuma/main/install.ps1 | iex
```

### Build from Source
```bash
cargo build --release
./target/release/Enuma
```

## Usage
3. Controls:
   - **Type**: Search query.
   - **Enter**: Search / Select / Play.
   - **Up/Down**: Navigate lists.
   - **Esc**: Go back/Quit.

## Note
Ensure you have `mpv` installed, or the player will not launch.
# Enuma
