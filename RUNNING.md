# Running BlueVault

## Quick Start

The simplest way to run the app is:

```bash
cd /home/chrisf/build/bluevault
cargo run
```

Or if you've already built it in release mode:

```bash
./target/release/bdarchive
```

## Prerequisites

Before running, make sure you have the required dependencies installed:

### Required Dependencies

```bash
# On Debian/Ubuntu:
sudo apt install xorriso dvd+rw-tools coreutils util-linux

# On Fedora/RHEL:
sudo dnf install xorriso dvd+rw-tools coreutils util-linux
```

### Optional Dependencies

These are optional but recommended:

```bash
# QR code generation
sudo apt install qrencode  # or: sudo dnf install qrencode

# Efficient file staging (faster than basic copy)
sudo apt install rsync     # or: sudo dnf install rsync

# File manager for folder selection
sudo apt install mc        # or: sudo dnf install mc
```

## Running Options

### Development Mode (with debug logging)

```bash
cargo run
```

This will:
- Build the app in debug mode (if needed)
- Run with debug symbols
- Show detailed logging

### Release Mode (optimized, faster)

```bash
# Build once:
cargo build --release

# Then run:
./target/release/bdarchive
```

Or combine:
```bash
cargo run --release
```

### With Debug Logging

To see detailed logs:

```bash
RUST_LOG=debug cargo run
# or
RUST_LOG=debug ./target/release/bdarchive
```

Log levels: `error`, `warn`, `info`, `debug`, `trace`

## First Run

On first run, the app will:

1. **Check dependencies** - Will error if required tools are missing
2. **Create directories**:
   - `~/.config/bdarchive/` - Configuration files
   - `~/.local/share/bdarchive/` - Database and data files
   - `~/.local/share/bdarchive/logs/` - Log files
   - `~/.local/share/bdarchive/qrcodes/` - Generated QR codes
3. **Create default config** - `~/.config/bdarchive/config.toml`
4. **Initialize database** - `~/.local/share/bdarchive/archive.db`

## Configuration

After first run, you can edit the config file:

```bash
nano ~/.config/bdarchive/config.toml
```

Key settings:
- `device` - Blu-ray device path (default: `/dev/sr0`)
- `staging_dir` - Where ISOs are built (default: `/tmp/bdarchive_staging`)
- `default_capacity_gb` - Disc capacity in GB (25 or 50)
- `verification.auto_verify_after_burn` - Auto-verify after burning
- `verification.auto_mount` - Auto-mount discs for verification

## Troubleshooting

### Permission Issues

If you see permission errors accessing `/dev/sr0`:

```bash
# Add yourself to the cdrom group
sudo usermod -a -G cdrom $USER

# Then logout and login again, or:
newgrp cdrom
```

### Missing Dependencies

If the app says dependencies are missing, install them:

```bash
# The app will tell you which ones are missing
# Then install them with your package manager
sudo apt install xorriso dvd+rw-tools  # or equivalent
```

### Terminal Issues

If the TUI doesn't display correctly:

1. Make sure you're running in a terminal (not a file manager)
2. Ensure your terminal supports UTF-8
3. Try resizing the terminal window

## Keyboard Controls

Once running:
- **↑/↓ or j/k** - Navigate menus
- **Enter** - Select/Confirm
- **Esc or q** - Go back/Quit
- **Tab** - Switch between input fields (in verify screen)

## Logs

Logs are automatically written to:
```
~/.local/share/bdarchive/logs/bdarchive-YYYY-MM-DD.log
```

View recent logs:
```bash
tail -f ~/.local/share/bdarchive/logs/bdarchive-$(date +%Y-%m-%d).log
```

## Installing System-Wide (Optional)

To install the app system-wide:

```bash
# Build release version
cargo build --release

# Install to /usr/local/bin
sudo cp target/release/bdarchive /usr/local/bin/

# Then you can run from anywhere:
bdarchive
```

## Testing Without Burning

The app doesn't have a built-in dry-run mode yet, but you can:
1. Use a test Blu-ray disc or DVD-RW for testing
2. Verify the ISO is created correctly before burning
3. Check logs for any errors

For actual testing, you might want to add a `--dry-run` flag in the future that:
- Creates ISO but doesn't burn
- Shows what commands would be run
- Tests the full workflow without disc interaction

