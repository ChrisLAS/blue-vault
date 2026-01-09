use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// Grid-aligned layout helper for stable, deterministic layouts
pub struct GridLayout;

impl GridLayout {
    /// Create a standard main layout with header, content, and footer
    pub fn main_layout(area: Rect) -> (Rect, Rect, Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Header
                Constraint::Min(10),   // Content
                Constraint::Length(2), // Footer
            ])
            .split(area);

        (chunks[0], chunks[1], chunks[2])
    }

    /// Create a two-column layout with fixed left sidebar
    pub fn two_column(area: Rect, left_width: u16) -> (Rect, Rect) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(left_width), Constraint::Min(10)])
            .split(area);

        (chunks[0], chunks[1])
    }

    /// Create a three-column layout (left, center, right)
    pub fn three_column(area: Rect, left_width: u16, right_width: u16) -> (Rect, Rect, Rect) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(left_width),
                Constraint::Min(10),
                Constraint::Length(right_width),
            ])
            .split(area);

        (chunks[0], chunks[1], chunks[2])
    }

    /// Create a centered dialog box (fixed width)
    pub fn centered_dialog(area: Rect, width: u16, height: u16) -> Rect {
        let dialog_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length((area.height.saturating_sub(height)) / 2),
                Constraint::Length(height),
                Constraint::Min(0),
            ])
            .split(area);

        let dialog_area = dialog_chunks[1];
        let dialog = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length((area.width.saturating_sub(width)) / 2),
                Constraint::Length(width),
                Constraint::Min(0),
            ])
            .split(dialog_area);

        dialog[1]
    }

    /// Create a list layout with fixed item height
    pub fn list_layout(area: Rect) -> Rect {
        // Return the area as-is; list will handle item rendering
        area
    }

    /// Split content area into fixed sections
    pub fn split_content(area: Rect, sections: &[Constraint]) -> std::rc::Rc<[Rect]> {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints(sections)
            .split(area)
    }
}

/// Box drawing characters for consistent borders
pub mod borders {
    use ratatui::symbols;

    /// Use double-line box drawing for main panels
    pub const DOUBLE: symbols::border::Set = symbols::border::DOUBLE;

    /// Use single-line box drawing for sub-panels (this is the default)
    pub const NORMAL: symbols::border::Set = symbols::border::PLAIN;

    /// Use rounded corners for modern look (if supported)
    pub const ROUNDED: symbols::border::Set = symbols::border::ROUNDED;

    /// ASCII fallback (for compatibility)
    pub const PLAIN: symbols::border::Set = symbols::border::PLAIN;

    /// Get default border set (PLAIN for single-line)
    pub fn default() -> symbols::border::Set {
        symbols::border::PLAIN
    }
}
