use anyhow::{Context, Result};
use crossterm::{
    event::{self, poll, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::io;
use std::path::PathBuf;
use tracing::info;
use bdarchive::*;
use bdarchive::tui::directory_selector::Focus as DirFocus;

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

struct App {
    state: AppState,
    main_menu: tui::MainMenu,
    config: Config,
    db_conn: rusqlite::Connection,
    theme: theme::Theme,
    footer: ui::header_footer::Footer,
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
        }
    }

    fn handle_key(&mut self, key: KeyCode) -> Result<bool> {
        match &mut self.state {
            AppState::Splash(ref mut splash) => {
                // Skip splash on any keypress
                splash.skip();
                self.state = AppState::MainMenu;
                return Ok(true);
            }
            AppState::MainMenu => {
                match key {
                    KeyCode::Up | KeyCode::Char('k') => self.main_menu.previous(),
                    KeyCode::Down | KeyCode::Char('j') => self.main_menu.next(),
                    KeyCode::Enter => {
                        match self.main_menu.selected_action() {
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
                        }
                    }
                    KeyCode::Esc | KeyCode::Char('q') => {
                        return Ok(false);
                    }
                    _ => {}
                }
            }
            AppState::NewDisc(ref mut flow) => {
                match key {
                    KeyCode::Esc => {
                        if flow.current_step() == tui::new_disc::NewDiscStep::Processing {
                            // Can't escape during processing
                            return Ok(true);
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
                                        flow.set_error(format!("Failed to initialize directory selector: {}", e));
                                        return Ok(true);
                                    }
                                }
                                
                                // Handle Enter based on focus - extract what we need first
                                let path_to_add: Option<PathBuf> = {
                                    if let Some(ref mut selector) = flow.directory_selector_mut() {
                                        match selector.focus() {
                                            DirFocus::Browser => {
                                                // Enter key in browser: add the highlighted directory to source folders
                                                // For "..", navigate up instead
                                                if let Some(selected_path) = selector.get_browser_selection() {
                                                    let current_path = selector.current_path().to_path_buf();
                                                    
                                                    // Check if this is ".." (parent)
                                                    if let Some(parent) = current_path.parent() {
                                                        if selected_path == parent {
                                                            // This is ".." - navigate up
                                                            let _ = selector.browser_enter();
                                                            return Ok(true);
                                                        }
                                                    }
                                                    
                                                    // This is a directory - add it to source folders
                                                    Some(selected_path)
                                                } else {
                                                    None
                                                }
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
                                flow.next_step();
                                // Start the actual disc creation process
                                // Extract what we need and release the borrow
                                let disc_id = flow.disc_id().to_string();
                                let notes = flow.notes().to_string();
                                let source_folders = flow.source_folders().to_vec();
                                // Release flow borrow (explicitly don't drop the reference)
                                let _ = flow;
                                // Temporarily extract state, work on it, then put it back
                                let app_state = std::mem::replace(&mut self.state, AppState::Quit);
                                if let AppState::NewDisc(mut f) = app_state {
                                    match self.start_disc_creation_internal(&mut f, &disc_id, &notes, &source_folders) {
                                        Ok(()) => {}
                                        Err(e) => {
                                            f.set_error(format!("Error: {}", e));
                                        }
                                    }
                                    self.state = AppState::NewDisc(f);
                                } else {
                                    self.state = app_state;
                                }
                                return Ok(true);
                            }
                            tui::new_disc::NewDiscStep::Processing => {
                                // If complete, go back to menu
                                if matches!(flow.processing_state(), tui::new_disc::ProcessingState::Complete) {
                                    self.state = AppState::MainMenu;
                                } else if matches!(flow.processing_state(), tui::new_disc::ProcessingState::Error(_)) {
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
                                        if let Some(selected_path) = selector.get_browser_selection() {
                                            let current_path = selector.current_path().to_path_buf();
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
                    KeyCode::Backspace => {
                        match flow.current_step() {
                            tui::new_disc::NewDiscStep::EnterDiscId | 
                            tui::new_disc::NewDiscStep::EnterNotes => {
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
                        }
                    }
                    KeyCode::Char(c) => {
                        match flow.current_step() {
                            tui::new_disc::NewDiscStep::EnterDiscId | 
                            tui::new_disc::NewDiscStep::EnterNotes => {
                                let mut buffer = flow.input_buffer().to_string();
                                buffer.push(c);
                                flow.set_input_buffer(buffer);
                            }
                            tui::new_disc::NewDiscStep::SelectFolders => {
                                // Initialize selector if needed
                                if flow.directory_selector_mut().is_none() {
                                    if let Err(e) = flow.init_directory_selector() {
                                        flow.set_error(format!("Failed to initialize directory selector: {}", e));
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
                        if matches!(verify.verification_state(), tui::verify_ui::VerificationState::Complete) ||
                           matches!(verify.verification_state(), tui::verify_ui::VerificationState::Error(_)) {
                            self.state = AppState::MainMenu;
                        } else if matches!(verify.verification_state(), tui::verify_ui::VerificationState::Mounting |
                            tui::verify_ui::VerificationState::Verifying | tui::verify_ui::VerificationState::Recording) {
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
                                    let app_state = std::mem::replace(&mut self.state, AppState::Quit);
                                    if let AppState::Verify(mut v) = app_state {
                                        match self.start_verification_internal(&mut v, &device, &mountpoint) {
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
                                    if verify.input_mode() == tui::verify_ui::VerifyInputMode::Ready {
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
                        if matches!(verify.verification_state(), tui::verify_ui::VerificationState::Idle) {
                            verify.commit_input();
                            verify.next_input_mode();
                        }
                    }
                    KeyCode::Backspace => {
                        if matches!(verify.verification_state(), tui::verify_ui::VerificationState::Idle) {
                            let mut buffer = verify.input_buffer().to_string();
                            buffer.pop();
                            verify.set_input_buffer(buffer);
                        }
                    }
                    KeyCode::Char(c) => {
                        if matches!(verify.verification_state(), tui::verify_ui::VerificationState::Idle) {
                            let mut buffer = verify.input_buffer().to_string();
                            buffer.push(c);
                            verify.set_input_buffer(buffer);
                        }
                    }
                    _ => {}
                }
            }
            AppState::ListDiscs(ref mut list) => {
                match key {
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
                }
            }
            AppState::Settings(_) => {
                match key {
                    KeyCode::Esc => {
                        self.state = AppState::MainMenu;
                    }
                    _ => {}
                }
            }
            AppState::Logs(_) => {
                match key {
                    KeyCode::Esc => {
                        self.state = AppState::MainMenu;
                    }
                    _ => {}
                }
            }
            AppState::Quit => {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn start_verification_internal(&mut self, verify: &mut tui::VerifyUI, device_str: &str, mountpoint_str: &str) -> Result<()> {
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
                verify.set_status(format!("Mounting {} to {}...", device, mountpoint.display()));
                bdarchive::verify::mount_device(&device, &mountpoint, dry_run)?;
            } else {
                verify.set_status(format!("Please mount {} at {}", device, mountpoint.display()));
                // Wait for user to mount manually
                // For now, check if it's mounted
                if !mountpoint.join("SHA256SUMS.txt").exists() {
                    verify.set_error(format!("Disc not mounted. Please mount {} at {}", device, mountpoint.display()));
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
            verify.set_status(format!("Verification successful! {} files checked.", result.files_checked));
        } else {
            verify.set_status(format!("Verification failed! {} files failed out of {} checked.", 
                result.files_failed, result.files_checked));
        }

        // Step 3: Record in database
        verify.set_verification_state(tui::verify_ui::VerificationState::Recording);
        verify.set_status("Recording verification results...".to_string());

        // Try to find disc_id from the disc
        // For now, we'll use a placeholder or try to read from DISC_INFO.txt
        let disc_id = if let Ok(disc_info) = std::fs::read_to_string(mountpoint.join("DISC_INFO.txt")) {
            // Parse disc ID from DISC_INFO.txt
            disc_info.lines()
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

    fn start_disc_creation_internal(&mut self, flow: &mut tui::NewDiscFlow, disc_id: &str, notes: &str, source_folders: &[PathBuf]) -> Result<()> {
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
                flow.set_error(format!("Source folder does not exist: {}", folder.display()));
                return Ok(());
            }
        }

        let dry_run = false; // Could be configurable

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
        let use_rsync = self.config.optional_tools.use_rsync && 
            dependencies::get_optional_command("rsync").is_some();
        
        staging::stage_files(&disc_root, source_folders, use_rsync, dry_run)?;

        // Step 2: Generate manifest and SHA256SUMS
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
        flow.set_processing_state(tui::new_disc::ProcessingState::CreatingISO);
        flow.set_status("Creating ISO image...".to_string());

        let volume_label = disc::generate_volume_label(disc_id);
        let iso_path = staging_dir.join(format!("{}.iso", disc_id));
        
        iso::create_iso(&disc_root, &iso_path, &volume_label, dry_run)?;
        let iso_size = iso::get_iso_size(&iso_path)?;

        flow.set_status(format!("ISO created: {:.2} GB", iso_size as f64 / 1_000_000_000.0));

        // Step 4: Burn to disc
        flow.set_processing_state(tui::new_disc::ProcessingState::Burning);
        flow.set_status(format!("Burning to {}...", self.config.device));

        if !dry_run {
            burn::burn_iso(&iso_path, &self.config.device, dry_run)?;
        }

        flow.set_status("Disc burned successfully".to_string());

        // Step 5: Index in database
        flow.set_processing_state(tui::new_disc::ProcessingState::Indexing);
        flow.set_status("Updating index...".to_string());

        let created_at = format_timestamp_now();
        
        let disc_record = database::Disc {
            disc_id: disc_id.to_string(),
            volume_label: volume_label.clone(),
            created_at: created_at.clone(),
            notes: if notes.is_empty() { None } else { Some(notes.to_string()) },
            iso_size: Some(iso_size),
            burn_device: Some(self.config.device.clone()),
            checksum_manifest_hash: None, // Could calculate hash of manifest
            qr_path: None, // Will be set after QR generation
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
            match qrcode::generate_qrcode(
                disc_id,
                &qrcodes_dir,
                qrcode::QrCodeFormat::PNG,
                dry_run,
            ) {
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

    fn render(&mut self, frame: &mut Frame) {
        // Set background color for entire frame
        let bg_rect = frame.size();
        let bg_block = ratatui::widgets::Block::default()
            .style(ratatui::style::Style::default().bg(self.theme.bg()));
        frame.render_widget(bg_block, bg_rect);
        use crate::ui::{GridLayout, header_footer};
        
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
    logging::init_logging()
        .context("Failed to initialize logging")?;

    info!("Starting BlueVault application");

    // Check dependencies
    dependencies::verify_dependencies()
        .context("Missing required dependencies")?;

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

        // Use non-blocking event reading with timeout for splash auto-dismiss
        let timeout = if matches!(app.state, AppState::Splash(_)) {
            Some(std::time::Duration::from_millis(100))
        } else {
            None
        };

        if poll(timeout.unwrap_or(std::time::Duration::from_secs(0)))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    running = app.handle_key(key.code)?;
                }
            }
        } else if timeout.is_none() {
            // Blocking wait if no timeout
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    running = app.handle_key(key.code)?;
                }
            }
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

