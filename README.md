# Enuma

A fast, lightweight, terminal-based anime streaming CLI written in Rust.

<p align="center">
  <img src="https://img.shields.io/badge/Rust-000000?style=for-the-badge&logo=rust&logoColor=white" />
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows-blue?style=for-the-badge" />
</p>

## Features

- **Search** for anime titles instantly
- **Browse** episodes with an intuitive TUI
- **Stream** directly in `mpv` player
- **Cross-platform** - works on macOS, Linux, and Windows
- **Fast & Lightweight** - written in Rust for optimal performance
- **No Browser Required** - everything happens in your terminal

## Demo

![Demo](https://via.placeholder.com/800x400?text=Enuma+Demo+Coming+Soon)

## Requirements

- [mpv](https://mpv.io/) media player (must be in your PATH)

### Installing mpv

**macOS:**
```bash
brew install mpv
```

**Ubuntu/Debian:**
```bash
sudo apt-get install mpv
```

**Arch Linux:**
```bash
sudo pacman -S mpv
```

**Windows:**
Download from [mpv.io](https://mpv.io/installation/) or use `winget`:
```powershell
winget install mpv
```

## Installation

### Quick Install (Recommended)

**macOS / Linux:**
```bash
curl -sSL https://raw.githubusercontent.com/meet447/Enuma/main/install.sh | bash
```

**Windows (PowerShell - Admin):**
```powershell
iwr -useb https://raw.githubusercontent.com/meet447/Enuma/main/install.ps1 | iex
```

### Using Cargo

If you have Rust installed:
```bash
cargo install --git https://github.com/meet447/Enuma.git
```

### Build from Source

```bash
# Clone the repository
git clone https://github.com/meet447/Enuma.git
cd Enuma

# Build in release mode
cargo build --release

# Run the binary
./target/release/Enuma
```

## Usage

Simply run:
```bash
enuma
```

### Controls

| Key | Action |
|-----|--------|
| `Type` | Search for anime |
| `↑ / ↓` | Navigate lists |
| `Enter` | Select / Play episode |
| `Esc` | Go back / Quit |

## Updating

To update to the latest version, simply run the install command again:

```bash
# macOS / Linux
curl -sSL https://raw.githubusercontent.com/meet447/Enuma/main/install.sh | bash

# Windows
iwr -useb https://raw.githubusercontent.com/meet447/Enuma/main/install.ps1 | iex
```

## Uninstallation

```bash
# Remove the binary
rm $(which enuma)

# Or if installed via cargo
cargo uninstall enuma
```

## How It Works

Enuma fetches anime data from online sources and provides a terminal-based interface for browsing and selecting content. When you choose to watch an episode, it streams directly to `mpv` player.

## Troubleshooting

**"mpv not found" error:**
- Make sure mpv is installed and in your system PATH
- Try running `mpv --version` to verify

**Stream not loading:**
- Check your internet connection
- Some content may be region-restricted

**Installation issues:**
- Ensure you have proper permissions to write to the install directory
- On Windows, run PowerShell as Administrator

## Contributing

Contributions are welcome! Feel free to:
- Report bugs
- Suggest features
- Submit pull requests

## License

This project is licensed under the MIT License.

## Acknowledgments

Built with:
- [Ratatui](https://github.com/ratatui-org/ratatui) - Terminal UI library
- [Tokio](https://tokio.rs/) - Async runtime
- [reqwest](https://github.com/seanmonstar/reqwest) - HTTP client

---

<p align="center">Made with ❤️ in Rust</p>
