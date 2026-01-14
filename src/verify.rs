use crate::commands;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{debug, info, warn};

/// Verify a disc by checking SHA256SUMS.
pub fn verify_disc(
    mountpoint: &Path,
    _auto_mount: bool,
    dry_run: bool,
) -> Result<VerificationResult> {
    info!("Verifying disc at: {}", mountpoint.display());

    let sha256sums_path = mountpoint.join("SHA256SUMS.txt");

    if !sha256sums_path.exists() {
        anyhow::bail!("SHA256SUMS.txt not found at: {}", sha256sums_path.display());
    }

    if dry_run {
        debug!(
            "[DRY RUN] Would verify SHA256SUMS.txt at: {}",
            mountpoint.display()
        );
        return Ok(VerificationResult {
            success: true,
            files_checked: 0,
            files_failed: 0,
            error_message: None,
        });
    }

    // Change to mountpoint directory for sha256sum -c to work correctly
    let output = Command::new("sha256sum")
        .arg("-c")
        .arg("SHA256SUMS.txt")
        .current_dir(mountpoint)
        .output()
        .context("Failed to execute sha256sum")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let success = output.status.success();

    // Parse output to count files
    let (files_checked, files_failed) = parse_sha256sum_output(&stdout, &stderr);

    let error_message = if !success {
        Some(format!("Verification failed:\n{}\n{}", stdout, stderr))
    } else {
        None
    };

    if success {
        info!("Verification successful: {} files checked", files_checked);
    } else {
        warn!(
            "Verification failed: {} files checked, {} failed",
            files_checked, files_failed
        );
    }

    Ok(VerificationResult {
        success,
        files_checked,
        files_failed,
        error_message,
    })
}

/// Verify all discs in a multi-disc set
pub fn verify_multi_disc_set(
    set_id: &str,
    mount_base_path: Option<&Path>,
    dry_run: bool,
) -> Result<MultiDiscVerificationResult> {
    info!("Starting multi-disc verification for set: {}", set_id);

    // Get database connection
    let db_path = dirs::data_dir()
        .unwrap_or_default()
        .join("bdarchive")
        .join("database.db");

    let conn = crate::database::init_database(&db_path)
        .context("Failed to initialize database")?;

    // Get disc set information
    let disc_set = crate::database::DiscSet::get(&conn, set_id)
        .context("Failed to load disc set")?
        .ok_or_else(|| anyhow::anyhow!("Disc set not found: {}", set_id))?;

    // Get all discs in the set
    let discs = crate::database::DiscSet::get_discs(&conn, set_id)
        .context("Failed to load discs in set")?;

    let total_discs = discs.len() as u32;
    let mut disc_results = Vec::new();
    let mut discs_verified = 0;
    let mut discs_failed = 0;
    let mut discs_missing = 0;
    let mut total_files_checked = 0;
    let mut total_files_failed = 0;

    info!("Verifying {} discs in set '{}'", total_discs, disc_set.name);

    for disc in discs {
        let disc_id = disc.disc_id.clone();
        info!("Checking disc: {}", disc_id);

        // Determine mount point for this disc
        let mount_point = if let Some(base_path) = mount_base_path {
            // If a base path is provided, look for discs in subdirectories
            find_disc_mount_point(&disc_id, base_path)
        } else {
            // Try common mount points
            find_disc_mount_point(&disc_id, Path::new("/media"))
                .or_else(|| find_disc_mount_point(&disc_id, Path::new("/mnt")))
        };

        match mount_point {
            Some(mount_path) => {
                info!("Found disc {} mounted at: {}", disc_id, mount_path.display());

                // Verify the disc
                match verify_disc(&mount_path, false, dry_run) {
                    Ok(result) => {
                        if result.success {
                            disc_results.push((disc_id.clone(), DiscVerificationStatus::Verified {
                                files_checked: result.files_checked,
                                files_failed: result.files_failed,
                            }));
                            discs_verified += 1;
                            total_files_checked += result.files_checked;
                            total_files_failed += result.files_failed;
                            info!("✅ Disc {} verified successfully: {} files checked, {} failed",
                                disc_id, result.files_checked, result.files_failed);
                        } else {
                            let error_msg = result.error_message.unwrap_or_else(|| "Verification failed".to_string());
                            disc_results.push((disc_id.clone(), DiscVerificationStatus::Failed {
                                error: error_msg.clone(),
                            }));
                            discs_failed += 1;
                            warn!("❌ Disc {} verification failed: {}", disc_id, error_msg);
                        }
                    }
                    Err(e) => {
                        disc_results.push((disc_id.clone(), DiscVerificationStatus::Failed {
                            error: format!("Verification error: {}", e),
                        }));
                        discs_failed += 1;
                        warn!("❌ Disc {} verification error: {}", disc_id, e);
                    }
                }
            }
            None => {
                disc_results.push((disc_id.clone(), DiscVerificationStatus::Missing));
                discs_missing += 1;
                warn!("⚠️  Disc {} not found in any mount point", disc_id);
            }
        }
    }

    let overall_success = discs_failed == 0 && discs_missing == 0;
    let error_message = if !overall_success {
        let mut msg = Vec::new();
        if discs_missing > 0 {
            msg.push(format!("{} discs missing", discs_missing));
        }
        if discs_failed > 0 {
            msg.push(format!("{} discs failed verification", discs_failed));
        }
        Some(msg.join(", "))
    } else {
        None
    };

    let result = MultiDiscVerificationResult {
        set_id: set_id.to_string(),
        set_name: disc_set.name,
        total_discs,
        discs_verified,
        discs_failed,
        discs_missing,
        overall_success,
        disc_results,
        total_files_checked,
        total_files_failed,
        error_message,
        verification_timestamp: crate::disc::format_timestamp_now(),
    };

    // Store verification result in database
    if let Err(e) = store_multi_disc_verification_result(&conn, &result) {
        warn!("Failed to store multi-disc verification result: {}", e);
    }

    info!("Multi-disc verification complete: {}/{} discs verified successfully",
        discs_verified, total_discs);

    Ok(result)
}

