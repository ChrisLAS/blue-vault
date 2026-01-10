# BlueVault Architecture

## Project Structure

```
bluevault/
├── Cargo.toml
├── Cargo.lock
├── src/
│   ├── main.rs                 # Entry point, TUI orchestration, event loop
│   ├── lib.rs                  # Library exports
│   ├── config.rs               # Configuration management (TOML)
│   ├── database.rs             # SQLite schema, migrations, queries
│   ├── manifest.rs             # Manifest + SHA256 generation
│   ├── staging.rs              # File staging logic, multi-disc planning
│   ├── disc.rs                 # Disc layout, DISC_INFO.txt generation
│   ├── iso.rs                  # ISO creation via xorriso
│   ├── burn.rs                 # Burning via growisofs
│   ├── verify.rs               # Disc verification (sha256sum -c)
│   ├── qrcode.rs               # QR code generation (optional qrencode)
│   ├── search.rs               # Search functionality
│   ├── commands.rs             # Safe command execution (no shell injection)
│   ├── dependencies.rs         # Dependency checking
│   ├── paths.rs                # Path normalization, XDG dirs
│   ├── logging.rs              # Structured logging
│   ├── theme.rs                # Theme system (phosphor/amber/mono)
│   ├── tui/                    # TUI screens
│   │   ├── mod.rs
│   │   ├── main_menu.rs        # Main menu screen
│   │   ├── new_disc.rs         # New disc creation flow
│   │   ├── directory_selector_simple.rs  # Dual-mode directory selector
│   │   ├── search_ui.rs        # Search interface
│   │   ├── verify_ui.rs        # Verify disc interface
│   │   ├── list_discs.rs       # List all discs
│   │   ├── settings.rs         # Settings management
│   │   ├── logs_view.rs        # Log viewer
│   │   └── splash.rs           # Startup splash screen
│   └── ui/                     # UI utilities
│       ├── mod.rs
│       ├── layout.rs           # Grid-aligned layout helpers
│       ├── animations.rs       # Animation throttling, spinners
│       ├── disc_activity.rs    # CD-style read/write indicators
│       └── header_footer.rs    # Consistent header/footer widgets
├── tests/                      # Unit and integration tests
├── README.md                   # GitHub front page
├── ARCHITECTURE.md             # This file - detailed architecture
├── DEVELOPMENT.md              # Developer guide and context
├── CONTRIBUTING.md             # Contribution guidelines
├── Project.md                  # Original project specification
├── RUNNING.md                  # Detailed running instructions
└── DIRECTORY_SELECTION_OPTIONS.md  # Research on directory selection
```

## Key Modules

### config.rs
- Load/save TOML config from `~/.config/bdarchive/config.toml`
- Defaults: device=/dev/sr0, staging_dir, db_path, capacity=25GB, verification flags
- Validate paths and create directories if needed

### database.rs
- SQLite connection management
- Schema migrations (versioned)
- Tables: `discs`, `files`, `verification_runs`
- CRUD operations for disc and file records

### manifest.rs
- Walk directory tree, collect files
- Generate MANIFEST.txt (one path per line)
- Generate SHA256SUMS.txt (sha256sum format)
- Return file metadata (size, mtime, sha256)

### staging.rs
- Copy files to staging directory preserving structure
- Check capacity before staging
- Map source folders to /ARCHIVE/<name> layout
- Handle errors and resume capability
- Auto-cleanup staging directory after successful/failed burns

