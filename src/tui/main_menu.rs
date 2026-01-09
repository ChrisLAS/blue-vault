use crate::theme::Theme;
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem},
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MainMenuAction {
    NewDisc,
    SearchIndex,
    VerifyDisc,
    ListDiscs,
    Settings,
    Logs,
    Quit,
}

#[derive(Debug, Clone, Copy)]
pub struct MainMenu {
    selected: usize,
}

impl Default for MainMenu {
    fn default() -> Self {
        Self { selected: 0 }
    }
}

impl MainMenu {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn next(&mut self) {
        self.selected = (self.selected + 1) % 7;
    }

    pub fn previous(&mut self) {
        if self.selected == 0 {
            self.selected = 6;
        } else {
            self.selected -= 1;
        }
    }

    pub fn selected_action(&self) -> MainMenuAction {
        match self.selected {
            0 => MainMenuAction::NewDisc,
            1 => MainMenuAction::SearchIndex,
            2 => MainMenuAction::VerifyDisc,
            3 => MainMenuAction::ListDiscs,
            4 => MainMenuAction::Settings,
            5 => MainMenuAction::Logs,
            6 => MainMenuAction::Quit,
            _ => MainMenuAction::Quit,
        }
    }

    pub fn render(&self, theme: &Theme, frame: &mut Frame, area: Rect) {
        let items = vec![
            ListItem::new("New Disc / Archive Folders"),
            ListItem::new("Search Index"),
            ListItem::new("Verify Disc"),
            ListItem::new("List Discs"),
            ListItem::new("Settings"),
            ListItem::new("Logs / Recent Runs"),
            ListItem::new("Quit"),
        ];

        let list = List::new(items)
            .block(
                Block::default()
                    .title("BlueVault")
                    .borders(Borders::ALL)
                    .border_style(theme.border_style())
                    .style(theme.primary_style()),
            )
            .highlight_style(theme.highlight_style())
            .highlight_symbol("▶ ");

        let mut state = ratatui::widgets::ListState::default();
        state.select(Some(self.selected));
        frame.render_stateful_widget(list, area, &mut state);
    }
}
