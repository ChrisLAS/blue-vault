// Simplified version that we'll refine based on actual fpicker API
// For now, let's create a working prototype with a placeholder for fpicker
// that we can replace once we understand the actual API

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, List, ListItem},
};
use std::path::{Path, PathBuf};
use std::fs;
use crate::theme::Theme;

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
    Parent,  // ".." to go up
    Directory(PathBuf),
}

impl Default for DirectorySelector {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
        
        Self {
            input_buffer: String::new(),
            focus: Focus::Input,
            current_dir: home.clone(),
            entries: Vec::new(),  // Lazy load entries - don't load on default
            selected_index: 0,
            error_message: None,
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
    pub fn ensure_entries_loaded(&mut self) -> anyhow::Result<bool> {
        if self.entries.is_empty() {
            self.refresh_entries()?;
            Ok(true)  // Entries were just loaded
        } else {
            Ok(false)  // Entries already loaded
        }
    }

    /// Refresh directory entries
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
        self.entries.sort_by(|a, b| {
            match (a, b) {
                (DirEntry::Parent, _) => std::cmp::Ordering::Less,
                (_, DirEntry::Parent) => std::cmp::Ordering::Greater,
                (DirEntry::Directory(a), DirEntry::Directory(b)) => {
                    a.file_name()
                        .unwrap_or_default()
                        .cmp(&b.file_name().unwrap_or_default())
                }
            }
        });
        
        // Reset selection
        if self.selected_index >= self.entries.len() && !self.entries.is_empty() {
            self.selected_index = self.entries.len() - 1;
        }
        
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
                        self.refresh_entries()?;
                        self.selected_index = 0;
                        // Update input buffer to match
                        self.input_buffer = self.current_dir.display().to_string();
                    }
                }
                DirEntry::Directory(path) => {
                    let new_path = path.clone();
                    let path_str = new_path.display().to_string();
                    self.current_dir = new_path;
                    self.refresh_entries()?;
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
            self.refresh_entries()?;
            self.selected_index = 0;
            self.error_message = None;
            Ok(())
        } else {
            self.error_message = Some(format!("Path does not exist or is not a directory: {}", path.display()));
            Err(anyhow::anyhow!("Invalid path"))
        }
    }

    /// Commit the current input buffer as the selected path
    pub fn commit_input(&mut self) -> anyhow::Result<PathBuf> {
        let path_str = self.input_buffer.trim();
        if path_str.is_empty() {
            return Err(anyhow::anyhow!("Path cannot be empty"));
        }

        let path = PathBuf::from(path_str);
        
        if !path.exists() {
            self.error_message = Some(format!("Path does not exist: {}", path.display()));
            return Err(anyhow::anyhow!("Path does not exist"));
        }

        if !path.is_dir() {
            self.error_message = Some(format!("Path is not a directory: {}", path.display()));
            return Err(anyhow::anyhow!("Not a directory"));
        }

        // Sync to browser
        self.set_current_path(path.clone())?;
        Ok(path)
    }

    /// Get error message
    pub fn error_message(&self) -> Option<&str> {
        self.error_message.as_deref()
    }

    /// Clear error message
    pub fn clear_error(&mut self) {
        self.error_message = None;
    }

    /// Render the dual-mode selector
    /// Returns true if entries were just loaded (to trigger a redraw)
    pub fn render(&mut self, theme: &Theme, frame: &mut Frame, area: Rect) -> bool {
        use ratatui::layout::{Layout, Direction, Constraint};
        
        // Only load entries when browser is focused or if entries are needed
        // This prevents blocking during initialization
        let entries_just_loaded = if self.focus == Focus::Browser && self.entries.is_empty() {
            // Load entries if browser is focused (or about to be focused)
            self.ensure_entries_loaded().unwrap_or(false)
        } else {
            false
        };
        
        // Split area: input box at top, browser below
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(4),  // Input box
                Constraint::Min(10),    // Browser
            ])
            .split(area);

        // Always render input box (always visible)
        self.render_input_box(theme, frame, chunks[0]);

        // Always render browser (always visible, shows loading if not loaded yet)
        self.render_browser(theme, frame, chunks[1]);
        
        entries_just_loaded  // Return true if we just loaded entries (triggers redraw)
    }

    fn render_input_box(&self, theme: &Theme, frame: &mut Frame, area: Rect) {
        let is_focused = self.focus == Focus::Input;
        
        let input_display = if self.input_buffer.is_empty() {
            "Enter folder path..."
        } else {
            &self.input_buffer
        };

        let error_line = if let Some(ref error) = self.error_message {
            format!("\n[ERR] {}", error)
        } else {
            String::new()
        };

        let text = format!(
            "Folder Path:\n{}\n{}",
            input_display,
            error_line
        );

        // Always show input box with clear focus indication
        let border_style = if is_focused {
            theme.primary_style().add_modifier(ratatui::style::Modifier::BOLD)
        } else {
            theme.border_style()
        };

        let title = if is_focused { 
            "Folder Path [FOCUSED] - Type path and press Enter" 
        } else { 
            "Folder Path - Press Tab to focus here"
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(border_style);

        let para = Paragraph::new(text)
            .block(block)
            .style(if is_focused {
                theme.primary_style()
            } else {
                theme.secondary_style()
            });

        frame.render_widget(para, area);
    }

    fn render_browser(&mut self, theme: &Theme, frame: &mut Frame, area: Rect) {
        let is_focused = self.focus == Focus::Browser;
        
        // Show loading message if entries not loaded yet
        if self.entries.is_empty() {
            let loading_text = format!("Loading directory: {}", self.current_dir.display());
            let para = Paragraph::new(loading_text)
                .block(
                    Block::default()
                        .title("Directory Browser")
                        .borders(Borders::ALL)
                        .border_style(theme.border_style())
                )
                .style(theme.dim_style());
            frame.render_widget(para, area);
            return;
        }
        
        // Create list items from directory entries
        let items: Vec<ListItem> = self.entries
            .iter()
            .map(|entry| {
                let display_name = match entry {
                    DirEntry::Parent => "..".to_string(),
                    DirEntry::Directory(path) => {
                        path.file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string()
                    }
                };
                
                ListItem::new(display_name)
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .title(if is_focused { 
                        format!("Directory Browser [FOCUSED] - {}", self.current_dir.display())
                    } else { 
                        format!("Directory Browser - {}", self.current_dir.display())
                    })
                    .borders(Borders::ALL)
                    .border_style(if is_focused {
                        theme.primary_style().add_modifier(ratatui::style::Modifier::BOLD)
                    } else {
                        theme.border_style()
                    })
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

