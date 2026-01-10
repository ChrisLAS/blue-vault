// Simplified version that we'll refine based on actual fpicker API
// For now, let's create a working prototype with a placeholder for fpicker
// that we can replace once we understand the actual API

use crate::theme::Theme;
use ratatui::{
    prelude::*,
    widgets::{Block, BorderType, Borders, List, ListItem, Paragraph},
};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;

/// Dual-mode directory selector: manual input + browser
#[derive(Debug)]
pub struct DirectorySelector {
    /// Manual path input buffer
    input_buffer: String,
    /// Current focus: Input or Browser
    focus: Focus,
    /// Current directory being browsed
    current_dir: PathBuf,
    /// Directory entries in current directory
    entries: Vec<DirEntry>,
    /// Selected entry index in browser
    selected_index: usize,
    /// Validation error message
    error_message: Option<String>,
    /// Loading state for async directory reading
    loading_state: LoadingState,
    /// Channel receiver for async loading results
    loading_receiver: Option<mpsc::Receiver<LoadingResult>>,
    /// Handle to the loading task
    _loading_task: Option<thread::JoinHandle<()>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// Focus is on the manual input box
    Input,
    /// Focus is on the file browser
    Browser,
}

#[derive(Debug, Clone)]
enum DirEntry {
    Parent, // ".." to go up
    Directory(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LoadingState {
    /// Not loading, entries are ready
    Idle,
    /// Currently loading directory entries
    Loading,
    /// Loading completed successfully
    Loaded,
    /// Loading failed with an error
    Error(String),
}

#[derive(Debug)]
struct LoadingResult {
    entries: Vec<DirEntry>,
    error: Option<String>,
}

impl Default for DirectorySelector {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));

        Self {
            input_buffer: String::new(),
            focus: Focus::Input,
            current_dir: home.clone(),
            entries: Vec::new(), // Lazy load entries - don't load on default
            selected_index: 0,
            error_message: None,
            loading_state: LoadingState::Idle,
            loading_receiver: None,
            _loading_task: None,
        }
    }
}

impl DirectorySelector {
    pub fn new() -> anyhow::Result<Self> {
        // Don't load entries here - truly lazy load on first browser focus or render
        // This makes initialization instant
        Ok(Self::default())
    }

    /// Initialize/refresh entries if empty (returns true if entries were loaded)
    /// Now supports async loading - will start background load if needed
    pub fn ensure_entries_loaded(&mut self) -> anyhow::Result<bool> {
        if self.entries.is_empty() && self.loading_state == LoadingState::Idle {
            // Start async loading instead of synchronous
            self.start_async_loading()?;
            Ok(false) // Loading started, but not complete yet
        } else {
            Ok(!self.entries.is_empty()) // Already loaded or loading
        }
    }

    /// Start asynchronous directory loading
    fn start_async_loading(&mut self) -> anyhow::Result<()> {
        if self.loading_state == LoadingState::Loading {
            return Ok(()); // Already loading
        }

        self.loading_state = LoadingState::Loading;
        let current_dir = self.current_dir.clone();
        let (tx, rx) = mpsc::channel();

        self.loading_receiver = Some(rx);

        // Spawn thread to load directory entries
        let handle = thread::spawn(move || {
            let result = load_directory_entries_sync(current_dir);
            let _ = tx.send(result);
        });

        self._loading_task = Some(handle);
        Ok(())
    }

    /// Check if async loading is complete and update state
    pub fn check_async_loading(&mut self) -> bool {
        if let Some(ref mut receiver) = self.loading_receiver {
            if let Ok(result) = receiver.try_recv() {
                // Loading completed
                match result.error {
                    None => {
                        self.entries = result.entries;
                        self.loading_state = LoadingState::Loaded;
                        // Sort entries: directories first, alphabetically
                        self.entries.sort_by(|a, b| match (a, b) {
                            (DirEntry::Parent, _) => std::cmp::Ordering::Less,
                            (_, DirEntry::Parent) => std::cmp::Ordering::Greater,
                            (DirEntry::Directory(a), DirEntry::Directory(b)) => a
                                .file_name()
                                .unwrap_or_default()
                                .cmp(&b.file_name().unwrap_or_default()),
                        });
                        // Reset selection
                        if self.selected_index >= self.entries.len() && !self.entries.is_empty() {
                            self.selected_index = self.entries.len() - 1;
                        }
                    }
                    Some(error) => {
                        self.loading_state = LoadingState::Error(error);
                    }
                }
                // Clean up
                self.loading_receiver = None;
                self._loading_task = None;
                return true; // State changed
            }
        }
        false // No change
    }

