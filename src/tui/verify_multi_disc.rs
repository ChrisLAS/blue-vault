use crate::database;
use crate::theme::Theme;
use crate::verify::{DiscVerificationStatus, MultiDiscVerificationResult};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

#[derive(Debug)]
pub struct VerifyMultiDiscUI {
    disc_sets: Vec<database::DiscSet>,
    selected_index: usize,
    verification_result: Option<MultiDiscVerificationResult>,
    verification_state: VerificationState,
    status_message: String,
    error_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum VerificationState {
    SelectingSet,
    Verifying,
    Complete,
    Error(String),
}

impl VerifyMultiDiscUI {
    pub fn new() -> Self {
        Self {
            disc_sets: Vec::new(),
            selected_index: 0,
            verification_result: None,
            verification_state: VerificationState::SelectingSet,
            status_message: "Select a multi-disc set to verify".to_string(),
            error_message: None,
        }
    }

    pub fn set_disc_sets(&mut self, sets: Vec<database::DiscSet>) {
        self.disc_sets = sets;
        self.selected_index = 0;
    }

    pub fn set_verification_result(&mut self, result: MultiDiscVerificationResult) {
        self.verification_result = Some(result);
        self.verification_state = VerificationState::Complete;
    }

    pub fn set_error(&mut self, error: String) {
        self.error_message = Some(error.clone());
        self.verification_state = VerificationState::Error(error);
    }

    pub fn set_status(&mut self, status: String) {
        self.status_message = status;
    }

    pub fn next(&mut self) {
        if !self.disc_sets.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.disc_sets.len();
        }
    }

    pub fn previous(&mut self) {
        if !self.disc_sets.is_empty() {
            if self.selected_index == 0 {
                self.selected_index = self.disc_sets.len() - 1;
            } else {
                self.selected_index -= 1;
            }
        }
    }

    pub fn selected_set(&self) -> Option<&database::DiscSet> {
        self.disc_sets.get(self.selected_index)
    }

    pub fn is_selecting(&self) -> bool {
        matches!(self.verification_state, VerificationState::SelectingSet)
    }

    pub fn is_complete(&self) -> bool {
        matches!(self.verification_state, VerificationState::Complete)
    }

