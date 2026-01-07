use ratatui::style::{Color, Style};
use std::env;

/// Theme identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeName {
    Phosphor,
    Amber,
    Mono,
}

/// Color palette for phosphor green theme
#[derive(Debug, Clone, Copy)]
pub struct PhosphorColors {
    pub background: Color,
    pub primary: Color,
    pub secondary: Color,
    pub dim: Color,
    pub accent_bg: Color,
    pub accent_fg: Color,
    pub border: Color,
    pub warning: Color,
    pub error: Color,
    pub success: Color,
}

impl PhosphorColors {
    /// Create phosphor colors with truecolor support detection
    pub fn new() -> Self {
        let supports_truecolor = env::var("COLORTERM")
            .map(|v| v == "truecolor" || v == "24bit")
            .unwrap_or(false);

        if supports_truecolor {
            Self::truecolor()
        } else {
            Self::ansi_fallback()
        }
    }

    /// Truecolor RGB colors
    fn truecolor() -> Self {
        Self {
            background: Color::Rgb(0x07, 0x11, 0x0A),
            primary: Color::Rgb(0x3C, 0xFF, 0x8A),
            secondary: Color::Rgb(0x1F, 0xBF, 0x62),
            dim: Color::Rgb(0x0E, 0x5A, 0x2E),
            accent_bg: Color::Rgb(0x0F, 0x2A, 0x1A),
            accent_fg: Color::Rgb(0xBF, 0xFF, 0xD6),
            border: Color::Rgb(0x16, 0x7A, 0x43),
            warning: Color::Rgb(0xC8, 0xFF, 0x5A),
            error: Color::Rgb(0xFF, 0x4D, 0x6D),
            success: Color::Rgb(0x53, 0xFF, 0xA7),
        }
    }

    /// ANSI 16/256 color fallback with best approximation
    fn ansi_fallback() -> Self {
        Self {
            // Dark green background -> ANSI black (0) or bright black (8)
            background: Color::Indexed(0),
            // Bright phosphor -> ANSI bright green (10)
            primary: Color::Indexed(10),
            // Muted phosphor -> ANSI green (2)
            secondary: Color::Indexed(2),
            // Dim -> ANSI dark green (22 in 256-color, or dim green)
            dim: Color::Indexed(22),
            // Accent background -> darker green
            accent_bg: Color::Indexed(22),
            // Accent foreground -> bright green
            accent_fg: Color::Indexed(10),
            // Border -> medium green
            border: Color::Indexed(2),
            // Warning -> bright yellow-green
            warning: Color::Indexed(11),
            // Error -> bright red (still readable)
            error: Color::Indexed(9),
            // Success -> bright cyan-green
            success: Color::Indexed(10),
        }
    }
}

/// Color palette for amber theme (optional)
#[derive(Debug, Clone, Copy)]
pub struct AmberColors {
    pub background: Color,
    pub primary: Color,
    pub secondary: Color,
    pub dim: Color,
    pub accent_bg: Color,
    pub accent_fg: Color,
    pub border: Color,
    pub warning: Color,
    pub error: Color,
    pub success: Color,
}

impl AmberColors {
    pub fn new() -> Self {
        let supports_truecolor = env::var("COLORTERM")
            .map(|v| v == "truecolor" || v == "24bit")
            .unwrap_or(false);

        if supports_truecolor {
            Self::truecolor()
        } else {
            Self::ansi_fallback()
        }
    }

    fn truecolor() -> Self {
        Self {
            background: Color::Rgb(0x0A, 0x08, 0x05),
            primary: Color::Rgb(0xFF, 0xB3, 0x40),
            secondary: Color::Rgb(0xCC, 0x88, 0x20),
            dim: Color::Rgb(0x66, 0x44, 0x11),
            accent_bg: Color::Rgb(0x1A, 0x14, 0x0A),
            accent_fg: Color::Rgb(0xFF, 0xCC, 0x80),
            border: Color::Rgb(0x7A, 0x5A, 0x20),
            warning: Color::Rgb(0xFF, 0xCC, 0x40),
            error: Color::Rgb(0xFF, 0x66, 0x66),
            success: Color::Rgb(0xCC, 0xFF, 0x66),
        }
    }

    fn ansi_fallback() -> Self {
        Self {
            background: Color::Indexed(0),
            primary: Color::Indexed(11),
            secondary: Color::Indexed(3),
            dim: Color::Indexed(58),
            accent_bg: Color::Indexed(58),
            accent_fg: Color::Indexed(11),
            border: Color::Indexed(3),
            warning: Color::Indexed(11),
            error: Color::Indexed(9),
            success: Color::Indexed(10),
        }
    }
}

/// Monochrome colors (for accessibility)
#[derive(Debug, Clone, Copy)]
pub struct MonoColors {
    pub background: Color,
    pub primary: Color,
    pub secondary: Color,
    pub dim: Color,
    pub accent_bg: Color,
    pub accent_fg: Color,
    pub border: Color,
    pub warning: Color,
    pub error: Color,
    pub success: Color,
}

impl MonoColors {
    pub fn new() -> Self {
        Self {
            background: Color::Black,
            primary: Color::White,
            secondary: Color::Gray,
            dim: Color::DarkGray,
            accent_bg: Color::DarkGray,
            accent_fg: Color::White,
            border: Color::Gray,
            warning: Color::White,
            error: Color::White,
            success: Color::White,
        }
    }
}

/// Theme system
#[derive(Debug, Clone)]
pub struct Theme {
    pub name: ThemeName,
    pub colors: ThemeColors,
}

