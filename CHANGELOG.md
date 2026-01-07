# Changelog

All notable changes to BlueVault will be documented in this file.

## [0.1.0] - Initial Release

### Added
- **Core TUI Application**: Full-featured terminal interface with retro 80s phosphor theme
- **Main Menu**: Keyboard-driven navigation with all major features accessible
- **New Disc Creation Flow**: Complete workflow from folder selection to burned disc
  - Multi-step wizard: Disc ID → Notes → Folder Selection → Review → Processing
  - Dual-mode directory selector (manual input + visual browser)
  - Progress indicators with disc activity animations
- **Directory Selection**: Custom-built directory browser with lazy loading
  - Manual path entry (always visible, default focus)
  - Visual directory browser (tab to focus)
  - Path syncing between input and browser
- **Database System**: SQLite-based indexing with migrations
  - Discs table with metadata
  - Files table with searchable paths
  - Verification runs table for audit trail
- **Search Functionality**: Search indexed files by path substring
- **Disc Verification**: Mount and verify discs using SHA256 checksums
- **QR Code Generation**: Generate QR codes for disc IDs (optional qrencode)
- **Theme System**: Customizable terminal themes
  - Phosphor (default): Classic green CRT aesthetic
  - Amber: Warm amber terminal colors
  - Mono: High-contrast accessibility mode
  - Environment variable support (TUI_THEME, TUI_NO_ANIM, TUI_REDUCED_MOTION)
  - ANSI 16/256 color fallbacks
- **UI Components**: Professional UI utilities
  - Grid-aligned layouts (stable, flicker-free)
  - Animation system with throttling
  - Disc activity widgets (80s-style CD indicators)
  - Consistent header/footer patterns
  - Startup splash screen
- **Configuration Management**: TOML-based config stored outside repo
- **Structured Logging**: Comprehensive logging with tracing
- **Dependency Checking**: Validates required tools before operations
- **Safe Command Execution**: Prevents shell injection vulnerabilities

### Technical Details
- Built with Rust (edition 2021)
- Uses ratatui for TUI rendering
- Uses rusqlite for database (bundled SQLite)
- Follows XDG Base Directory specification
- All external commands use safe argument arrays

### Known Limitations
- Directory browser loads synchronously (can be slow for large directories)
- No multi-disc packing (warns on capacity but doesn't auto-split)
- Search only supports substring matching (no regex yet)
- No resume support for interrupted operations
- Basic progress indicators (no file-by-file progress)

### Documentation
- Comprehensive README with quick start guide
- Detailed architecture documentation
- Developer guide with context for future work
- Running instructions and troubleshooting

