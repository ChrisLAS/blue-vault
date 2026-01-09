use std::time::{Duration, Instant};

/// Animation throttling and frame rate control
pub struct AnimationThrottle {
    last_frame: Instant,
    min_interval: Duration,
    frame_count: u64,
    started_at: Instant,
    max_duration: Option<Duration>,
}

impl AnimationThrottle {
    /// Create a new throttle with target FPS
    pub fn new(fps: u32) -> Self {
        let min_interval = Duration::from_secs_f64(1.0 / fps as f64);
        Self {
            last_frame: Instant::now(),
            min_interval,
            frame_count: 0,
            started_at: Instant::now(),
            max_duration: Some(Duration::from_secs(60)), // After 60s, slow down
        }
    }

    /// Check if we should render a new frame
    pub fn should_render(&mut self) -> bool {
        if crate::theme::no_animations() {
            return false;
        }

        let now = Instant::now();

        // Check if enough time has passed
        if now.duration_since(self.last_frame) < self.min_interval {
            return false;
        }

        // After max duration, slow down significantly
        if let Some(max_dur) = self.max_duration {
            if now.duration_since(self.started_at) > max_dur {
                // Only render every 500ms (2 fps) after 60s
                if now.duration_since(self.last_frame) < Duration::from_millis(500) {
                    return false;
                }
            }
        }

        self.last_frame = now;
        self.frame_count += 1;
        true
    }

    /// Reset the throttle
    pub fn reset(&mut self) {
        self.last_frame = Instant::now();
        self.started_at = Instant::now();
        self.frame_count = 0;
    }
}

/// Simple spinner animation (retro style)
pub struct Spinner {
    frames: Vec<&'static str>,
    current: usize,
}

impl Spinner {
    /// Create a retro spinner with ASCII characters
    pub fn new() -> Self {
        Self {
            frames: vec!["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"],
            current: 0,
        }
    }

    /// Create a block-style spinner
    pub fn blocks() -> Self {
        Self {
            frames: vec!["▁", "▃", "▅", "▇", "█", "▇", "▅", "▃"],
            current: 0,
        }
    }

    /// Get current frame and advance
    pub fn next(&mut self) -> &'static str {
        let frame = self.frames[self.current];
        self.current = (self.current + 1) % self.frames.len();
        frame
    }

    /// Get current frame without advancing
    pub fn current(&self) -> &'static str {
        self.frames[self.current]
    }

    /// Reset to start
    pub fn reset(&mut self) {
        self.current = 0;
    }
}

impl Default for Spinner {
    fn default() -> Self {
        Self::new()
    }
}

/// Progress bar with retro style
pub struct ProgressBar {
    width: u16,
    filled_char: char,
    empty_char: char,
    left_char: char,
    right_char: char,
}

impl ProgressBar {
    pub fn new(width: u16) -> Self {
        Self {
            width,
            filled_char: '█',
            empty_char: '░',
            left_char: '[',
            right_char: ']',
        }
    }

    pub fn render(&self, progress: f64) -> String {
        let clamped = progress.max(0.0).min(1.0);
        let filled = (clamped * self.width as f64) as u16;
        let empty = self.width.saturating_sub(filled);

        format!(
            "{}{}{}{}",
            self.left_char,
            self.filled_char.to_string().repeat(filled as usize),
            self.empty_char.to_string().repeat(empty as usize),
            self.right_char
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spinner() {
        let mut spinner = Spinner::new();
        let _ = spinner.next();
        assert_eq!(spinner.current(), spinner.frames[1]);
    }

    #[test]
    fn test_progress_bar() {
        let bar = ProgressBar::new(10);
        let output = bar.render(0.5);
        assert!(output.contains("["));
        assert!(output.contains("]"));
    }
}
