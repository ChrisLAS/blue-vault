# Commit Plan for GitHub Push

This document outlines the logical commit grouping for pushing to GitHub.

## Commit Strategy

Group related changes into logical commits that tell a story. Each commit should be:
- Focused on a single feature or change
- Self-contained (code compiles and works)
- Well-documented with clear commit messages

## Proposed Commits

### 1. Initial project structure and core functionality

**Files**: Core Rust modules (config, database, manifest, staging, disc, iso, burn, verify, search, etc.)

**Message**:
```
Add core application infrastructure and modules

Implements the foundational modules for BlueVault:
- Configuration management with TOML (config.rs)
- SQLite database with migrations (database.rs)
- Manifest and SHA256 generation (manifest.rs)
- File staging and disc layout (staging.rs, disc.rs)
- ISO creation and Blu-ray burning (iso.rs, burn.rs)
- Disc verification (verify.rs)
- Search functionality (search.rs)
- Safe command execution (commands.rs)
- Dependency checking (dependencies.rs)
- XDG path handling (paths.rs)
- Structured logging (logging.rs)

All modules use safe command execution (no shell injection) and
follow XDG Base Directory specification for data storage.
```

### 2. TUI framework with phosphor theme system

**Files**: theme.rs, ui/ directory, tui/splash.rs, tui/main_menu.rs, main.rs (TUI setup)

**Message**:
```
Implement TUI framework with retro 80s phosphor theme

Adds a complete theme system and UI utilities:
- Theme system with phosphor, amber, and mono themes (theme.rs)
- Truecolor RGB support with ANSI 16/256 fallbacks
- Grid-aligned layout helpers for stable rendering (ui/layout.rs)
- Animation system with throttling (ui/animations.rs)
- Disc activity widgets with 80s-style CD indicators (ui/disc_activity.rs)
- Header/footer patterns (ui/header_footer.rs)
- Startup splash screen (tui/splash.rs)
- Main menu screen (tui/main_menu.rs)

Theme supports environment variables:
- TUI_THEME: phosphor|amber|mono
- TUI_NO_ANIM: disable animations
- TUI_REDUCED_MOTION: reduce motion

All UI components use theme colors for consistent styling.
```

### 3. New disc creation flow with dual-mode directory selector

**Files**: tui/new_disc.rs, tui/directory_selector_simple.rs, main.rs (NewDisc handling)

**Message**:
```
Add new disc creation workflow with dual-mode directory selector

Implements the complete disc creation flow:
- Multi-step wizard: Disc ID → Notes → Folder Selection → Review → Processing
- Dual-mode directory selector:
  * Manual input box (always visible, default focus)
  * Visual directory browser (lazy loaded, tab to focus)
- Path syncing between input and browser modes
- Progress indicators with disc activity animations
- Full workflow integration: staging → manifest → ISO → burn → index → QR

Directory browser uses custom implementation with ratatui List widget.
Entries load lazily only when browser is focused to avoid slow startup.
Tab key toggles focus between input and browser modes.
```

### 4. Search, verification, and management screens

**Files**: tui/search_ui.rs, tui/verify_ui.rs, tui/list_discs.rs, tui/settings.rs, tui/logs_view.rs, main.rs (handlers)

**Message**:
```
Add search, verification, and management TUI screens

Implements remaining TUI screens:
- Search interface with real-time results (tui/search_ui.rs)
- Disc verification flow with mount/unmount (tui/verify_ui.rs)
- Disc listing view (tui/list_discs.rs)
- Settings screen showing theme/motion config (tui/settings.rs)
- Log viewer (tui/logs_view.rs)

All screens use consistent phosphor theme styling and keyboard navigation.
Search supports substring matching on file paths.
Verification handles device mounting and SHA256 checksum verification.
```

### 5. QR code generation and optional features

**Files**: qrcode.rs, integration in new_disc.rs

**Message**:
```
Add QR code generation for disc IDs

Implements QR code generation using qrencode CLI tool:
- Optional QR code generation (requires qrencode)
- Generates PNG images stored in user data directory
- QR codes contain disc ID for easy identification
- Non-fatal if qrencode is unavailable (graceful degradation)

QR codes are stored at ~/.local/share/bdarchive/qrcodes/
and can be printed for disc labels/spines.
```

### 6. Documentation and project files

**Files**: README.md, DEVELOPMENT.md, ARCHITECTURE.md, CONTRIBUTING.md, CHANGELOG.md, Project.md, RUNNING.md, .cursorrules

**Message**:
```
Add comprehensive documentation for GitHub release

Creates documentation for users and developers:
- README.md: GitHub front page with overview, features, quick start
- DEVELOPMENT.md: Developer guide with context for future work
- ARCHITECTURE.md: Updated architecture with current module structure
- CONTRIBUTING.md: Contribution guidelines
- CHANGELOG.md: Version history
- .cursorrules: AI assistant context for Cursor editor
- Project.md: Original project specification (preserved)

Documentation includes:
- Quick start guide for new Linux machines
- Feature descriptions and visual design notes
- Development setup and debugging tips
- Code style and contribution guidelines
```

### 7. Project metadata and build configuration

**Files**: Cargo.toml, Cargo.lock, .gitignore

**Message**:
```
Configure Rust project with dependencies and build settings

Sets up Rust project metadata:
- Cargo.toml with all dependencies (ratatui, rusqlite, etc.)
- Cargo.lock for reproducible builds (committed for binary)
- .gitignore for build artifacts and user data directories

Binary name: bdarchive
Display name: BlueVault
License: MIT OR Apache-2.0
```

## Alternative: Single Commit

If you prefer a single commit for the initial push:

**Message**:
```
Initial commit: BlueVault - Blu-ray archive manager with retro TUI

Complete implementation of BlueVault, a TUI application for managing
Blu-ray cold storage archives on Linux.

Features:
- Retro 80s phosphor green terminal theme
- Dual-mode directory selector (manual input + visual browser)
- Complete disc creation workflow (staging → ISO → burn → index)
- SQLite-based searchable index
- Disc verification with SHA256 checksums
- QR code generation for disc IDs
- Structured logging and error handling

All external commands use safe argument arrays (no shell injection).
Follows XDG Base Directory specification for data storage.
Comprehensive documentation included.
```

## Recommended Approach

**Option A: Logical Commits** (Recommended for history)
- Commit 1: Core infrastructure
- Commit 2: TUI framework and theme
- Commit 3: New disc flow
- Commit 4: Other screens
- Commit 5: QR codes
- Commit 6: Documentation
- Commit 7: Project config

**Option B: Single Initial Commit** (Simpler, good for first push)
- Single commit with all code and documentation

Choose based on your preference for git history granularity.

