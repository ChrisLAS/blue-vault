use anyhow::{Context, Result};
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

/// Validate that a device path exists and is accessible.
pub fn validate_device(path: &Path) -> Result<()> {
    if !path.exists() {
        anyhow::bail!("Device does not exist: {}", path.display());
    }

    // Check if it's a block device
    let _metadata = std::fs::metadata(path)
        .with_context(|| format!("Failed to read device metadata: {}", path.display()))?;

    // On Linux, we can check if the path starts with /dev/
    if !path.starts_with("/dev/") {
        anyhow::bail!("Device path must be under /dev/: {}", path.display());
    }

    // Try to open the device for reading to check permissions
    // This will fail if the user doesn't have permission
    std::fs::File::open(path)
        .with_context(|| format!("Cannot access device. Make sure you have permission to access optical drives (try: sudo usermod -a -G cdrom $USER): {}", path.display()))?;

    Ok(())
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
}