    pub fn render(&mut self, theme: &Theme, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // Title/status
                Constraint::Min(1),    // Main content
                Constraint::Length(3), // Help/status
            ])
            .split(area);

        // Title/status bar
        let title = match self.verification_state {
            VerificationState::SelectingSet => "🔍 Multi-Disc Set Verification",
            VerificationState::Verifying => "🔄 Verifying Multi-Disc Set",
            VerificationState::Complete => "✅ Verification Complete",
            VerificationState::Error(_) => "❌ Verification Error",
        };

        let title_para = Paragraph::new(title)
            .style(theme.highlight_style())
            .alignment(Alignment::Center);
        frame.render_widget(title_para, chunks[0]);

        // Main content
        match self.verification_state {
            VerificationState::SelectingSet => self.render_set_selection(theme, frame, chunks[1]),
            VerificationState::Verifying => self.render_verification_progress(theme, frame, chunks[1]),
            VerificationState::Complete => self.render_verification_results(theme, frame, chunks[1]),
            VerificationState::Error(ref err) => self.render_error(theme, frame, chunks[1], err),
        }

        // Help/status bar
        let help_text = match self.verification_state {
            VerificationState::SelectingSet => {
                if self.disc_sets.is_empty() {
                    "No multi-disc sets found. Create one first."
                } else {
                    "↑/↓: Navigate  Enter: Verify set  Esc: Back"
                }
            }
            VerificationState::Verifying => "Verifying discs... Please wait.",
            VerificationState::Complete => "Esc: Back to main menu",
            VerificationState::Error(_) => "Esc: Back to set selection",
        };

        let help_para = Paragraph::new(help_text)
            .style(theme.secondary_style())
            .alignment(Alignment::Center);
        frame.render_widget(help_para, chunks[2]);
    }

    fn render_set_selection(&self, theme: &Theme, frame: &mut Frame, area: Rect) {
        if self.disc_sets.is_empty() {
            let para = Paragraph::new("No multi-disc sets found.\n\nCreate a multi-disc archive first to use this verification feature.")
                .style(theme.secondary_style())
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true });
            frame.render_widget(para, area);
            return;
        }

        let items: Vec<ListItem> = self.disc_sets.iter().enumerate().map(|(i, set)| {
            let disc_count_text = if set.disc_count == 1 {
                "1 disc".to_string()
            } else {
                format!("{} discs", set.disc_count)
            };

            let size_mb = set.total_size / (1024 * 1024);
            let item_text = format!(
                "{} - {} ({} MB)",
                set.name, disc_count_text, size_mb
            );

            let mut style = theme.secondary_style();
            if i == self.selected_index {
                style = theme.highlight_style();
            }

            ListItem::new(item_text).style(style)
        }).collect();

        let list = List::new(items)
            .block(Block::default().borders(Borders::NONE).title("Available Multi-Disc Sets"))
            .highlight_symbol("▶ ");

        let mut list_state = ListState::default();
        list_state.select(Some(self.selected_index));
        frame.render_stateful_widget(list, area, &mut list_state);
    }

    fn render_verification_progress(&self, theme: &Theme, frame: &mut Frame, area: Rect) {
        let progress_text = format!("{}\n\n{}", self.status_message, "⏳ Checking discs...");

        let para = Paragraph::new(progress_text)
            .style(theme.secondary_style())
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });
        frame.render_widget(para, area);
    }

    fn render_verification_results(&self, theme: &Theme, frame: &mut Frame, area: Rect) {
        if let Some(ref result) = self.verification_result {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3), // Summary
                    Constraint::Min(1),    // Disc details
                ])
                .split(area);

            // Summary
            let summary_lines = vec![
                format!("Set: {}", result.set_name),
                format!("Total Discs: {}", result.total_discs),
                format!("Verified: {}  Failed: {}  Missing: {}",
                    result.discs_verified, result.discs_failed, result.discs_missing),
                format!("Files Checked: {}  Failed: {}",
                    result.total_files_checked, result.total_files_failed),
            ];

            let status_icon = if result.overall_success { "✅" } else { "❌" };
            let summary_text = format!("{} {}\n{}",
                status_icon,
                if result.overall_success { "VERIFICATION SUCCESSFUL" } else { "VERIFICATION ISSUES FOUND" },
                summary_lines.join("\n")
            );

            let summary_para = Paragraph::new(summary_text)
                .style(if result.overall_success { theme.success_style() } else { theme.error_style() })
                .wrap(Wrap { trim: true });
            frame.render_widget(summary_para, chunks[0]);

            // Disc details
            let disc_items: Vec<ListItem> = result.disc_results.iter().map(|(disc_id, status)| {
                let (status_icon, status_text, style) = match status {
                    DiscVerificationStatus::Verified { files_checked, files_failed } => {
                        ("✅", format!("Verified ({} files, {} failed)", files_checked, files_failed), theme.success_style())
                    }
                    DiscVerificationStatus::Failed { error } => {
                        ("❌", format!("Failed: {}", error), theme.error_style())
                    }
                    DiscVerificationStatus::Missing => {
                        ("⚠️", "Missing/Not Found".to_string(), theme.warning_style())
                    }
                    DiscVerificationStatus::NotAttempted => {
                        ("⏭️", "Skipped".to_string(), theme.secondary_style())
                    }
                };

                let item_text = format!("{} {} - {}", status_icon, disc_id, status_text);
                ListItem::new(item_text).style(style)
            }).collect();

            let disc_list = List::new(disc_items)
                .block(Block::default().borders(Borders::NONE).title("Disc Verification Details"))
                .highlight_symbol("");

            frame.render_widget(disc_list, chunks[1]);
        }
    }

    fn render_error(&self, theme: &Theme, frame: &mut Frame, area: Rect, error: &str) {
        let error_text = format!("❌ Verification Error\n\n{}", error);

        let para = Paragraph::new(error_text)
            .style(theme.error_style())
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });
        frame.render_widget(para, area);
    }
}