    /// Refresh directory entries synchronously (fallback method)
    fn refresh_entries(&mut self) -> anyhow::Result<()> {
        self.entries.clear();

        // Add parent entry if not at root
        if self.current_dir.parent().is_some() {
            self.entries.push(DirEntry::Parent);
        }

        // Read directory entries
        if let Ok(entries) = fs::read_dir(&self.current_dir) {
            for entry in entries {
                if let Ok(entry) = entry {
                    let path = entry.path();
                    if path.is_dir() {
                        self.entries.push(DirEntry::Directory(path));
                    }
                }
            }
        }

        // Sort entries: directories first, alphabetically
        self.entries.sort_by(|a, b| match (a, b) {
            (DirEntry::Parent, _) => std::cmp::Ordering::Less,
            (_, DirEntry::Parent) => std::cmp::Ordering::Greater,
            (DirEntry::Directory(a), DirEntry::Directory(b)) => a
                .file_name()
                .unwrap_or_default()
                .cmp(&b.file_name().unwrap_or_default()),
        });

        // Reset selection
        if self.selected_index >= self.entries.len() && !self.entries.is_empty() {
            self.selected_index = self.entries.len() - 1;
        }

        Ok(())
    }

    /// Force synchronous refresh (for when async loading fails)
    pub fn force_sync_refresh(&mut self) -> anyhow::Result<()> {
        self.loading_state = LoadingState::Idle;
        self.loading_receiver = None;
        self._loading_task = None;
        self.refresh_entries()?;
        self.loading_state = LoadingState::Loaded;
        Ok(())
    }

    /// Get current input buffer
    pub fn input_buffer(&self) -> &str {
        &self.input_buffer
    }

    /// Set input buffer
    pub fn set_input_buffer(&mut self, buffer: String) {
        self.input_buffer = buffer;
        self.error_message = None;
    }

    /// Clear input buffer
    pub fn clear_input_buffer(&mut self) {
        self.input_buffer.clear();
        self.error_message = None;
    }

    /// Get current focus
    pub fn focus(&self) -> Focus {
        self.focus
    }

    /// Set focus
    pub fn set_focus(&mut self, focus: Focus) {
        self.focus = focus;
    }

