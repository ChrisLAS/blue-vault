# BlueVault Architecture

## Project Structure

```
bluevault/
├── Cargo.toml
├── Cargo.lock
├── src/
│   ├── main.rs                 # Entry point, TUI orchestration, event loop, session management
│   ├── lib.rs                  # Library exports
│   ├── config.rs               # Configuration management (TOML-based)
│   ├── database.rs             # SQLite schema, migrations, queries (v3: multi-disc + sessions)
│   ├── manifest.rs             # Manifest + SHA256/CRC32 generation (multi-core)
│   ├── staging.rs              # File staging logic, advanced bin-packing algorithm
│   ├── disc.rs                 # Disc layout, DISC_INFO.txt generation, multi-disc naming
│   ├── iso.rs                  # ISO creation via xorriso
│   ├── burn.rs                 # Burning via growisofs with progress parsing
│   ├── verify.rs               # Disc verification (single + multi-disc sets)
│   ├── qrcode.rs               # QR code generation (optional qrencode)
│   ├── search.rs               # Search functionality (substring matching)
│   ├── commands.rs             # Safe command execution (no shell injection)
│   ├── dependencies.rs         # Dependency checking and validation
│   ├── paths.rs                # Path normalization, XDG directory handling
│   ├── logging.rs              # Structured logging with tracing
│   ├── theme.rs                # Theme system (phosphor/amber/mono + accessibility)
│   ├── tui/                    # TUI screens and components
│   │   ├── mod.rs
│   │   ├── main_menu.rs        # Main menu with 9 options
│   │   ├── new_disc.rs         # Multi-disc creation flow with pause/resume
│   │   ├── resume_burn.rs      # Session management and cleanup UI
│   │   ├── verify_multi_disc.rs # Multi-disc set verification interface
│   │   ├── directory_selector_simple.rs  # Dual-mode directory selector
│   │   ├── search_ui.rs        # Search interface
│   │   ├── verify_ui.rs        # Single disc verification
│   │   ├── list_discs.rs       # List all discs with set relationships
│   │   ├── settings.rs         # Settings management
│   │   ├── logs_view.rs        # Log viewer
│   │   └── splash.rs           # Startup splash screen with status
│   └── ui/                     # UI utilities and components
│       ├── mod.rs
│       ├── layout.rs           # Grid-aligned layout system (flicker-free)
│       ├── animations.rs       # Animation throttling, spinners, progress bars
│       ├── disc_activity.rs    # 80s-style CD read/write indicators with LBA
│       └── header_footer.rs    # Consistent header/footer patterns
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
    disc_id TEXT PRIMARY KEY,              -- e.g., "2026-BD-ARCHIVE-1"
    volume_label TEXT NOT NULL,            -- ISO volume label (BDARCHIVE_2026D1_OF_3)
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

### disc_sets table (v2+)
```sql
CREATE TABLE disc_sets (
    set_id TEXT PRIMARY KEY,              -- Unique set identifier
    name TEXT NOT NULL,                   -- Human-readable set name
    description TEXT,                     -- User notes about the set
    total_size INTEGER NOT NULL,          -- Total size of all discs in bytes
    disc_count INTEGER NOT NULL,          -- Number of discs in set
    created_at TEXT NOT NULL,             -- ISO 8601 creation timestamp
    source_roots TEXT                     -- JSON array of original source paths
);

CREATE INDEX idx_disc_sets_created_at ON disc_sets(created_at);
```

### burn_sessions table (v3+)
```sql
CREATE TABLE burn_sessions (
    session_id TEXT PRIMARY KEY,          -- Unique session identifier
    set_id TEXT NOT NULL,                 -- Associated disc set
    session_name TEXT NOT NULL,           -- Display name for session
    current_disc INTEGER NOT NULL,        -- Current disc being processed (1-based)
    total_discs INTEGER NOT NULL,         -- Total discs in set
    completed_discs TEXT NOT NULL,        -- JSON array of completed disc numbers
    failed_discs TEXT,                    -- JSON array of failed disc numbers
    source_folders TEXT NOT NULL,         -- JSON array of source folder paths
    config_json TEXT NOT NULL,            -- Serialized burn configuration
    staging_state TEXT,                   -- JSON state of staging directories
    created_at TEXT NOT NULL,             -- Session creation timestamp
    updated_at TEXT NOT NULL,             -- Last update timestamp
    status TEXT NOT NULL DEFAULT 'active', -- active, paused, completed, cancelled
    notes TEXT,                           -- User notes about session
    FOREIGN KEY (set_id) REFERENCES disc_sets(set_id) ON DELETE CASCADE
);

