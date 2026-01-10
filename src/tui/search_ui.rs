use crate::search::{SearchQuery, SearchResult};
use crate::theme::Theme;
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

#[derive(Debug, Clone)]
pub struct SearchUI {
    query: String,
    results: Vec<SearchResult>,
    selected: Option<usize>,
}

impl Default for SearchUI {
    fn default() -> Self {
        Self {
            query: String::new(),
            results: Vec::new(),
            selected: None,
        }
    }
}

impl SearchUI {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_char(&mut self, c: char) {
        self.query.push(c);
    }

    pub fn delete_char(&mut self) {
        self.query.pop();
    }

    pub fn clear_query(&mut self) {
        self.query.clear();
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn set_results(&mut self, results: Vec<SearchResult>) {
        self.results = results;
        self.selected = if self.results.is_empty() {
            None
        } else {
            Some(0)
        };
    }

    pub fn results(&self) -> &[SearchResult] {
        &self.results
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub fn next_result(&mut self) {
        if let Some(sel) = self.selected {
            if sel < self.results.len().saturating_sub(1) {
                self.selected = Some(sel + 1);
            }
        } else if !self.results.is_empty() {
            self.selected = Some(0);
        }
    }

    pub fn previous_result(&mut self) {
        if let Some(sel) = self.selected {
            if sel > 0 {
                self.selected = Some(sel - 1);
            }
        }
    }

    pub fn build_search_query(&self) -> SearchQuery {
        // Check if query looks like a SHA256 (64 hex chars)
        let is_sha256 = self.query.len() == 64 && self.query.chars().all(|c| c.is_ascii_hexdigit());

        SearchQuery {
            path_substring: if self.query.is_empty() || is_sha256 {
                None
            } else {
                Some(self.query.clone())
            },
            exact_filename: None,
            sha256: if is_sha256 {
                Some(self.query.clone())
            } else {
                None
            },
            regex: None,
        }
    }

    pub fn render(&self, theme: &Theme, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(area);

        // Search input
        let input = Paragraph::new(self.query.as_str())
            .block(
                Block::default()
                    .title("Search")
                    .borders(Borders::ALL)
                    .border_style(theme.border_style()),
            )
            .style(theme.primary_style());
        frame.render_widget(input, chunks[0]);

        // Results list
        if self.results.is_empty() {
            let message = Paragraph::new("No results. Type to search.")
                .block(
                    Block::default()
                        .title("Results")
                        .borders(Borders::ALL)
                        .border_style(theme.border_style()),
                )
                .style(theme.dim_style());
            frame.render_widget(message, chunks[1]);
        } else {
            let items: Vec<ListItem> = self
                .results
                .iter()
                .map(|r| {
                    ListItem::new(format!(
                        "{} │ {} │ {} │ {}",
                        r.disc_id,
                        r.rel_path,
                        crate::search::format_size(r.size),
                        r.mtime
                    ))
                })
                .collect();

            let list = List::new(items)
                .style(theme.secondary_style())
                .block(
                    Block::default()
                        .title("Results")
                        .borders(Borders::ALL)
                        .border_style(theme.border_style()),
                )
                .highlight_style(theme.highlight_style())
                .highlight_symbol("▶ ");

            let mut state = ratatui::widgets::ListState::default();
            if let Some(sel) = self.selected {
                state.select(Some(sel));
            }

            frame.render_stateful_widget(list, chunks[1], &mut state);
        }
    }
}
