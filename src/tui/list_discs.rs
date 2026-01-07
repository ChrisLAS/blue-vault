use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, Paragraph},
};
use crate::database::Disc;
use crate::theme::Theme;

#[derive(Debug, Clone)]
pub struct ListDiscs {
    discs: Vec<Disc>,
    selected: Option<usize>,
}

impl Default for ListDiscs {
    fn default() -> Self {
        Self {
            discs: Vec::new(),
            selected: None,
        }
    }
}

impl ListDiscs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_discs(&mut self, discs: Vec<Disc>) {
        self.discs = discs;
        self.selected = if self.discs.is_empty() {
            None
        } else {
            Some(0)
        };
    }

    pub fn discs(&self) -> &[Disc] {
        &self.discs
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub fn next(&mut self) {
        if let Some(sel) = self.selected {
            if sel < self.discs.len().saturating_sub(1) {
                self.selected = Some(sel + 1);
            }
        } else if !self.discs.is_empty() {
            self.selected = Some(0);
        }
    }

    pub fn previous(&mut self) {
        if let Some(sel) = self.selected {
            if sel > 0 {
                self.selected = Some(sel - 1);
            }
        }
    }

    pub fn render(&self, theme: &Theme, frame: &mut Frame, area: Rect) {
        if self.discs.is_empty() {
            let text = "No discs in archive.";
            let para = Paragraph::new(text)
                .block(
                    Block::default()
                        .title("Discs")
                        .borders(Borders::ALL)
                        .border_style(theme.border_style())
                )
                .style(theme.dim_style());
            frame.render_widget(para, area);
        } else {
            let items: Vec<ListItem> = self.discs
                .iter()
                .map(|d| {
                    ListItem::new(format!(
                        "{} │ {} │ {}",
                        d.disc_id,
                        d.created_at,
                        d.notes.as_deref().unwrap_or("(no notes)")
                    ))
                })
                .collect();

            let list = List::new(items)
                .block(
                    Block::default()
                        .title("Discs")
                        .borders(Borders::ALL)
                        .border_style(theme.border_style())
                )
                .highlight_style(theme.highlight_style())
                .highlight_symbol("▶ ");

            let mut state = ratatui::widgets::ListState::default();
            if let Some(sel) = self.selected {
                state.select(Some(sel));
            }

            frame.render_stateful_widget(list, area, &mut state);
        }
    }
}