### disc.rs
- Generate DISC_INFO.txt with metadata
- Assemble complete disc layout structure
- Disc ID generation (YYYY-BD-###)

### iso.rs
- Build xorriso command arguments safely
- Create ISO image with UDF filesystem
- Set volume label
- Handle large files (>4GB support)

### burn.rs
- Build growisofs command arguments safely
- Burn ISO to Blu-ray device
- Monitor progress (if possible)
- Handle errors gracefully

### verify.rs
- Mount/unmount disc (with user confirmation)
- Run sha256sum -c SHA256SUMS.txt
- Parse verification results
- Store results in verification_runs table

### qrcode.rs
- Check for qrencode availability
- Generate QR code PNG/SVG for disc ID
- Store in user data directory
- Render ASCII QR in terminal (optional)

### search.rs
- Search files by substring, exact filename, sha256, regex
- Return results with disc_id, path, size, mtime
- Pagination for large result sets

### commands.rs
- Safe command execution (std::process::Command)
- Validate paths and arguments
- Prevent shell injection
- Support dry-run mode (print commands)

### dependencies.rs
- Check for required tools: xorriso, growisofs, sha256sum, mount, umount
- Check for optional tools: qrencode, rsync, mc
- Provide helpful error messages with installation hints

### paths.rs
- XDG directory resolution (~/.local/share/bdarchive, ~/.config/bdarchive)
- Path normalization (canonicalize, handle symlinks)
- Safe path validation

### logging.rs
- Structured logging to file (~/.local/share/bdarchive/logs/)
- Log rotation (daily or size-based)
- Log levels and formatting

### tui/
- Ratatui-based TUI components
- State management for each screen
- Keyboard navigation (vim keys, arrow keys)
- Universal quit key ('Q') works from all screens
- Progress indicators for long operations

## Database Schema

### discs table
```sql
CREATE TABLE discs (
    disc_id TEXT PRIMARY KEY,              -- e.g., "2026-BD-ARCHIVE-001"
    volume_label TEXT NOT NULL,            -- ISO volume label (BDARCHIVE_2026D001_OF_003)
    created_at TEXT NOT NULL,              -- ISO 8601 timestamp
    notes TEXT,                            -- User notes (includes set info for multi-disc)
    iso_size INTEGER,                      -- ISO size in bytes
    burn_device TEXT,                      -- Device path used
    checksum_manifest_hash TEXT,           -- SHA256 of MANIFEST.txt
    qr_path TEXT,                          -- Path to QR code image
    source_roots TEXT,                     -- JSON array of source paths
    tool_version TEXT,                     -- App version used
    set_id TEXT,                           -- Multi-disc set identifier (NULL for single discs)
    sequence_number INTEGER                -- Position in multi-disc set (NULL for single discs)
);

CREATE INDEX idx_discs_created_at ON discs(created_at);
```

### files table
```sql
CREATE TABLE files (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    disc_id TEXT NOT NULL,                 -- Foreign key to discs
    rel_path TEXT NOT NULL,                -- Relative path on disc
    sha256 TEXT NOT NULL,                  -- SHA256 hash
    size INTEGER NOT NULL,                 -- File size in bytes
    mtime TEXT NOT NULL,                   -- ISO 8601 modification time
    added_at TEXT NOT NULL,                -- ISO 8601 when indexed
    FOREIGN KEY (disc_id) REFERENCES discs(disc_id) ON DELETE CASCADE
);

CREATE INDEX idx_files_disc_id ON files(disc_id);
CREATE INDEX idx_files_rel_path ON files(rel_path);
CREATE INDEX idx_files_sha256 ON files(sha256);
CREATE INDEX idx_files_disc_path ON files(disc_id, rel_path);
```

### verification_runs table
```sql
CREATE TABLE verification_runs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    disc_id TEXT NOT NULL,                 -- Foreign key to discs
    verified_at TEXT NOT NULL,             -- ISO 8601 timestamp
    mountpoint TEXT,                       -- Where disc was mounted
    device TEXT,                           -- Device path
    success INTEGER NOT NULL,              -- 0 = failed, 1 = success
    error_message TEXT,                    -- Error if failed
    files_checked INTEGER,                 -- Number of files checked
    files_failed INTEGER,                  -- Number of files that failed
    FOREIGN KEY (disc_id) REFERENCES discs(disc_id) ON DELETE CASCADE
);

CREATE INDEX idx_verification_disc_id ON verification_runs(disc_id);
CREATE INDEX idx_verification_verified_at ON verification_runs(verified_at);
```

## Multi-Disc Processing

### Planning Algorithm

BlueVault uses a greedy bin-packing algorithm to distribute files across multiple Blu-ray discs while preserving directory integrity when possible:

1. **Directory Analysis**: Recursively analyze source folders to build a tree of `DirectoryEntry` structures
2. **Size Calculation**: Compute total sizes for all files and directories
3. **Greedy Packing**: Sort entries by size (largest first) and pack into discs using these rules:
   - Try to fit entire directories first (preserves structure)
   - Split directories at subdirectory boundaries only when necessary
   - Fill remaining space with individual files
4. **Sequential Naming**: Generate disc IDs like `2026-BD-ARCHIVE-001`, `2026-BD-ARCHIVE-002`, etc.
5. **Database Tracking**: Store set relationships using `set_id` and `sequence_number` fields

### Processing Flow

```
User selects folders → Size calculation → Multi-disc planning → Sequential burning
                                      ↓
                            If exceeds capacity: plan_disc_layout_with_progress()
                                      ↓
                            Generate DiscPlan[] with file assignments
                                      ↓
                            For each disc: stage → create ISO → burn → index
```

### Key Components

- **`staging::plan_disc_layout_with_progress()`**: Main planning function with progress callbacks
- **`staging::analyze_directory_structure()`**: Recursive directory analysis
- **`DiscPlan`**: Represents content and layout for a single disc
- **`DirectoryEntry`**: File/directory metadata with size and children
- **Database Multi-disc Ops**: Set management and relationship tracking

## Disc Layout

Each disc follows this structure:

```
/
├── ARCHIVE/
│   ├── folder1/
│   │   └── ... (original structure)
│   └── folder2/
│       └── ... (original structure)
├── DISC_INFO.txt
├── MANIFEST.txt
└── SHA256SUMS.txt
```

### DISC_INFO.txt format
```
Disc-ID: 2024-BD-001
Created: 2024-01-15T10:30:00Z
Notes: Backup of project archives
Source Roots:
  /home/user/documents
  /home/user/photos
Tool Version: 1.0.0
Volume Label: BDARCHIVE_2024_BD_001
```

### MANIFEST.txt format
```
ARCHIVE/folder1/file1.txt
ARCHIVE/folder1/subdir/file2.pdf
ARCHIVE/folder2/image.jpg
DISC_INFO.txt
MANIFEST.txt
SHA256SUMS.txt
```

### SHA256SUMS.txt format
```
<sha256_hash>  ARCHIVE/folder1/file1.txt
<sha256_hash>  ARCHIVE/folder1/subdir/file2.pdf
<sha256_hash>  ARCHIVE/folder2/image.jpg
<sha256_hash>  DISC_INFO.txt
<sha256_hash>  MANIFEST.txt
<sha256_hash>  SHA256SUMS.txt
```

## Configuration File

Location: `~/.config/bdarchive/config.toml`

```toml
# Blu-ray device (default /dev/sr0)
device = "/dev/sr0"

# Staging directory for building ISO
staging_dir = "/tmp/bdarchive_staging"

# Database path (default ~/.local/share/bdarchive/archive.db)
database_path = "~/.local/share/bdarchive/archive.db"

# Default disc capacity in GB (25 or 50)
default_capacity_gb = 25

# Verification settings
[verification]
auto_verify_after_burn = false
auto_mount = false

# Optional tools
[optional_tools]
use_qrencode = true
use_rsync = true
use_mc = true
```

## Data Directory Structure

```
~/.local/share/bdarchive/
├── archive.db                  # SQLite database
├── logs/
│   ├── bdarchive-2024-01-15.log
│   └── ...
└── qrcodes/
    ├── 2024-BD-001.png
    └── ...
```

## Error Handling Strategy

- Use Result types throughout
- Structured error types with context
- Log all errors with context
- User-friendly error messages in TUI
- Dry-run mode for safety
- Confirmation prompts for destructive operations

## Testing Strategy

- Unit tests for pure functions (manifest generation, path normalization)
- Integration tests for database operations
- Mock command execution for ISO/burn operations
- Test with temporary directories and test databases

