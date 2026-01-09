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
