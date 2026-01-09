use anyhow::{Context, Result};
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};

/// Resolve the XDG data directory for the application.
/// Defaults to ~/.local/share/bdarchive if XDG_DATA_HOME is not set.
pub fn data_dir() -> Result<PathBuf> {
    dirs::data_dir()
        .map(|d| d.join("bdarchive"))
        .or_else(|| dirs::home_dir().map(|h| h.join(".local/share/bdarchive")))
        .context("Could not determine data directory")
}

/// Resolve the XDG config directory for the application.
/// Defaults to ~/.config/bdarchive if XDG_CONFIG_HOME is not set.
pub fn config_dir() -> Result<PathBuf> {
    dirs::config_dir()
        .map(|d| d.join("bdarchive"))
        .or_else(|| dirs::home_dir().map(|h| h.join(".config/bdarchive")))
        .context("Could not determine config directory")
}

/// Get the default database path.
pub fn default_database_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("archive.db"))
}

/// Get the default logs directory.
pub fn logs_dir() -> Result<PathBuf> {
    Ok(data_dir()?.join("logs"))
}

/// Get the default QR codes directory.
pub fn qrcodes_dir() -> Result<PathBuf> {
    Ok(data_dir()?.join("qrcodes"))
}

/// Ensure a directory exists, creating it if necessary.
pub fn ensure_dir(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path)
        .with_context(|| format!("Failed to create directory: {}", path.display()))
}

/// Ensure the data directory exists.
pub fn ensure_data_dir() -> Result<PathBuf> {
    let dir = data_dir()?;
    ensure_dir(&dir)?;
    ensure_dir(&logs_dir()?)?;
    ensure_dir(&qrcodes_dir()?)?;
    Ok(dir)
}

/// Ensure the config directory exists.
pub fn ensure_config_dir() -> Result<PathBuf> {
    let dir = config_dir()?;
    ensure_dir(&dir)?;
    Ok(dir)
}

/// Normalize a path by canonicalizing it and resolving symlinks.
pub fn normalize_path(path: &Path) -> Result<PathBuf> {
    path.canonicalize()
        .with_context(|| format!("Failed to canonicalize path: {}", path.display()))
}

/// Validate that a path exists and is a directory.
pub fn validate_dir(path: &Path) -> Result<()> {
    if !path.exists() {
        anyhow::bail!("Path does not exist: {}", path.display());
    }
    if !path.is_dir() {
        anyhow::bail!("Path is not a directory: {}", path.display());
    }
    Ok(())
}

/// Validate that a path exists and is a file.
pub fn validate_file(path: &Path) -> Result<()> {
    if !path.exists() {
        anyhow::bail!("Path does not exist: {}", path.display());
    }
    if !path.is_file() {
        anyhow::bail!("Path is not a file: {}", path.display());
    }
    Ok(())
}

/// Auto-detect the primary optical drive on Linux systems.
/// Returns the first available optical drive device, preferring Blu-ray capable drives.
pub fn detect_optical_drive() -> Option<String> {
    // Method 1: Check /proc/sys/dev/cdrom/info for registered drives
    if let Ok(content) = std::fs::read_to_string("/proc/sys/dev/cdrom/info") {
        if let Some(drive_line) = content.lines().find(|line| line.starts_with("drive name:")) {
            let drives: Vec<&str> = drive_line.split_whitespace().skip(2).collect();
            if let Some(first_drive) = drives.first() {
                let device_path = format!("/dev/{}", first_drive);
                if validate_device_quiet(&PathBuf::from(&device_path)).is_ok() {
                    return Some(device_path);
                }
            }
        }
    }

    // Method 2: Scan /dev/sr* devices (most common on modern Linux)
    for i in 0..10 {  // Check sr0 through sr9
        let device_path = format!("/dev/sr{}", i);
        let path = PathBuf::from(&device_path);
        if validate_device_quiet(&path).is_ok() {
            return Some(device_path);
        }
    }

    // Method 3: Check common optical drive device names
    let common_devices = [
        "/dev/cdrom",
        "/dev/dvd",
        "/dev/bluray",
        "/dev/sr",
        "/dev/scd0",
        "/dev/hdc",  // Older IDE drives
        "/dev/hdd",
    ];

    for device in &common_devices {
        let path = PathBuf::from(device);
        if validate_device_quiet(&path).is_ok() {
            return Some(device.to_string());
        }
    }

    None
}