CREATE INDEX idx_burn_sessions_status ON burn_sessions(status);
CREATE INDEX idx_burn_sessions_updated ON burn_sessions(updated_at);
```

## Multi-Disc Archive System

BlueVault's multi-disc system is a production-grade solution for distributing large archives across multiple Blu-ray discs with enterprise-level reliability and user experience.

### Advanced Bin-Packing Algorithm

The system uses a sophisticated multi-heuristic bin-packing algorithm that optimizes space utilization while preserving data integrity:

#### **Sorting Strategy**
1. **Directory Priority**: Directories before files (preserves structure)
2. **Size Efficiency**: Items sized 40-80% of disc capacity prioritized (optimal fit)
3. **Child Distribution**: Varied child sizes get preference (better packing potential)
4. **Depth Penalty**: Shallow directory structures preferred (easier to split)
5. **Large Child Bonus**: Directories with large children prioritized (FFD principle)

#### **Packing Heuristics**
- **Best Fit Decreasing**: Finds disc with least remaining space that can fit item
- **Space Utilization**: Prefers placements with high space efficiency
- **Gap Prevention**: Avoids leaving unusable small gaps (<1MB heavily penalized)
- **Directory Cohesion**: Keeps related directories together when possible

#### **Splitting Logic**
- **Boundary Preservation**: Splits at directory boundaries, not mid-file
- **Partial Directory**: Fits as much of a directory as possible on current disc
- **Continuation**: Remaining content goes to next disc with clear naming

### Session Management & Recovery

#### **Pause/Resume Architecture**
- **State Persistence**: Complete session state saved to `burn_sessions` table
- **Progress Tracking**: Current disc, completed discs, failed discs
- **Staging State**: Temporary directory contents tracked for cleanup
- **Configuration Preservation**: All burn settings maintained across sessions

#### **Recovery Scenarios**
- **User Pause**: Manual interruption with clean state preservation
- **System Crash**: Automatic recovery on app restart
- **Hardware Failure**: Resume from last successful disc
- **Partial Success**: Continue with remaining discs in set

### Multi-Disc Verification System

#### **Set Completeness Checking**
- **Disc Presence**: Scans mount points for all discs in set
- **ID Matching**: Verifies discs by `DISC_INFO.txt` content
- **Status Aggregation**: Reports Verified/Failed/Missing for each disc

#### **Integrity Verification**
- **Individual Disc Checks**: SHA256 verification per disc
- **Aggregate Reporting**: Overall set health status
- **Partial Verification**: Verify available discs in incomplete sets
- **Historical Tracking**: Database storage of verification results

### Processing Pipeline

```
User selects folders → Advanced planning with bin-packing → Session creation
                                      ↓
                     Sequential burning with pause/resume capability
                                      ↓
                     Error recovery and user choice handling
                                      ↓
                     Database tracking and cleanup management
                                      ↓
                     Multi-disc verification and integrity checking
```

### Key Components

#### **Planning & Packing**
- **`staging::plan_disc_layout_with_progress()`**: Advanced planning with real-time feedback
- **`staging::sort_for_bin_packing()`**: Multi-heuristic item prioritization
- **`staging::find_best_fit_disc()`**: Optimal disc selection algorithm
- **`DiscPlan`**: Content layout representation with space utilization tracking

#### **Session Management**
- **`BurnSession`**: Complete session state with persistence
- **`BurnSessionOps`**: Database operations for session management
- **Pause/Resume UI**: User interface for session control and cleanup

#### **Error Recovery**
- **`MultiDiscError`**: Structured error types for different failure scenarios
- **User Choice Prompts**: Interactive error recovery options
- **Transactional Operations**: Database consistency during failures
- **Cleanup Management**: Safe removal of failed session data

#### **Verification**
- **`verify_multi_disc_set()`**: Set-level verification orchestration
- **`find_disc_mount_point()`**: Intelligent disc detection across mount points
- **`MultiDiscVerificationResult`**: Comprehensive verification reporting
- **Database Integration**: Historical verification result storage

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

