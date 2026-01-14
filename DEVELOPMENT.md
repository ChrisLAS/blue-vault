# BlueVault Development Guide

This document provides context for developers (including AI assistants like Cursor) to understand the project structure, current state, and how to continue development.

## Project Context

BlueVault is a terminal-based Blu-ray archive manager written in Rust. It features a retro 80s phosphor terminal aesthetic and provides a complete workflow for creating, indexing, and verifying Blu-ray archives.

## Current State (v0.1.2 - Production Ready)

### Implemented Features

✅ **Core TUI System**
- Main menu with keyboard navigation (↑/↓/Enter, hjkl vim keys)
- Phosphor green theme system with accessibility fallbacks
- Header/footer patterns with consistent branding
- Startup splash screen with system status
- Settings screen for theme/motion configuration
- Universal quit ('Q') and navigation ('Esc') keys

✅ **Multi-Disc Archive System** 🚀
- **Automatic Splitting**: Intelligently distributes large archives across multiple Blu-ray discs
- **Advanced Bin-Packing**: Sophisticated algorithm minimizes disc count while preserving directory structure
- **Smart Planning**: Pre-burn preview shows exact content distribution
- **Sequential Burning**: Guided workflow for burning multi-disc sets
- **Session Management**: Pause/resume capability for long operations
- **Progress Tracking**: Real-time feedback throughout multi-disc operations
- **Error Recovery**: Comprehensive handling of hardware failures and user interruptions
- **Set Verification**: Integrity checking for entire multi-disc archives

✅ **New Disc Creation Flow**
- Multi-step wizard: Disc ID → Notes → Folder Selection → Review → Processing
- **Dual-mode directory selector**:
  - **Manual input box**: Always visible, default focus, type paths directly
  - **Directory browser**: Tab to focus, navigate with ↑/↓, Enter to select, lazy loading
- **Progress indicators** with disc activity animations (80s CD-style)
- **Full workflow**: staging → manifest → ISO → burn → index → QR
- **Dry run testing** with actual ISO creation and size reporting
- **ISO path reporting** shows locations of all created files

✅ **Session Management & Recovery** ⏸️▶️
- **Pause/Resume**: Interrupt and resume multi-disc operations at any point
- **Session Persistence**: State survives app restarts and system interruptions
- **Progress Preservation**: Continue exactly where you left off
- **Cleanup Management**: Safe removal of paused session data
- **Space Monitoring**: User visibility into temporary file usage

✅ **Directory Selection**
- Custom-built directory browser using ratatui List widget
- Lazy loading: entries only load when browser is focused
- Path syncing between input and browser modes
- Tab toggles focus between manual input and visual browser

✅ **Database & Indexing** 💾
- SQLite database with versioned migrations (current: v3)
- **Enhanced schema**: discs, files, verification_runs, disc_sets, burn_sessions
- **Multi-disc relationships**: Proper set tracking and sequencing
- **Session persistence**: Pause/resume state storage
- Search functionality (substring match on paths, extensible for regex)

✅ **Verification System** 🔍
- **Single-disc verification**: Mount/unmount handling with SHA256 checksums
- **Multi-disc verification**: Set completeness and integrity checking
- **Intelligent disc detection**: Automatic scanning of mount points
- **Partial verification**: Verify available discs in incomplete sets
- Results stored in database with detailed reporting