    /// Switch focus between input and browser
    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Input => Focus::Browser,
            Focus::Browser => Focus::Input,
        };

        // If switching to browser and entries not loaded, start loading
        if self.focus == Focus::Browser && self.entries.is_empty() {
            // Load entries when browser gets focus - trigger background load
            // Note: We do this synchronously for now, but it will update on next render
            let _ = self.refresh_entries();
        }
    }

    /// Navigate up in browser
    pub fn browser_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    /// Navigate down in browser
    pub fn browser_down(&mut self) {
        if self.selected_index < self.entries.len().saturating_sub(1) {
            self.selected_index += 1;
        }
    }

    /// Enter selected directory in browser
    pub fn browser_enter(&mut self) -> anyhow::Result<()> {
        if let Some(entry) = self.entries.get(self.selected_index) {
            match entry {
                DirEntry::Parent => {
                    if let Some(parent) = self.current_dir.parent() {
                        self.current_dir = parent.to_path_buf();
                        // Start async loading for new directory
                        self.loading_state = LoadingState::Idle;
                        self.entries.clear();
                        self.start_async_loading()?;
                        self.selected_index = 0;
                        // Update input buffer to match
                        self.input_buffer = self.current_dir.display().to_string();
                    }
                }
                DirEntry::Directory(path) => {
                    let new_path = path.clone();
                    let path_str = new_path.display().to_string();
                    self.current_dir = new_path;
                    // Start async loading for new directory
                    self.loading_state = LoadingState::Idle;
                    self.entries.clear();
                    self.start_async_loading()?;
                    self.selected_index = 0;
                    // Update input buffer to match
                    self.input_buffer = path_str;
                }
            }
        }
        Ok(())
    }

    /// Get selected directory from browser
    pub fn get_browser_selection(&self) -> Option<PathBuf> {
        if let Some(entry) = self.entries.get(self.selected_index) {
            match entry {
                DirEntry::Parent => self.current_dir.parent().map(|p| p.to_path_buf()),
                DirEntry::Directory(path) => Some(path.clone()),
            }
        } else {
            Some(self.current_dir.clone())
        }
    }

    /// Get current directory path
    pub fn current_path(&self) -> &Path {
        &self.current_dir
    }

    /// Set current path (syncs between input and browser)
    pub fn set_current_path(&mut self, path: PathBuf) -> anyhow::Result<()> {
        if path.exists() && path.is_dir() {
            self.current_dir = path.clone();
            self.input_buffer = path.display().to_string();
            // Start async loading for new directory
            self.loading_state = LoadingState::Idle;
            self.entries.clear();
            self.start_async_loading()?;
            self.selected_index = 0;
            self.error_message = None;
            Ok(())
        } else {
            self.error_message = Some(format!(
                "Path does not exist or is not a directory: {}",
                path.display()
            ));
            Err(anyhow::anyhow!("Invalid path"))
        }
    }

    /// Commit the current input buffer as the selected path
    pub fn commit_input(&mut self) -> anyhow::Result<PathBuf> {
        let path_str = self.input_buffer.trim();
        if path_str.is_empty() {
            return Err(anyhow::anyhow!("Path cannot be empty"));
        }

        // Expand tilde if present
        let expanded_path = if path_str.starts_with("~/") {
            if let Some(home) = dirs::home_dir() {
                home.join(&path_str[2..])
            } else {
                PathBuf::from(path_str)
            }
        } else {
            PathBuf::from(path_str)
        };

        if !expanded_path.exists() {
            self.error_message = Some(format!("Path does not exist: {}", expanded_path.display()));
            return Err(anyhow::anyhow!("Path does not exist"));
        }

        if !expanded_path.is_dir() {
            self.error_message = Some(format!(
                "Path is not a directory: {}",
                expanded_path.display()
            ));
            return Err(anyhow::anyhow!("Not a directory"));
        }

        // Sync to browser
        self.set_current_path(expanded_path.clone())?;
        Ok(expanded_path)
    }

    /// Get error message
    pub fn error_message(&self) -> Option<&str> {
        self.error_message.as_deref()
    }

    /// Clear error message
    pub fn clear_error(&mut self) {
        self.error_message = None;
    }

    /// Retry loading if there was an error
    pub fn retry_loading(&mut self) -> anyhow::Result<()> {
        if let LoadingState::Error(_) = self.loading_state {
            self.start_async_loading()?;
        }
        Ok(())
    }

    /// Render the dual-mode selector
    /// Returns true if entries were just loaded (to trigger a redraw)
    pub fn render(&mut self, theme: &Theme, frame: &mut Frame, area: Rect) -> bool {
        use ratatui::layout::{Constraint, Direction, Layout};

        // Check if async loading completed
        let async_completed = self.check_async_loading();

        // Start loading if needed and not already loading
        let started_loading = if self.entries.is_empty() && self.loading_state == LoadingState::Idle
        {
            self.ensure_entries_loaded().unwrap_or(false)
        } else {
            false
        };

        // Split area: input box at top, browser below
        // Give input box enough height for borders, title, and content (at least 5 lines)
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(5), // Input box (borders + title + content)
                Constraint::Min(10),   // Browser
            ])
            .split(area);

        // Always render input box (always visible)
        self.render_input_box(theme, frame, chunks[0]);

        // Always render browser (always visible, shows loading if not loaded yet)
        self.render_browser(theme, frame, chunks[1]);

        async_completed || started_loading // Return true if state changed
    }

    fn render_input_box(&self, theme: &Theme, frame: &mut Frame, area: Rect) {
        let is_focused = self.focus == Focus::Input;

        // Always show placeholder text when empty - make it clearly visible
        // Use a clear label format to ensure it's always rendered
        let input_line = if self.input_buffer.is_empty() {
            "Enter folder path..."
        } else {
            self.input_buffer.as_str()
        };

        // Format error line separately (only if present)
        let error_section = if let Some(ref error) = self.error_message {
            format!("\n\n[ERR] {}", error)
        } else {
            String::new()
        };

        // Build text content - ensure there's always visible content on the first line
        // Put placeholder/input on first line, error on subsequent lines
        let text_content = format!("{}{}", input_line, error_section);

        // Use double border when focused for clear visibility
        let border_type = if is_focused {
            BorderType::Double
        } else {
            BorderType::Plain
        };

        // Bold, bright border style when focused
        let border_style = if is_focused {
            theme
                .primary_style()
                .add_modifier(ratatui::style::Modifier::BOLD)
        } else {
            theme.border_style()
        };

        let title = if is_focused {
            " Folder Path [ACTIVE] - Type path (~/ supported) and press Enter "
        } else {
            " Folder Path - Press Tab to focus here "
        };

        let block = Block::default()
            .title(title)
            .title_alignment(ratatui::layout::Alignment::Left)
            .borders(Borders::ALL)
            .border_type(border_type)
            .border_style(border_style);

        // Always use primary style for text to ensure it's clearly visible
        // The border (double when focused) indicates focus state
        let text_style = theme.primary_style();

        // Always render with explicit alignment - ensure text is visible
        // Use wrap to ensure long paths don't break the layout
        let para = Paragraph::new(text_content)
            .block(block)
            .style(text_style)
            .alignment(ratatui::layout::Alignment::Left)
            .wrap(ratatui::widgets::Wrap { trim: true });

        frame.render_widget(para, area);
    }

    fn render_browser(&mut self, theme: &Theme, frame: &mut Frame, area: Rect) {
        let is_focused = self.focus == Focus::Browser;

        // Show different content based on loading state
        match self.loading_state {
            LoadingState::Idle | LoadingState::Loading => {
                let status_text = if self.loading_state == LoadingState::Loading {
                    format!("🔄 Loading directory: {}", self.current_dir.display())
                } else {
                    format!("⏳ Preparing to load: {}", self.current_dir.display())
                };

                let para = Paragraph::new(status_text)
                    .block(
                        Block::default()
                            .title("Directory Browser")
                            .borders(Borders::ALL)
                            .border_style(theme.border_style())
                            .style(theme.secondary_style()),
                    )
                    .style(theme.secondary_style());
                frame.render_widget(para, area);
                return;
            }
            LoadingState::Error(ref error_msg) => {
                let error_text = format!(
                    "❌ Failed to load directory: {}\n\nError: {}\n\nPress 'R' to retry",
                    self.current_dir.display(),
                    error_msg
                );

                let para = Paragraph::new(error_text)
                    .block(
                        Block::default()
                            .title("Directory Browser - ERROR")
                            .borders(Borders::ALL)
                            .border_style(theme.error_style()),
                    )
                    .style(theme.error_style());
                frame.render_widget(para, area);
                return;
            }
            LoadingState::Loaded => {
                // Directory loaded successfully, show entries
            }
        }

        // Create list items from directory entries
        let items: Vec<ListItem> = self
            .entries
            .iter()
            .map(|entry| {
                let display_name = match entry {
                    DirEntry::Parent => "..".to_string(),
                    DirEntry::Directory(path) => path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                };

                ListItem::new(display_name)
            })
            .collect();

        let list = List::new(items)
            .style(theme.secondary_style())
            .block(
                Block::default()
                    .title(if is_focused {
                        format!(
                            "Directory Browser [FOCUSED] - Enter: navigate, Insert: select - {}",
                            self.current_dir.display()
                        )
                    } else {
                        format!(
                            "Directory Browser - Tab to focus - {}",
                            self.current_dir.display()
                        )
                    })
                    .borders(Borders::ALL)
                    .border_style(if is_focused {
                        theme
                            .primary_style()
                            .add_modifier(ratatui::style::Modifier::BOLD)
                    } else {
                        theme.border_style()
                    }),
            )
            .highlight_style(if is_focused {
                theme.highlight_style()
            } else {
                theme.secondary_style()
            })
            .highlight_symbol("▶ ");

        let mut state = ratatui::widgets::ListState::default();
        state.select(Some(self.selected_index));

        frame.render_stateful_widget(list, area, &mut state);
    }
}

/// Synchronous function to load directory entries in a background thread
fn load_directory_entries_sync(current_dir: PathBuf) -> LoadingResult {
    let mut entries = Vec::new();

    // Add parent entry if not at root
    if current_dir.parent().is_some() {
        entries.push(DirEntry::Parent);
    }

    // Read directory entries synchronously
    match fs::read_dir(&current_dir) {
        Ok(dir_entries) => {
            for entry in dir_entries {
                if let Ok(entry) = entry {
                    let path = entry.path();
                    if path.is_dir() {
                        entries.push(DirEntry::Directory(path));
                    }
                }
            }
            LoadingResult {
                entries,
                error: None,
            }
        }
        Err(e) => LoadingResult {
            entries: Vec::new(),
            error: Some(format!("Failed to read directory: {}", e)),
        },
    }
}
