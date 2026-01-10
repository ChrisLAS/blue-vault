use crate::theme::Theme;
use crate::tui::directory_selector;
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Gauge, Paragraph},
};
use std::path::PathBuf;

#[derive(Debug)]
pub struct NewDiscFlow {
    disc_id: String,
    notes: String,
    source_folders: Vec<PathBuf>,
    current_step: NewDiscStep,
    input_buffer: String,
    status_message: String,
    error_message: Option<String>,
    processing_state: ProcessingState,
    /// Directory selector for folder selection step
    directory_selector: Option<directory_selector::DirectorySelector>,
    /// Whether to do a dry run (no actual burning)
    dry_run: bool,
    /// Current file being processed (for progress display)
    file_progress: String,
}

#[derive(Debug)]
pub enum ProcessingState {
    Idle,
    Staging,
    GeneratingManifest,
    CreatingISO,
    Burning,
    Indexing,
    GeneratingQR,
    Complete,
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NewDiscStep {
    EnterDiscId,
    EnterNotes,
    SelectFolders,
    Review,
    Processing,
}

impl Default for NewDiscFlow {
    fn default() -> Self {
        Self {
            disc_id: String::new(),
            notes: String::new(),
            source_folders: Vec::new(),
            current_step: NewDiscStep::EnterDiscId,
            input_buffer: String::new(),
            status_message: String::new(),
            error_message: None,
            processing_state: ProcessingState::Idle,
            directory_selector: None,
            dry_run: false,
            file_progress: String::new(),
        }
    }
}

impl NewDiscFlow {
    pub fn new(default_disc_id: String) -> Self {
        Self {
            disc_id: default_disc_id,
            notes: String::new(),
            source_folders: Vec::new(),
            current_step: NewDiscStep::EnterDiscId,
            input_buffer: String::new(),
            status_message: String::new(),
            error_message: None,
            processing_state: ProcessingState::Idle,
            directory_selector: None,
            dry_run: false,
            file_progress: String::new(),
        }
    }

    /// Initialize directory selector (call when entering SelectFolders step)
    pub fn init_directory_selector(&mut self) -> anyhow::Result<()> {
        if self.directory_selector.is_none() {
            self.directory_selector = Some(directory_selector::DirectorySelector::new()?);
        }
        Ok(())
    }

    /// Get directory selector (mutable)
    pub fn directory_selector_mut(&mut self) -> Option<&mut directory_selector::DirectorySelector> {
        self.directory_selector.as_mut()
    }

    pub fn disc_id(&self) -> &str {
        &self.disc_id
    }

    pub fn set_disc_id(&mut self, id: String) {
        self.disc_id = id;
    }

    pub fn notes(&self) -> &str {
        &self.notes
    }

    pub fn set_notes(&mut self, notes: String) {
        self.notes = notes;
    }

    pub fn source_folders(&self) -> &[PathBuf] {
        &self.source_folders
    }

    pub fn add_source_folder(&mut self, folder: PathBuf) {
        if !self.source_folders.contains(&folder) {
            self.source_folders.push(folder);
        }
    }

    pub fn remove_source_folder(&mut self, index: usize) {
        if index < self.source_folders.len() {
            self.source_folders.remove(index);
        }
    }

    pub fn current_step(&self) -> NewDiscStep {
        self.current_step
    }

    pub fn input_buffer(&self) -> &str {
        &self.input_buffer
    }

    pub fn set_input_buffer(&mut self, buffer: String) {
        self.input_buffer = buffer;
    }

    pub fn clear_input_buffer(&mut self) {
        self.input_buffer.clear();
    }

    pub fn commit_input(&mut self) {
        match self.current_step {
            NewDiscStep::EnterDiscId => {
                if !self.input_buffer.is_empty() {
                    let validation = Self::validate_disc_id(&self.input_buffer);
                    if validation.is_empty() {
                        // Valid custom ID, use it
                        self.disc_id = self.input_buffer.clone();
                    }
                    // If invalid, keep the default ID
                }
            }
            NewDiscStep::EnterNotes => {
                self.notes = self.input_buffer.clone();
            }
            _ => {}
        }
        self.input_buffer.clear();
    }

