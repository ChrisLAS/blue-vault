use crate::theme::Theme;
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Gauge, Paragraph},
};

// Forward declaration for VerificationResult
// This will be resolved when used via bdarchive::verify::VerificationResult

#[derive(Debug)]
pub struct VerifyUI {
    device: String,
    mountpoint: String,
    input_buffer: String,
    input_mode: VerifyInputMode,
    status_message: String,
    error_message: Option<String>,
    verification_state: VerificationState,
    verification_result: Option<super::super::verify::VerificationResult>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VerifyInputMode {
    Device,
    Mountpoint,
    Ready,
}

#[derive(Debug)]
pub enum VerificationState {
    Idle,
    Mounting,
    Verifying,
    Recording,
    Complete,
    Error(String),
}

impl Default for VerifyUI {
    fn default() -> Self {
        Self {
            device: String::new(),
            mountpoint: String::new(),
            input_buffer: String::new(),
            input_mode: VerifyInputMode::Device,
            status_message: String::new(),
            error_message: None,
            verification_state: VerificationState::Idle,
            verification_result: None,
        }
    }
}

impl VerifyUI {
    pub fn new() -> Self {
        let mut ui = Self::default();
        ui.device = "/dev/sr0".to_string(); // Default device
        ui
    }

    pub fn device(&self) -> &str {
        &self.device
    }

    pub fn set_device(&mut self, device: String) {
        self.device = device;
    }

    pub fn mountpoint(&self) -> &str {
        &self.mountpoint
    }

    pub fn set_mountpoint(&mut self, mountpoint: String) {
        self.mountpoint = mountpoint;
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

    pub fn input_mode(&self) -> VerifyInputMode {
        self.input_mode
    }

    pub fn next_input_mode(&mut self) {
        self.input_mode = match self.input_mode {
            VerifyInputMode::Device => VerifyInputMode::Mountpoint,
            VerifyInputMode::Mountpoint => VerifyInputMode::Ready,
            VerifyInputMode::Ready => VerifyInputMode::Ready,
        };
    }

    pub fn commit_input(&mut self) {
        match self.input_mode {
            VerifyInputMode::Device => {
                if !self.input_buffer.is_empty() {
                    self.device = self.input_buffer.clone();
                }
            }
            VerifyInputMode::Mountpoint => {
                if !self.input_buffer.is_empty() {
                    self.mountpoint = self.input_buffer.clone();
                }
            }
            VerifyInputMode::Ready => {}
        }
        self.input_buffer.clear();
    }

    pub fn set_verification_state(&mut self, state: VerificationState) {
        self.verification_state = state;
    }

    pub fn verification_state(&self) -> &VerificationState {
        &self.verification_state
    }

    pub fn set_status(&mut self, message: String) {
        self.status_message = message;
    }

    pub fn set_error(&mut self, error: String) {
        self.error_message = Some(error.clone());
        self.verification_state = VerificationState::Error(error);
    }

    pub fn clear_error(&mut self) {
        self.error_message = None;
        self.verification_state = VerificationState::Idle;
    }

    pub fn set_verification_result(&mut self, result: super::super::verify::VerificationResult) {
        self.verification_result = Some(result);
    }

    pub fn render(&self, theme: &Theme, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(5), Constraint::Length(3)])
            .split(area);

        let block = Block::default()
            .title("Verify Disc")
            .borders(Borders::ALL)
            .border_style(theme.border_style());

        match self.verification_state {
            VerificationState::Idle
            | VerificationState::Mounting
            | VerificationState::Verifying
            | VerificationState::Recording => {
                if matches!(
                    self.verification_state,
                    VerificationState::Mounting
                        | VerificationState::Verifying
                        | VerificationState::Recording
                ) {
                    // Processing state
                    let status = match self.verification_state {
                        VerificationState::Mounting => "Mounting disc...",
                        VerificationState::Verifying => "Verifying checksums...",
                        VerificationState::Recording => "Recording results...",
                        _ => "",
                    };

                    let text = format!("Status: {}\n\n{}", status, self.status_message);
                    let para = Paragraph::new(text)
                        .block(block.clone())
                        .style(theme.primary_style());
                    frame.render_widget(para, chunks[0]);

                    let progress = match self.verification_state {
                        VerificationState::Mounting => 20,
                        VerificationState::Verifying => 60,
                        VerificationState::Recording => 90,
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
                    frame.render_widget(gauge, chunks[1]);
                } else {
                    // Input state
                    let device_display = match self.input_mode {
                        VerifyInputMode::Device => {
                            if self.input_buffer.is_empty() {
                                &self.device
                            } else {
                                &self.input_buffer
                            }
                        }
                        _ => &self.device,
                    };

                    let mountpoint_display = match self.input_mode {
                        VerifyInputMode::Mountpoint => {
                            if self.input_buffer.is_empty() {
                                if self.mountpoint.is_empty() {
                                    "(auto)"
                                } else {
                                    &self.mountpoint
                                }
                            } else {
                                &self.input_buffer
                            }
                        }
                        _ => {
                            if self.mountpoint.is_empty() {
                                "(auto)"
                            } else {
                                &self.mountpoint
                            }
                        }
                    };

                    let mode_text = match self.input_mode {
                        VerifyInputMode::Device => " [editing device]",
                        VerifyInputMode::Mountpoint => " [editing mountpoint]",
                        VerifyInputMode::Ready => "",
                    };

                    let text = format!(
                        "Verify Disc{}\n\nDevice: {}\nMountpoint: {}\n\nType to edit, [Tab] Next, [Enter] Verify, [Esc] Cancel",
                        mode_text, device_display, mountpoint_display
                    );
                    let para = Paragraph::new(text)
                        .block(block)
                        .style(theme.primary_style());
                    frame.render_widget(para, chunks[0]);
                }
            }
            VerificationState::Complete => {
                if let Some(ref result) = self.verification_result {
                    let status_text = if result.success {
                        format!(
                            "[OK] Verification successful!\n\nFiles checked: {}\nFiles failed: {}",
                            result.files_checked, result.files_failed
                        )
                    } else {
                        format!("[ERR] Verification failed!\n\nFiles checked: {}\nFiles failed: {}\n\nError: {}",
                            result.files_checked, result.files_failed,
                            result.error_message.as_deref().unwrap_or("Unknown error"))
                    };
                    let text = format!("{}\n\n[Esc] Back to menu", status_text);
                    let para = Paragraph::new(text)
                        .block(block.clone())
                        .style(if result.success {
                            theme.success_style()
                        } else {
                            theme.error_style()
                        });
                    frame.render_widget(para, chunks[0]);
                } else {
                    let text = "Verification complete.\n\n[Esc] Back to menu";
                    let para = Paragraph::new(text)
                        .block(block)
                        .style(theme.primary_style());
                    frame.render_widget(para, chunks[0]);
                }
            }
            VerificationState::Error(ref error) => {
                let text = format!("[ERR] {}\n\n[Esc] Go back", error);
                let para = Paragraph::new(text)
                    .block(
                        Block::default()
                            .title("Error")
                            .borders(Borders::ALL)
                            .border_style(theme.border_style()),
                    )
                    .style(theme.error_style());
                frame.render_widget(para, chunks[0]);
            }
        }
    }
}