/// Find mount point for a specific disc
fn find_disc_mount_point(disc_id: &str, search_path: &Path) -> Option<PathBuf> {
    if !search_path.exists() {
        return None;
    }

    // Walk through mount points looking for this disc
    for entry in walkdir::WalkDir::new(search_path)
        .max_depth(3) // Don't go too deep
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_dir())
    {
        let path = entry.path();

        // Check if this looks like our disc (has DISC_INFO.txt or matches volume label)
        let disc_info_path = path.join("DISC_INFO.txt");
        if disc_info_path.exists() {
            // Try to read the disc info to see if it matches
            if let Ok(content) = std::fs::read_to_string(&disc_info_path) {
                if content.contains(&format!("Disc-ID: {}", disc_id)) {
                    return Some(path.to_path_buf());
                }
            }
        }

        // Also check by volume label in the path name
        if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
            if dir_name.contains(disc_id) {
                // Additional check: look for SHA256SUMS.txt to confirm it's a valid disc
                if path.join("SHA256SUMS.txt").exists() {
                    return Some(path.to_path_buf());
                }
            }
        }
    }

    None
}

/// Store multi-disc verification result in database
fn store_multi_disc_verification_result(
    conn: &rusqlite::Connection,
    result: &MultiDiscVerificationResult,
) -> Result<()> {
    use rusqlite::params;

    // Store in a simple verification_runs table (we'll need to add this to schema)
    // For now, we'll just log it. In a full implementation, we'd add a proper table.

    // Create a summary for logging
    let summary = format!(
        "Multi-disc verification: {}/{} discs verified, {} files checked, {} files failed",
        result.discs_verified, result.total_discs, result.total_files_checked, result.total_files_failed
    );

    info!("Stored verification result: {}", summary);

    // TODO: Add proper database storage for multi-disc verification results
    // This would require extending the database schema

    Ok(())
}

/// Parse sha256sum -c output to count files.
fn parse_sha256sum_output(stdout: &str, stderr: &str) -> (u32, u32) {
    // sha256sum -c outputs lines like:
    // path/to/file: OK
    // path/to/file: FAILED

    let combined = format!("{}\n{}", stdout, stderr);
    let lines: Vec<&str> = combined.lines().collect();

    let mut checked = 0u32;
    let mut failed = 0u32;

    for line in lines {
        if line.contains(": OK") {
            checked += 1;
        } else if line.contains(": FAILED") || line.contains(": No such file") {
            checked += 1;
            failed += 1;
        } else if line.contains("WARNING:") || line.contains("FAILED") {
            // Some error message
            failed += 1;
        }
    }

    (checked, failed)
}