    pub fn next_step(&mut self) {
        if self.current_step == NewDiscStep::EnterDiscId && !self.input_buffer.is_empty() {
            self.commit_input();
        }
        self.current_step = match self.current_step {
            NewDiscStep::EnterDiscId => NewDiscStep::EnterNotes,
            NewDiscStep::EnterNotes => {
                self.commit_input();
                // Initialize directory selector when entering SelectFolders step
                let _ = self.init_directory_selector();
                NewDiscStep::SelectFolders
            }
            NewDiscStep::SelectFolders => NewDiscStep::Review,
            NewDiscStep::Review => NewDiscStep::Processing,
            NewDiscStep::Processing => NewDiscStep::Processing,
        };
    }

    pub fn previous_step(&mut self) {
        if self.current_step == NewDiscStep::Processing {
            return; // Can't go back during processing
        }
        self.current_step = match self.current_step {
            NewDiscStep::EnterDiscId => NewDiscStep::EnterDiscId,
            NewDiscStep::EnterNotes => NewDiscStep::EnterDiscId,
            NewDiscStep::SelectFolders => NewDiscStep::EnterNotes,
            NewDiscStep::Review => NewDiscStep::SelectFolders,
            NewDiscStep::Processing => NewDiscStep::Review,
        };
    }

    pub fn set_processing_state(&mut self, state: ProcessingState) {
        self.processing_state = state;
    }

    pub fn processing_state(&self) -> &ProcessingState {
        &self.processing_state
    }

    pub fn dry_run(&self) -> bool {
        self.dry_run
    }

    pub fn set_dry_run(&mut self, dry_run: bool) {
        self.dry_run = dry_run;
    }

    pub fn file_progress(&self) -> &str {
        &self.file_progress
    }

    pub fn set_file_progress(&mut self, progress: String) {
        self.file_progress = progress;
    }

    pub fn set_status(&mut self, message: String) {
        self.status_message = message;
    }

    /// Validate a custom disc ID for basic constraints
    fn validate_disc_id(disc_id: &str) -> String {
        // Check length
        if disc_id.is_empty() {
            return "Disc ID cannot be empty".to_string();
        }

        if disc_id.len() > 50 {
            return "Disc ID too long (max 50 characters)".to_string();
        }

        // Check for invalid characters (filesystem-safe)
        let invalid_chars = ['/', '\\', ':', '*', '?', '"', '<', '>', '|', '\0'];
        for &ch in disc_id.chars().collect::<Vec<char>>().as_slice() {
            if invalid_chars.contains(&ch) {
                return format!("Invalid character '{}' in disc ID", ch);
            }
        }

        // Check for control characters
        if disc_id
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t')
        {
            return "Disc ID contains invalid control characters".to_string();
        }

        // Check for reserved names (basic check)
        let reserved_names = [
            "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "LPT1", "LPT2", "LPT3",
            "LPT4",
        ];
        if reserved_names.contains(&disc_id.to_uppercase().as_str()) {
            return "Disc ID uses a reserved system name".to_string();
        }

        // All checks passed
        String::new()
    }

    pub fn set_error(&mut self, error: String) {
        let error_clone = error.clone();
        self.error_message = Some(error);
        self.processing_state = ProcessingState::Error(error_clone);
    }

    pub fn clear_error(&mut self) {
        self.error_message = None;
        self.processing_state = ProcessingState::Idle;
    }

    pub fn render(&mut self, theme: &Theme, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(5), Constraint::Length(3)])
            .split(area);

        let block = Block::default()
            .title("New Disc")
            .borders(Borders::ALL)
            .border_style(theme.border_style());

