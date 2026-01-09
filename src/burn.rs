use crate::commands;
use anyhow::{Context, Result};
use std::path::Path;
use tracing::{error, info, warn};

/// Burn an ISO image or directory to a Blu-ray disc using xorriso.
pub fn burn_iso(iso_path: &Path, device: &str, dry_run: bool) -> Result<()> {
    burn_with_method(iso_path, device, dry_run, "iso")
}

/// Burn using specified method: "iso" (burn ISO file) or "direct" (burn directory)
pub fn burn_with_method(source_path: &Path, device: &str, dry_run: bool, method: &str) -> Result<()> {
    match method {
        "iso" => {
            info!("Burning ISO to device: {} -> {} (dry_run: {})", source_path.display(), device, dry_run);
            // Validate ISO exists
            info!("Validating ISO file: {}", source_path.display());
            crate::paths::validate_file(source_path).context("ISO file validation failed")?;
            info!("ISO validation passed");
        }
        "direct" => {
            info!("Burning directory directly to device: {} -> {} (dry_run: {})", source_path.display(), device, dry_run);
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
            anyhow::bail!("Unknown burn method: {}. Supported: 'iso', 'direct'", method);
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

    let args = match method {
        "iso" => {
            // Burn existing ISO file
            vec!["-as", "cdrecord", "-v", &dev_arg, "-data", &source_path_str]
        }
        "direct" => {
            // Burn directory contents directly (no intermediate ISO)
            vec!["-as", "cdrecord", "-v", &dev_arg, &source_path_str]
        }
        _ => unreachable!(),
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
        let error_msg: String = if output.stderr.contains("Closed media with data detected") ||
                        output.stderr.contains("Disc status unsuitable for writing") {
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

    info!("ISO burned successfully to: {}", device);
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
                error!("❌ Drive does not support Blu-ray burning");
                eprintln!();
                eprintln!("🚫 DRIVE DOES NOT SUPPORT BLU-RAY BURNING");
                eprintln!();
                eprintln!("Your drive reports these profiles: {}", profiles);
                eprintln!("No BD-R profiles found, so this drive cannot burn Blu-ray discs.");
                eprintln!();
                eprintln!("SOLUTION:");
                eprintln!("• Use a Blu-ray burner that supports BD-R recording");
                eprintln!("• Check your drive model specifications");
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

            if stderr.contains("media current: bd-rom") {
                // BD-ROM = Blu-ray Read-Only Memory (pre-recorded, like movie discs)
                error!("❌ BD-ROM (read-only) disc detected - cannot burn to this media");
                eprintln!();
                eprintln!("🚫 CANNOT BURN TO BD-ROM DISC");
                eprintln!();
                eprintln!("The drive contains a BD-ROM disc (pre-recorded media).");
                eprintln!("BD-ROM discs are read-only and CANNOT be written to.");
                eprintln!();
                eprintln!("SOLUTION:");
                eprintln!("1. Eject the current BD-ROM disc");
                eprintln!("2. Insert a BLANK BD-R (writable) disc");
                eprintln!("3. Try again");
                eprintln!();
                eprintln!("BD-ROM discs look like silver movie discs.");
                eprintln!("BD-R discs are usually blue/purple and say 'BD-R' on them.");
                eprintln!("BD-R DL discs say 'BD-R DL' (dual layer, higher capacity).");
                anyhow::bail!("BD-ROM disc detected - cannot burn to read-only media");
            } else if stderr.contains("media current: bd-r") {
                if stderr.contains("media status : blank") {
                    info!("✅ Blank BD-R disc detected - ready for burning");
                    return Ok(());
                } else if stderr.contains("media status : is written") {
                    warn!("⚠️  BD-R disc contains data - needs to be blank");
                    eprintln!("WARNING: The BD-R disc in {} already contains data.", device);
                    eprintln!("Please use a completely blank BD-R disc.");
                    eprintln!("If this is a rewritable BD-RE disc, it needs to be erased first.");
                }
            } else if stderr.contains("no readable medium found") || stderr.contains("no medium present") || stderr.contains("is not present") {
                error!("❌ No disc detected in drive");
                eprintln!();
                eprintln!("💿 NO DISC DETECTED");
                eprintln!();
                eprintln!("No disc was found in drive {}.", device);
                eprintln!();
                eprintln!("SOLUTION:");
                eprintln!("1. Insert a blank BD-R or BD-R DL disc");
                eprintln!("2. Make sure it's seated properly");
                eprintln!("3. Try again");
                eprintln!();
                eprintln!("BD-R discs are usually blue/purple.");
                eprintln!("BD-R DL discs say 'BD-R DL' (dual layer, ~50GB capacity).");
                anyhow::bail!("No disc detected in drive");
            } else {
                warn!("⚠️  Unknown or no media detected - drive may need a disc");
                eprintln!("WARNING: Could not identify media in drive {}.", device);
                eprintln!("Please ensure you have inserted a blank BD-R disc.");
                info!("Media detection output: {}", stderr);
            }
        }
        Err(e) => {
            warn!("Could not query media type ({}), proceeding anyway", e);
            eprintln!("WARNING: Could not check media type. Make sure you have a blank BD-R disc inserted.");
        }
    }

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
