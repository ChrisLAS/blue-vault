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
    Search(tui::SearchUI),
    Verify(tui::VerifyUI),
    ListDiscs(tui::ListDiscs),
    Settings(tui::Settings),
    Logs(tui::LogsView),
    Quit,
}

#[derive(Debug)]
enum DiscCreationMessage {
    Status(String),
    StateAndStatus(tui::new_disc::ProcessingState, String),
    Progress(String),
    Complete,
    Error(String),
}

struct App {
    state: AppState,
    main_menu: tui::MainMenu,
    config: Config,
    db_conn: rusqlite::Connection,
    theme: theme::Theme,
    footer: ui::header_footer::Footer,
    disc_creation_rx: Option<mpsc::Receiver<DiscCreationMessage>>,
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
                        flow.set_file_progress(progress);
                        updated = true;
                    }
                    Ok(DiscCreationMessage::Complete) => {
                        flow.set_processing_state(tui::new_disc::ProcessingState::Complete);
                        flow.set_status("Disc creation completed successfully!".to_string());
                        self.disc_creation_rx = None; // Clean up
                        updated = true;
                    }
                    Ok(DiscCreationMessage::Error(error)) => {
                        flow.set_error(error);
                        self.disc_creation_rx = None; // Clean up
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
                    KeyCode::Enter => {
                        match flow.current_step() {
                            tui::new_disc::NewDiscStep::EnterDiscId => {
                                flow.next_step();
                            }
                            tui::new_disc::NewDiscStep::EnterNotes => {
                                flow.next_step();
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
                                    flow.next_step();
                                }
                            }
                            tui::new_disc::NewDiscStep::Review => {
                                // For Review step, Enter starts the process
                                flow.next_step();

                                // Start the disc creation process in a background thread
                                let disc_id = flow.disc_id().to_string();
                                let notes = flow.notes().to_string();
                                let source_folders = flow.source_folders().to_vec();
                                let dry_run = flow.dry_run();
                                info!("User selected burn mode - dry_run: {}", dry_run);
                                let config = self.config.clone();

                                // Create channel for communication
                                let (tx, rx) = mpsc::channel();
                                self.disc_creation_rx = Some(rx);

                                // Spawn background thread for disc creation
                                // Clone the database path and create a new connection in the background thread
                                let db_path = self
                                    .config
                                    .database_path()
                                    .unwrap_or_else(|_| PathBuf::from(":memory:"));
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

                                    if let Err(_e) = Self::run_disc_creation_background(
                                        disc_id,
                                        notes,
                                        source_folders,
                                        dry_run,
                                        config,
                                        db_conn,
                                        tx,
                                    ) {
                                        // Error already sent via tx in the function
                                    }
                                });

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

        // Complete!
        flow.set_processing_state(tui::new_disc::ProcessingState::Complete);
        flow.set_status(format!("Disc {} created successfully!", disc_id));

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

        // Write DISC_INFO.txt
        let source_roots: Vec<PathBuf> = source_folders;
        match disc::write_disc_info(
            &disc_root,
            &disc_id,
            if notes.is_empty() { None } else { Some(&notes) },
            &source_roots,
            &disc::get_tool_version(),
        ) {
            Ok(_) => info!("Disc info written successfully"),
            Err(e) => {
                error!("Failed to write disc info: {}", e);
                let _ = tx.send(DiscCreationMessage::Error(format!("Failed to write disc info: {}", e)));
                return Err(anyhow::anyhow!("Failed to write disc info: {}", e));
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
                flow.render(&self.theme, frame, content_area);
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
    let config = Config::load()?;
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
        terminal.draw(|f| app.render(f))?;

        // Check if splash should auto-dismiss
        if let AppState::Splash(ref splash) = app.state {
            if !splash.should_show() {
                app.state = AppState::MainMenu;
            }
        }

        // Check for background messages first (always poll these)
        let has_background_task = matches!(app.state, AppState::NewDisc(_))
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
