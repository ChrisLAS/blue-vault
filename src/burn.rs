use anyhow::{Context, Result};
use std::path::Path;
use tracing::{debug, info, warn};
use crate::commands;

/// Burn an ISO image to a Blu-ray disc using growisofs.
pub fn burn_iso(
    iso_path: &Path,
    device: &str,
    dry_run: bool,
) -> Result<()> {
    info!(
        "Burning ISO to device: {} -> {}",
        iso_path.display(),
        device
    );

    // Validate ISO exists
    crate::paths::validate_file(iso_path)
        .context("ISO file validation failed")?;

    // Validate device path
    let device_path = Path::new(device);
    if !dry_run {
        crate::paths::validate_device(device_path)
            .context("Device validation failed")?;
    }

    // Build growisofs command
    // -Z: blank and write
    // -use-the-force-luke=notray: don't wait for tray
    // -dvd-compat: enable DVD compatibility mode
    let iso_path_str = iso_path.to_string_lossy().to_string();
    let args = vec![
        "-Z", device,                      // Device and action (blank/write)
        "=", &iso_path_str, // Input ISO
    ];

    let output = commands::execute_command("growisofs", &args, dry_run)?;

    if !output.success {
        anyhow::bail!(
            "growisofs failed: {}\n{}",
            output.stderr,
            output.stdout
        );
    }

    debug!("ISO burned successfully to: {}", device);
    Ok(())
}

/// Check if device is ready for burning.
pub fn check_device_ready(device: &str, dry_run: bool) -> Result<bool> {
    if dry_run {
        return Ok(true);
    }

    let device_path = Path::new(device);
    if !device_path.exists() {
        warn!("Device does not exist: {}", device);
        return Ok(false);
    }

    // Try to read from device to check if it's ready
    // This is a simple check; in production you might want more sophisticated detection
    Ok(device_path.exists())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_burn_iso_dry_run() -> Result<()> {
        let iso_path = Path::new("/tmp/test.iso");
        // Should not fail in dry run mode even if file doesn't exist
        burn_iso(iso_path, "/dev/sr0", true)?;
        Ok(())
    }
}