        match self.current_step {
            NewDiscStep::EnterDiscId => {
                let display_id = if self.input_buffer.is_empty() {
                    &self.disc_id
                } else {
                    &self.input_buffer
                };

                // Validate the current input
                let validation_msg = if !self.input_buffer.is_empty() {
                    Self::validate_disc_id(&self.input_buffer)
                } else {
                    String::new()
                };

                let id_label = if self.input_buffer.is_empty() {
                    "Disc ID (auto-generated):"
                } else {
                    "Disc ID (custom):"
                };

                let instructions = if validation_msg.is_empty() {
                    "Type to customize, [Enter] Accept, [Esc] Cancel".to_string()
                } else {
                    format!(
                        "❌ {} - [Enter] Use default '{}', [Esc] Cancel",
                        validation_msg, self.disc_id
                    )
                };

                let text = format!("{} {}\n\n{}", id_label, display_id, instructions);
                let para = Paragraph::new(text)
                    .block(block)
                    .style(if validation_msg.is_empty() {
                        theme.primary_style()
                    } else {
                        theme.error_style()
                    });
                frame.render_widget(para, chunks[0]);
            }
            NewDiscStep::EnterNotes => {
                let display_notes = if self.input_buffer.is_empty() {
                    &self.notes
                } else {
                    &self.input_buffer
                };
                let text = format!(
                    "Notes: {}\n\nType to edit, [Enter] Continue, [Esc] Back",
                    display_notes
                );
                let para = Paragraph::new(text)
                    .block(block)
                    .style(theme.primary_style());
                frame.render_widget(para, chunks[0]);
            }
            NewDiscStep::SelectFolders => {
                // Ensure directory selector is initialized
                if self.directory_selector.is_none() {
                    let _ = self.init_directory_selector();
                }

                // Split into three sections: selected folders, directory selector, instructions
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(8), // Selected folders list
                        Constraint::Min(15),   // Directory selector
                        Constraint::Length(2), // Instructions
                    ])
                    .split(chunks[0]);

                // Show selected folders at top
                let folders_text = if self.source_folders.is_empty() {
                    "No folders selected".to_string()
                } else {
                    self.source_folders
                        .iter()
                        .enumerate()
                        .map(|(i, f)| format!("{}. {}", i + 1, f.display()))
                        .collect::<Vec<_>>()
                        .join("\n")
                };

                let selected_block = Block::default()
                    .title(format!("Selected Folders ({})", self.source_folders.len()))
                    .borders(Borders::ALL)
                    .border_style(theme.border_style());

                let para = Paragraph::new(folders_text)
                    .block(selected_block)
                    .style(theme.primary_style());
                frame.render_widget(para, chunks[0]);

                // Render directory selector (always visible)
                if let Some(ref mut selector) = self.directory_selector {
                    // Render returns true if entries were just loaded (triggers redraw)
                    let needs_redraw = selector.render(theme, frame, chunks[1]);
                    if needs_redraw {
                        // Force a redraw if entries were just loaded
                        // This is handled by the main loop, but we can trigger it
                    }
                } else {
                    // Fallback if selector initialization failed - show message
                    let text =
                        "Directory selector initialization failed.\nPress any key to continue...";
                    let para = Paragraph::new(text)
                        .block(
                            Block::default()
                                .title("Directory Selector")
                                .borders(Borders::ALL)
                                .border_style(theme.error_style()),
                        )
                        .style(theme.error_style());
                    frame.render_widget(para, chunks[1]);
                }

