use crate::theme::Theme;
use crate::ui::animations::{AnimationThrottle, Spinner};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph},
};

/// Disc read/write activity indicator (80s style)
pub struct DiscActivity {
    spinner: Spinner,
    throttle: AnimationThrottle,
    lba: u64,
    lba_target: u64,
    buffer: f64, // 0.0 to 1.0
    operation: DiscOperation,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DiscOperation {
    Reading,
    Writing,
    Verifying,
    Idle,
}

impl DiscActivity {
    pub fn new() -> Self {
        Self {
            spinner: Spinner::blocks(),
            throttle: AnimationThrottle::new(8), // 8 FPS for disc activity
            lba: 0,
            lba_target: 0,
            buffer: 0.0,
            operation: DiscOperation::Idle,
        }
    }

    pub fn set_operation(&mut self, op: DiscOperation) {
        self.operation = op;
        self.throttle.reset();
    }

    pub fn set_lba(&mut self, current: u64, target: u64) {
        self.lba = current;
        self.lba_target = target;
    }

    pub fn set_buffer(&mut self, percent: f64) {
        self.buffer = percent.max(0.0).min(1.0);
    }

    pub fn update(&mut self) {
        if self.operation == DiscOperation::Idle {
            return;
        }

        if self.throttle.should_render() {
            let _ = self.spinner.next();
        }
    }

    pub fn render(&self, theme: &Theme, area: Rect, frame: &mut Frame) {
        if self.operation == DiscOperation::Idle {
            return;
        }

        let op_symbol = match self.operation {
            DiscOperation::Reading => "▶",
            DiscOperation::Writing => "◀",
            DiscOperation::Verifying => "○",
            DiscOperation::Idle => " ",
        };

        let op_text = match self.operation {
            DiscOperation::Reading => "READ",
            DiscOperation::Writing => "WRITE",
            DiscOperation::Verifying => "VERIFY",
            DiscOperation::Idle => "",
        };

        // Disc icon with spinner
        let spinner_frame = self.spinner.current();
        let disc_icon = format!("{} {}", spinner_frame, op_symbol);

        // LBA counter
        let lba_text = if self.lba_target > 0 {
            let percent = (self.lba as f64 / self.lba_target as f64 * 100.0) as u8;
            format!(
                "LBA {:06} → {:06} ({:3}%)",
                self.lba, self.lba_target, percent
            )
        } else {
            format!("LBA {:06}", self.lba)
        };

        // Buffer indicator
        let buffer_percent = (self.buffer * 100.0) as u8;
        let buffer_bar = create_mini_bar(buffer_percent, 10);
        let buffer_text = format!("BUF {:3}% {}", buffer_percent, buffer_bar);

        // Combine into status line
        let status_text = format!("{} {} │ {} │ {}", disc_icon, op_text, lba_text, buffer_text);

        let paragraph = Paragraph::new(status_text)
            .style(theme.primary_style())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(theme.border_style())
                    .title("Disc Activity"),
            );

        frame.render_widget(paragraph, area);
    }
}

impl Default for DiscActivity {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a mini progress bar
fn create_mini_bar(percent: u8, width: usize) -> String {
    let filled = (percent as usize * width) / 100;
    let empty = width.saturating_sub(filled);

    format!("[{}{}]", "█".repeat(filled), "░".repeat(empty))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disc_activity() {
        let mut activity = DiscActivity::new();
        activity.set_operation(DiscOperation::Reading);
        activity.set_lba(1234, 10000);
        activity.set_buffer(0.5);

        // Just ensure it doesn't panic
        activity.update();
    }
}
