use crate::theme::{no_animations, reduced_motion, Theme, ThemeName};
use ratatui::{
    prelude::*,
    style::Modifier,
    widgets::{Block, Borders, Paragraph},
};

#[derive(Debug, Clone)]
pub struct Settings {
    // Placeholder for settings UI
}

impl Default for Settings {
    fn default() -> Self {
        Self {}
    }
}

impl Settings {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn render(&self, theme: &Theme, frame: &mut Frame, area: Rect) {
        use ratatui::layout::{Constraint, Direction, Layout};

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(area);

        let title = Paragraph::new("Settings")
            .block(
                Block::default()
                    .title("Settings")
                    .borders(Borders::ALL)
                    .border_style(theme.border_style()),
            )
            .style(theme.primary_style().add_modifier(Modifier::BOLD));
        frame.render_widget(title, chunks[0]);

        // Display current settings
        let theme_name = match theme.name {
            ThemeName::Phosphor => "Phosphor (default)",
            ThemeName::Amber => "Amber",
            ThemeName::Mono => "Monochrome",
        };

        let motion_status = if no_animations() {
            "Disabled"
        } else if reduced_motion() {
            "Reduced"
        } else {
            "Full"
        };

        let settings_text = format!(
            "Theme: {}\n\nMotion:\n  Animations: {}\n  Reduced Motion: {}\n\nEnvironment Variables:\n  TUI_THEME={}\n  TUI_NO_ANIM={}\n  TUI_REDUCED_MOTION={}\n\n[Esc] Back to menu",
            theme_name,
            motion_status,
            if reduced_motion() { "Yes" } else { "No" },
            std::env::var("TUI_THEME").unwrap_or_else(|_| "(not set)".to_string()),
            std::env::var("TUI_NO_ANIM").unwrap_or_else(|_| "(not set)".to_string()),
            std::env::var("TUI_REDUCED_MOTION").unwrap_or_else(|_| "(not set)".to_string())
        );

        let para = Paragraph::new(settings_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(theme.border_style()),
            )
            .style(theme.primary_style());
        frame.render_widget(para, chunks[1]);
    }
}