                // Instructions
                let instructions = format!(
                    "[Tab] Switch focus  [Enter] Select/Add  [↑↓] Navigate  [Del] Remove  [Esc] Back"
                );
                let inst_para = Paragraph::new(instructions).style(theme.secondary_style());
                frame.render_widget(inst_para, chunks[2]);
            }
            NewDiscStep::Review => {
                let folders_list = self
                    .source_folders
                    .iter()
                    .map(|f| f.display().to_string())
                    .collect::<Vec<_>>()
                    .join("\n  ");
                let mode = if self.dry_run {
                    "DRY RUN (no burning)"
                } else {
                    "ACTUAL BURN"
                };
                let text = format!(
                    "Review:\n\nDisc ID: {}\nNotes: {}\n\nSource Folders:\n  {}\n\nMode: {}\n\n[Enter] Start, [D] Toggle Dry Run, [Esc] Back",
                    self.disc_id,
                    if self.notes.is_empty() { "(none)" } else { &self.notes },
                    if folders_list.is_empty() { "(none)" } else { &folders_list },
                    mode
                );
                let para = Paragraph::new(text)
                    .block(block)
                    .style(theme.primary_style());
                frame.render_widget(para, chunks[0]);
            }
            NewDiscStep::Processing => {
                let status = match &self.processing_state {
                    ProcessingState::Idle => "Ready",
                    ProcessingState::Staging => "Staging files...",
                    ProcessingState::GeneratingManifest => "Generating manifest...",
                    ProcessingState::CreatingISO => "Creating ISO image...",
                    ProcessingState::Burning => "Burning to disc...",
                    ProcessingState::Indexing => "Updating index...",
                    ProcessingState::GeneratingQR => "Generating QR code...",
                    ProcessingState::Complete => "Complete!",
                    ProcessingState::Error(msg) => {
                        return self.render_error(theme, frame, area, msg)
                    }
                };

                // Split into main content and activity area
                let processing_chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Min(8),
                        Constraint::Length(6), // Disc activity
                    ])
                    .split(chunks[0]);

                let base_text = if self.file_progress.is_empty() {
                    format!("Status: {}\n\n{}", status, self.status_message)
                } else {
                    format!(
                        "Status: {}\n\n{}\n\n{}",
                        status, self.status_message, self.file_progress
                    )
                };

                let text = if matches!(self.processing_state, ProcessingState::Complete) {
                    format!("{}\n\n[Esc] Return to Main Menu", base_text)
                } else if matches!(self.processing_state, ProcessingState::Error(_)) {
                    format!("{}\n\n[Esc] Go Back", base_text)
                } else {
                    base_text
                };
                let para = Paragraph::new(text)
                    .block(block)
                    .style(theme.primary_style());
                frame.render_widget(para, processing_chunks[0]);

                // Disc activity indicator for long operations
                if matches!(
                    &self.processing_state,
                    ProcessingState::GeneratingManifest
                        | ProcessingState::CreatingISO
                        | ProcessingState::Burning
                ) {
                    use crate::ui::disc_activity::{DiscActivity, DiscOperation};
                    let mut disc_activity = DiscActivity::new();
                    disc_activity.set_operation(
                        if matches!(&self.processing_state, ProcessingState::Burning) {
                            DiscOperation::Writing
                        } else {
                            DiscOperation::Reading // For manifest generation and ISO creation
                        },
                    );

                    // Simulate LBA progress
                    let progress = match &self.processing_state {
                        ProcessingState::CreatingISO => 50,
                        ProcessingState::Burning => 75,
                        _ => 0,
                    };
                    disc_activity.set_lba((progress as u64) * 1000, 100000);
                    disc_activity.set_buffer(progress as f64 / 100.0);
                    disc_activity.update();
                    disc_activity.render(theme, processing_chunks[1], frame);
                } else {
                    // Progress bar for other operations
                    let progress = match &self.processing_state {
                        ProcessingState::Staging => 10,
                        ProcessingState::GeneratingManifest => 30,
                        ProcessingState::CreatingISO => 50,
                        ProcessingState::Burning => 70,
                        ProcessingState::Indexing => 90,
                        ProcessingState::GeneratingQR => 95,
                        ProcessingState::Complete => 100,
                        _ => 0,
                    };
                    let gauge = Gauge::default()
                        .block(
                            Block::default()
                                .title("Progress")
                                .borders(Borders::ALL)
                                .border_style(theme.border_style()),
                        )
                        .gauge_style(theme.primary_style())
                        .percent(progress);
                    frame.render_widget(gauge, processing_chunks[1]);
                }

                // Overall progress bar at bottom
                let progress = match &self.processing_state {
                    ProcessingState::Staging => 10,
                    ProcessingState::GeneratingManifest => 30,
                    ProcessingState::CreatingISO => 50,
                    ProcessingState::Burning => 70,
                    ProcessingState::Indexing => 90,
                    ProcessingState::GeneratingQR => 95,
                    ProcessingState::Complete => 100,
                    _ => 0,
                };
                let gauge = Gauge::default()
                    .block(
                        Block::default()
                            .title("Overall Progress")
                            .borders(Borders::ALL)
                            .border_style(theme.border_style()),
                    )
                    .gauge_style(theme.primary_style())
                    .percent(progress);
                frame.render_widget(gauge, chunks[1]);
            }
        }
    }

    fn render_error(&self, theme: &Theme, frame: &mut Frame, area: Rect, error: &str) {
        let text = format!("[ERR] {}\n\n[Esc] Go back", error);
        let para = Paragraph::new(text)
            .block(
                Block::default()
                    .title("Error")
                    .borders(Borders::ALL)
                    .border_style(theme.border_style()),
            )
            .style(theme.error_style());
        frame.render_widget(para, area);
    }
}
