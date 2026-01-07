use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph},
};
use crate::theme::Theme;

#[derive(Debug, Clone)]
pub struct LogsView {
    // Placeholder for logs viewer
}

impl Default for LogsView {
    fn default() -> Self {
        Self {}
    }
}

impl LogsView {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn render(&self, theme: &Theme, frame: &mut Frame, area: Rect) {
        let text = "Logs / Recent Runs\n\n[Esc] Back to menu";
        let para = Paragraph::new(text)
            .block(
                Block::default()
                    .title("Logs")
                    .borders(Borders::ALL)
                    .border_style(theme.border_style())
            )
            .style(theme.primary_style());
        frame.render_widget(para, area);
    }
}

