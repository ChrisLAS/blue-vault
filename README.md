# BlueVault

A production-quality TUI application for managing Blu-ray "cold storage" archives on Linux.

![BlueVault](https://img.shields.io/badge/version-0.1.0-blue) ![Rust](https://img.shields.io/badge/rust-1.70%2B-orange) ![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-green)

![BlueVault Screenshot](bluevaulscreen1.jpg)

## Overview

BlueVault is a terminal-based application that helps you create long-term archives on Blu-ray discs. It provides a complete workflow from selecting folders to burning discs, with built-in verification, indexing, and search capabilities. The application features a retro 80s "phosphor green" terminal aesthetic that makes long archiving sessions comfortable and visually distinct.

### What It Does

BlueVault helps you:
- **Archive folders to Blu-ray** (BD-R/BD-RE discs)
- **Stage content** with a standard, mountable disc layout
- **Generate manifests and checksums** (MANIFEST.txt + SHA256SUMS.txt) for verification
- **Burn to Blu-ray** using standard Linux tools (xorriso, growisofs)
- **Maintain a searchable index** (SQLite) stored outside the repo
- **Search for files** to locate which disc contains them
- **Generate QR codes** for disc IDs (for printing stickers/spines)
- **Verify discs** and maintain verification history

## Visual Design

BlueVault features a distinctive retro 80s phosphor terminal aesthetic:

- **Phosphor Green Theme**: High-contrast green-on-black color scheme (#3CFF8A on #07110A)
- **Grid-aligned Layout**: Fixed margins and consistent gutters for stable, flicker-free rendering
- **Monospace Typography**: Professional, archival feel
- **Disc Activity Indicators**: 80s-style CD read/write animations with LBA counters
- **Subtle Animations**: Loading spinners and progress bars (throttled, non-blocking)
- **Accessibility**: Supports reduced motion, monochrome mode, and graceful terminal degradation

The theme system supports:
- **Phosphor** (default): Classic green phosphor CRT look
- **Amber** (optional): Warm amber terminal colors
- **Mono** (optional): High-contrast monochrome for accessibility

## Features

### Core Functionality

- ✅ **Dual-mode Directory Selection**: Manual path entry + visual directory browser
- ✅ **Disc Creation Workflow**: Complete flow from folder selection to burned disc
- ✅ **Centralized Index**: SQLite database with file metadata and search capabilities
- ✅ **Disc Verification**: Verify disc integrity using SHA256 checksums
- ✅ **QR Code Generation**: Generate QR codes for disc identification
- ✅ **Structured Logging**: Detailed logs for troubleshooting and audit trails

### User Interface

- ✅ **Main Menu**: Keyboard-driven navigation with arrow keys or vim bindings
- ✅ **New Disc Flow**: Step-by-step disc creation with progress indicators
- ✅ **Search Interface**: Real-time search through indexed files
- ✅ **Verify Disc Flow**: Interactive disc verification with mount/unmount
- ✅ **Settings Screen**: View and manage configuration
- ✅ **Log Viewer**: Browse application logs

### Safety & Robustness

- ✅ **Dependency Checking**: Validates required tools before operations
- ✅ **Safe Command Execution**: No shell injection vulnerabilities
- ✅ **Error Handling**: Comprehensive error reporting with context
- ✅ **Configuration Management**: TOML-based config stored outside repo
- ✅ **Path Validation**: Validates paths and handles edge cases

## Quick Start

### Prerequisites

**Required:**
- Linux (tested on NixOS)
- Rust 1.70+ and Cargo
- `xorriso` - ISO image creation
- `growisofs` - Blu-ray burning (from `dvd+rw-tools` package)
- `sha256sum` - Checksum verification (usually pre-installed)
- `mount/umount` - Disc mounting (usually pre-installed)

**Optional (but recommended):**
- `qrencode` - QR code generation
- `rsync` - Faster file staging

### Installation

#### Option 1: Using Nix Flake (Recommended for NixOS)

1. **Run directly:**
```bash
nix run github:ChrisLAS/blue-vault
```

2. **Build and install:**
```bash
# Build the package
nix build github:ChrisLAS/blue-vault

# Install to your user profile
nix profile install github:ChrisLAS/blue-vault

# Or if you have the repository cloned:
cd bluevault
nix profile install .
```

3. **Add to your system configuration (NixOS):**
```nix
environment.systemPackages = [
  (builtins.getFlake "github:ChrisLAS/blue-vault").packages.${system}.default
];
```

4. **Development environment:**
```bash
nix develop github:ChrisLAS/blue-vault
# or if cloned:
cd bluevault
nix develop
```

#### Option 2: Manual Build (Other Linux Distributions)

1. **Install system dependencies:**

```bash
# On Debian/Ubuntu:
sudo apt install xorriso dvd+rw-tools qrencode rsync

# On Fedora/RHEL:
sudo dnf install xorriso dvd+rw-tools qrencode rsync
```

2. **Clone and build:**

```bash
git clone <repository-url>
cd bluevault
cargo build --release
```

3. **Install (optional):**

```bash
sudo cp target/release/bdarchive /usr/local/bin/bdarchive
```

### First Run

Simply run:

```bash
cargo run
# or if installed:
bdarchive
```

On first run, BlueVault will:
- Check for required dependencies
- Create configuration directory: `~/.config/bdarchive/`
- Create data directory: `~/.local/share/bdarchive/`
- Initialize SQLite database: `~/.local/share/bdarchive/archive.db`
- Show a startup splash screen with system status

### Using the Application

1. **Navigate the menu** with `↑/↓` or `j/k`
2. **Select options** with `Enter`
3. **Go back** with `Esc` or `q`
4. **Tab** between input fields (in directory selector)

#### Creating a New Disc

1. Select "New Disc / Archive Folders" from the main menu
2. Enter or accept the auto-generated disc ID (e.g., `2024-BD-001`)
3. Add optional notes
4. Select source folders using:
   - **Input box**: Type full paths manually (default, always visible)
   - **Directory browser**: Tab to browser mode and navigate with `↑/↓`, press `Enter` to select
5. Review and confirm
6. The app will stage files, create ISO, and burn to disc

#### Searching the Index

1. Select "Search Index" from the main menu
2. Type your search query (searches file paths)
3. Results show: Disc ID, path, size, modification time
4. Navigate results with `↑/↓` or `j/k`

#### Verifying a Disc

1. Select "Verify Disc" from the main menu
2. Enter device path (default: `/dev/sr0`) or mountpoint
3. The app will mount (if needed) and verify SHA256SUMS.txt
4. Results are recorded in the database

## Configuration

Configuration is stored in `~/.config/bdarchive/config.toml`:

```toml
# Blu-ray device path
device = "/dev/sr0"

# Staging directory for building ISO
staging_dir = "/tmp/bdarchive_staging"

# Database path
database_path = "~/.local/share/bdarchive/archive.db"

# Default disc capacity (GB)
default_capacity_gb = 25

# Verification settings
[verification]
auto_verify_after_burn = false
auto_mount = false

# Optional tools
[optional_tools]
use_qrencode = true
use_rsync = true
```

## Disc Layout

Each disc follows a standard, mountable layout:

```
/
├── ARCHIVE/
│   ├── folder1/
│   │   └── ... (original structure)
│   └── folder2/
│       └── ... (original structure)
├── DISC_INFO.txt      # Disc metadata
├── MANIFEST.txt       # All file paths (one per line)
└── SHA256SUMS.txt     # SHA256 checksums (sha256sum format)
```

This layout is:
- **Mountable**: Standard ISO/UDF format, mounts on any Linux system
- **Browsable**: Standard directory structure, no proprietary formats
- **Verifiable**: Contains all metadata needed for long-term verification
- **Self-contained**: Each disc includes its own manifest and checksums

## Database Schema

The SQLite database (`~/.local/share/bdarchive/archive.db`) contains:

- **`discs`**: Disc metadata (ID, creation date, notes, volume label, etc.)
- **`files`**: File index (disc_id, path, SHA256, size, mtime)
- **`verification_runs`**: Verification history (disc_id, success, files checked, etc.)

The database is versioned with migrations for future schema changes.

## Project Structure

```
bluevault/
├── src/
│   ├── main.rs              # Application entry point, TUI orchestration
│   ├── lib.rs               # Library exports
│   ├── config.rs            # Configuration management (TOML)
│   ├── database.rs          # SQLite schema, migrations, queries
│   ├── manifest.rs          # Manifest + SHA256 generation
│   ├── staging.rs           # File staging logic
│   ├── disc.rs              # Disc layout, DISC_INFO.txt generation
│   ├── iso.rs               # ISO creation via xorriso
│   ├── burn.rs              # Burning via growisofs
│   ├── verify.rs            # Disc verification (sha256sum -c)
│   ├── qrcode.rs            # QR code generation
│   ├── search.rs            # Search functionality
│   ├── commands.rs          # Safe command execution
│   ├── dependencies.rs      # Dependency checking
│   ├── paths.rs             # Path utilities, XDG dirs
│   ├── logging.rs           # Structured logging
│   ├── theme.rs             # Theme system (phosphor/amber/mono)
│   ├── tui/                 # TUI components
│   │   ├── mod.rs
│   │   ├── main_menu.rs
│   │   ├── new_disc.rs      # New disc creation flow
│   │   ├── directory_selector_simple.rs  # Dual-mode folder selector
│   │   ├── search_ui.rs
│   │   ├── verify_ui.rs
│   │   ├── list_discs.rs
│   │   ├── settings.rs
│   │   ├── logs_view.rs
│   │   └── splash.rs        # Startup splash screen
│   └── ui/                  # UI utilities
│       ├── mod.rs
│       ├── layout.rs        # Grid-aligned layout helpers
│       ├── animations.rs    # Animation throttling
│       ├── disc_activity.rs # CD-style activity indicators
│       └── header_footer.rs # Consistent header/footer widgets
├── Cargo.toml
├── README.md
├── ARCHITECTURE.md          # Detailed architecture documentation
├── DEVELOPMENT.md           # Developer guide
├── Project.md               # Original project specification
└── RUNNING.md               # Detailed running instructions
```

## Safety Notes

⚠️ **WARNING**: Burning discs is destructive!

1. **Verify Device**: Always verify `/dev/sr0` (or your configured device) is your Blu-ray writer
   - Check with: `lsblk` or `ls -la /dev/sr*`
   - The app will use your configured device path

2. **Review Content**: Check the staged content before burning

3. **Backup Important Data**: Always maintain backups before archiving

4. **Verify After Burning**: Use the "Verify Disc" feature after burning important data

5. **Device Permissions**: You may need to add your user to the `cdrom` group:
   ```bash
   sudo usermod -a -G cdrom $USER
   newgrp cdrom  # or logout/login
   ```

## Logging

Logs are stored in `~/.local/share/bdarchive/logs/bdarchive-YYYY-MM-DD.log`.

View logs:
```bash
tail -f ~/.local/share/bdarchive/logs/bdarchive-$(date +%Y-%m-%d).log
```

Set log level:
```bash
RUST_LOG=debug bdarchive  # or: info, warn, error, trace
```

## Development

See [DEVELOPMENT.md](DEVELOPMENT.md) for detailed development instructions.

Quick development commands:

```bash
# Build
cargo build

# Run
cargo run

# Test
cargo test

# Check for issues
cargo clippy

# Format code
cargo fmt
```

## License

MIT OR Apache-2.0

## Contributing

Contributions are welcome! Please see [DEVELOPMENT.md](DEVELOPMENT.md) for development guidelines.

Key principles:
- Code must compile without warnings
- All tests must pass
- New features should include tests
- Follow Rust style guidelines
- Maintain the phosphor theme aesthetic

## Status

**Current Version**: 0.1.0

This is an early-stage project. Core functionality is implemented and working, but some features may be missing or incomplete. See the [Project.md](Project.md) for the full specification.

**Implemented:**
- ✅ Core TUI with phosphor theme
- ✅ Directory selection (manual + browser)
- ✅ Disc creation workflow
- ✅ Database indexing
- ✅ Search functionality
- ✅ Disc verification
- ✅ QR code generation
- ✅ Structured logging

**Planned:**
- 🔄 Multi-disc packing (currently warns on capacity)
- 🔄 Regex search (substring search works)
- 🔄 More detailed progress indicators
- 🔄 Resume support for interrupted operations

## Support

For issues, questions, or contributions, please open an issue on GitHub.

---

**Built with Rust ❤️ for long-term data preservation**
