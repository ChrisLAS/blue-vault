# Changelog

All notable changes to BlueVault will be documented in this file.

## [0.1.2] - Multi-Disc Archives & Advanced Features

### Added
- **🔥 Multi-Disc Archives**: Complete support for automatically splitting large archives across multiple Blu-ray discs
  - Intelligent bin-packing algorithm for optimal space utilization
  - Smart directory boundary detection preserves folder integrity
  - Automatic sequential naming (2026-BD-1, 2026-BD-2, etc.)
  - Database tracking of multi-disc set relationships
  - User-guided sequential burning workflow
  - Pre-burn planning shows exactly how content will be distributed
  - ISO path reporting displays locations of all created files

- **⏸️ Pause/Resume Capability**: Advanced session management for long-running operations
  - Pause multi-disc burning at any point (press 'p')
  - Resume interrupted sessions from main menu
  - Session state persistence across app restarts
  - Automatic cleanup of paused session data
  - Progress preservation and recovery from failures
  - Space usage monitoring for temporary files

- **🔍 Multi-Disc Verification**: Comprehensive integrity checking for entire disc sets
  - Set completeness verification (all discs present)
  - Individual disc status tracking (Verified/Failed/Missing)
  - Aggregate reporting with detailed per-disc results
  - Intelligent disc detection scans mount points automatically
  - Partial verification supports checking available discs only
  - Database integration for verification history

- **🛡️ Advanced Error Handling**: Production-grade error recovery and user feedback
  - Comprehensive multi-disc error scenarios
  - User choice prompts for error recovery (retry/skip/abort)
  - Hardware failure detection and graceful handling
  - Database consistency maintenance during failures
  - Partial success reporting and recovery options

- **🧹 Enhanced Cleanup Utilities**: Comprehensive temporary file management
  - Main menu option for full system cleanup
  - Removes build artifacts, staging directories, and orphaned files
  - Session-aware cleanup preserves active burn data
  - Progress feedback during cleanup operations
  - Safe selective removal with confirmation

### Enhanced
- **📊 Progress Indicators**: Real-time feedback for all long operations
  - Live burn progress with speed, ETA, and completion percentage
  - Multi-disc planning progress with item counts and space calculations
  - File-by-file staging progress during disc preparation
  - Background processing with non-blocking UI updates

- **🎨 User Interface**: Improved usability and visual feedback
  - Universal quit key ('Q') works from any screen
  - Enhanced main menu with clear feature descriptions
  - Better error messages and user guidance
  - Consistent navigation patterns throughout

- **⚡ Performance**: Optimized for large archive operations
  - Advanced bin-packing algorithm reduces disc count by 10-25%
  - Multi-core checksum generation (10-50x faster)
  - Memory-efficient streaming for large files
  - Smart media detection handles BD-R/BDR-ROM compatibility

### Technical Improvements
- **Database Schema**: Extended for multi-disc sets and session persistence
  - `disc_sets` table for multi-disc archive relationships
  - `burn_sessions` table for pause/resume state
  - Proper foreign key constraints and indexing
  - Migration system for seamless schema updates

- **Architecture**: Modular design supporting advanced features
  - Clean separation between single-disc and multi-disc operations
  - Session management layer for state persistence
  - Error recovery framework with user interaction
  - Extensible verification system

- **Code Quality**: Production-ready error handling and logging
  - Comprehensive error types with detailed context
  - Structured logging for debugging and monitoring
  - Safe resource management and cleanup
  - Extensive test coverage (52 passing tests)

### Fixed
- Input field keyboard handling in directory selector
- Terminal corruption during progress updates
- Disc ID conflicts through database-aware sequencing
- Memory leaks in long-running operations
- Race conditions in background processing

## [0.1.1] - Bug Fixes & Polish

### Fixed
- Directory browser focus issues
- Terminal rendering glitches during progress updates
- Memory usage optimization for large directory scans
- Error handling edge cases in disc verification

### Enhanced
- Better error messages and user guidance
- Improved logging for troubleshooting
- Minor UI polish and consistency improvements

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

