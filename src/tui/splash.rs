use crate::theme::Theme;
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph},
};
use std::path::PathBuf;
use std::time::Instant;

/// Startup splash screen
pub struct SplashScreen {
    created_at: Instant,
    db_path: PathBuf,
    disc_count: usize,
    db_status: DbStatus,
    skipped: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbStatus {
    Ok,
    NeedsMigration,
    ReadOnly,
    Error,
}

impl SplashScreen {
    pub fn new(db_path: PathBuf, disc_count: usize, db_status: DbStatus) -> Self {
        Self {
            created_at: Instant::now(),
            db_path,
            disc_count,
            db_status,
            skipped: false,
        }
    }

    /// Check if splash should be shown (<= 1 second)
    pub fn should_show(&self) -> bool {
        !self.skipped && self.created_at.elapsed() < std::time::Duration::from_secs(1)
    }

    /// Mark splash as skipped
    pub fn skip(&mut self) {
        self.skipped = true;
    }

    pub fn render(&self, theme: &Theme, area: Rect, frame: &mut Frame) {
        let center_area = crate::ui::layout::GridLayout::centered_dialog(area, 70, 12);

        let status_text = match self.db_status {
            DbStatus::Ok => format!("[OK] {}", self.disc_count),
            DbStatus::NeedsMigration => "[NEEDS MIGRATION]".to_string(),
            DbStatus::ReadOnly => "[READ-ONLY]".to_string(),
            DbStatus::Error => "[ERROR]".to_string(),
        };

        let status_style = match self.db_status {
            DbStatus::Ok => theme.success_style(),
            DbStatus::NeedsMigration | DbStatus::ReadOnly => theme.warning_style(),
            DbStatus::Error => theme.error_style(),
        };

        let discs_text = if self.db_status == DbStatus::Ok {
            self.disc_count.to_string()
        } else {
            "N/A".to_string()
        };

        // Build styled text with color for status
        use ratatui::text::{Line, Span, Text};
        let splash_text = Text::from(vec![
            Line::from(format!("BlueVault v{}", env!("CARGO_PKG_VERSION"))),
            Line::from(""),
            Line::from(format!("Database: {}", self.db_path.display())),
            Line::from(vec![
                Span::styled("Status: ", theme.primary_style()),
                Span::styled(&status_text, status_style),
            ]),
            Line::from(format!("Discs Indexed: {}", discs_text)),
            Line::from(""),
            Line::from("Press any key to continue..."),
        ]);

        let paragraph = Paragraph::new(splash_text).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme.border_style())
                .title("Cold Boot"),
        );

        frame.render_widget(paragraph, center_area);
    }
}
