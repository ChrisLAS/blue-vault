use crate::theme::Theme;
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem},
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MainMenuAction {
    NewDisc,
    ResumeBurn,
    SearchIndex,
    VerifyDisc,
    VerifyMultiDisc,
    ListDiscs,
    Settings,
    Logs,
    Cleanup,
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
        self.selected = (self.selected + 1) % 10;
    }

    pub fn previous(&mut self) {
        if self.selected == 0 {
            self.selected = 9;
        } else {
            self.selected -= 1;
        }
    }

    pub fn selected_action(&self) -> MainMenuAction {
        match self.selected {
            0 => MainMenuAction::NewDisc,
            1 => MainMenuAction::ResumeBurn,
            2 => MainMenuAction::SearchIndex,
            3 => MainMenuAction::VerifyDisc,
            4 => MainMenuAction::VerifyMultiDisc,
            5 => MainMenuAction::ListDiscs,
            6 => MainMenuAction::Settings,
            7 => MainMenuAction::Logs,
            8 => MainMenuAction::Cleanup,
            9 => MainMenuAction::Quit,
            _ => MainMenuAction::Quit,
        }
    }

    pub fn render(&self, theme: &Theme, frame: &mut Frame, area: Rect) {
        let items = vec![
            ListItem::new("New Disc / Archive Folders"),
            ListItem::new("⏸️  Resume Paused Burn"),
            ListItem::new("Search Index"),
            ListItem::new("Verify Disc"),
            ListItem::new("🔍 Verify Multi-Disc Set"),
            ListItem::new("List Discs"),
            ListItem::new("Settings"),
            ListItem::new("Logs / Recent Runs"),
            ListItem::new("🧹 Cleanup Temporary Files"),
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