/// Quiet device validation - doesn't return detailed error messages.
fn validate_device_quiet(path: &Path) -> Result<()> {
    if !path.exists() {
        return Err(anyhow::anyhow!("Not found"));
    }

    // Check if it's a block device
    let metadata = std::fs::metadata(path).map_err(|_| anyhow::anyhow!("No access"))?;

    // Must be a block device or character device (some optical drives appear as char devices)
    if !metadata.file_type().is_block_device() && !metadata.file_type().is_char_device() {
        return Err(anyhow::anyhow!("Not a device"));
    }

    // On Linux, we can check if the path starts with /dev/
    if !path.starts_with("/dev/") {
        return Err(anyhow::anyhow!("Not in /dev"));
    }

    // Try to open the device for reading to check permissions
    std::fs::File::open(path).map_err(|_| anyhow::anyhow!("Permission denied"))?;

    Ok(())
}

/// Validate that a device path exists and is accessible.
/// For dry runs, we allow devices without discs.
/// Provides detailed error messages for user guidance.
pub fn validate_device(path: &Path) -> Result<()> {
    if !path.exists() {
        // Suggest auto-detection if the default device doesn't exist
        let suggestion = if path.to_string_lossy() == "/dev/sr0" {
            if let Some(auto_detected) = detect_optical_drive() {
                format!("\n\n💡 Suggestion: Use auto-detected drive: {}", auto_detected)
            } else {
                "\n\n💡 No optical drives detected on this system.".to_string()
            }
        } else {
            String::new()
        };
        anyhow::bail!("Device does not exist: {}{}", path.display(), suggestion);
    }

    // Check if it's a block device
    let metadata = std::fs::metadata(path)
        .with_context(|| format!("Failed to read device metadata: {}", path.display()))?;

    // Must be a block device or character device (some optical drives appear as char devices)
    if !metadata.file_type().is_block_device() && !metadata.file_type().is_char_device() {
        anyhow::bail!("Path is not a device: {}", path.display());
    }

    // On Linux, we can check if the path starts with /dev/
    if !path.starts_with("/dev/") {
        anyhow::bail!("Device path must be under /dev/: {}", path.display());
    }

    // Try to open the device for reading to check permissions
    // Note: This will fail if there's no disc in the drive, which is OK for dry runs
    match std::fs::File::open(path) {
        Ok(_) => Ok(()), // Device accessible
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            // Permission denied - suggest adding to cdrom group
            Err(anyhow::anyhow!(
                "Cannot access device. Make sure you have permission to access optical drives (try: sudo usermod -a -G cdrom $USER): {}",
                path.display()
            ).into())
        }
        Err(e) if e.raw_os_error() == Some(123) => {
            // ENOMEDIUM - no disc in drive, but device exists
            // This is OK for dry runs, but we'll warn during actual burning
            Ok(())
        }
        Err(e) => {
            // Other error
            Err(anyhow::anyhow!("Cannot access device {}: {}", path.display(), e).into())
        }
    }
}

/// Expand user home directory in path (e.g., ~/path -> /home/user/path).
pub fn expand_tilde(path: &str) -> PathBuf {
    if path.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(&path[2..]);
        }
    }
    PathBuf::from(path)
}

/// Make a path relative to a base directory.
pub fn make_relative(path: &Path, base: &Path) -> Result<PathBuf> {
    path.strip_prefix(base)
        .map(|p| p.to_path_buf())
        .with_context(|| {
            format!(
                "Path {} is not under base {}",
                path.display(),
                base.display()
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_expand_tilde() {
        let expanded = expand_tilde("~/test");
        assert!(expanded.to_string_lossy().contains("test"));
        assert!(!expanded.to_string_lossy().starts_with("~"));
    }

    #[test]
    fn test_make_relative() {
        let base = PathBuf::from("/base");
        let path = PathBuf::from("/base/sub/file.txt");
        let rel = make_relative(&path, &base).unwrap();
        assert_eq!(rel, PathBuf::from("sub/file.txt"));
    }

    #[test]
    fn test_make_relative_fails_outside_base() {
        let base = PathBuf::from("/base");
        let path = PathBuf::from("/other/file.txt");
        assert!(make_relative(&path, &base).is_err());
    }

    #[test]
    fn test_detect_optical_drive() {
        // This test will pass on systems with optical drives
        // On systems without optical drives, it should return None
        let result = detect_optical_drive();
        // We can't assert much here since it depends on the system
        // But we can ensure it doesn't panic
        let _ = result; // Just to use the variable
    }

    #[test]
    fn test_validate_device_quiet() {
        // Test with a known non-device path
        let result = validate_device_quiet(&PathBuf::from("/tmp"));
        assert!(result.is_err());

        // Test with /dev/null (should work if accessible)
        let result = validate_device_quiet(&PathBuf::from("/dev/null"));
        // This might pass or fail depending on permissions, but shouldn't panic
        let _ = result;
    }
}
