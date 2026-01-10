use crate::commands;
use anyhow::{Context, Result};
use std::path::Path;
use tracing::{error, info, warn};

/// Burn an ISO image or directory to a Blu-ray disc using xorriso.
pub fn burn_iso(iso_path: &Path, device: &str, dry_run: bool) -> Result<()> {
    burn_with_method(iso_path, device, dry_run, "iso")
}

/// Burn using specified method: "iso" (burn ISO file) or "direct" (burn directory)
pub fn burn_with_method(
    source_path: &Path,
    device: &str,
    dry_run: bool,
    method: &str,
) -> Result<()> {
    match method {
        "iso" => {
            info!(
                "Burning ISO to device: {} -> {} (dry_run: {})",
                source_path.display(),
                device,
                dry_run
            );
            // Validate ISO exists (skip in dry run mode)
            if !dry_run {
                info!("Validating ISO file: {}", source_path.display());
                crate::paths::validate_file(source_path).context("ISO file validation failed")?;
                info!("ISO validation passed");
            } else {
                info!("Skipping ISO validation (dry run)");
            }
        }
        "direct" => {
            info!(
                "Burning directory directly to device: {} -> {} (dry_run: {})",
                source_path.display(),
                device,
                dry_run
            );
            // Validate directory exists
            if !source_path.exists() {
                anyhow::bail!("Source directory does not exist: {}", source_path.display());
            }
            if !source_path.is_dir() {
                anyhow::bail!("Source path is not a directory: {}", source_path.display());
            }
            info!("Directory validation passed");
        }
        _ => {
            anyhow::bail!(
                "Unknown burn method: {}. Supported: 'iso', 'direct'",
                method
            );
        }
    }

    // Validate device path and check media type
    let device_path = Path::new(device);
    if !dry_run {
        info!("Validating device: {}", device);
        crate::paths::validate_device(device_path).context("Device validation failed")?;
        info!("Device validation passed");

        // Check what type of media is in the drive
        check_media_type(device)?;
    } else {
        info!("Skipping device validation (dry run)");
    }

    // Build xorriso command
    let source_path_str = source_path.to_string_lossy().to_string();
    let dev_arg = format!("dev={}", device);

    // For direct method, create temp ISO first
    let temp_iso_str_storage;
    let temp_iso_path = if method == "direct" {
        let temp_dir = std::env::temp_dir();
        let temp_iso = temp_dir.join("bluevault_direct.iso");
        temp_iso_str_storage = temp_iso.to_string_lossy().to_string();

        // Create the ISO
        let mkisofs_args = vec![
            "-as",
            "mkisofs",
            "-r",
            "-J",
            "-o",
            &temp_iso_str_storage,
            &source_path_str,
        ];
        info!(
            "Creating temporary ISO for direct burn: xorriso {}",
            mkisofs_args.join(" ")
        );
        let iso_output = commands::execute_command("xorriso", &mkisofs_args, dry_run)?;
        if !iso_output.success {
            anyhow::bail!(
                "Failed to create ISO for direct burn: {}",
                iso_output.stderr
            );
        }

        Some(temp_iso)
    } else {
        temp_iso_str_storage = String::new(); // Won't be used
        None
    };

    // Now build the args
    let args = if method == "iso" {
        vec!["-as", "cdrecord", "-v", &dev_arg, "-data", &source_path_str]
    } else {
        // For direct, use the temp ISO path
        vec![
            "-as",
            "cdrecord",
            "-v",
            &dev_arg,
            "-data",
            &temp_iso_str_storage,
        ]
    };

    info!(
        "About to execute xorriso command (dry_run: {}): xorriso {}",
        dry_run,
        args.join(" ")
    );
    let output = commands::execute_command("xorriso", &args, dry_run)?;
    info!(
        "xorriso command completed with exit code: {:?}",
        output.exit_code
    );

    if !output.success {
        error!("xorriso burn failed with exit code {:?}", output.exit_code);
        error!("stdout: {}", output.stdout);
        error!("stderr: {}", output.stderr);

        // Check for specific error conditions and provide better error messages
        let error_msg: String = if output.stderr.contains("Closed media with data detected")
            || output.stderr.contains("Disc status unsuitable for writing")
        {
            "❌ BLANK DISC NEEDED\n\nThe Blu-ray drive contains a disc that already has data written to it.\n\nSOLUTION:\n1. Eject the current disc from the drive\n2. Insert a blank Blu-ray disc\n3. Try again\n\nThe disc must be completely blank (not just rewritable with existing data).".to_string()
        } else if output.stderr.contains("No writable medium found") {
            "❌ NO WRITABLE DISC FOUND\n\nNo blank or rewritable Blu-ray disc was detected in the drive.\n\nSOLUTION:\n• Insert a blank Blu-ray disc (BD-R)\n• Or use a rewritable Blu-ray disc (BD-RE) that has been properly erased".to_string()
        } else if output.stderr.contains("Device or resource busy") {
            "❌ DRIVE BUSY OR LOCKED\n\nThe Blu-ray drive is currently busy or locked by another process.\n\nSOLUTION:\n• Wait a moment and try again\n• Close any other disc burning applications\n• Check if the drive is being accessed by another program".to_string()
        } else {
            // Generic error with the actual stderr
            format!("xorriso burn failed: {}\n{}", output.stderr, output.stdout)
        };

        anyhow::bail!("{}", error_msg);
    }

    info!("Burn completed successfully to: {}", device);

    // Clean up temporary ISO file if it was created for direct burning
    if let Some(temp_iso_path) = temp_iso_path {
        if !dry_run {
            if std::fs::remove_file(&temp_iso_path).is_ok() {
                info!("Cleaned up temporary ISO file: {}", temp_iso_path.display());
            } else {
                warn!(
                    "Could not remove temporary ISO file: {}",
                    temp_iso_path.display()
                );
            }
        }
    }

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

/// Check the type of media currently in the drive and warn about issues.
pub fn check_media_type(device: &str) -> Result<()> {
    info!("Checking media type in drive: {}", device);

    // First check drive capabilities
    let profile_args = vec!["-outdev", device, "-list_profiles"];
    match commands::execute_command("xorriso", &profile_args, false) {
        Ok(profile_output) => {
            let profiles = profile_output.stderr.to_lowercase();
            if !profiles.contains("bd-r") {
                error!("❌ Drive does not support Blu-ray burning - no BD-R profiles found");
                anyhow::bail!("Drive does not support Blu-ray burning");
            }
            info!("✅ Drive supports Blu-ray burning (BD-R profiles detected)");
        }
        Err(e) => {
            warn!("Could not check drive profiles: {}", e);
        }
    }

    // Now check media type
    let args = vec!["-outdev", device, "-toc"];

    match commands::execute_command("xorriso", &args, false) {
        Ok(output) => {
            let stderr = output.stderr.to_lowercase();
            info!("Media detection raw output: {}", output.stderr);

            if stderr.contains("media current: bd-rom") {
                // Check if this might actually be a writable BD-R disc misreported as BD-ROM
                // Some BD-R discs come with minimal formatting data or get misdetected

                // First, check if it appears blank despite BD-ROM reporting
                if stderr.contains("media status : blank")
                    || stderr.contains("media status : is blank")
                {
                    info!("⚠️  Drive reports BD-ROM but status is blank - proceeding as this might be a BD-R disc");
                    return Ok(());
                }

                // Check for discs with very little data (likely BD-R with minimal formatting)
                // Look for patterns like "320k data" or similar small amounts
                let has_little_data = {
                    // Extract data size from media summary line
                    let media_summary = output
                        .stderr
                        .lines()
                        .find(|line| line.to_lowercase().contains("media summary"))
                        .unwrap_or("");

                    // Look for small data amounts: k, M, or small numbers
                    media_summary.contains("k data")
                        && !media_summary.contains("M data")
                        && !media_summary.contains("G data")
                        || media_summary.contains("data blocks,  0k data")
                        || (media_summary.contains("data blocks,")
                            && media_summary.contains("k data")
                            && {
                                // Parse the data size more carefully
                                if let Some(data_part) = media_summary.split("data blocks,").nth(1)
                                {
                                    if let Some(k_pos) = data_part.find("k data") {
                                        let size_str = &data_part[..k_pos].trim();
                                        if let Ok(size_kb) = size_str.parse::<f64>() {
                                            size_kb < 1024.0 // Less than 1MB
                                        } else {
                                            false
                                        }
                                    } else {
                                        false
                                    }
                                } else {
                                    false
                                }
                            })
                };

                if has_little_data {
                    info!("⚠️  BD-ROM disc detected but contains very little data (<1MB) - likely a BD-R disc with minimal formatting");
                    warn!("BD-ROM disc with minimal data detected - proceeding as this appears to be a BD-R disc with formatting data");
                    return Ok(());
                }

                // Check for discs that claim to be full but might be BD-R
                if stderr.contains("0 free") && stderr.contains("data,") {
                    // This could be a pre-recorded BD-ROM (movie disc) OR a BD-R with data
                    // For safety, we'll be more permissive and check the data size
                    let media_summary = output
                        .stderr
                        .lines()
                        .find(|line| line.to_lowercase().contains("media summary"))
                        .unwrap_or("");

                    // If it's a movie disc, it will have significant data (GB range)
                    if media_summary.contains("G data")
                        || media_summary.contains("M data") && !has_little_data
                    {
                        error!("❌ BD-ROM disc with substantial data detected (likely movie/game disc) - cannot burn to read-only media");
                        anyhow::bail!("BD-ROM disc with substantial data detected - cannot burn to read-only media");
                    } else {
                        // Small amount of data - likely BD-R with formatting
                        info!("⚠️  BD-ROM disc with small data amount - proceeding as this might be a writable BD-R");
                        warn!(
                            "BD-ROM disc with minimal data detected - proceeding with burn attempt"
                        );
                        return Ok(());
                    }
                }

                // Unknown BD-ROM status - give benefit of doubt for BD-R
                warn!("⚠️  BD-ROM disc detected but status unclear - proceeding anyway");
                warn!("BD-ROM disc with unclear status detected - proceeding with burn attempt");
                return Ok(());
            } else if stderr.contains("media current: bd-r")
                || stderr.contains("media current: bd-re")
            {
                if stderr.contains("media status : blank") || stderr.contains("0 data,") {
                    info!("✅ Blank BD-R/BD-RE disc detected - ready for burning");
                    return Ok(());
                } else if stderr.contains("media status : is written") {
                    warn!("⚠️  BD-R/BD-RE disc contains data - needs to be blank");
                    warn!(
                        "BD-R/BD-RE disc in {} contains data - proceeding anyway",
                        device
                    );
                    // Allow proceeding anyway for rewritable discs
                    return Ok(());
                } else {
                    info!("ℹ️  BD-R/BD-RE disc detected with unknown status - proceeding");
                    return Ok(());
                }
            } else if stderr.contains("no readable medium found")
                || stderr.contains("no medium present")
                || stderr.contains("is not present")
            {
                error!("❌ No disc detected in drive {}", device);
                anyhow::bail!("No disc detected in drive");
            } else {
                warn!("⚠️  Unknown media type detected - proceeding anyway");
                warn!(
                    "Could not clearly identify media type in drive {} - proceeding anyway",
                    device
                );
                info!("Unknown media detection output: {}", stderr);
                return Ok(());
            }
        }
        Err(e) => {
            warn!("Could not query media type ({}), proceeding anyway", e);
            warn!("Could not check media type - proceeding anyway");
            return Ok(());
        }
    }

    // All paths in the Ok(output) match arm return or bail, so this is unreachable
    // but we keep it for clarity in case logic changes
    #[allow(unreachable_code)]
    Ok(())
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
