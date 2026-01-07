# Directory Selection Options for BlueVault

## Current Implementation

Currently, users must manually type folder paths during the "Select Folders" step in the New Disc flow. The implementation supports:
- Manual path entry via keyboard
- Validation that paths exist and are directories
- Display of selected folders as a list

**Current UI Flow:**
- User presses 'A' to add a folder
- User types the full path
- User presses Enter to commit the path
- Path is validated and added to the list

## Research Results

### Option 1: fpicker (Recommended) ⭐

**Library:** `fpicker` - A file explorer widget for ratatui

**Pros:**
- Built specifically for ratatui (compatible with our framework)
- Simple API for file/directory exploration
- Customizable theming
- Actively maintained
- Supports directory selection
- Good documentation

**Cons:**
- Adds a dependency
- May need minor adjustments to match our theme

**Usage Example:**
```rust
use fpicker::{FileExplorer, Theme};

// In NewDiscFlow struct
directory_browser: Option<FileExplorer>,

// Initialize
let theme = Theme::default().add_default_title();
let file_explorer = FileExplorer::with_theme(theme)?;

// In render
if let Some(ref browser) = self.directory_browser {
    frame.render_widget(browser.widget(), area);
}

// Handle events
if let Some(ref mut browser) = self.directory_browser {
    browser.handle(&event)?;
    if browser.selected().is_some() {
        // User selected a directory
        let path = browser.current_dir().join(browser.selected().unwrap());
        // Add to source_folders
    }
}
```

**Installation:**
```toml
[dependencies]
fpicker = "0.8"  # Check latest version
```

**Repository:** https://github.com/aome510/fpicker
**Documentation:** https://docs.rs/fpicker/

---

### Option 2: Custom Implementation with ratatui List Widget

**Pros:**
- No external dependencies
- Full control over UI/UX
- Can match our phosphor theme exactly
- Lightweight
- Can implement exactly the features we need

**Cons:**
- More code to maintain
- Need to handle directory navigation logic ourselves
- Need to handle symlinks, permissions, etc.

**Implementation Approach:**
1. Use `std::fs::read_dir` to list directory contents
2. Use ratatui's `List` widget to display entries
3. Handle navigation (Enter to enter dir, .. to go up, Space/Enter to select)
4. Filter to show only directories (or allow files too)
5. Support path completion/search

**Example Structure:**
```rust
pub struct DirectoryBrowser {
    current_path: PathBuf,
    entries: Vec<DirEntry>,
    selected: usize,
    mode: BrowserMode, // Browse, Select
}

enum DirEntry {
    Directory(PathBuf),
    Parent, // ".."
}

impl DirectoryBrowser {
    fn refresh(&mut self) -> Result<()> {
        // Read current directory
        // Sort entries (directories first, parent last)
        // Store in self.entries
    }
    
    fn navigate_into(&mut self, entry: &DirEntry) {
        // Change current_path and refresh
    }
    
    fn select_current(&self) -> Option<PathBuf> {
        // Return current_path or selected directory
    }
}
```

---

### Option 3: tui-file-dialog

**Library:** `tui-file-dialog` 

**Pros:**
- Purpose-built for file/directory dialogs
- Simple dialog-based interface

**Cons:**
- Built for `tui-rs` (older framework, not ratatui)
- May have compatibility issues
- Less active maintenance
- Might require adaptation for ratatui

**Verdict:** ❌ Not recommended - built for wrong framework

---

### Option 4: nucleo-picker

**Library:** `nucleo-picker`

**Pros:**
- Very fast fuzzy finding
- Unicode-aware
- Good for searching

**Cons:**
- Primarily a fuzzy finder, not a file browser
- Would need to pre-populate with all directories (could be slow)
- Overkill for directory selection
- More complex API

**Verdict:** ⚠️ Not ideal - designed for different use case

---

## Recommended Approach

### Hybrid Solution: Browser + Manual Entry

**Best of both worlds:**
1. **Directory Browser Mode** (Default)
   - Navigate filesystem visually
   - Arrow keys to navigate
   - Enter to enter directory
   - Space/Enter to select current directory
   - '..' entry to go up

2. **Manual Path Entry Mode** (Fallback)
   - Press 'T' (Type) to switch to manual entry
   - Type full path
   - Tab completion if possible
   - Enter to add

3. **Quick Add**
   - Press 'A' for "Add" dialog
   - Choose: Browse or Type
   - Or just start typing to enter manual mode

### Implementation Plan

**Phase 1: Add fpicker integration**
1. Add `fpicker` dependency
2. Create `DirectoryBrowser` wrapper that:
   - Uses fpicker for navigation
   - Applies our phosphor theme
   - Integrates with NewDiscFlow
3. Add toggle between browser and manual entry
4. Update UI to show both modes

**Phase 2: Enhance manual entry** (if needed)
1. Add path completion/suggestions
2. Add history of previously entered paths
3. Add validation feedback in real-time

**Phase 3: Custom implementation** (if fpicker doesn't meet needs)
1. Build custom browser using ratatui List
2. Match our theme exactly
3. Add features like:
   - Bookmarks/favorites
   - Recent directories
   - Filter/search directories
   - Symlink handling

---

## UI/UX Design

### Directory Browser Mode

```
┌─────────────────────────────────────────┐
│ Select Directory                        │
├─────────────────────────────────────────┤
│  📁 ..                                  │
│  📁 Documents                           │
│  📁 Pictures                            │
│  📁 Videos                              │
│▶ 📁 Archive   [current]                │
│  📁 Downloads                           │
├─────────────────────────────────────────┤
│ Path: /home/user/Archive               │
│                                         │
│ [↑↓] Navigate  [Enter] Select  [T] Type│
│ [Esc] Cancel   [..] Up                 │
└─────────────────────────────────────────┘
```

### Manual Entry Mode

```
┌─────────────────────────────────────────┐
│ Select Directory                        │
├─────────────────────────────────────────┤
│                                         │
│ Type folder path:                       │
│ /home/user/archive                     │
│                                         │
│ Selected folders:                       │
│ 1. /home/user/documents                │
│ 2. /home/user/photos                   │
│                                         │
├─────────────────────────────────────────┤
│ Path: /home/user/archive               │
│                                         │
│ [Enter] Add  [B] Browse  [Esc] Cancel  │
└─────────────────────────────────────────┘
```

---

## Keybindings

### Browser Mode
- `↑` / `k` - Navigate up
- `↓` / `j` - Navigate down
- `Enter` - Enter selected directory / Select directory (if on dir)
- `Space` - Select current directory
- `..` (typing) - Go up one level
- `T` - Switch to manual entry mode
- `Esc` - Cancel / Back

### Manual Entry Mode
- `Tab` - Path completion (if implemented)
- `Enter` - Add path
- `B` - Switch to browse mode
- `Esc` - Cancel / Back

---

## Next Steps

1. **Decision:** Choose fpicker or custom implementation
2. **Prototype:** Create a simple directory browser widget
3. **Integration:** Integrate into NewDiscFlow
4. **Testing:** Test with various directory structures
5. **Polish:** Apply theme, add keyboard hints, improve UX

---

## Resources

- **fpicker:** https://docs.rs/fpicker/
- **ratatui List widget:** https://docs.rs/ratatui/latest/ratatui/widgets/struct.List.html
- **std::fs::read_dir:** https://doc.rust-lang.org/std/fs/fn.read_dir.html

