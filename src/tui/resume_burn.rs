use crate::database;
use crate::theme::Theme;
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

#[derive(Debug, Clone)]
pub struct ResumeBurnUI {
    sessions: Vec<database::BurnSession>,
    selected_index: usize,
    cleanup_mode: bool,
    message: Option<String>,
}

impl ResumeBurnUI {
    pub fn new() -> Self {
        Self {
            sessions: Vec::new(),
            selected_index: 0,
            cleanup_mode: false,
            message: None,
        }
    }

    pub fn set_sessions(&mut self, sessions: Vec<database::BurnSession>) {
        self.sessions = sessions;
        self.selected_index = 0;
    }

    pub fn set_message(&mut self, message: String) {
        self.message = Some(message);
    }

    pub fn selected_session(&self) -> Option<database::BurnSession> {
        if self.cleanup_mode || self.sessions.is_empty() {
            None
        } else {
            self.sessions.get(self.selected_index).cloned()
        }
    }

    pub fn selected_session_for_cleanup(&self) -> Option<String> {
        if !self.cleanup_mode || self.sessions.is_empty() {
            None
        } else {
            self.sessions.get(self.selected_index).map(|s| s.session_id.clone())
        }
    }

    pub fn is_cleanup_mode(&self) -> bool {
        self.cleanup_mode
    }

    pub fn toggle_cleanup_mode(&mut self) {
        self.cleanup_mode = !self.cleanup_mode;
        self.selected_index = 0;
    }

    pub fn next(&mut self) {
        if !self.sessions.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.sessions.len();
        }
    }

    pub fn previous(&mut self) {
        if !self.sessions.is_empty() {
            if self.selected_index == 0 {
                self.selected_index = self.sessions.len() - 1;
            } else {
                self.selected_index -= 1;
            }
        }
    }

    pub fn render(&mut self, theme: &Theme, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .title("Resume Paused Burn")
            .borders(Borders::ALL)
            .border_style(theme.border_style());

        let inner_area = block.inner(area);
        frame.render_widget(block, area);

        if let Some(message) = &self.message {
            // Show message when no sessions available
            let paragraph = Paragraph::new(message.as_str())
                .style(theme.secondary_style())
                .wrap(Wrap { trim: true });
            frame.render_widget(paragraph, inner_area);
            return;
        }

        if self.sessions.is_empty() {
            let paragraph = Paragraph::new("No paused burn sessions found.\n\nStart a new multi-disc archive to create resumable sessions.")
                .style(theme.secondary_style())
                .wrap(Wrap { trim: true });
            frame.render_widget(paragraph, inner_area);
            return;
        }

        // Split the area for mode indicator and session list
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // Mode indicator
                Constraint::Min(1),    // Session list
                Constraint::Length(3), // Help text
            ])
            .split(inner_area);

        // Mode indicator
        let mode_text = if self.cleanup_mode {
            "🗑️  CLEANUP MODE - Select session to delete"
        } else {
            "⏸️  RESUME MODE - Select session to continue"
        };
        let mode_para = Paragraph::new(mode_text)
            .style(theme.highlight_style())
            .alignment(Alignment::Center);
        frame.render_widget(mode_para, chunks[0]);

        // Session list
        let mut list_items: Vec<ListItem> = self.sessions.iter().enumerate().map(|(i, session)| {
            let completed = session.completed_discs.len();
            let total = session.total_discs;
            let status_icon = match session.status {
                database::BurnSessionStatus::Active => "🔄",
                database::BurnSessionStatus::Paused => "⏸️",
                database::BurnSessionStatus::Completed => "✅",
                database::BurnSessionStatus::Cancelled => "❌",
            };

            let item_text = format!(
                "{} {} - Disc {}/{} ({:.1}%) - {}",
                status_icon,
                session.session_name,
                session.current_disc,
                total,
                if total > 0 { (completed as f64 / total as f64) * 100.0 } else { 0.0 },
                Self::format_time_ago(&session.updated_at)
            );

            let mut style = theme.secondary_style();
            if i == self.selected_index {
                style = theme.highlight_style();
            }

            ListItem::new(item_text).style(style)
        }).collect();

        // Add space usage information if in cleanup mode
        if self.cleanup_mode {
            if let Ok(space_usage) = database::BurnSessionOps::get_sessions_space_usage(&crate::database::init_database(&dirs::data_dir().unwrap().join("bdarchive").join("database.db")).unwrap()) {
                let space_mb = space_usage / (1024 * 1024);
                list_items.push(ListItem::new(format!("💾 Total temporary space used: {} MB", space_mb))
                    .style(theme.secondary_style()));
            }
        }

        let list = List::new(list_items)
            .block(Block::default().borders(Borders::NONE))
            .highlight_symbol("▶ ");

        let mut list_state = ListState::default();
        list_state.select(Some(self.selected_index));
        frame.render_stateful_widget(list, chunks[1], &mut list_state);

        // Help text
        let help_text = if self.cleanup_mode {
            "↑/↓: Navigate  Enter: Delete session  'c': Resume mode  Esc: Back"
        } else {
            "↑/↓: Navigate  Enter: Resume session  'c': Cleanup mode  Esc: Back"
        };
        let help_para = Paragraph::new(help_text)
            .style(theme.secondary_style())
            .alignment(Alignment::Center);
        frame.render_widget(help_para, chunks[2]);
    }

    fn format_time_ago(timestamp: &str) -> String {
        // Simple time formatting - could be enhanced with proper date parsing
        format!("updated {}", timestamp)
    }
}