#[derive(Debug, Clone, Copy)]
pub enum ThemeColors {
    Phosphor(PhosphorColors),
    Amber(AmberColors),
    Mono(MonoColors),
}

impl Theme {
    pub fn new(name: ThemeName) -> Self {
        let colors = match name {
            ThemeName::Phosphor => ThemeColors::Phosphor(PhosphorColors::new()),
            ThemeName::Amber => ThemeColors::Amber(AmberColors::new()),
            ThemeName::Mono => ThemeColors::Mono(MonoColors::new()),
        };
        Self { name, colors }
    }

    pub fn default() -> Self {
        Self::new(ThemeName::Phosphor)
    }

    pub fn from_env() -> Self {
        if let Ok(theme_str) = env::var("TUI_THEME") {
            match theme_str.to_lowercase().as_str() {
                "amber" => Self::new(ThemeName::Amber),
                "mono" => Self::new(ThemeName::Mono),
                _ => Self::default(),
            }
        } else {
            Self::default()
        }
    }

    /// Get background color
    pub fn bg(&self) -> Color {
        match self.colors {
            ThemeColors::Phosphor(c) => c.background,
            ThemeColors::Amber(c) => c.background,
            ThemeColors::Mono(c) => c.background,
        }
    }

    /// Get primary text color
    pub fn primary(&self) -> Color {
        match self.colors {
            ThemeColors::Phosphor(c) => c.primary,
            ThemeColors::Amber(c) => c.primary,
            ThemeColors::Mono(c) => c.primary,
        }
    }

    /// Get secondary text color
    pub fn secondary(&self) -> Color {
        match self.colors {
            ThemeColors::Phosphor(c) => c.secondary,
            ThemeColors::Amber(c) => c.secondary,
            ThemeColors::Mono(c) => c.secondary,
        }
    }

    /// Get dim/disabled color
    pub fn dim(&self) -> Color {
        match self.colors {
            ThemeColors::Phosphor(c) => c.dim,
            ThemeColors::Amber(c) => c.dim,
            ThemeColors::Mono(c) => c.dim,
        }
    }

    /// Get accent background color
    pub fn accent_bg(&self) -> Color {
        match self.colors {
            ThemeColors::Phosphor(c) => c.accent_bg,
            ThemeColors::Amber(c) => c.accent_bg,
            ThemeColors::Mono(c) => c.accent_bg,
        }
    }

    /// Get accent foreground color
    pub fn accent_fg(&self) -> Color {
        match self.colors {
            ThemeColors::Phosphor(c) => c.accent_fg,
            ThemeColors::Amber(c) => c.accent_fg,
            ThemeColors::Mono(c) => c.accent_fg,
        }
    }

    /// Get border color
    pub fn border(&self) -> Color {
        match self.colors {
            ThemeColors::Phosphor(c) => c.border,
            ThemeColors::Amber(c) => c.border,
            ThemeColors::Mono(c) => c.border,
        }
    }

    /// Get warning color
    pub fn warning(&self) -> Color {
        match self.colors {
            ThemeColors::Phosphor(c) => c.warning,
            ThemeColors::Amber(c) => c.warning,
            ThemeColors::Mono(c) => c.warning,
        }
    }

    /// Get error color
    pub fn error(&self) -> Color {
        match self.colors {
            ThemeColors::Phosphor(c) => c.error,
            ThemeColors::Amber(c) => c.error,
            ThemeColors::Mono(c) => c.error,
        }
    }

    /// Get success color
    pub fn success(&self) -> Color {
        match self.colors {
            ThemeColors::Phosphor(c) => c.success,
            ThemeColors::Amber(c) => c.success,
            ThemeColors::Mono(c) => c.success,
        }
    }

    /// Create primary text style
    pub fn primary_style(&self) -> Style {
        Style::default().fg(self.primary())
    }

    /// Create secondary text style
    pub fn secondary_style(&self) -> Style {
        Style::default().fg(self.secondary())
    }

    /// Create dim text style
    pub fn dim_style(&self) -> Style {
        Style::default().fg(self.dim())
    }

    /// Create highlighted/selected style
    pub fn highlight_style(&self) -> Style {
        Style::default()
            .bg(self.accent_bg())
            .fg(self.accent_fg())
    }

    /// Create border style
    pub fn border_style(&self) -> Style {
        Style::default().fg(self.border())
    }

    /// Create warning style
    pub fn warning_style(&self) -> Style {
        Style::default().fg(self.warning())
    }

    /// Create error style
    pub fn error_style(&self) -> Style {
        Style::default().fg(self.error())
    }

    /// Create success style
    pub fn success_style(&self) -> Style {
        Style::default().fg(self.success())
    }

    /// Create block style with border
    pub fn block_style(&self) -> Style {
        Style::default().fg(self.border()).bg(self.bg())
    }
}

/// Check if reduced motion is enabled
pub fn reduced_motion() -> bool {
    env::var("TUI_REDUCED_MOTION")
        .map(|v| v == "1" || v == "true")
        .unwrap_or(false)
}

/// Check if animations are disabled
pub fn no_animations() -> bool {
    env::var("TUI_NO_ANIM")
        .map(|v| v == "1" || v == "true")
        .unwrap_or(false)
        || reduced_motion()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_creation() {
        let theme = Theme::default();
        assert_eq!(theme.name, ThemeName::Phosphor);
    }

    #[test]
    fn test_theme_colors() {
        let theme = Theme::default();
        let _bg = theme.bg();
        let _primary = theme.primary();
        let _style = theme.primary_style();
        // Just ensure they don't panic
    }
}

