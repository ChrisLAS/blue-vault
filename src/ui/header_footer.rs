use crate::theme::Theme;
use crate::ui::animations::Spinner;
use ratatui::{
    prelude::*,
    style::Modifier,
    widgets::{Block, Borders, Paragraph},
};

/// Header widget showing app name, current screen, and hint
pub struct Header {
    current_screen: String,
    hint: String,
}

impl Header {
    pub fn new(current_screen: impl Into<String>) -> Self {
        Self {
            current_screen: current_screen.into(),
            hint: String::new(),
        }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = hint.into();
        self
    }

    pub fn render(&self, theme: &Theme, area: Rect, frame: &mut Frame) {
        let title = format!("BlueVault v{}", env!("CARGO_PKG_VERSION"));
        let screen_text = format!(" │ {} ", self.current_screen);
        let hint_text = if !self.hint.is_empty() {
            format!(" │ {}", self.hint)
        } else {
            String::new()
        };

        let header_text = format!("{}{}{}", title, screen_text, hint_text);

        let paragraph = Paragraph::new(header_text)
            .style(theme.primary_style().add_modifier(Modifier::BOLD))
            .block(
                Block::default()
                    .borders(Borders::BOTTOM)
                    .border_style(theme.border_style()),
            );

        frame.render_widget(paragraph, area);
    }
}

/// Footer widget showing keybind hints and status
pub struct Footer {
    left_hint: String,
    right_hint: String,
    status: FooterStatus,
    spinner: Option<Spinner>,
}

#[derive(Debug, Clone)]
pub enum FooterStatus {
    Ready,
    Processing(String),
    Success(String),
    Warning(String),
    Error(String),
}

impl Footer {
    pub fn new() -> Self {
        Self {
            left_hint: "↑↓/jk: nav  Enter: select  Esc/q: back".to_string(),
            right_hint: String::new(),
            status: FooterStatus::Ready,
            spinner: None,
        }
    }

    pub fn with_hints(mut self, left: impl Into<String>, right: impl Into<String>) -> Self {
        self.left_hint = left.into();
        self.right_hint = right.into();
        self
    }

    pub fn set_status(&mut self, status: FooterStatus) {
        self.status = status.clone();

        // Show spinner for processing
        match status {
            FooterStatus::Processing(_) => {
                if self.spinner.is_none() {
                    self.spinner = Some(Spinner::new());
                }
            }
            _ => {
                self.spinner = None;
            }
        }
    }

    pub fn update(&mut self) {
        if let Some(ref mut spinner) = self.spinner {
            let _ = spinner.next();
        }
    }

    pub fn render(&self, theme: &Theme, area: Rect, frame: &mut Frame) {
        // Build status text
        let (status_text, status_style) = match &self.status {
            FooterStatus::Ready => (String::new(), theme.secondary_style()),
            FooterStatus::Processing(msg) => {
                let spinner_text = self
                    .spinner
                    .as_ref()
                    .map(|s| format!("{} ", s.current()))
                    .unwrap_or_default();
                let text = format!("{}{}", spinner_text, msg);
                (text, theme.primary_style())
            }
            FooterStatus::Success(msg) => (msg.clone(), theme.success_style()),
            FooterStatus::Warning(msg) => (format!("[WARN] {}", msg), theme.warning_style()),
            FooterStatus::Error(msg) => (format!("[ERR] {}", msg), theme.error_style()),
        };

        // Combine hints and status
        let left_padding = "  ";
        let right_hint_part = if !self.right_hint.is_empty() {
            format!(" │ {}", self.right_hint)
        } else {
            String::new()
        };
        let footer_text = format!(
            "{}{} │ {}{}  ",
            left_padding, self.left_hint, status_text, right_hint_part
        );

        let paragraph = Paragraph::new(footer_text).style(status_style).block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(theme.border_style()),
        );

        frame.render_widget(paragraph, area);
    }
}

impl Default for Footer {
    fn default() -> Self {
        Self::new()
    }
}