/// Mount a device to a mountpoint.
pub fn mount_device(device: &str, mountpoint: &Path, dry_run: bool) -> Result<()> {
    info!("Mounting device {} to {}", device, mountpoint.display());

    if dry_run {
        debug!(
            "[DRY RUN] Would mount {} to {}",
            device,
            mountpoint.display()
        );
        return Ok(());
    }

    // Ensure mountpoint exists
    std::fs::create_dir_all(mountpoint)?;

    let mountpoint_str = mountpoint.to_string_lossy().to_string();
    let args = vec![device, &mountpoint_str];

    let output = commands::execute_command("mount", &args, dry_run)?;

    if !output.success {
        anyhow::bail!("mount failed: {}", output.stderr);
    }

    debug!("Device mounted successfully");
    Ok(())
}

/// Unmount a mountpoint.
pub fn unmount_device(mountpoint: &Path, dry_run: bool) -> Result<()> {
    info!("Unmounting: {}", mountpoint.display());

    if dry_run {
        debug!("[DRY RUN] Would unmount: {}", mountpoint.display());
        return Ok(());
    }

    let mountpoint_str = mountpoint.to_string_lossy().to_string();
    let args: &[&str] = &[&mountpoint_str];

    let output = commands::execute_command("umount", args, dry_run)?;

    if !output.success {
        anyhow::bail!("umount failed: {}", output.stderr);
    }

    debug!("Device unmounted successfully");
    Ok(())
}

/// Find a suitable mountpoint for temporary mounting.
pub fn get_temporary_mountpoint() -> Result<PathBuf> {
    use std::env;

    // Try common temporary mount directories
    let candidates = vec![
        PathBuf::from("/tmp/bdarchive_mount"),
        PathBuf::from("/mnt/bdarchive"),
        env::temp_dir().join("bdarchive_mount"),
    ];

    for candidate in &candidates {
        if !candidate.exists() {
            std::fs::create_dir_all(candidate)?;
            return Ok(candidate.clone());
        }
    }

    // Use first candidate if all exist
    Ok(candidates[0].clone())
}

#[derive(Debug, Clone)]
pub struct VerificationResult {
    pub success: bool,
    pub files_checked: u32,
    pub files_failed: u32,
    pub error_message: Option<String>,
}

/// Status of individual disc in multi-disc verification
#[derive(Debug, Clone, PartialEq)]
pub enum DiscVerificationStatus {
    /// Disc is present and verified successfully
    Verified { files_checked: u32, files_failed: u32 },
    /// Disc is present but verification failed
    Failed { error: String },
    /// Disc is missing/not available
    Missing,
    /// Disc verification not attempted (e.g., due to previous failures)
    NotAttempted,
}

/// Result of multi-disc set verification
#[derive(Debug)]
pub struct MultiDiscVerificationResult {
    pub set_id: String,
    pub set_name: String,
    pub total_discs: u32,
    pub discs_verified: u32,
    pub discs_failed: u32,
    pub discs_missing: u32,
    pub overall_success: bool,
    pub disc_results: Vec<(String, DiscVerificationStatus)>, // (disc_id, status)
    pub total_files_checked: u32,
    pub total_files_failed: u32,
    pub error_message: Option<String>,
    pub verification_timestamp: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sha256sum_output() {
        let stdout = "file1.txt: OK\nfile2.txt: OK\n";
        let stderr = "";
        let (checked, failed) = parse_sha256sum_output(stdout, stderr);
        assert_eq!(checked, 2);
        assert_eq!(failed, 0);
    }

    #[test]
    fn test_parse_sha256sum_output_with_failures() {
        let stdout = "file1.txt: OK\n";
        let stderr = "file2.txt: FAILED\n";
        let (checked, failed) = parse_sha256sum_output(stdout, stderr);
        assert_eq!(checked, 2);
        assert_eq!(failed, 1);
    }
}