✅ **Theme System** 🎨
- **Phosphor** (default): Classic green CRT aesthetic (#3CFF8A on #07110A)
- **Amber** (optional): Warm amber terminal colors
- **Mono** (optional): High-contrast accessibility mode
- Environment variables: `TUI_THEME`, `TUI_NO_ANIM`, `TUI_REDUCED_MOTION`
- ANSI 16/256 color fallbacks for limited terminals
- Accessibility support with reduced motion options

✅ **UI Components** 🖥️
- Grid-aligned layouts (stable, flicker-free rendering)
- Animation system with throttling (8-12 FPS, auto-slowdown after 60s)
- Disc activity widgets (80s-style CD read/write indicators with LBA counters)
- Consistent header/footer patterns with screen titles
- Startup splash with dependency checking and system status

✅ **Cleanup & Maintenance** 🧹
- **Comprehensive cleanup** via main menu option
- **Selective removal**: build artifacts, staging directories, orphaned files
- **Session-aware**: preserves active burn data while cleaning completed sessions
- **Progress feedback** during cleanup operations
- **Safe operations** with confirmation and error handling

### Known Issues / Limitations

⚠️ **Directory Browser Performance**
- Loads synchronously when focused, can be slow for directories with 1000+ entries
- Loading happens when you Tab to browser, not on screen entry
- Future: Async/background loading with progress indicators

✅ **Multi-Disc Archives - FULLY IMPLEMENTED**
- Advanced bin-packing algorithm with directory integrity preservation
- Automatic sequential naming (2026-BD-1, 2026-BD-2, etc. - zero padding removed)
- Complete database relationship tracking for disc sets
- User-guided sequential burning with comprehensive error recovery
- Pre-burn planning shows exact content distribution
- ISO path reporting for all created files
- Session pause/resume capability for long operations

✅ **Resume/Pause Capability - FULLY IMPLEMENTED**
- Pause multi-disc operations at any point ('p' key)
- Resume interrupted sessions from main menu
- Session state persistence across app restarts
- Automatic cleanup management for paused sessions
- Progress preservation and failure recovery
- Space usage monitoring for temporary files

✅ **Multi-Disc Verification - FULLY IMPLEMENTED**
- Set completeness verification (all discs present)
- Individual disc integrity checking
- Intelligent mount point scanning
- Partial verification for incomplete sets
- Detailed per-disc status reporting
- Database integration for verification history

⚠️ **Search Functionality**
- Current: Substring matching on file paths only
- Missing: Regex support, filename-only search, date filtering, size filtering
- Future: Advanced query options and fuzzy search

✅ **Progress Indicators - ENHANCED**
- Real-time burn progress with speed, ETA, completion percentage
- Multi-disc planning progress with item counts and space calculations
- File-by-file staging progress during preparation
- Background processing with non-blocking UI updates
- Comprehensive completion summaries with ISO path reporting
- Disc activity animations (80s CD-style indicators)

⚠️ **Future Enhancement Opportunities**
- **Advanced Verification**: Cross-disc consistency checking, corruption repair
- **Search Enhancements**: Regex patterns, fuzzy matching, metadata filters
- **Performance**: Async directory loading, parallel verification
- **Network Features**: Remote verification, distributed archives
- **Advanced UI**: Keyboard shortcuts, mouse support, themes

### Technical Debt

- Some code duplication in TUI rendering (could be abstracted)
- Directory browser could be replaced with `fpicker` crate once API is stable
- Some borrow checker workarounds in main.rs that could be refactored
- Error messages could be more user-friendly in some places

## Development Environment Setup

### Prerequisites

1. **Rust Toolchain** (1.70+)
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **System Dependencies**
   ```bash
   # Debian/Ubuntu
   sudo apt install xorriso dvd+rw-tools qrencode rsync
   
   # Fedora/RHEL
   sudo dnf install xorriso dvd+rw-tools qrencode rsync
   ```

3. **Development Tools**
   ```bash
   # Rust tools
   cargo install cargo-watch  # Optional: auto-rebuild on file changes
   cargo install cargo-clippy # Linting (or use rustup component add clippy)
   ```

### Build Commands

```bash
# Development build
cargo build

# Release build (optimized)
cargo build --release

# Run in development
cargo run

# Run with debug logging
RUST_LOG=debug cargo run

# Run with theme override
TUI_THEME=amber cargo run
TUI_THEME=mono cargo run
TUI_NO_ANIM=1 cargo run
```

### Testing

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run specific test
cargo test test_name

# Run tests in release mode (faster, but less debugging info)
cargo test --release
```

### Code Quality

```bash
# Format code
cargo fmt

# Check for issues
cargo clippy

# Check for unused dependencies
cargo machete  # If installed: cargo install cargo-machete
```

## Project Structure

### Key Modules

**Core Functionality:**
- `src/main.rs`: Application entry point, TUI event loop, state management
- `src/lib.rs`: Library exports for testing/integration

**Configuration & Paths:**
- `src/config.rs`: TOML configuration management
- `src/paths.rs`: XDG directory handling, path utilities
- `src/logging.rs`: Structured logging with tracing

**Database:**
- `src/database.rs`: SQLite schema, migrations, CRUD operations

**Disc Operations:**
- `src/disc.rs`: Disc ID generation, layout creation, DISC_INFO.txt
- `src/staging.rs`: File staging to temporary directory
- `src/manifest.rs`: MANIFEST.txt and SHA256SUMS.txt generation
- `src/iso.rs`: ISO image creation via xorriso
- `src/burn.rs`: Blu-ray burning via growisofs
- `src/verify.rs`: Disc verification, mount/unmount

**Utilities:**
- `src/commands.rs`: Safe command execution (no shell injection)
- `src/dependencies.rs`: Dependency checking
- `src/qrcode.rs`: QR code generation (qrencode wrapper)
- `src/search.rs`: Database search functionality

**UI System:**
- `src/theme.rs`: Theme system (phosphor/amber/mono)
- `src/ui/`: UI utilities (layout, animations, widgets)
- `src/tui/`: TUI screens (main menu, new disc, search, etc.)

### TUI Module Structure

```
src/tui/
├── mod.rs                      # Module exports
├── main_menu.rs                # Main menu screen
├── new_disc.rs                 # New disc creation flow
├── directory_selector_simple.rs # Dual-mode directory selector
├── search_ui.rs                # Search interface
├── verify_ui.rs                # Disc verification UI
├── list_discs.rs               # Disc listing
├── settings.rs                 # Settings screen
├── logs_view.rs                # Log viewer
└── splash.rs                   # Startup splash
```

### UI Utilities

```
src/ui/
├── mod.rs                      # Module exports
├── layout.rs                   # Grid-aligned layout helpers
├── animations.rs               # Animation throttling, spinners
├── disc_activity.rs            # CD-style read/write indicators
└── header_footer.rs            # Consistent header/footer widgets
```

## Key Design Decisions

### Theme System

The theme system uses a hierarchy:
1. **Truecolor RGB** if `COLORTERM=truecolor|24bit` (detected automatically)
2. **ANSI 256 colors** as fallback (approximates phosphor colors)
3. **ANSI 16 colors** as final fallback (still readable)

Themes are loaded from `TUI_THEME` environment variable or default to phosphor.

### Directory Selector

We built a custom directory browser instead of using `fpicker` because:
- Full control over styling (matches phosphor theme exactly)
- Simpler API (no need to understand fpicker's interface)
- Lazy loading (only loads when focused)
- Easy to extend with features like favorites/bookmarks

The browser uses `std::fs::read_dir` and ratatui's `List` widget.

### State Management

The main application uses an `AppState` enum to track current screen:
- `Splash`: Startup splash screen
- `MainMenu`: Main menu
- `NewDisc(Box<NewDiscFlow>)`: New disc creation flow
- `Search(SearchUI)`: Search interface
- `Verify(VerifyUI)`: Verification flow
- `ListDiscs(ListDiscs)`: Disc listing
- `Settings(Settings)`: Settings screen
- `Logs(LogsView)`: Log viewer
- `Quit`: Exit state

State is managed in `main.rs` with careful borrow checker handling.

### Error Handling

- Uses `anyhow::Result` for most operations
- Uses `thiserror` for structured error types where needed
- All errors logged with context via `tracing`
- User-facing errors shown in TUI with helpful messages

### Command Execution

All external commands use `std::process::Command` with argument arrays (never shell strings) to prevent shell injection. See `src/commands.rs`.

## Adding New Features

### Adding a New TUI Screen

1. Create new file in `src/tui/` (e.g., `new_screen.rs`)
2. Implement struct with `render(&mut self, theme: &Theme, frame: &mut Frame, area: Rect)` method
3. Add to `AppState` enum in `src/main.rs`
4. Add state transition in `handle_key()` method
5. Add render case in `render()` method
6. Export in `src/tui/mod.rs`

### Adding a New Theme Color

1. Add color to theme structs in `src/theme.rs` (PhosphorColors, AmberColors, MonoColors)
2. Add getter method to `Theme` impl
3. Add style method if needed (e.g., `new_color_style()`)
4. Update truecolor and ANSI fallback mappings

### Adding Database Fields

1. Create migration in `src/database.rs` `migrate_database()` function
2. Update structs (e.g., `Disc`, `FileRecord`)
3. Update SQL queries that use the new fields
4. Test migration with old database

### Adding External Tool Support

1. Add dependency check in `src/dependencies.rs`
2. Add optional config flag in `src/config.rs`
3. Add wrapper function in appropriate module (or `src/commands.rs`)
4. Use safe command execution (no shell strings)

## Debugging Tips

### Common Issues

**Borrow Checker Errors:**
- Common in `main.rs` when accessing `self.state` and `self.db_conn` simultaneously
- Solution: Extract values into owned types before use, or refactor into helper functions
- See `start_disc_creation_internal()` and `start_verification_internal()` for patterns

**TUI Not Rendering:**
- Check terminal supports the features used (truecolor, UTF-8)
- Try running with `TUI_THEME=mono` to test fallback rendering
- Check logs in `~/.local/share/bdarchive/logs/`

**Directory Browser Slow:**
- Large directories (>1000 entries) can be slow
- Consider adding a filter/search feature
- Could paginate results in future

**Permission Issues:**
- Blu-ray device access: add user to `cdrom` group
- Staging directory: ensure writable
- Database: ensure parent directory exists and is writable

### Debug Logging

```bash
# Set log level
RUST_LOG=debug cargo run

# Trace all operations
RUST_LOG=trace cargo run

# Specific module
RUST_LOG=bdarchive::database=debug cargo run
```

### Testing Without Hardware

- Use dry-run mode (when implemented)
- Test with ISO creation only (skip burning)
- Mock command execution in tests
- Use test databases in temporary directories

## Code Style

- Follow Rust standard formatting (`cargo fmt`)
- Use `cargo clippy` for linting (fix all warnings)
- Prefer `anyhow::Result` for error handling unless structured errors needed
- Use `tracing` for logging (not `println!` in production code)
- Document public APIs with doc comments
- Keep functions focused and single-purpose

## Testing Strategy

### Unit Tests

Test pure functions in isolation:
- Manifest generation (`manifest.rs`)
- Path normalization (`paths.rs`)
- Command building (`commands.rs`, `iso.rs`, `burn.rs`)
- Database operations (`database.rs`)

### Integration Tests

Test workflows:
- Disc creation flow (staging → manifest → ISO)
- Search functionality
- Database migrations
- Configuration loading/saving

### Mock External Commands

For testing without hardware:
- Mock `std::process::Command` execution
- Return expected output for xorriso/growisofs
- Test error cases (command failures, missing tools)

## Future Work

### High Priority

1. **Async Directory Loading**: Load directory entries in background with progress
2. **Better Progress Indicators**: Show file-by-file progress during staging/burning
3. **Resume Support**: Checkpoint/resume for interrupted operations

### Medium Priority

4. **Multi-disc Packing**: Automatic bin-packing across multiple discs
5. **Regex Search**: Add regex support to search functionality
6. **Favorites/Bookmarks**: Remember frequently used directories

### Low Priority

7. **fpicker Integration**: Replace custom browser with fpicker once API stabilizes
8. **Export Functionality**: Export search results to CSV/JSON
9. **Advanced Search**: Filename-only, date range, size filters
10. **Theme Customization**: Allow user-defined color schemes

## Resources

- **Ratatui Docs**: https://docs.rs/ratatui/
- **Rusqlite Docs**: https://docs.rs/rusqlite/
- **Tracing Docs**: https://docs.rs/tracing/
- **XDG Base Directory**: https://specifications.freedesktop.org/basedir-spec/

## Getting Help

- Check logs: `~/.local/share/bdarchive/logs/`
- Review `ARCHITECTURE.md` for detailed module documentation
- See `Project.md` for original specification
- Run with `RUST_LOG=debug` for detailed output

---

**Last Updated**: See git history for latest changes and context.

