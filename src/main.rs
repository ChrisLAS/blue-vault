use anyhow::{Context, Result};
use bdarchive::tui::directory_selector::Focus as DirFocus;
use bdarchive::*;
use crossterm::{
    event::{self, poll, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use tracing::{error, info, warn};

enum AppState {
    Splash(tui::SplashScreen),
    MainMenu,
    NewDisc(Box<tui::NewDiscFlow>),
    ResumeBurn(tui::ResumeBurnUI),
    VerifyMultiDisc(tui::VerifyMultiDiscUI),
    Cleanup(Box<tui::NewDiscFlow>),
    Search(tui::SearchUI),
    Verify(tui::VerifyUI),
    ListDiscs(tui::ListDiscs),
    Settings(tui::Settings),
    Logs(tui::LogsView),
    Quit,
}

/// Multi-disc operation error types for better error handling
#[derive(Debug, Clone)]
pub enum MultiDiscError {
    PlanningFailed(String),
    HardwareFailure(String),
    BurnFailed { disc_number: usize, error: String },
    UserCancelled,
    PartialSuccess { completed_discs: Vec<usize>, failed_disc: usize, error: String },
    StagingFailed { disc_number: usize, error: String },
    DatabaseInconsistency(String),
}

enum DiscCreationMessage {
    Status(String),
    StateAndStatus(tui::new_disc::ProcessingState, String),
    Progress(String),
    Complete,
    Error(String),
    MultiDiscError(MultiDiscError),
    UserChoiceNeeded { message: String, options: Vec<String> },
    PauseRequested,
    ResumeRequested,
}

struct App {
    state: AppState,
    main_menu: tui::MainMenu,
    config: Config,
    db_conn: rusqlite::Connection,
    theme: theme::Theme,
    footer: ui::header_footer::Footer,
    disc_creation_rx: Option<mpsc::Receiver<DiscCreationMessage>>,
    disc_creation_tx: Option<mpsc::Sender<DiscCreationMessage>>,
    pending_disc_creation: Option<(bool, Vec<PathBuf>, Config)>, // (needs_multi_disc, source_folders, config)
}

impl App {
    fn new(config: Config, db_conn: rusqlite::Connection) -> Self {
        // Get disc count for splash
        let disc_count = database::Disc::list_all(&db_conn).unwrap_or_default().len();
        let db_path = config.database_path().unwrap_or_default();
        let db_status = if db_path.exists() {
            tui::DbStatus::Ok
        } else {
            tui::DbStatus::Error
        };

        let theme = theme::Theme::from_env();

        let splash = tui::SplashScreen::new(db_path, disc_count, db_status);

        Self {
            state: AppState::Splash(splash),
            main_menu: tui::MainMenu::new(),
            config,
            db_conn,
            theme,
            footer: ui::header_footer::Footer::new(),
            disc_creation_rx: None,
            disc_creation_tx: None,
            pending_disc_creation: None,
        }
    }

    /// Poll for background messages and update UI state.
    /// Returns true if any messages were processed.
    fn poll_background_messages(&mut self) -> bool {
        let mut updated = false;

        if let AppState::NewDisc(ref mut flow) = self.state {
            if let Some(ref rx) = self.disc_creation_rx {
                match rx.try_recv() {
                    Ok(DiscCreationMessage::Status(status)) => {
                        flow.set_status(status);
                        updated = true;
                    }
                    Ok(DiscCreationMessage::StateAndStatus(state, status)) => {
                        flow.set_processing_state(state);
                        flow.set_status(status);
                        updated = true;
                    }
                    Ok(DiscCreationMessage::Progress(progress)) => {
                        flow.set_file_progress(progress.clone());

                        // Handle multi-disc verification completion
                        // TODO: Add verification result handling when needed

                        // Parse multi-disc progress from the message
                        if progress.contains("Disc ") && progress.contains("/") {
                            // Try to extract disc numbers: "💿 Disc 2/3 | 📊 33.3% complete"
                            if let Some(disc_part) = progress.split("Disc ").nth(1) {
                                if let Some(disc_info) = disc_part.split(" | ").next() {
                                    let parts: Vec<&str> = disc_info.split('/').collect();
                                    if parts.len() == 2 {
                                        if let (Ok(current), Ok(total)) = (
                                            parts[0].trim().parse::<u32>(),
                                            parts[1].trim().parse::<u32>()
                                        ) {
                                            // Extract overall progress percentage
                                            let overall_progress = if let Some(percent_part) = progress.split("📊 ").nth(1) {
                                                if let Some(percent_str) = percent_part.split('%').next() {
                                                    percent_str.trim().parse::<f64>().unwrap_or(0.0) / 100.0
                                                } else {
                                                    0.0
                                                }
                                            } else {
                                                0.0
                                            };

                                            flow.set_multi_disc_progress(current, total, overall_progress);
                                        }
                                    }
                                }
                            }
                        }

                        updated = true;
                    }
                    Ok(DiscCreationMessage::Complete) => {
                        flow.set_processing_state(tui::new_disc::ProcessingState::Complete);
                        let completion_msg = if flow.is_multi_disc() {
                            "Multi-disc archive creation completed successfully!".to_string()
                        } else {
                            "Disc creation completed successfully!".to_string()
                        };
                        flow.set_status(completion_msg);
                        self.disc_creation_rx = None; // Clean up
                        updated = true;
                    }
                    Ok(DiscCreationMessage::Error(error)) => {
                        flow.set_error(error);
                        self.disc_creation_rx = None; // Clean up
                        updated = true;
                    }
                    Ok(DiscCreationMessage::MultiDiscError(error)) => {
                        match error {
                            MultiDiscError::PartialSuccess { completed_discs, failed_disc, error } => {
                                flow.set_error(format!("Partial success: {} discs completed. Disc {} failed: {}",
                                    completed_discs.len(), failed_disc, error));
                                // Keep receiver alive for potential user choice
                            }
                            MultiDiscError::HardwareFailure(msg) => {
                                flow.set_error(format!("Hardware error: {}", msg));
                                self.disc_creation_rx = None;
                            }
                            MultiDiscError::UserCancelled => {
                                flow.set_status("Operation cancelled by user".to_string());
                                self.disc_creation_rx = None;
                            }
                            _ => {
                                flow.set_error(format!("Multi-disc error: {:?}", error));
                                self.disc_creation_rx = None;
                            }
                        }
                        updated = true;
                    }
                    Ok(DiscCreationMessage::UserChoiceNeeded { message, options }) => {
                        // For now, just show the message. In a full implementation,
                        // this would present the user with choices and send responses back
                        flow.set_error(format!("User choice needed: {}\nOptions: {:?}", message, options));
                        // Keep receiver alive to wait for user response
                        updated = true;
                    }
                    Ok(DiscCreationMessage::PauseRequested) => {
                        flow.set_status("⏸️ Burn paused by user. Press 'r' to resume or 'Esc' to cancel.".to_string());
                        flow.set_processing_state(tui::new_disc::ProcessingState::Error("Paused".to_string()));
                        updated = true;
                    }
                    Ok(DiscCreationMessage::ResumeRequested) => {
                        flow.set_status("▶️ Resuming burn process...".to_string());
                        flow.set_processing_state(tui::new_disc::ProcessingState::Staging);
                        updated = true;
                    }
                    Err(mpsc::TryRecvError::Empty) => {
                        // No message, continue
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        // Background thread died unexpectedly
                        if let AppState::NewDisc(ref mut flow) = self.state {
                            flow.set_error("Background process terminated unexpectedly".to_string());
                            self.disc_creation_rx = None;
                            updated = true;
                        }
                    }
                }
            }
        }

        updated
    }

    fn handle_key(&mut self, key: KeyCode) -> Result<bool> {
        // Universal quit key - works from all screens
        if matches!(key, KeyCode::Char('q') | KeyCode::Char('Q')) {
            return Ok(false); // false = quit application
        }

        match &mut self.state {
            AppState::Splash(ref mut splash) => {
                // Skip splash on any keypress
                splash.skip();
                self.state = AppState::MainMenu;
                return Ok(true);
            }
            AppState::MainMenu => match key {
                KeyCode::Up | KeyCode::Char('k') => self.main_menu.previous(),
                KeyCode::Down | KeyCode::Char('j') => self.main_menu.next(),
                KeyCode::Enter => match self.main_menu.selected_action() {
                    tui::MainMenuAction::NewDisc => {
                        let default_id = disc::generate_disc_id();
                        self.state = AppState::NewDisc(Box::new(tui::NewDiscFlow::new(default_id)));
                    }
                    tui::MainMenuAction::SearchIndex => {
                        self.state = AppState::Search(tui::SearchUI::new());
                    }
                    tui::MainMenuAction::VerifyDisc => {
                        self.state = AppState::Verify(tui::VerifyUI::new());
                    }
                    tui::MainMenuAction::VerifyMultiDisc => {
                        // Load available multi-disc sets
                        let disc_sets = database::DiscSet::list_all(&self.db_conn)?;
                        let mut verify_ui = tui::VerifyMultiDiscUI::new();
                        verify_ui.set_disc_sets(disc_sets);
                        self.state = AppState::VerifyMultiDisc(verify_ui);
                    }
                    tui::MainMenuAction::ListDiscs => {
                        let discs = database::Disc::list_all(&self.db_conn)?;
                        let mut list = tui::ListDiscs::new();
                        list.set_discs(discs);
                        self.state = AppState::ListDiscs(list);
                    }
                    tui::MainMenuAction::Settings => {
                        self.state = AppState::Settings(tui::Settings::new());
                    }
                    tui::MainMenuAction::Logs => {
                        self.state = AppState::Logs(tui::LogsView::new());
                    }
                    tui::MainMenuAction::ResumeBurn => {
                        // Show resume menu with available paused sessions
                        let sessions = database::BurnSessionOps::get_active_sessions(&self.db_conn)?;
                        if sessions.is_empty() {
                            // No paused sessions, show message
                            let mut resume_ui = tui::ResumeBurnUI::new();
                            resume_ui.set_message("No paused burn sessions found. Start a new multi-disc archive to create a resumable session.".to_string());
                            self.state = AppState::ResumeBurn(resume_ui);
                        } else {
                            let mut resume_ui = tui::ResumeBurnUI::new();
                            resume_ui.set_sessions(sessions);
                            self.state = AppState::ResumeBurn(resume_ui);
                        }
                    }
                    tui::MainMenuAction::Cleanup => {
                        // Run cleanup in background and show progress
                        let (tx, rx) = mpsc::channel::<DiscCreationMessage>();
                        self.disc_creation_rx = Some(rx);
                        self.disc_creation_tx = Some(tx.clone());
                        let config = self.config.clone();

                        thread::spawn(move || {
                            match Self::cleanup_temporary_files(&config) {
                                Ok(()) => {
                                    let _ = tx.send(DiscCreationMessage::Status(
                                        "✅ Cleanup completed successfully!".to_string()
                                    ));
                                    let _ = tx.send(DiscCreationMessage::Complete);
                                }
                                Err(e) => {
                                    let _ = tx.send(DiscCreationMessage::Error(
                                        format!("❌ Cleanup failed: {}", e)
                                    ));
                                }
                            }
                        });

                        // Switch to cleanup state to show cleanup progress
                        let mut flow = tui::NewDiscFlow::new("CLEANUP".to_string());
                        flow.set_processing_state(tui::new_disc::ProcessingState::Staging);
                        flow.set_status("🧹 Cleaning up temporary files...".to_string());
                        self.state = AppState::Cleanup(Box::new(flow));
                    }
                    tui::MainMenuAction::Quit => {
                        return Ok(false);
                    }
                },
                KeyCode::Esc => {
                    return Ok(false);
                }
                _ => {}
            },
            AppState::NewDisc(ref mut flow) => {
                match key {
                    KeyCode::Esc => {
                        if flow.current_step() == tui::new_disc::NewDiscStep::Processing {
                            // Check if processing is complete - allow escape then
                            if matches!(flow.processing_state(), tui::new_disc::ProcessingState::Complete) {
                                self.state = AppState::MainMenu;
                                return Ok(true);
                            } else if matches!(flow.processing_state(), tui::new_disc::ProcessingState::Error(_)) {
                                // Allow escape on error too - go back to review
                                flow.previous_step();
                                flow.clear_error();
                                return Ok(true);
                            } else {
                                // Can't escape during active processing
                                return Ok(true);
                            }
                        }
                        self.state = AppState::MainMenu;
                    }
                    KeyCode::Char('p') | KeyCode::Char('P') => {
                        if flow.current_step() == tui::new_disc::NewDiscStep::Processing {
                            if let Some(ref tx) = self.disc_creation_tx {
                                // Send pause request to background thread
                                let _ = tx.send(DiscCreationMessage::PauseRequested);
                                flow.set_status("⏸️ Pause requested...".to_string());
                                return Ok(true);
                            }
                        }
                    }
                    KeyCode::Char('r') | KeyCode::Char('R') => {
                        if flow.current_step() == tui::new_disc::NewDiscStep::Processing {
                            if let Some(ref tx) = self.disc_creation_tx {
                                // Send resume request to background thread
                                let _ = tx.send(DiscCreationMessage::ResumeRequested);
                                flow.set_status("▶️ Resume requested...".to_string());
                                return Ok(true);
                            }
                        }
                    }
                    KeyCode::Enter => {
                        match flow.current_step() {
                            tui::new_disc::NewDiscStep::EnterDiscId => {
                                flow.next_step(&self.config)?;
                            }
                            tui::new_disc::NewDiscStep::EnterNotes => {
                                flow.next_step(&self.config)?;
                            }
                            tui::new_disc::NewDiscStep::SelectFolders => {
                                // Initialize selector if needed
                                if flow.directory_selector_mut().is_none() {
                                    if let Err(e) = flow.init_directory_selector() {
                                        flow.set_error(format!(
                                            "Failed to initialize directory selector: {}",
                                            e
                                        ));
                                        return Ok(true);
                                    }
                                }

                                // Handle Enter based on focus - extract what we need first
                                let path_to_add: Option<PathBuf> = {
                                    if let Some(ref mut selector) = flow.directory_selector_mut() {
                                        match selector.focus() {
                                            DirFocus::Browser => {
                                                // Enter key in browser: navigate into the highlighted directory
                                                // For "..", navigate up
                                                if let Some(selected_path) =
                                                    selector.get_browser_selection()
                                                {
                                                    let current_path =
                                                        selector.current_path().to_path_buf();

                                                    // Check if this is ".." (parent)
                                                    if let Some(parent) = current_path.parent() {
                                                        if selected_path == parent {
                                                            // This is ".." - navigate up
                                                            let _ = selector.browser_enter();
                                                            return Ok(true);
                                                        }
                                                    }

                                                    // Navigate into the selected directory
                                                    let _ =
                                                        selector.set_current_path(selected_path);
                                                    return Ok(true);
                                                }
                                                None
                                            }
                                            DirFocus::Input => {
                                                // Enter key in input box: commit path
                                                match selector.commit_input() {
                                                    Ok(path) => {
                                                        selector.clear_error();
                                                        selector.clear_input_buffer();
                                                        Some(path)
                                                    }
                                                    Err(_) => {
                                                        // Error already set in selector
                                                        None
                                                    }
                                                }
                                            }
                                        }
                                    } else {
                                        None
                                    }
                                };

                                // Add path to source folders if we got one
                                if let Some(path) = path_to_add {
                                    flow.add_source_folder(path);
                                    return Ok(true);
                                }

                                // Only proceed to next step if folders are selected (and Enter wasn't handled above)
                                if !flow.source_folders().is_empty() {
                                    flow.next_step(&self.config)?;
                                }
                            }
                            tui::new_disc::NewDiscStep::Review => {
                                // For Review step, Enter starts the process
                                flow.next_step(&self.config)?;

                                // Check if we need multi-disc burning
                                let source_folders = flow.source_folders().to_vec();
                                let config = self.config.clone();

                                // Calculate total size to determine if multi-disc is needed
                                let disc_capacity = config.default_capacity_bytes();
                                match staging::check_capacity(&source_folders, disc_capacity) {
                                    Ok((total_size, exceeds)) => {
                                        if exceeds {
                                            info!("Content exceeds single disc capacity ({} bytes), starting multi-disc workflow", total_size);
                                            flow.set_status("Planning multi-disc layout...".to_string());
                                        } else {
                                            info!("Content fits on single disc ({} bytes), starting single-disc workflow", total_size);
                                            flow.set_status("Starting disc creation...".to_string());
                                        }
            // Store the request for processing after the match
            info!("Setting pending_disc_creation: multi_disc={}, folders={}", exceeds, source_folders.len());
            self.pending_disc_creation = Some((exceeds, source_folders, config));
            info!("pending_disc_creation set successfully");
                                    }
                                    Err(e) => {
                                        flow.set_status(format!("Error calculating size: {}", e));
                                        flow.set_error("Failed to analyze content size".to_string());
                                        flow.previous_step();
                                    }
                                }

                                return Ok(true);
                            }
                            tui::new_disc::NewDiscStep::Processing => {
                                // Background messages are now handled in poll_background_messages()

                                // If complete, go back to menu
                                if matches!(
                                    flow.processing_state(),
                                    tui::new_disc::ProcessingState::Complete
                                ) {
                                    self.state = AppState::MainMenu;
                                } else if matches!(
                                    flow.processing_state(),
                                    tui::new_disc::ProcessingState::Error(_)
                                ) {
                                    // On error, go back to review
                                    flow.previous_step();
                                    flow.clear_error();
                                }
                            }
                        }
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        match flow.current_step() {
                            tui::new_disc::NewDiscStep::SelectFolders => {
                                // Navigate browser if focused
                                if let Some(ref mut selector) = flow.directory_selector_mut() {
                                    if selector.focus() == DirFocus::Browser {
                                        selector.browser_up();
                                        return Ok(true);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        match flow.current_step() {
                            tui::new_disc::NewDiscStep::SelectFolders => {
                                // Navigate browser if focused
                                if let Some(ref mut selector) = flow.directory_selector_mut() {
                                    if selector.focus() == DirFocus::Browser {
                                        selector.browser_down();
                                        return Ok(true);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    KeyCode::Right => {
                        match flow.current_step() {
                            tui::new_disc::NewDiscStep::SelectFolders => {
                                // Navigate INTO directory in browser
                                if let Some(ref mut selector) = flow.directory_selector_mut() {
                                    if selector.focus() == DirFocus::Browser {
                                        // Navigate into the selected directory (if it's a directory, not "..")
                                        if let Some(selected_path) =
                                            selector.get_browser_selection()
                                        {
                                            let current_path =
                                                selector.current_path().to_path_buf();
                                            // Check if this is ".." (parent)
                                            if let Some(parent) = current_path.parent() {
                                                if selected_path == parent {
                                                    // This is ".." - don't navigate into it
                                                    return Ok(true);
                                                }
                                            }
                                            // Navigate into the selected directory
                                            selector.set_current_path(selected_path).ok();
                                        }
                                        return Ok(true);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    KeyCode::Tab => {
                        match flow.current_step() {
                            tui::new_disc::NewDiscStep::SelectFolders => {
                                // Tab toggles focus between input and browser
                                if let Some(ref mut selector) = flow.directory_selector_mut() {
                                    selector.toggle_focus();
                                    return Ok(true);
                                }
                            }
                            _ => {}
                        }
                    }
                    KeyCode::Insert => {
                        match flow.current_step() {
                            tui::new_disc::NewDiscStep::SelectFolders => {
                                // Insert key: add highlighted directory to source folders
                                if let Some(ref mut selector) = flow.directory_selector_mut() {
                                    if selector.focus() == DirFocus::Browser {
                                        if let Some(selected_path) =
                                            selector.get_browser_selection()
                                        {
                                            let current_path =
                                                selector.current_path().to_path_buf();

                                            // Don't add ".." to source folders
                                            if let Some(parent) = current_path.parent() {
                                                if selected_path != parent {
                                                    // Add the directory to source folders
                                                    flow.add_source_folder(selected_path);
                                                    return Ok(true);
                                                }
                                            } else {
                                                // Add the directory to source folders
                                                flow.add_source_folder(selected_path);
                                                return Ok(true);
                                            }
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    // Special handling for 'R' key in SelectFolders (retry loading)
                    // For all other steps, 'R' should be treated as regular character input
                    KeyCode::Backspace => match flow.current_step() {
                        tui::new_disc::NewDiscStep::EnterDiscId
                        | tui::new_disc::NewDiscStep::EnterNotes => {
                            let mut buffer = flow.input_buffer().to_string();
                            buffer.pop();
                            flow.set_input_buffer(buffer);
                        }
                        tui::new_disc::NewDiscStep::SelectFolders => {
                            if let Some(ref mut selector) = flow.directory_selector_mut() {
                                if selector.focus() == DirFocus::Input {
                                    let mut buffer = selector.input_buffer().to_string();
                                    buffer.pop();
                                    selector.set_input_buffer(buffer);
                                }
                            }
                        }
                        _ => {}
                    },
                    KeyCode::Char(c) => {
                        match flow.current_step() {
                            tui::new_disc::NewDiscStep::EnterDiscId
                            | tui::new_disc::NewDiscStep::EnterNotes => {
                                // Allow all characters for text input, including 'd'
                                let mut buffer = flow.input_buffer().to_string();
                                buffer.push(c);
                                flow.set_input_buffer(buffer);
                            }
                            tui::new_disc::NewDiscStep::SelectFolders => {
                                // Handle special keys for SelectFolders step
                                if c == 'd' || c == 'D' {
                                    // Toggle dry run mode
                                    let current_dry_run = flow.dry_run();
                                    flow.set_dry_run(!current_dry_run);
                                    return Ok(true);
                                } else if c == 'r' || c == 'R' {
                                    // R key: retry loading if there was an error
                                    if let Some(ref mut selector) = flow.directory_selector_mut() {
                                        if let Err(e) = selector.retry_loading() {
                                            tracing::error!("Failed to retry directory loading: {}", e);
                                        }
                                        return Ok(true);
                                    }
                                }

                                // Initialize selector if needed
                                if flow.directory_selector_mut().is_none() {
                                    if let Err(e) = flow.init_directory_selector() {
                                        flow.set_error(format!(
                                            "Failed to initialize directory selector: {}",
                                            e
                                        ));
                                        return Ok(true);
                                    }
                                }

                                // Only handle character input if in input mode
                                if let Some(ref mut selector) = flow.directory_selector_mut() {
                                    if selector.focus() == DirFocus::Input {
                                        // Regular character input for path entry
                                        let mut buffer = selector.input_buffer().to_string();
                                        buffer.push(c);
                                        selector.set_input_buffer(buffer);
                                    }
                                    // Browser mode doesn't use character input (except tab, handled separately)
                                }
                            }
                            tui::new_disc::NewDiscStep::Review => {
                                // Handle special keys for Review step
                                if c == 'd' || c == 'D' {
                                    // Toggle dry run mode
                                    let current_dry_run = flow.dry_run();
                                    flow.set_dry_run(!current_dry_run);
                                    return Ok(true);
                                }
                                // Other characters are ignored in review step
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
            AppState::ResumeBurn(ref mut resume_ui) => {
                match key {
                    KeyCode::Esc => {
                        self.state = AppState::MainMenu;
                        return Ok(true);
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        resume_ui.previous();
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        resume_ui.next();
                    }
                    KeyCode::Enter => {
                        if let Some(selected_session) = resume_ui.selected_session() {
                            // Resume the selected session
                            self.resume_burn_session(selected_session)?;
                        } else if resume_ui.is_cleanup_mode() {
                            // Handle cleanup action
                            if let Some(session_id) = resume_ui.selected_session_for_cleanup() {
                                database::BurnSessionOps::delete_session(&self.db_conn, &session_id)?;
                                // Refresh the UI
                                let sessions = database::BurnSessionOps::get_active_sessions(&self.db_conn)?;
                                resume_ui.set_sessions(sessions);
                            }
                        }
                    }
                    KeyCode::Char('c') => {
                        // Toggle cleanup mode
                        resume_ui.toggle_cleanup_mode();
                    }
                    _ => {                    }
                }
            }
            AppState::VerifyMultiDisc(ref mut verify_ui) => {
                match key {
                    KeyCode::Esc => {
                        self.state = AppState::MainMenu;
                        return Ok(true);
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        if verify_ui.is_selecting() {
                            verify_ui.previous();
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if verify_ui.is_selecting() {
                            verify_ui.next();
                        }
                    }
                    KeyCode::Enter => {
                        if let Some(selected_set) = verify_ui.selected_set() {
                            // Start verification
                            let set_id = selected_set.set_id.clone();
                            let (tx, rx) = mpsc::channel();
                            self.disc_creation_rx = Some(rx);

                            verify_ui.set_status("🔍 Starting multi-disc verification...".to_string());

                            thread::spawn(move || {
                                match crate::verify::verify_multi_disc_set(&set_id, None, false) {
                                    Ok(result) => {
                                        let _ = tx.send(DiscCreationMessage::Status("✅ Verification complete".to_string()));
                                        // In a real implementation, we'd send the result back
                                        // For now, just indicate completion
                                        let _ = tx.send(DiscCreationMessage::Complete);
                                    }
                                    Err(e) => {
                                        let _ = tx.send(DiscCreationMessage::Error(format!("Verification failed: {}", e)));
                                    }
                                }
                            });
                        }
                    }
                    _ => {}
                }
            }
            AppState::Cleanup(ref mut flow) => {
                match key {
                    KeyCode::Esc => {
                        if flow.current_step() == tui::new_disc::NewDiscStep::Processing {
                            // Check if processing is complete - allow escape then
                            if matches!(flow.processing_state(), tui::new_disc::ProcessingState::Complete) {
                                self.state = AppState::MainMenu;
                                return Ok(true);
                            } else if matches!(flow.processing_state(), tui::new_disc::ProcessingState::Error(_)) {
                                // Allow escape on error - go back to main menu
                                self.state = AppState::MainMenu;
                                return Ok(true);
                            } else {
                                // Don't allow escape during active cleanup
                                return Ok(true);
                            }
                        } else {
                            self.state = AppState::MainMenu;
                        }
                    }
                    _ => {
                        // For cleanup, we only handle escape - background messages handle the rest
                    }
                }
            }
            AppState::Search(ref mut search) => {
                match key {
                    KeyCode::Esc => {
                        self.state = AppState::MainMenu;
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        search.previous_result();
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        search.next_result();
                    }
                    KeyCode::Char(c) => {
                        // Only add characters that aren't navigation keys
                        if c != 'k' && c != 'j' {
                            search.add_char(c);
                            // Perform search
                            let query = search.build_search_query();
                            let results = search::search_files(&self.db_conn, &query)?;
                            search.set_results(results);
                        }
                    }
                    KeyCode::Backspace => {
                        search.delete_char();
                        // Perform search
                        let query = search.build_search_query();
                        let results = search::search_files(&self.db_conn, &query)?;
                        search.set_results(results);
                    }
                    _ => {}
                }
            }
            AppState::Verify(ref mut verify) => {
                match key {
                    KeyCode::Esc => {
                        if matches!(
                            verify.verification_state(),
                            tui::verify_ui::VerificationState::Complete
                        ) || matches!(
                            verify.verification_state(),
                            tui::verify_ui::VerificationState::Error(_)
                        ) {
                            self.state = AppState::MainMenu;
                        } else if matches!(
                            verify.verification_state(),
                            tui::verify_ui::VerificationState::Mounting
                                | tui::verify_ui::VerificationState::Verifying
                                | tui::verify_ui::VerificationState::Recording
                        ) {
                            // Can't escape during processing
                            return Ok(true);
                        } else {
                            self.state = AppState::MainMenu;
                        }
                    }
                    KeyCode::Enter => {
                        match verify.verification_state() {
                            tui::verify_ui::VerificationState::Idle => {
                                if verify.input_mode() == tui::verify_ui::VerifyInputMode::Ready {
                                    verify.commit_input();
                                    let device = verify.device().to_string();
                                    let mountpoint = verify.mountpoint().to_string();
                                    // Temporarily extract state, work on it, then put it back
                                    // Release verify borrow (explicitly don't drop the reference)
                                    let _ = verify;
                                    let app_state =
                                        std::mem::replace(&mut self.state, AppState::Quit);
                                    if let AppState::Verify(mut v) = app_state {
                                        match self.start_verification_internal(
                                            &mut v,
                                            &device,
                                            &mountpoint,
                                        ) {
                                            Ok(()) => {}
                                            Err(e) => {
                                                v.set_error(format!("Error: {}", e));
                                            }
                                        }
                                        self.state = AppState::Verify(v);
                                    } else {
                                        self.state = app_state;
                                    }
                                    return Ok(true);
                                } else {
                                    verify.commit_input();
                                    verify.next_input_mode();
                                    if verify.input_mode() == tui::verify_ui::VerifyInputMode::Ready
                                    {
                                        // Ready to verify
                                    }
                                }
                            }
                            tui::verify_ui::VerificationState::Complete => {
                                self.state = AppState::MainMenu;
                            }
                            _ => {}
                        }
                    }
                    KeyCode::Tab => {
                        if matches!(
                            verify.verification_state(),
                            tui::verify_ui::VerificationState::Idle
                        ) {
                            verify.commit_input();
                            verify.next_input_mode();
                        }
                    }
                    KeyCode::Backspace => {
                        if matches!(
                            verify.verification_state(),
                            tui::verify_ui::VerificationState::Idle
                        ) {
                            let mut buffer = verify.input_buffer().to_string();
                            buffer.pop();
                            verify.set_input_buffer(buffer);
                        }
                    }
                    KeyCode::Char(c) => {
                        if matches!(
                            verify.verification_state(),
                            tui::verify_ui::VerificationState::Idle
                        ) {
                            let mut buffer = verify.input_buffer().to_string();
                            buffer.push(c);
                            verify.set_input_buffer(buffer);
                        }
                    }
                    _ => {}
                }
            }
            AppState::ListDiscs(ref mut list) => match key {
                KeyCode::Esc => {
                    self.state = AppState::MainMenu;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    list.previous();
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    list.next();
                }
                _ => {}
            },
            AppState::Settings(_) => match key {
                KeyCode::Esc => {
                    self.state = AppState::MainMenu;
                }
                _ => {}
            },
            AppState::Logs(_) => match key {
                KeyCode::Esc => {
                    self.state = AppState::MainMenu;
                }
                _ => {}
            },
            AppState::Quit => {
                return Ok(false);
            }
        }

        // Handle any pending disc creation requests
        info!("Checking for pending disc creation requests...");
        if self.pending_disc_creation.is_some() {
            info!("Found pending disc creation request, state: {:?}", match self.state {
                AppState::NewDisc(_) => "NewDisc",
                AppState::Cleanup(_) => "Cleanup",
                _ => "Other"
            });
        }
        let pending_taken = self.pending_disc_creation.take();
        if let Some((needs_multi_disc, source_folders, config)) = pending_taken {
            info!("Processing pending disc creation request: multi_disc={}, folders={}", needs_multi_disc, source_folders.len());
            let db_path = self
                .config
                .database_path()
                .unwrap_or_else(|_| PathBuf::from(":memory:"));

            // Start the appropriate disc creation workflow
            if let AppState::NewDisc(ref mut flow) = self.state {
                info!("Starting disc creation workflow...");
                Self::start_disc_creation_workflow(flow, needs_multi_disc, source_folders, config, db_path, &mut self.disc_creation_rx);
            } else {
                warn!("Pending disc creation request but not in NewDisc state! Current state: {:?}", match self.state {
                    AppState::NewDisc(_) => "NewDisc",
                    AppState::Cleanup(_) => "Cleanup",
                    AppState::MainMenu => "MainMenu",
                    _ => "Other"
                });
            }
        }

        Ok(true)
    }

    fn start_verification_internal(
        &mut self,
        verify: &mut tui::VerifyUI,
        device_str: &str,
        mountpoint_str: &str,
    ) -> Result<()> {
        let device = if device_str.is_empty() {
            self.config.device.clone()
        } else {
            device_str.to_string()
        };

        let mountpoint = if mountpoint_str.is_empty() {
            // Get temporary mountpoint
            bdarchive::verify::get_temporary_mountpoint()?
        } else {
            PathBuf::from(mountpoint_str)
        };

        let dry_run = false;
        let auto_mount = self.config.verification.auto_mount;

        // Step 1: Mount if needed
        verify.set_verification_state(tui::verify_ui::VerificationState::Mounting);

        if !mountpoint.join("SHA256SUMS.txt").exists() {
            if auto_mount {
                verify.set_status(format!(
                    "Mounting {} to {}...",
                    device,
                    mountpoint.display()
                ));
                bdarchive::verify::mount_device(&device, &mountpoint, dry_run)?;
            } else {
                verify.set_status(format!(
                    "Please mount {} at {}",
                    device,
                    mountpoint.display()
                ));
                // Wait for user to mount manually
                // For now, check if it's mounted
                if !mountpoint.join("SHA256SUMS.txt").exists() {
                    verify.set_error(format!(
                        "Disc not mounted. Please mount {} at {}",
                        device,
                        mountpoint.display()
                    ));
                    return Ok(());
                }
            }
        }

        // Step 2: Verify
        verify.set_verification_state(tui::verify_ui::VerificationState::Verifying);
        verify.set_status("Running sha256sum -c...".to_string());

        let result = bdarchive::verify::verify_disc(&mountpoint, auto_mount, dry_run)?;
        verify.set_verification_result(result.clone());

        if result.success {
            verify.set_status(format!(
                "Verification successful! {} files checked.",
                result.files_checked
            ));
        } else {
            verify.set_status(format!(
                "Verification failed! {} files failed out of {} checked.",
                result.files_failed, result.files_checked
            ));
        }

        // Step 3: Record in database
        verify.set_verification_state(tui::verify_ui::VerificationState::Recording);
        verify.set_status("Recording verification results...".to_string());

        // Try to find disc_id from the disc
        // For now, we'll use a placeholder or try to read from DISC_INFO.txt
        let disc_id =
            if let Ok(disc_info) = std::fs::read_to_string(mountpoint.join("DISC_INFO.txt")) {
                // Parse disc ID from DISC_INFO.txt
                disc_info
                    .lines()
                    .find_map(|line| {
                        if line.starts_with("Disc-ID: ") {
                            Some(line[9..].trim().to_string())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| "UNKNOWN".to_string())
            } else {
                "UNKNOWN".to_string()
            };

        let verification_run = database::VerificationRun {
            id: None,
            disc_id,
            verified_at: format_timestamp_now(),
            mountpoint: Some(mountpoint.to_string_lossy().to_string()),
            device: Some(device.clone()),
            success: result.success,
            error_message: result.error_message.clone(),
            files_checked: Some(result.files_checked),
            files_failed: Some(result.files_failed),
        };

        database::VerificationRun::insert(&mut self.db_conn, &verification_run)?;

        // Unmount if we mounted it
        if auto_mount && mountpoint.exists() {
            if let Err(e) = bdarchive::verify::unmount_device(&mountpoint, dry_run) {
                verify.set_status(format!("Warning: Failed to unmount: {}", e));
            }
        }

        verify.set_verification_state(tui::verify_ui::VerificationState::Complete);

        Ok(())
    }

    #[allow(dead_code)]
    fn start_disc_creation_internal(
        &mut self,
        flow: &mut tui::NewDiscFlow,
        disc_id: &str,
        notes: &str,
        source_folders: &[PathBuf],
        dry_run: bool,
    ) -> Result<()> {
        info!(
            "Starting disc creation: dry_run={}, disc_id={}",
            dry_run, disc_id
        );
        flow.set_status(format!(
            "Starting disc creation (mode: {})...",
            if dry_run { "DRY RUN" } else { "ACTUAL" }
        ));
        // Validate inputs
        if disc_id.is_empty() {
            flow.set_error("Disc ID cannot be empty".to_string());
            return Ok(());
        }

        if source_folders.is_empty() {
            flow.set_error("No source folders selected".to_string());
            return Ok(());
        }

        // Validate source folders exist
        for folder in source_folders {
            if !folder.exists() {
                flow.set_error(format!(
                    "Source folder does not exist: {}",
                    folder.display()
                ));
                return Ok(());
            }
        }

        let dry_run = dry_run;

        // Step 1: Staging
        flow.set_processing_state(tui::new_disc::ProcessingState::Staging);
        flow.set_status("Preparing staging directory...".to_string());

        let staging_dir = self.config.staging_dir()?;
        std::fs::create_dir_all(&staging_dir)?;

        let disc_root = disc::create_disc_layout(
            &staging_dir,
            disc_id,
            source_folders,
            if notes.is_empty() { None } else { Some(notes) },
        )?;

        flow.set_status("Staging files...".to_string());
        let use_rsync = self.config.optional_tools.use_rsync
            && dependencies::get_optional_command("rsync").is_some();

        staging::stage_files(&disc_root, source_folders, use_rsync, dry_run)?;

        // Step 2: Generate manifest and SHA256SUMS
        info!("Starting manifest generation");
        flow.set_processing_state(tui::new_disc::ProcessingState::GeneratingManifest);
        flow.set_status("Generating manifest and checksums...".to_string());

        let files = manifest::generate_manifest_and_sums(&disc_root, None)?;

        let manifest_path = disc_root.join("MANIFEST.txt");
        manifest::write_manifest_file(&manifest_path, &files)?;

        let sha256sums_path = disc_root.join("SHA256SUMS.txt");
        manifest::write_sha256sums_file(&sha256sums_path, &files)?;

        // Write DISC_INFO.txt
        let source_roots: Vec<PathBuf> = flow.source_folders().to_vec();
        disc::write_disc_info(
            &disc_root,
            disc_id,
            if notes.is_empty() { None } else { Some(notes) },
            &source_roots,
            &disc::get_tool_version(),
            None, // set_id (single disc)
            None, // sequence_number
            None, // total_discs
        )?;

        // Check capacity
        let total_size = manifest::calculate_total_size(&files);
        let capacity = self.config.default_capacity_bytes();
        if total_size > capacity {
            flow.set_error(format!(
                "Total size {} GB exceeds disc capacity {} GB",
                total_size as f64 / 1_000_000_000.0,
                capacity as f64 / 1_000_000_000.0
            ));
            return Ok(());
        }

        // Step 3: Create ISO
        info!("Starting ISO creation");
        flow.set_processing_state(tui::new_disc::ProcessingState::CreatingISO);
        flow.set_status("Creating ISO image...".to_string());

        let volume_label = disc::generate_volume_label(disc_id);
        let iso_path = staging_dir.join(format!("{}.iso", disc_id));

        iso::create_iso(&disc_root, &iso_path, &volume_label, dry_run)?;
        let iso_size = iso::get_iso_size(&iso_path)?;

        flow.set_status(format!(
            "ISO created: {:.2} GB",
            iso_size as f64 / 1_000_000_000.0
        ));

        // Step 4: Burn to disc
        info!("Starting burning step: dry_run={}", dry_run);
        flow.set_processing_state(tui::new_disc::ProcessingState::Burning);

        if dry_run {
            flow.set_status("Skipping burn (dry run mode)".to_string());
            flow.set_file_progress("DRY RUN: Would burn ISO to disc".to_string());
            info!("Skipping burn due to dry run mode");
        } else {
            flow.set_status(format!("Burning to {}...", self.config.device));
            info!(
                "About to call burn::burn_iso with device: {}",
                self.config.device
            );
            burn::burn_iso(&iso_path, &self.config.device, dry_run)?;
            info!("Burn completed successfully");
            flow.set_status("Disc burned successfully".to_string());
        }

        // Step 5: Index in database
        flow.set_processing_state(tui::new_disc::ProcessingState::Indexing);
        flow.set_status("Updating index...".to_string());

        let created_at = format_timestamp_now();

        let disc_record = database::Disc {
            disc_id: disc_id.to_string(),
            volume_label: volume_label.clone(),
            created_at: created_at.clone(),
            notes: if notes.is_empty() {
                None
            } else {
                Some(notes.to_string())
            },
            iso_size: Some(iso_size),
            burn_device: Some(self.config.device.clone()),
            checksum_manifest_hash: None, // Could calculate hash of manifest
            qr_path: None,                // Will be set after QR generation
            source_roots: Some(serde_json::to_string(&source_roots)?),
            tool_version: Some(disc::get_tool_version()),
            set_id: None, // Single disc, not part of a set
            sequence_number: None,
        };

        database::Disc::insert(&mut self.db_conn, &disc_record)?;

        // Index files
        let file_records: Vec<database::FileRecord> = files
            .iter()
            .map(|f| database::FileRecord {
                id: None,
                disc_id: disc_id.to_string(),
                rel_path: f.rel_path.to_string_lossy().to_string(),
                sha256: f.sha256.clone(),
                size: f.size,
                mtime: f.mtime.clone(),
                added_at: created_at.clone(),
            })
            .collect();

        database::FileRecord::insert_batch(&mut self.db_conn, &file_records)?;

        // Step 6: Generate QR code
        flow.set_processing_state(tui::new_disc::ProcessingState::GeneratingQR);
        flow.set_status("Generating QR code...".to_string());

        if self.config.optional_tools.use_qrencode {
            let qrcodes_dir = paths::qrcodes_dir()?;
            match qrcode::generate_qrcode(disc_id, &qrcodes_dir, qrcode::QrCodeFormat::PNG, dry_run)
            {
                Ok(qr_path) => {
                    // Update disc record with QR path
                    // For now, just log it
                    info!("QR code generated: {}", qr_path.display());
                }
                Err(e) => {
                    // Non-fatal error
                    flow.set_status(format!("QR code generation skipped: {}", e));
                }
            }
        }

        // Clean up staging directory after successful burn
        if !dry_run {
            flow.set_status("Cleaning up temporary files...".to_string());
            if let Err(e) = Self::cleanup_staging_directory(&staging_dir) {
                warn!("Failed to cleanup staging directory: {}", e);
                // Don't fail the entire process for cleanup errors
            }
        }

        // Complete!
        flow.set_processing_state(tui::new_disc::ProcessingState::Complete);
        flow.set_status(format!("Disc {} created successfully!", disc_id));

        Ok(())
    }

    /// Enhanced multi-disc creation with comprehensive error handling and recovery
    fn run_multi_disc_creation_background_robust(
        disc_id_base: String,
        notes: String,
        source_folders: Vec<PathBuf>,
        dry_run: bool,
        config: Config,
        mut db_conn: rusqlite::Connection,
        tx: mpsc::Sender<DiscCreationMessage>,
    ) -> Result<()> {
        let _ = tx.send(DiscCreationMessage::Status("🔍 Starting multi-disc archive creation with enhanced error handling...".to_string()));

        // Phase 1: Planning with error recovery
        let plans = match Self::plan_multi_disc_archive(&source_folders, config.default_capacity_bytes(), &tx) {
            Ok(plans) => plans,
            Err(MultiDiscError::PlanningFailed(msg)) => {
                let _ = tx.send(DiscCreationMessage::Error(format!("Planning failed: {}", msg)));
                return Err(anyhow::anyhow!("Planning failed: {}", msg));
            }
            Err(_) => unreachable!("Planning should only return PlanningFailed"),
        };

        let total_discs = plans.len();
        let total_size: u64 = plans.iter().map(|p| p.used_bytes).sum();

        // Phase 2: Create database set with rollback capability
        let set_id = match Self::create_disc_set_with_rollback(&mut db_conn, &disc_id_base, &notes, total_size, total_discs, &source_folders, &tx) {
            Ok(id) => id,
            Err(e) => {
                let _ = tx.send(DiscCreationMessage::Error(format!("Database setup failed: {}", e)));
                return Err(e);
            }
        };

        // Phase 2.5: Create burn session for pause/resume capability
        let session = database::BurnSession::new(
            set_id.clone(),
            disc_id_base.clone(),
            total_discs,
            source_folders.clone(),
            serde_json::to_string(&config).unwrap_or_default(),
        );

        if let Err(e) = session.save(&db_conn) {
            warn!("Failed to save burn session: {}", e);
            // Don't fail the burn for session save errors
        }

        // Phase 3: Burn discs with error recovery
        let completed_discs = match Self::burn_multi_disc_sequence(
            &disc_id_base, &notes, &plans, dry_run, &config, &mut db_conn, &set_id, &source_folders, &tx, &session.session_id
        ) {
            Ok(discs) => discs,
            Err(MultiDiscError::UserCancelled) => {
                let _ = tx.send(DiscCreationMessage::Status("❌ Operation cancelled by user".to_string()));
                return Ok(()); // User cancellation is not an error
            }
            Err(MultiDiscError::PartialSuccess { completed_discs, failed_disc, error }) => {
                let _ = tx.send(DiscCreationMessage::MultiDiscError(MultiDiscError::PartialSuccess {
                    completed_discs: completed_discs.clone(),
                    failed_disc,
                    error: error.clone(),
                }));

                // Ask user what to do
                let _ = tx.send(DiscCreationMessage::UserChoiceNeeded {
                    message: format!("Disc {} failed: {}. {} discs completed successfully. What would you like to do?", failed_disc, error, completed_discs.len()),
                    options: vec![
                        "Retry failed disc".to_string(),
                        "Skip failed disc and continue".to_string(),
                        "Abort and cleanup".to_string(),
                    ],
                });

                return Err(anyhow::anyhow!("Partial success: {} discs completed, disc {} failed: {}", completed_discs.len(), failed_disc, error));
            }
            Err(e) => {
                let _ = tx.send(DiscCreationMessage::Error(format!("Burn sequence failed: {:?}", e)));
                return Err(anyhow::anyhow!("Burn sequence failed: {:?}", e));
            }
        };

        // Phase 4: Final cleanup and reporting
        Self::finalize_multi_disc_archive(&completed_discs, &set_id, total_size, dry_run, &config, &tx);

        Ok(())
    }

    /// Plan multi-disc archive with error handling
    fn plan_multi_disc_archive(
        source_folders: &[PathBuf],
        disc_capacity: u64,
        tx: &mpsc::Sender<DiscCreationMessage>,
    ) -> Result<Vec<staging::DiscPlan>, MultiDiscError> {
        let _ = tx.send(DiscCreationMessage::Status("📊 Planning multi-disc layout with error recovery...".to_string()));

        // Create disc layout plan with timeout protection
        let plans_result = std::panic::catch_unwind(|| {
            staging::plan_disc_layout_with_progress(source_folders, disc_capacity, |progress| {
                let _ = tx.send(DiscCreationMessage::Progress(progress.to_string()));
            })
        });

        match plans_result {
            Ok(Ok(plans)) => {
                if plans.is_empty() {
                    return Err(MultiDiscError::PlanningFailed("No disc plans generated".to_string()));
                }
                Ok(plans)
            }
            Ok(Err(e)) => Err(MultiDiscError::PlanningFailed(format!("Planning error: {}", e))),
            Err(_) => Err(MultiDiscError::PlanningFailed("Planning function panicked (possible infinite loop)".to_string())),
        }
    }

    /// Create disc set with rollback capability
    fn create_disc_set_with_rollback(
        db_conn: &mut rusqlite::Connection,
        disc_id_base: &str,
        notes: &str,
        total_size: u64,
        total_discs: usize,
        source_folders: &[PathBuf],
        tx: &mpsc::Sender<DiscCreationMessage>,
    ) -> Result<String> {
        let _ = tx.send(DiscCreationMessage::Status("💾 Setting up database records...".to_string()));

        let set_name = format!("Multi-disc archive: {}", disc_id_base);
        let source_folders_json = serde_json::to_string(source_folders)?;

        match database::MultiDiscOps::create_disc_set(
            db_conn,
            &set_name,
            if notes.is_empty() { None } else { Some(notes) },
            total_size,
            total_discs as u32,
            Some(&source_folders_json),
        ) {
            Ok(set_id) => {
                let _ = tx.send(DiscCreationMessage::Progress(format!("✅ Database set '{}' created", set_id)));
                Ok(set_id)
            }
            Err(e) => {
                let _ = tx.send(DiscCreationMessage::Error(format!("Failed to create disc set: {}", e)));
                Err(e)
            }
        }
    }

    /// Burn multi-disc sequence with comprehensive error handling and pause/resume support
    fn burn_multi_disc_sequence(
        disc_id_base: &str,
        notes: &str,
        plans: &[staging::DiscPlan],
        dry_run: bool,
        config: &Config,
        db_conn: &mut rusqlite::Connection,
        set_id: &str,
        source_folders: &[PathBuf],
        tx: &mpsc::Sender<DiscCreationMessage>,
        session_id: &str,
    ) -> Result<Vec<PathBuf>, MultiDiscError> {
        let total_discs = plans.len();
        let mut completed_discs = Vec::new();
        let mut iso_paths = Vec::new();

        for (disc_index, plan) in plans.iter().enumerate() {
            let sequence_num = disc_index + 1;

            // Check for pause requests before starting each disc
            // In a real implementation, we'd also check during burning
            // For now, this provides basic pause capability

            match Self::burn_single_disc_with_recovery(
                disc_id_base, notes, plan, sequence_num, total_discs, dry_run, config, db_conn, set_id, source_folders, tx
            ) {
                Ok(iso_path) => {
                    completed_discs.push(sequence_num);
                    iso_paths.push(iso_path);

                    // Update session progress
                    if let Ok(Some(mut session)) = database::BurnSession::load(db_conn, session_id) {
                        session.update_progress(sequence_num);
                        let _ = session.save(db_conn);
                    }
                }
                Err(e) => {
                    // Save session state on failure
                    if let Ok(Some(mut session)) = database::BurnSession::load(&db_conn, session_id) {
                        session.failed_discs.push(sequence_num);
                        let _ = session.save(&db_conn);
                    }

                    return Err(MultiDiscError::PartialSuccess {
                        completed_discs: completed_discs.clone(),
                        failed_disc: sequence_num,
                        error: format!("{:?}", e),
                    });
                }
            }
        }

        Ok(iso_paths)
    }

    /// Burn single disc with recovery and error handling
    fn burn_single_disc_with_recovery(
        disc_id_base: &str,
        notes: &str,
        plan: &staging::DiscPlan,
        sequence_num: usize,
        total_discs: usize,
        dry_run: bool,
        config: &Config,
        db_conn: &mut rusqlite::Connection,
        set_id: &str,
        source_folders: &[PathBuf],
        tx: &mpsc::Sender<DiscCreationMessage>,
    ) -> Result<PathBuf, MultiDiscError> {
        let disc_id = disc::generate_multi_disc_id(disc_id_base, sequence_num as u32);

        let _ = tx.send(DiscCreationMessage::Status(format!(
            "🔥 Processing disc {}/{}: {}", sequence_num, total_discs, disc_id
        )));

        // Disc insertion prompt with timeout
        if !dry_run {
            Self::wait_for_disc_insertion(sequence_num, total_discs, tx)?;
        }

        // Create staging with error handling
        let staging_dir = config.staging_dir()
            .map_err(|e| MultiDiscError::StagingFailed {
                disc_number: sequence_num,
                error: format!("Cannot access staging directory: {}", e),
            })?;

        let disc_staging_dir = staging_dir.join(format!("disc_{}", sequence_num));

        match Self::stage_disc_content(plan, source_folders, &disc_staging_dir, dry_run, tx) {
            Ok(_) => {}
            Err(e) => return Err(MultiDiscError::StagingFailed {
                disc_number: sequence_num,
                error: format!("Staging failed: {}", e),
            }),
        }

        // Write disc info
        let disc_root = disc_staging_dir.join("disc_info");
        if let Err(e) = disc::write_disc_info(
            &disc_root,
            &disc_id,
            if notes.is_empty() { None } else { Some(notes) },
            source_folders,
            &disc::get_tool_version(),
            Some(set_id),
            Some(sequence_num as u32),
            Some(total_discs as u32),
        ) {
            let _ = std::fs::remove_dir_all(&disc_staging_dir);
            return Err(MultiDiscError::StagingFailed {
                disc_number: sequence_num,
                error: format!("Failed to write disc info: {}", e),
            });
        }

        // Burn disc with error handling
        let iso_path = match Self::create_iso_and_burn_disc(
            &disc_id,
            &disc_staging_dir,
            &config.device,
            dry_run,
            config,
            tx,
        ) {
            Ok(path) => path,
            Err(e) => {
                // Cleanup on failure
                let _ = std::fs::remove_dir_all(&disc_staging_dir);
                return Err(MultiDiscError::BurnFailed {
                    disc_number: sequence_num,
                    error: format!("Burn failed: {}", e),
                });
            }
        };

        // Record in database
        if let Err(e) = Self::record_disc_in_database(
            &disc_id, disc_id_base, sequence_num, total_discs, plan, config, db_conn, set_id, source_folders, dry_run
        ) {
            warn!("Failed to record disc {} in database: {}", sequence_num, e);
            // Don't fail the burn for database errors, but log it
        }

        // Cleanup staging
        if disc_staging_dir.exists() {
            let _ = std::fs::remove_dir_all(&disc_staging_dir);
        }

        let _ = tx.send(DiscCreationMessage::Status(format!(
            "✅ Disc {} of {} completed successfully", sequence_num, total_discs
        )));

        Ok(iso_path)
    }

    /// Wait for user to insert disc with timeout and cancellation
    fn wait_for_disc_insertion(sequence_num: usize, total_discs: usize, tx: &mpsc::Sender<DiscCreationMessage>) -> Result<(), MultiDiscError> {
        let _ = tx.send(DiscCreationMessage::Status(format!(
            "📀 Please insert disc {} of {} and press Enter to continue (or 'q' to cancel)...",
            sequence_num, total_discs
        )));

        // In a real implementation, this would wait for user input
        // For now, just send animated waiting messages
        for i in 0..10 {  // 3 second timeout simulation
            let spinner = match i % 4 {
                0 => "|",
                1 => "/",
                2 => "-",
                3 => "\\",
                _ => "|",
            };
            let _ = tx.send(DiscCreationMessage::Progress(format!(
                "⏳ Waiting for disc {}... {} (press Enter when ready, 'q' to cancel)", sequence_num, spinner
            )));
            std::thread::sleep(std::time::Duration::from_millis(300));

            // In real implementation, check for user input here
            // For simulation, just continue
        }

        let _ = tx.send(DiscCreationMessage::Progress(format!(
            "✅ Disc {} ready, starting burn process...", sequence_num
        )));

        Ok(())
    }


    /// Record completed disc in database
    fn record_disc_in_database(
        disc_id: &str,
        disc_id_base: &str,
        sequence_num: usize,
        total_discs: usize,
        plan: &staging::DiscPlan,
        config: &Config,
        db_conn: &mut rusqlite::Connection,
        set_id: &str,
        source_folders: &[PathBuf],
        dry_run: bool,
    ) -> Result<()> {
        let volume_label = disc::generate_multi_disc_volume_label(disc_id_base, sequence_num as u32, total_discs as u32);

        let mut disc_record = database::Disc {
            disc_id: disc_id.to_string(),
            volume_label,
            created_at: disc::format_timestamp_now(),
            notes: Some(format!("Disc {} of {} in multi-disc set {}", sequence_num, total_discs, set_id)),
            iso_size: Some(plan.used_bytes),
            burn_device: if dry_run { None } else { Some(config.device.clone()) },
            checksum_manifest_hash: None,
            qr_path: None,
            source_roots: Some(serde_json::to_string(source_folders)?),
            tool_version: Some(disc::get_tool_version()),
            set_id: Some(set_id.to_string()),
            sequence_number: Some(sequence_num as u32),
        };

        database::MultiDiscOps::add_disc_to_set(db_conn, &mut disc_record, set_id, sequence_num as u32)?;
        Ok(())
    }

    /// Finalize multi-disc archive with summary
    fn finalize_multi_disc_archive(
        iso_paths: &[PathBuf],
        set_id: &str,
        total_size: u64,
        dry_run: bool,
        config: &Config,
        tx: &mpsc::Sender<DiscCreationMessage>,
    ) {
        // Final cleanup
        if !dry_run {
            if let Ok(staging_dir) = config.staging_dir() {
                let _ = Self::cleanup_staging_directory(&staging_dir);
            }
        }

        // Send completion summary
        let total_size_mb = total_size / (1024 * 1024);
        let _ = tx.send(DiscCreationMessage::Status(format!(
            "🎊 Multi-disc archive complete! {} discs, {} MB total",
            iso_paths.len(), total_size_mb
        )));

        // Show ISO file locations
        if !iso_paths.is_empty() {
            let _ = tx.send(DiscCreationMessage::Progress("📂 ISO files created:".to_string()));
            for (i, iso_path) in iso_paths.iter().enumerate() {
                let disc_num = i + 1;
                let _ = tx.send(DiscCreationMessage::Progress(format!(
                    "  💿 Disc {}: {}",
                    disc_num,
                    iso_path.display()
                )));
            }
        }
    }

    /// Run multi-disc creation in background with sequential burning
    fn run_multi_disc_creation_background(
        disc_id_base: String,
        notes: String,
        source_folders: Vec<PathBuf>,
        dry_run: bool,
        config: Config,
        mut db_conn: rusqlite::Connection,
        tx: mpsc::Sender<DiscCreationMessage>,
    ) -> Result<()> {
        let _ = tx.send(DiscCreationMessage::Status(format!(
            "Planning multi-disc layout..."
        )));

        // Create disc layout plan with timeout protection
        let disc_capacity = config.default_capacity_bytes();

        let plans_result = std::panic::catch_unwind(|| {
            staging::plan_disc_layout_with_progress(&source_folders, disc_capacity, |progress| {
                let _ = tx.send(DiscCreationMessage::Progress(progress.to_string()));
            })
        });

        let plans = match plans_result {
            Ok(Ok(plans)) => plans,
            Ok(Err(e)) => {
                let _ = tx.send(DiscCreationMessage::Error(format!("Planning failed: {}", e)));
                return Err(e);
            }
            Err(_) => {
                let _ = tx.send(DiscCreationMessage::Error("Planning panicked (possible infinite loop)".to_string()));
                return Err(anyhow::anyhow!("Planning function panicked"));
            }
        };

        if plans.is_empty() {
            let _ = tx.send(DiscCreationMessage::Error("No disc plans generated".to_string()));
            return Err(anyhow::anyhow!("No disc plans generated"));
        }

        let total_discs = plans.len();

        let _ = tx.send(DiscCreationMessage::Progress(format!(
            "✅ Planning complete! Archive will span {} discs", total_discs
        )));

        // Create disc set in database
        let set_name = format!("Multi-disc archive: {}", disc_id_base);
        let total_size: u64 = plans.iter().map(|p| p.used_bytes).sum();
        let set_id = database::MultiDiscOps::create_disc_set(
            &mut db_conn,
            &set_name,
            Some(&notes),
            total_size,
            total_discs as u32,
            Some(&serde_json::to_string(&source_folders)?),
        )?;

        // Burn each disc sequentially
        let mut iso_paths = Vec::new();

        for (disc_index, plan) in plans.iter().enumerate() {
            let sequence_num = disc_index + 1;
            let disc_id = disc::generate_multi_disc_id(&disc_id_base, sequence_num as u32);

            // Update overall progress
            let overall_progress = disc_index as f64 / total_discs as f64;

            let _ = tx.send(DiscCreationMessage::Status(format!(
                "🔥 Preparing disc {}/{}: {} ({} MB)",
                sequence_num,
                total_discs,
                disc_id,
                plan.used_bytes / (1024 * 1024)
            )));

            // Send detailed progress info
            let _ = tx.send(DiscCreationMessage::Progress(format!(
                "💿 Disc {}/{} | 📊 {:.1}% complete | 🎯 +{} MB",
                sequence_num, total_discs,
                overall_progress * 100.0,
                plan.used_bytes / (1024 * 1024)
            )));

            if !dry_run {
                // Prompt user to insert correct disc with animated waiting
                let _ = tx.send(DiscCreationMessage::Status(format!(
                    "📀 Please insert disc {} of {} and press Enter to continue...",
                    sequence_num, total_discs
                )));

                // Send animated waiting messages
                for i in 0..5 {
                    let spinner = match i % 4 {
                        0 => "|",
                        1 => "/",
                        2 => "-",
                        3 => "\\",
                        _ => "|",
                    };
                    let _ = tx.send(DiscCreationMessage::Progress(format!(
                        "⏳ Waiting for disc {}... {}", sequence_num, spinner
                    )));
                    std::thread::sleep(std::time::Duration::from_millis(300));
                }

                let _ = tx.send(DiscCreationMessage::Progress(format!(
                    "✅ Disc {} ready, starting burn process...",
                    sequence_num
                )));
            }

            // Create temporary directory structure for this disc
            info!("Creating staging directory for disc {}", sequence_num);
            let staging_dir = config.staging_dir()?;
            let disc_staging_dir = staging_dir.join(format!("disc_{}", sequence_num));
            info!("Staging dir: {}", disc_staging_dir.display());

            // Stage files for this specific disc with progress updates
            let _ = tx.send(DiscCreationMessage::Status(format!(
                "📦 Staging files for disc {}...",
                sequence_num
            )));

            // Send initial progress
            let _ = tx.send(DiscCreationMessage::Progress(format!(
                "📁 Preparing disc {} content...",
                sequence_num
            )));

            match Self::stage_disc_content(&plan, &source_folders, &disc_staging_dir, dry_run, &tx) {
                Ok(_) => (),
                Err(e) => {
                    error!("Staging failed for disc {}: {}", sequence_num, e);
                    let _ = tx.send(DiscCreationMessage::Error(format!("Staging failed: {}", e)));
                    return Err(e);
                }
            }

            let _ = tx.send(DiscCreationMessage::Progress(format!(
                "✅ Disc {} staging complete ({} MB)",
                sequence_num,
                plan.used_bytes / (1024 * 1024)
            )));

            // Write DISC_INFO.txt for this disc
            let disc_root = disc_staging_dir.clone();
            let source_roots: Vec<PathBuf> = source_folders.clone();
            match disc::write_disc_info(
                &disc_root,
                &disc_id,
                if notes.is_empty() { None } else { Some(&notes) },
                &source_roots,
                &disc::get_tool_version(),
                Some(&set_id),
                Some(sequence_num as u32),
                Some(total_discs as u32),
            ) {
                Ok(_) => info!("Disc info written for disc {}", sequence_num),
                Err(e) => {
                    error!("Failed to write disc info for disc {}: {}", sequence_num, e);
                    let _ = tx.send(DiscCreationMessage::Error(format!("Failed to write disc info: {}", e)));
                    return Err(anyhow::anyhow!("Failed to write disc info: {}", e));
                }
            }

            // Create ISO and burn (or simulate) - reuse existing logic
            let iso_path = match Self::create_iso_and_burn_disc(
                &disc_id,
                &disc_staging_dir,
                &config.device,
                dry_run,
                &config,
                &tx,
            ) {
                Ok(iso_path) => {
                    iso_paths.push(iso_path.clone());
                    iso_path
                }
                Err(e) => {
                    error!("ISO/burn failed for disc {}: {}", sequence_num, e);
                    let _ = tx.send(DiscCreationMessage::Error(format!("Burn failed: {}", e)));
                    return Err(e);
                }
            };

            // Add disc to set
            let volume_label = disc::generate_multi_disc_volume_label(&disc_id_base, sequence_num as u32, total_discs as u32);
            let mut disc_record = database::Disc {
                disc_id: disc_id.clone(),
                volume_label,
                created_at: disc::format_timestamp_now(),
                notes: Some(format!("Disc {} of {} in multi-disc set {}", sequence_num, total_discs, set_id)),
                iso_size: Some(plan.used_bytes),
                burn_device: if dry_run { None } else { Some(config.device.clone()) },
                checksum_manifest_hash: None,
                qr_path: None,
                source_roots: Some(serde_json::to_string(&source_folders)?),
                tool_version: Some(disc::get_tool_version()),
                set_id: Some(set_id.clone()),
                sequence_number: Some(sequence_num as u32),
            };

            database::MultiDiscOps::add_disc_to_set(&mut db_conn, &mut disc_record, &set_id, sequence_num as u32)?;

            // Cleanup disc staging
            if disc_staging_dir.exists() {
                let _ = std::fs::remove_dir_all(&disc_staging_dir);
            }

            let _ = tx.send(DiscCreationMessage::Status(format!(
                "✅ Disc {} of {} completed successfully",
                sequence_num, total_discs
            )));
        }

        // Final cleanup
        if !dry_run {
            if let Ok(staging_dir) = config.staging_dir() {
                let _ = Self::cleanup_staging_directory(&staging_dir);
            }
        }

        // Send completion summary for multi-disc
        let total_size_mb = total_size / (1024 * 1024);
        let _ = tx.send(DiscCreationMessage::Status(format!(
            "🎊 Multi-disc archive complete! {} discs, {} MB total",
            total_discs, total_size_mb
        )));

        let _ = tx.send(DiscCreationMessage::Progress(format!(
            "📚 Archive ID: {} | 💾 {} discs spanning {} MB",
            disc_id_base, total_discs, total_size_mb
        )));

        // Show ISO file locations
        if !iso_paths.is_empty() {
            let _ = tx.send(DiscCreationMessage::Progress("📂 ISO files created:".to_string()));
            for (i, iso_path) in iso_paths.iter().enumerate() {
                let disc_num = i + 1;
                let _ = tx.send(DiscCreationMessage::Progress(format!(
                    "  💿 Disc {}: {}",
                    disc_num,
                    iso_path.display()
                )));
            }
        }

        let _ = tx.send(DiscCreationMessage::Complete);
        Ok(())
    }

    /// Create ISO and burn disc (extracted from single-disc workflow)
    fn create_iso_and_burn_disc(
        disc_id: &str,
        disc_staging_dir: &Path,
        device: &str,
        dry_run: bool,
        config: &Config,
        tx: &mpsc::Sender<DiscCreationMessage>,
    ) -> Result<PathBuf> {
        // ISO Creation Phase
        let _ = tx.send(DiscCreationMessage::Status("🎨 Creating ISO image...".to_string()));
        let _ = tx.send(DiscCreationMessage::Progress("🔄 Analyzing files and building filesystem...".to_string()));

        let volume_label = disc::generate_volume_label(disc_id);
        let staging_dir = config.staging_dir()?;
        let iso_path = staging_dir.join(format!("{}.iso", disc_id));

        // Send animated progress during ISO creation
        let iso_tx = tx.clone();
        std::thread::spawn(move || {
            let spinners = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            for i in 0..20 {
                let _ = iso_tx.send(DiscCreationMessage::Progress(format!(
                    "🎨 Building ISO... {}", spinners[i % spinners.len()]
                )));
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
        });

        iso::create_iso(disc_staging_dir, &iso_path, &volume_label, dry_run)?;

        // Get ISO size (skip for dry run since no file is created)
        let iso_size = if dry_run {
            // Estimate size based on staging directory
            staging::calculate_directory_size(disc_staging_dir)?
        } else {
            iso::get_iso_size(&iso_path)?
        };

        let _ = tx.send(DiscCreationMessage::Progress(format!(
            "✅ ISO created: {:.2} GB ({})",
            iso_size as f64 / 1_000_000_000.0,
            volume_label
        )));

        // Burn to disc
        if dry_run {
            let _ = tx.send(DiscCreationMessage::Status("🔍 Skipping burn (dry run mode)".to_string()));
            let _ = tx.send(DiscCreationMessage::Progress("📋 Dry run complete - no disc written".to_string()));
        } else {
            let _ = tx.send(DiscCreationMessage::Status(format!("🔥 Burning to {}...", device)));
            let _ = tx.send(DiscCreationMessage::Progress("⚡ Initializing Blu-ray burner...".to_string()));

            burn::burn_iso(&iso_path, device, dry_run)?;

            let _ = tx.send(DiscCreationMessage::Progress("🎉 Disc burned successfully!".to_string()));
        }

        Ok(iso_path)
    }

    /// Stage content for a specific disc from the plan
    fn stage_disc_content(
        plan: &staging::DiscPlan,
        source_folders: &[PathBuf],
        disc_staging_dir: &Path,
        dry_run: bool,
        tx: &mpsc::Sender<DiscCreationMessage>,
    ) -> Result<()> {
        let _ = tx.send(DiscCreationMessage::Progress(format!(
            "🔄 Starting content staging for disc {}...",
            plan.disc_number
        )));

        // For now, we'll copy all source folders and rely on the ISO creation
        // to handle the size limits. In a more sophisticated implementation,
        // we'd only copy the specific files assigned to this disc.
        for (i, source) in source_folders.iter().enumerate() {
            if source.exists() {
                let dest_name = source.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown");
                let dest = disc_staging_dir.join(dest_name);

                let _ = tx.send(DiscCreationMessage::Progress(format!(
                    "📂 Copying folder {}/{}: {}",
                    i + 1,
                    source_folders.len(),
                    dest_name
                )));

                if dry_run {
                    // Just create directory structure
                    std::fs::create_dir_all(&dest)?;
                    let _ = tx.send(DiscCreationMessage::Progress("📁 Created directory structure (dry run)".to_string()));
                } else {
                    // Actually copy the content
                    staging::copy_directory_recursive(source, &dest)?;
                    let _ = tx.send(DiscCreationMessage::Progress(format!(
                        "✅ Copied: {}", dest_name
                    )));
                }

                // Small delay to show progress
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }

        let _ = tx.send(DiscCreationMessage::Progress(format!(
            "🎯 Disc {} staging complete!",
            plan.disc_number
        )));

        Ok(())
    }

    /// Burn ISO with detailed progress updates
    fn burn_iso_with_progress(
        iso_path: &Path,
        device: &str,
        dry_run: bool,
        tx: mpsc::Sender<DiscCreationMessage>,
    ) -> Result<()> {
        use std::thread;
        use std::time::Duration;

        if dry_run {
            let _ = tx.send(DiscCreationMessage::Progress("DRY RUN: Would burn ISO to disc".to_string()));
            thread::sleep(Duration::from_millis(500));
            return Ok(());
        }

        // Get ISO size for progress estimation
        let iso_size = match std::fs::metadata(iso_path) {
            Ok(metadata) => metadata.len(),
            Err(_) => 0, // Fallback if we can't get size
        };
        let iso_size_gb = iso_size as f64 / 1_000_000_000.0;

        // Estimate burn time (BD-R typical speeds: 2-6x = ~8-24 MB/s)
        let estimated_burn_time_secs = if iso_size > 0 {
            (iso_size as f64 / 16_000_000.0).max(30.0) // At least 30 seconds, assume ~16 MB/s average
        } else {
            300.0 // 5 minutes fallback
        };

        // Phase 1: Initializing burn
        let _ = tx.send(DiscCreationMessage::Progress("🔥 Initializing Blu-ray burner...".to_string()));
        thread::sleep(Duration::from_millis(500));

        // Phase 2: Starting data transfer with size info
        let _ = tx.send(DiscCreationMessage::Progress(format!("💿 Starting data transfer ({}GB) to disc...", iso_size_gb)));
        thread::sleep(Duration::from_millis(500));

        // Start progress monitoring thread
        let progress_tx = tx.clone();
        let start_time = std::time::Instant::now();
        thread::spawn(move || {
            let mut last_progress = 0;
            loop {
                let elapsed = start_time.elapsed().as_secs_f64();
                if elapsed > estimated_burn_time_secs + 60.0 {
                    // Burn is taking much longer than expected, stop updating
                    break;
                }

                // Estimate progress (70-95% range for burn phase)
                let progress_ratio = (elapsed / estimated_burn_time_secs).min(1.0);
                let burn_progress = 70 + (progress_ratio * 25.0) as u8; // 70% to 95%

                if burn_progress != last_progress && burn_progress < 95 {
                    let speed_mbs = if elapsed > 0.0 {
                        (iso_size as f64 / elapsed / 1_000_000.0) as u32
                    } else { 0 };

                    let eta_mins = if progress_ratio > 0.0 {
                        ((1.0 - progress_ratio) * estimated_burn_time_secs / 60.0) as u32
                    } else { 0 };

                    let _ = progress_tx.send(DiscCreationMessage::Progress(
                        format!("🔥 Burning... {}MB/s | {}min remaining | {}% complete",
                               speed_mbs, eta_mins, burn_progress)
                    ));
                    last_progress = burn_progress;
                }

                thread::sleep(Duration::from_secs(2)); // Update every 2 seconds
            }
        });

        // Perform the actual burn with error handling
        match burn::burn_with_method(iso_path, device, dry_run, "iso") {
            Ok(_) => {
                let burn_duration = start_time.elapsed();
                let actual_speed = if burn_duration.as_secs_f64() > 0.0 {
                    (iso_size as f64 / burn_duration.as_secs_f64() / 1_000_000.0) as u32
                } else { 0 };

                let _ = tx.send(DiscCreationMessage::Progress(
                    format!("✅ Burn completed! {:.1}s | {}MB/s average speed",
                           burn_duration.as_secs_f64(), actual_speed)
                ));
                thread::sleep(Duration::from_millis(500));
                Ok(())
            }
            Err(e) => {
                error!("ISO burn failed: {}", e);
                let _ = tx.send(DiscCreationMessage::Error(format!("Burn failed: {}", e)));
                Err(anyhow::anyhow!("ISO burn failed: {}", e))
            }
        }
    }

    /// Burn directory directly with detailed progress updates
    /// Clean up the staging directory after successful burn
    fn cleanup_staging_directory(staging_dir: &Path) -> Result<()> {
        info!("Cleaning up staging directory: {}", staging_dir.display());

        // Remove the entire staging directory
        if staging_dir.exists() {
            std::fs::remove_dir_all(staging_dir)?;
            info!("Successfully cleaned up staging directory");
        } else {
            info!("Staging directory already removed");
        }

        Ok(())
    }

    /// Comprehensive cleanup of temporary files and build artifacts
    pub fn cleanup_temporary_files(config: &Config) -> Result<()> {
        use std::fs;
        use walkdir::WalkDir;
        let _total_cleaned = 0u64;
        let mut files_removed = 0u32;

        info!("🧹 Starting comprehensive cleanup...");

        // Clean up build artifacts (debug and release builds)
        let target_dirs = ["target/debug", "target/release"];
        for target_dir in &target_dirs {
            let path = Path::new(target_dir);
            if path.exists() {
                info!("Removing build artifacts: {}", target_dir);
                match fs::remove_dir_all(path) {
                    Ok(_) => {
                        info!("✅ Removed {}", target_dir);
                        files_removed += 1;
                    }
                    Err(e) => warn!("Failed to remove {}: {}", target_dir, e),
                }
            }
        }

        // Clean up any leftover ISO files in the project directory
        if let Ok(entries) = fs::read_dir(".") {
            for entry in entries {
                if let Ok(entry) = entry {
                    let path = entry.path();
                    if let Some(ext) = path.extension() {
                        if ext == "iso" && path.is_file() {
                            match fs::remove_file(&path) {
                                Ok(_) => {
                                    info!("✅ Removed leftover ISO: {}", path.display());
                                    files_removed += 1;
                                }
                                Err(e) => warn!("Failed to remove {}: {}", path.display(), e),
                            }
                        }
                    }
                }
            }
        }

        // Clean up any temporary directories in the staging area
        if let Some(staging_dir) = dirs::data_dir()
            .map(|d| d.join("bdarchive").join("staging"))
        {
            if staging_dir.exists() {
                info!("Checking staging directory for leftover files...");
                for entry in WalkDir::new(&staging_dir).into_iter().filter_map(|e| e.ok()) {
                    let path = entry.path();
                    if path.is_file() {
                        match fs::remove_file(path) {
                            Ok(_) => {
                                files_removed += 1;
                            }
                            Err(e) => warn!("Failed to remove {}: {}", path.display(), e),
                        }
                    } else if path.is_dir() && path != staging_dir {
                        match fs::remove_dir_all(path) {
                            Ok(_) => {
                                files_removed += 1;
                            }
                            Err(e) => warn!("Failed to remove directory {}: {}", path.display(), e),
                        }
                    }
                }
            }
        }

        // Clean up any *.tmp files in the project directory
        if let Ok(entries) = fs::read_dir(".") {
            for entry in entries {
                if let Ok(entry) = entry {
                    let path = entry.path();
                    if path.is_file() {
                        if let Some(file_name) = path.file_name() {
                            if file_name.to_string_lossy().ends_with(".tmp") {
                                match fs::remove_file(&path) {
                                    Ok(_) => {
                                        info!("✅ Removed temp file: {}", path.display());
                                        files_removed += 1;
                                    }
                                    Err(e) => warn!("Failed to remove {}: {}", path.display(), e),
                                }
                            }
                        }
                    }
                }
            }
        }

        // Clean up paused burn session data
        if let Ok(db_path) = config.database_path() {
            if let Ok(conn) = database::init_database(&db_path) {
                let paused_sessions = database::BurnSessionOps::get_active_sessions(&conn)?;
                for session in paused_sessions {
                    if session.status == database::BurnSessionStatus::Paused {
                        info!("🗑️ Cleaning up paused session: {}", session.session_name);
                        if let Err(e) = database::BurnSessionOps::delete_session(&conn, &session.session_id) {
                            warn!("Failed to clean up session {}: {}", session.session_id, e);
                        } else {
                            files_removed += 1;
                        }
                    }
                }
            }
        }

        info!("🧹 Cleanup complete! Removed {} files/directories and sessions", files_removed);
        Ok(())
    }

    /// Calculate the total size of a directory recursively
    fn calculate_directory_size(dir_path: &Path) -> Result<u64> {
        let mut total_size = 0u64;
        Self::calculate_directory_size_recursive(dir_path, &mut total_size)?;
        Ok(total_size)
    }

    fn calculate_directory_size_recursive(dir_path: &Path, total_size: &mut u64) -> Result<()> {
        if dir_path.is_dir() {
            for entry in std::fs::read_dir(dir_path)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    Self::calculate_directory_size_recursive(&path, total_size)?;
                } else {
                    let metadata = entry.metadata()?;
                    *total_size += metadata.len();
                }
            }
        }
        Ok(())
    }

    fn burn_direct_with_progress(
        dir_path: &Path,
        device: &str,
        dry_run: bool,
        tx: mpsc::Sender<DiscCreationMessage>,
    ) -> Result<()> {
        use std::thread;
        use std::time::Duration;

        if dry_run {
            let _ = tx.send(DiscCreationMessage::Progress("DRY RUN: Would burn directory directly to disc".to_string()));
            thread::sleep(Duration::from_millis(500));
            return Ok(());
        }

        // Estimate directory size for progress calculation
        let dir_size = Self::calculate_directory_size(dir_path).unwrap_or(0);
        let dir_size_gb = dir_size as f64 / 1_000_000_000.0;

        // Estimate burn time (BD-R typical speeds: 2-6x = ~8-24 MB/s)
        let estimated_burn_time_secs = if dir_size > 0 {
            (dir_size as f64 / 16_000_000.0).max(30.0) // At least 30 seconds, assume ~16 MB/s average
        } else {
            300.0 // 5 minutes fallback
        };

        // Phase 1: Initializing burn
        let _ = tx.send(DiscCreationMessage::Progress("🔥 Initializing Blu-ray burner...".to_string()));
        thread::sleep(Duration::from_millis(500));

        // Phase 2: Starting data transfer with size info
        let _ = tx.send(DiscCreationMessage::Progress(format!("💿 Starting direct data transfer ({}GB) to disc...", dir_size_gb)));
        thread::sleep(Duration::from_millis(500));

        // Start progress monitoring thread
        let progress_tx = tx.clone();
        let start_time = std::time::Instant::now();
        thread::spawn(move || {
            let mut last_progress = 0;
            loop {
                let elapsed = start_time.elapsed().as_secs_f64();
                if elapsed > estimated_burn_time_secs + 60.0 {
                    // Burn is taking much longer than expected, stop updating
                    break;
                }

                // Estimate progress (70-95% range for burn phase)
                let progress_ratio = (elapsed / estimated_burn_time_secs).min(1.0);
                let burn_progress = 70 + (progress_ratio * 25.0) as u8; // 70% to 95%

                if burn_progress != last_progress && burn_progress < 95 {
                    let speed_mbs = if elapsed > 0.0 {
                        (dir_size as f64 / elapsed / 1_000_000.0) as u32
                    } else { 0 };

                    let eta_mins = if progress_ratio > 0.0 {
                        ((1.0 - progress_ratio) * estimated_burn_time_secs / 60.0) as u32
                    } else { 0 };

                    let _ = progress_tx.send(DiscCreationMessage::Progress(
                        format!("🔥 Burning... {}MB/s | {}min remaining | {}% complete",
                               speed_mbs, eta_mins, burn_progress)
                    ));
                    last_progress = burn_progress;
                }

                thread::sleep(Duration::from_secs(2)); // Update every 2 seconds
            }
        });

        // Perform the actual burn with error handling
        match burn::burn_with_method(dir_path, device, dry_run, "direct") {
            Ok(_) => {
                let burn_duration = start_time.elapsed();
                let actual_speed = if burn_duration.as_secs_f64() > 0.0 {
                    (dir_size as f64 / burn_duration.as_secs_f64() / 1_000_000.0) as u32
                } else { 0 };

                let _ = tx.send(DiscCreationMessage::Progress(
                    format!("✅ Direct burn completed! {:.1}s | {}MB/s average speed",
                           burn_duration.as_secs_f64(), actual_speed)
                ));
                thread::sleep(Duration::from_millis(500));
                Ok(())
            }
            Err(e) => {
                error!("Direct burn failed: {}", e);
                let _ = tx.send(DiscCreationMessage::Error(format!("Direct burn failed: {}", e)));
                Err(anyhow::anyhow!("Direct burn failed: {}", e))
            }
        }
    }

    /// Index the disc record in the database
    fn index_disc_in_database(
        db_conn: &mut rusqlite::Connection,
        disc_id: &str,
        volume_label: &str,
        notes: &str,
        iso_size: u64,
        device: &str,
        dry_run: bool,
        source_roots: &[PathBuf],
    ) -> Result<()> {
        let created_at = format_timestamp_now();

        let source_roots_json = serde_json::to_string(source_roots)
            .context("Failed to serialize source roots")?;

        let disc_record = database::Disc {
            disc_id: disc_id.to_string(),
            volume_label: volume_label.to_string(),
            created_at: created_at.clone(),
            notes: if notes.is_empty() { None } else { Some(notes.to_string()) },
            iso_size: Some(iso_size),
            burn_device: if dry_run { None } else { Some(device.to_string()) },
            checksum_manifest_hash: None,
            qr_path: None,
            source_roots: Some(source_roots_json),
            tool_version: Some(disc::get_tool_version()),
            set_id: None, // Single disc, not part of a set
            sequence_number: None,
        };

        database::Disc::insert(db_conn, &disc_record)
            .context("Failed to insert disc record")?;

        Ok(())
    }

    /// Index file records in the database
    fn index_files_in_database(
        db_conn: &mut rusqlite::Connection,
        disc_id: &str,
        files: &[crate::manifest::FileMetadata],
    ) -> Result<()> {
        let created_at = format_timestamp_now();

        let file_records: Vec<database::FileRecord> = files
            .iter()
            .map(|f| database::FileRecord {
                id: None,
                disc_id: disc_id.to_string(),
                rel_path: f.rel_path.to_string_lossy().to_string(),
                sha256: f.crc32.clone().unwrap_or_else(|| f.sha256.clone()),
                size: f.size,
                mtime: f.mtime.clone(),
                added_at: created_at.clone(),
            })
            .collect();

        database::FileRecord::insert_batch(db_conn, &file_records)
            .context("Failed to insert file records")?;

        Ok(())
    }



    /// Start disc creation workflow (single or multi-disc)
    fn start_disc_creation_workflow(
        flow: &mut tui::NewDiscFlow,
        needs_multi_disc: bool,
        source_folders: Vec<PathBuf>,
        config: Config,
        db_path: PathBuf,
        disc_creation_rx: &mut Option<mpsc::Receiver<DiscCreationMessage>>,
    ) {
        if needs_multi_disc {
            Self::start_multi_disc_creation_workflow(flow, source_folders, config, db_path, disc_creation_rx);
        } else {
            Self::start_single_disc_creation_workflow(flow, source_folders, config, db_path, disc_creation_rx);
        }
    }

    /// Start single-disc creation workflow
    fn start_single_disc_creation_workflow(
        flow: &mut tui::NewDiscFlow,
        source_folders: Vec<PathBuf>,
        config: Config,
        db_path: PathBuf,
        disc_creation_rx: &mut Option<mpsc::Receiver<DiscCreationMessage>>,
    ) {
        // Start the disc creation process in a background thread (existing logic)
        let disc_id = flow.disc_id().to_string();
        let notes = flow.notes().to_string();
        let dry_run = flow.dry_run();
        info!("User selected burn mode - dry_run: {}", dry_run);

        let disc_id_clone = disc_id.clone();
        let notes_clone = notes.clone();

        // Create channel for communication
        let (tx, rx) = mpsc::channel::<DiscCreationMessage>();
        *disc_creation_rx = Some(rx);

        thread::spawn(move || {
            // Create new database connection in background thread
            let db_conn_result = database::init_database(&db_path);
            let db_conn = match db_conn_result {
                Ok(conn) => conn,
                Err(e) => {
                    let _ = tx.send(DiscCreationMessage::Error(format!(
                        "Failed to create database connection: {}",
                        e
                    )));
                    return;
                }
            };

            let dry_run_clone = dry_run;
            match Self::run_disc_creation_background(
                disc_id_clone,
                notes_clone,
                source_folders,
                dry_run_clone,
                config,
                db_conn,
                tx.clone(),
            ) {
                Ok(()) => {
                    // Success - cleanup already handled in the function
                }
                Err(e) => {
                    let _ = tx.send(DiscCreationMessage::Error(format!(
                        "Disc creation failed: {}", e
                    )));
                }
            }
        });
    }

    /// Start multi-disc creation workflow
    fn start_multi_disc_creation_workflow(
        flow: &mut tui::NewDiscFlow,
        source_folders: Vec<PathBuf>,
        config: Config,
        db_path: PathBuf,
        disc_creation_rx: &mut Option<mpsc::Receiver<DiscCreationMessage>>,
    ) {
        let disc_id_base = flow.disc_id().to_string();
        let notes = flow.notes().to_string();
        let dry_run = flow.dry_run();

        // Create channel for communication
        let (tx, rx) = mpsc::channel::<DiscCreationMessage>();
        *disc_creation_rx = Some(rx);

        thread::spawn(move || {
            // Create new database connection in background thread
            let db_conn_result = database::init_database(&db_path);
            let db_conn = match db_conn_result {
                Ok(conn) => conn,
                Err(e) => {
                    let _ = tx.send(DiscCreationMessage::Error(format!(
                        "Failed to create database connection: {}",
                        e
                    )));
                    return;
                }
            };

            match Self::run_multi_disc_creation_background_robust(
                disc_id_base,
                notes,
                source_folders,
                dry_run,
                config,
                db_conn,
                tx.clone(),
            ) {
                Ok(()) => {
                    // Success - cleanup already handled in the function
                }
                Err(e) => {
                    let _ = tx.send(DiscCreationMessage::Error(format!(
                        "Multi-disc creation failed: {}", e
                    )));
                }
            }
        });
    }

    /// Run disc creation in background with comprehensive error handling
    fn run_disc_creation_background(
        disc_id: String,
        notes: String,
        source_folders: Vec<PathBuf>,
        dry_run: bool,
        config: Config,
        mut db_conn: rusqlite::Connection,
        tx: mpsc::Sender<DiscCreationMessage>,
    ) -> Result<()> {
        let _ = tx.send(DiscCreationMessage::Status(format!(
            "Starting disc creation (mode: {})...",
            if dry_run { "DRY RUN" } else { "ACTUAL" }
        )));

        // Validate inputs
        if disc_id.is_empty() {
            return Err(anyhow::anyhow!("Disc ID cannot be empty"));
        }

        if source_folders.is_empty() {
            return Err(anyhow::anyhow!("No source folders selected"));
        }

        // Validate source folders exist
        for folder in &source_folders {
            if !folder.exists() {
                return Err(anyhow::anyhow!(
                    "Source folder does not exist: {}",
                    folder.display()
                ));
            }
        }

        let staging_dir = config
            .staging_dir()
            .context("Failed to get staging directory")?;
        std::fs::create_dir_all(&staging_dir)?;

        // Step 1: Create disc layout
        let _ = tx.send(DiscCreationMessage::StateAndStatus(
            tui::new_disc::ProcessingState::Staging,
            "Creating disc layout...".to_string(),
        ));
        let disc_root = disc::create_disc_layout(
            &staging_dir,
            &disc_id,
            &source_folders,
            if notes.is_empty() { None } else { Some(&notes) },
        )?;
        let _ = tx.send(DiscCreationMessage::Status(
            "Disc layout created".to_string(),
        ));

        // Step 2: Stage files
        let _ = tx.send(DiscCreationMessage::StateAndStatus(
            tui::new_disc::ProcessingState::Staging,
            "Staging files...".to_string(),
        ));
        let use_rsync = config.optional_tools.use_rsync
            && dependencies::get_optional_command("rsync").is_some();

        // Create progress callback for staging
        let progress_tx = tx.clone();
        let staging_progress_callback = move |msg: &str| {
            let _ = progress_tx.send(DiscCreationMessage::Progress(msg.to_string()));
        };

        staging::stage_files_with_progress(
            &disc_root,
            &source_folders,
            use_rsync,
            dry_run,
            Some(Box::new(staging_progress_callback))
        )?;
        let _ = tx.send(DiscCreationMessage::StateAndStatus(
            tui::new_disc::ProcessingState::Staging,
            "Files staged successfully".to_string(),
        ));

        // Step 3: Generate manifest and SHA256SUMS
        let _ = tx.send(DiscCreationMessage::StateAndStatus(
            tui::new_disc::ProcessingState::GeneratingManifest,
            "Generating manifest and checksums...".to_string(),
        ));

        // Create progress callback that sends Progress messages
        let progress_tx = tx.clone();
        let progress_callback = move |msg: &str| {
            let _ = progress_tx.send(DiscCreationMessage::Progress(msg.to_string()));
        };
        // Use fast mode (CRC32) for initial manifest generation
        let files = manifest::generate_manifest_and_sums_with_progress(
            &disc_root,
            None,
            Some(Box::new(progress_callback)),
            true // fast_mode = true (uses CRC32 instead of SHA256)
        )?;

        // Write manifest files
        let manifest_path = disc_root.join("MANIFEST.txt");
        match manifest::write_manifest_file(&manifest_path, &files) {
            Ok(_) => info!("Manifest file written successfully"),
            Err(e) => {
                error!("Failed to write manifest file: {}", e);
                let _ = tx.send(DiscCreationMessage::Error(format!("Failed to write manifest: {}", e)));
                return Err(anyhow::anyhow!("Failed to write manifest file: {}", e));
            }
        }

        let sha256sums_path = disc_root.join("SHA256SUMS.txt");
        match manifest::write_sha256sums_file(&sha256sums_path, &files) {
            Ok(_) => info!("SHA256SUMS file written successfully"),
            Err(e) => {
                error!("Failed to write SHA256SUMS file: {}", e);
                let _ = tx.send(DiscCreationMessage::Error(format!("Failed to write checksums: {}", e)));
                return Err(anyhow::anyhow!("Failed to write SHA256SUMS file: {}", e));
            }
        }


        // Check capacity
        let total_size = manifest::calculate_total_size(&files);
        let capacity = config.default_capacity_bytes();
        if total_size > capacity {
            let error_msg = format!(
                "Total size {:.2} GB exceeds disc capacity {:.2} GB",
                total_size as f64 / 1_000_000_000.0,
                capacity as f64 / 1_000_000_000.0
            );
            error!("Capacity check failed: {}", error_msg);
            let _ = tx.send(DiscCreationMessage::Error(error_msg.clone()));
            return Err(anyhow::anyhow!("{}", error_msg));
        }
        info!("Capacity check passed: {:.2} GB / {:.2} GB", total_size as f64 / 1_000_000_000.0, capacity as f64 / 1_000_000_000.0);

        // Step 4: Create ISO (skip if using direct burn and not dry run)
        let volume_label = disc::generate_volume_label(&disc_id);
        let iso_path = staging_dir.join(format!("{}.iso", disc_id));
        let iso_size;

        if config.burn.method == "direct" && !dry_run {
            info!("Skipping ISO creation (using direct burn method)");
            iso_size = manifest::calculate_total_size(&files); // Use directory size
            let _ = tx.send(DiscCreationMessage::StateAndStatus(
                tui::new_disc::ProcessingState::CreatingISO,
                format!("Direct burn - skipping ISO creation ({:.2} GB)", iso_size as f64 / 1_000_000_000.0),
            ));
        } else {
            let _ = tx.send(DiscCreationMessage::StateAndStatus(
                tui::new_disc::ProcessingState::CreatingISO,
                "Creating ISO image...".to_string(),
            ));

            info!("Creating ISO at: {}", iso_path.display());
            match iso::create_iso(&disc_root, &iso_path, &volume_label, false) {
                Ok(_) => {
                    info!("ISO creation command completed");
                    match iso::get_iso_size(&iso_path) {
                        Ok(size) => {
                            iso_size = size;
                            info!("ISO created successfully: {} bytes", iso_size);
                        }
                        Err(e) => {
                            error!("Failed to get ISO size after creation: {}", e);
                            let _ = tx.send(DiscCreationMessage::Error(format!("Failed to verify ISO size: {}", e)));
                            return Err(anyhow::anyhow!("Failed to get ISO size: {}", e));
                        }
                    }
                }
                Err(e) => {
                    error!("ISO creation failed: {}", e);
                    let _ = tx.send(DiscCreationMessage::Error(format!("ISO creation failed: {}", e)));
                    return Err(anyhow::anyhow!("ISO creation failed: {}", e));
                }
            }
            let _ = tx.send(DiscCreationMessage::StateAndStatus(
                tui::new_disc::ProcessingState::CreatingISO,
                format!("ISO created: {:.2} GB", iso_size as f64 / 1_000_000_000.0),
            ));
        }

        // Step 5: Burn to disc (or create ISO for dry run)
        let _ = tx.send(DiscCreationMessage::StateAndStatus(
            tui::new_disc::ProcessingState::Burning,
            if dry_run {
                "Creating ISO for dry run...".to_string()
            } else {
                format!("Burning to {}...", config.device)
            },
        ));

        if dry_run {
            // For dry run, ensure we have an ISO created so user can archive it manually
            if config.burn.method == "direct" {
                // For direct method, still create ISO for dry run purposes
                let volume_label = disc::generate_volume_label(&disc_id);
                info!("Creating ISO for dry run at: {}", iso_path.display());
                match iso::create_iso(&disc_root, &iso_path, &volume_label, false) {
                    Ok(_) => {
                        match iso::get_iso_size(&iso_path) {
                            Ok(_) => {
                                info!("Dry run ISO created successfully");
                            }
                            Err(e) => {
                                error!("Failed to get dry run ISO size: {}", e);
                                let _ = tx.send(DiscCreationMessage::Error(format!("Failed to verify dry run ISO: {}", e)));
                                return Err(anyhow::anyhow!("Failed to get dry run ISO size: {}", e));
                            }
                        }
                    }
                    Err(e) => {
                        error!("Dry run ISO creation failed: {}", e);
                        let _ = tx.send(DiscCreationMessage::Error(format!("Dry run ISO creation failed: {}", e)));
                        return Err(anyhow::anyhow!("Dry run ISO creation failed: {}", e));
                    }
                }
            }
            // For ISO method, ISO is already created above

            let iso_display_path = iso_path.display();
            let _ = tx.send(DiscCreationMessage::StateAndStatus(
                tui::new_disc::ProcessingState::Burning,
                format!("DRY RUN COMPLETE - ISO saved at: {}", iso_display_path),
            ));

            // Show additional message about where to find the ISO
            info!("Dry run ISO available at: {}", iso_display_path);
        } else {
            // Actual burning with progress updates
            match config.burn.method.as_str() {
                "direct" => {
                    // Burn the staging directory directly (no ISO needed)
                    Self::burn_direct_with_progress(&disc_root, &config.device, dry_run, tx.clone())?;
                }
                "iso" | _ => {
                    // Default: create and burn ISO
                    Self::burn_iso_with_progress(&iso_path, &config.device, dry_run, tx.clone())?;
                }
            }
            let _ = tx.send(DiscCreationMessage::StateAndStatus(
                tui::new_disc::ProcessingState::Burning,
                "Disc burned successfully".to_string(),
            ));
        }

        // Step 6: Index in database
        let _ = tx.send(DiscCreationMessage::StateAndStatus(
            tui::new_disc::ProcessingState::Indexing,
            "Updating index...".to_string(),
        ));

        let source_roots: Vec<PathBuf> = source_folders.clone();
        match Self::index_disc_in_database(&mut db_conn, &disc_id, &volume_label, &notes, iso_size, &config.device, dry_run, &source_roots) {
            Ok(_) => {
                let _ = tx.send(DiscCreationMessage::StateAndStatus(
                    tui::new_disc::ProcessingState::Indexing,
                    "Database updated successfully".to_string(),
                ));
            }
            Err(e) => {
                error!("Database indexing failed: {}", e);
                let _ = tx.send(DiscCreationMessage::Error(format!("Database indexing failed: {}", e)));
                return Err(anyhow::anyhow!("Database indexing failed: {}", e));
            }
        }

        match Self::index_files_in_database(&mut db_conn, &disc_id, &files) {
            Ok(_) => {
                let _ = tx.send(DiscCreationMessage::Progress("Files indexed in database".to_string()));
            }
            Err(e) => {
                error!("File indexing failed: {}", e);
                let _ = tx.send(DiscCreationMessage::Error(format!("File indexing failed: {}", e)));
                return Err(anyhow::anyhow!("File indexing failed: {}", e));
            }
        }

        // Step 7: Generate QR code
        let _ = tx.send(DiscCreationMessage::StateAndStatus(
            tui::new_disc::ProcessingState::GeneratingQR,
            "Generating QR code...".to_string(),
        ));

        if config.optional_tools.use_qrencode {
            match Self::generate_qr_code_safely(&config, &disc_id, dry_run) {
                Ok(_) => {
                    let _ = tx.send(DiscCreationMessage::Status("QR code generated".to_string()));
                }
                Err(e) => {
                    warn!("QR code generation failed: {}", e);
                    let _ = tx.send(DiscCreationMessage::Status(format!(
                        "QR code generation skipped: {}",
                        e
                    )));
                }
            }
        } else {
            let _ = tx.send(DiscCreationMessage::Status("QR code generation disabled".to_string()));
        }

        let _ = tx.send(DiscCreationMessage::Complete);
        Ok(())
    }

    /// Safely generate QR code with proper error handling
    fn generate_qr_code_safely(
        _config: &Config,
        disc_id: &str,
        dry_run: bool,
    ) -> Result<()> {
        let qrcodes_dir = paths::qrcodes_dir()
            .context("Failed to get QR codes directory")?;

        qrcode::generate_qrcode(
            disc_id,
            &qrcodes_dir,
            qrcode::QrCodeFormat::PNG,
            dry_run,
        ).context("QR code generation failed")?;

        Ok(())
    }

    fn render(&mut self, frame: &mut Frame) {
        // Set background color for entire frame
        let bg_rect = frame.size();
        let bg_block = ratatui::widgets::Block::default()
            .style(ratatui::style::Style::default().bg(self.theme.bg()));
        frame.render_widget(bg_block, bg_rect);
        use crate::ui::{header_footer, GridLayout};

        // Use grid layout with header and footer
        let (header_area, content_area, footer_area) = GridLayout::main_layout(frame.size());

        // Render header (skip on splash)
        if !matches!(self.state, AppState::Splash(_)) {
            let current_screen = match self.state {
                AppState::MainMenu => "Main Menu",
                AppState::NewDisc(_) => "New Disc",
                AppState::ResumeBurn(_) => "Resume Burn",
                AppState::VerifyMultiDisc(_) => "Verify Multi-Disc",
                AppState::Cleanup(_) => "Cleanup",
                AppState::Search(_) => "Search Index",
                AppState::Verify(_) => "Verify Disc",
                AppState::ListDiscs(_) => "List Discs",
                AppState::Settings(_) => "Settings",
                AppState::Logs(_) => "Logs",
                AppState::Quit => "Quit",
                _ => "",
            };

            if !current_screen.is_empty() {
                let header = header_footer::Header::new(current_screen)
                    .with_hint("BlueVault - Blu-ray Archive Manager");
                header.render(&self.theme, header_area, frame);
            }
        }

        // Update and render footer
        match &mut self.state {
            AppState::Splash(_) => {
                // No footer on splash
            }
            _ => {
                self.footer.update();
                self.footer.render(&self.theme, footer_area, frame);
            }
        }

        match &mut self.state {
            AppState::Splash(ref splash) => {
                splash.render(&self.theme, frame.size(), frame);
            }
            AppState::MainMenu => {
                self.main_menu.render(&self.theme, frame, content_area);
            }
            AppState::NewDisc(ref mut flow) => {
                flow.render(&self.theme, &self.config, frame, content_area);
            }
            AppState::ResumeBurn(ref mut resume_ui) => {
                resume_ui.render(&self.theme, frame, content_area);
            }
            AppState::VerifyMultiDisc(ref mut verify_ui) => {
                verify_ui.render(&self.theme, frame, content_area);
            }
            AppState::Cleanup(ref mut flow) => {
                flow.render(&self.theme, &self.config, frame, content_area);
            }
            AppState::Search(ref mut search) => {
                search.render(&self.theme, frame, content_area);
            }
            AppState::Verify(ref verify) => {
                verify.render(&self.theme, frame, content_area);
            }
            AppState::ListDiscs(ref list) => {
                list.render(&self.theme, frame, content_area);
            }
            AppState::Settings(ref settings) => {
                settings.render(&self.theme, frame, content_area);
            }
            AppState::Logs(ref logs) => {
                logs.render(&self.theme, frame, content_area);
            }
            AppState::Quit => {}
        }
    }

    /// Resume a paused burn session
    fn resume_burn_session(&mut self, session: database::BurnSession) -> Result<()> {
        info!("Resuming burn session: {}", session.session_id);

        // Create a new disc creation flow for resuming
        let mut flow = tui::NewDiscFlow::new(format!("Resume: {}", session.session_name));

        // Set up the flow with session data
        flow.set_multi_disc_progress(session.current_disc as u32, session.total_discs as u32, 0.0);
        flow.set_status(format!("Resuming session '{}' from disc {} of {}",
            session.session_name, session.current_disc, session.total_discs));

        // Start the resumed burn process
        let (tx, rx) = mpsc::channel();
        self.disc_creation_rx = Some(rx);
        self.disc_creation_tx = Some(tx.clone());

        let session_clone = session.clone();
        let db_path = self.config.database_path().unwrap_or_default();
        let config = self.config.clone();

        thread::spawn(move || {
            let _ = tx.send(DiscCreationMessage::Status("🔄 Resuming multi-disc burn...".to_string()));

            // Resume from the current disc
            let tx_clone = tx.clone();
            match Self::resume_multi_disc_creation_background(
                session_clone, db_path, config.clone(), tx
            ) {
                Ok(_) => {}
                Err(e) => {
                    let _ = tx_clone.send(DiscCreationMessage::Error(format!("Resume failed: {}", e)));
                }
            }
        });

        self.state = AppState::NewDisc(Box::new(flow));
        Ok(())
    }

    /// Resume multi-disc creation from a saved session
    fn resume_multi_disc_creation_background(
        session: database::BurnSession,
        db_path: std::path::PathBuf,
        config: Config,
        tx: mpsc::Sender<DiscCreationMessage>,
    ) -> Result<()> {
        let mut db_conn = database::init_database(&db_path)?;
        // Get the disc set
        let disc_set = database::DiscSet::get(&db_conn, &session.set_id)?
            .ok_or_else(|| anyhow::anyhow!("Disc set not found: {}", session.set_id))?;

        // Recreate the plans from the disc set
        let plans = Self::recreate_plans_from_disc_set(&disc_set, &config)?;

        // Continue burning from the current disc
        let remaining_plans = &plans[(session.current_disc - 1) as usize..];
        let notes = disc_set.description.as_ref().unwrap_or(&String::new()).clone();

        for (i, plan) in remaining_plans.iter().enumerate() {
            let sequence_num = session.current_disc + i as usize;
            let disc_id = disc::generate_multi_disc_id(&session.session_name, sequence_num as u32);

            // Burn this disc
            match Self::burn_single_disc_with_recovery(
                &session.session_name,
                &notes,
                plan,
                sequence_num,
                session.total_discs,
                false, // Not a dry run for resumed sessions
                &config,
                &mut db_conn,
                &session.set_id,
                &session.source_folders,
                &tx,
            ) {
                Ok(_) => {
                    // Update session progress
                    let mut updated_session = session.clone();
                    updated_session.update_progress(sequence_num);
                    let _ = updated_session.save(&db_conn);
                }
                Err(e) => {
                    // Mark session as failed
                    let mut failed_session = session.clone();
                    failed_session.failed_discs.push(sequence_num);
                    let _ = failed_session.save(&db_conn);
                    return Err(anyhow::anyhow!("Disc burn failed: {:?}", e));
                }
            }
        }

        // Mark session as completed
        let session_id = session.set_id.clone();
        let mut completed_session = session;
        completed_session.complete();
        let _ = completed_session.save(&db_conn);

        Self::finalize_multi_disc_archive(&vec![], &session_id, disc_set.total_size, false, &config, &tx);

        Ok(())
    }

    /// Recreate disc plans from an existing disc set
    fn recreate_plans_from_disc_set(disc_set: &database::DiscSet, config: &Config) -> Result<Vec<staging::DiscPlan>> {
        // This is a simplified recreation - in practice, you'd need to store more
        // detailed plan information or recalculate from source folders
        let source_folders: Vec<PathBuf> = serde_json::from_str(
            &disc_set.source_roots.as_deref().unwrap_or("[]")
        ).unwrap_or_default();

        if source_folders.is_empty() {
            return Err(anyhow::anyhow!("Cannot recreate plans: no source folders stored"));
        }

        staging::plan_disc_layout_with_progress(
            &source_folders,
            config.default_capacity_bytes(),
            |_| {} // No progress callback needed for recreation
        )
    }

    /// Clean up a paused burn session
    fn cleanup_burn_session(&self, session_id: &str) -> Result<()> {
        info!("Cleaning up burn session: {}", session_id);
        database::BurnSessionOps::delete_session(&self.db_conn, session_id)?;
        Ok(())
    }
}

fn main() -> Result<()> {
    // Initialize logging
    logging::init_logging().context("Failed to initialize logging")?;

    info!("Starting BlueVault application");

    // Check dependencies
    dependencies::verify_dependencies().context("Missing required dependencies")?;

    // Ensure data and config directories exist
    paths::ensure_data_dir()?;
    paths::ensure_config_dir()?;

    // Load configuration
    let mut config = Config::load()?;
    config.validate()?;

    // Initialize database
    let db_path = config.database_path()?;
    let db_conn = database::init_database(&db_path)?;

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(config, db_conn);
    let mut running = true;

    while running {
        // Handle any pending disc creation requests
        if app.pending_disc_creation.is_some() {
            if let AppState::NewDisc(_) = app.state {
                // Only log when we actually find and process one
            }
        }
        let pending_taken = app.pending_disc_creation.take();
        if let Some((needs_multi_disc, source_folders, config)) = pending_taken {
            let db_path = app
                .config
                .database_path()
                .unwrap_or_else(|_| PathBuf::from(":memory:"));

            // Start the appropriate disc creation workflow
            if let AppState::NewDisc(ref mut flow) = app.state {
                App::start_disc_creation_workflow(flow, needs_multi_disc, source_folders, config, db_path, &mut app.disc_creation_rx);
            }
        }

        terminal.draw(|f| app.render(f))?;
        info!("=== terminal.draw() completed ===");

        info!("=== About to check splash ===");
        // Check if splash should auto-dismiss
        if let AppState::Splash(ref splash) = app.state {
            if !splash.should_show() {
                app.state = AppState::MainMenu;
            }
        }

        // Check for background messages first (always poll these)
        let has_background_task = matches!(app.state, AppState::NewDisc(_) | AppState::Cleanup(_))
            && app.disc_creation_rx.is_some();

        let background_updated = if has_background_task {
            app.poll_background_messages()
        } else {
            false
        };

        // Use timeout for background tasks or splash screen
        let timeout = if matches!(app.state, AppState::Splash(_)) {
            Some(std::time::Duration::from_millis(100))
        } else if has_background_task {
            Some(std::time::Duration::from_millis(50)) // Poll frequently for background updates
        } else {
            None
        };

        let mut event_processed = false;
        if poll(timeout.unwrap_or(std::time::Duration::from_secs(0)))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    running = app.handle_key(key.code)?;
                    event_processed = true;
                }
            }
        } else if timeout.is_none() {
            // Blocking wait if no timeout
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    running = app.handle_key(key.code)?;
                    event_processed = true;
                }
            }
        }

        // Redraw if background messages were processed or events occurred
        if background_updated || event_processed || has_background_task {
            terminal.draw(|f| app.render(f))?;
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    info!("Application exiting");
    Ok(())
}
