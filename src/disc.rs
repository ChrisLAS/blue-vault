use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::debug;

/// Generate a disc ID in the format YYYY-BD-###.
pub fn generate_disc_id() -> String {
    let year = get_current_year();
    let number = get_next_disc_number(&year).unwrap_or(1);
    format!("{:04}-BD-{:03}", year, number)
}

/// Get current year (simplified).
fn get_current_year() -> u32 {
    // For now, use a simple approach
    // In production, you might want to use a proper date library
    use std::time::SystemTime;
    match SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(duration) => {
            let days = duration.as_secs() / 86400;
            1970 + (days / 365) as u32
        }
        Err(_) => 2024,
    }
}

/// Get next disc number for a year (checks database if available).
/// For now, just return None to always start from 001.
fn get_next_disc_number(_year: &u32) -> Option<u32> {
    // TODO: Query database for existing discs with this year prefix
    // For now, always start from 1
    None
}

/// Generate volume label from disc ID.
pub fn generate_volume_label(disc_id: &str) -> String {
    // Convert to uppercase and replace hyphens with underscores
    disc_id.to_uppercase().replace("-", "_")
}

/// Create disc layout in staging directory.
pub fn create_disc_layout(
    staging_dir: &Path,
    disc_id: &str,
    _source_folders: &[PathBuf],
    _notes: Option<&str>,
) -> Result<PathBuf> {
    let disc_root = staging_dir.join(disc_id);
    fs::create_dir_all(&disc_root)?;

    // Create ARCHIVE directory
    let archive_dir = disc_root.join("ARCHIVE");
    fs::create_dir_all(&archive_dir)?;

    debug!("Created disc layout: {}", disc_root.display());
    Ok(disc_root)
}

/// Write DISC_INFO.txt file.
pub fn write_disc_info(
    disc_root: &Path,
    disc_id: &str,
    notes: Option<&str>,
    source_roots: &[PathBuf],
    tool_version: &str,
) -> Result<()> {
    let disc_info_path = disc_root.join("DISC_INFO.txt");

    let volume_label = generate_volume_label(disc_id);
    let created_at = format_timestamp_now();

    let mut info = String::new();
    info.push_str(&format!("Disc-ID: {}\n", disc_id));
    info.push_str(&format!("Created: {}\n", created_at));
    info.push_str(&format!("Volume Label: {}\n", volume_label));

    if let Some(notes_str) = notes {
        info.push_str(&format!("Notes: {}\n", notes_str));
    }

    info.push_str("\nSource Roots:\n");
    for root in source_roots {
        info.push_str(&format!("  {}\n", root.display()));
    }

    info.push_str(&format!("\nTool Version: {}\n", tool_version));

    fs::write(&disc_info_path, info).with_context(|| {
        format!(
            "Failed to write DISC_INFO.txt: {}",
            disc_info_path.display()
        )
    })?;

    debug!("Wrote DISC_INFO.txt: {}", disc_info_path.display());
    Ok(())
}

/// Format current timestamp as ISO 8601.
pub fn format_timestamp_now() -> String {
    use std::time::SystemTime;
    match SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(duration) => {
            let secs = duration.as_secs();
            format_timestamp_simple(secs)
        }
        Err(_) => "1970-01-01T00:00:00Z".to_string(),
    }
}

/// Simple timestamp formatting (approximate UTC).
fn format_timestamp_simple(secs: u64) -> String {
    let days = secs / 86400;
    let secs_in_day = secs % 86400;

    let year = 1970 + (days / 365);
    let day_of_year = days % 365;
    let month = 1 + (day_of_year / 30);
    let day = 1 + (day_of_year % 30);

    let hours = secs_in_day / 3600;
    let mins = (secs_in_day % 3600) / 60;
    let secs_remainder = secs_in_day % 60;

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, mins, secs_remainder
    )
}

/// Get disc version string.
pub fn get_tool_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_generate_disc_id() {
        let disc_id = generate_disc_id();
        assert!(disc_id.starts_with("202"));
        assert!(disc_id.contains("-BD-"));
    }

    #[test]
    fn test_generate_volume_label() {
        let label = generate_volume_label("2024-BD-001");
        assert_eq!(label, "2024_BD_001");
    }

    #[test]
    fn test_create_disc_layout() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let staging = temp_dir.path();

        let source_folders = vec![PathBuf::from("/tmp/test1")];
        let disc_root = create_disc_layout(staging, "2024-BD-001", &source_folders, None)?;

        assert!(disc_root.exists());
        assert!(disc_root.join("ARCHIVE").exists());

        Ok(())
    }

    #[test]
    fn test_write_disc_info() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let disc_root = temp_dir.path();

        let source_roots = vec![PathBuf::from("/tmp/test1"), PathBuf::from("/tmp/test2")];
        write_disc_info(
            disc_root,
            "2024-BD-001",
            Some("Test disc"),
            &source_roots,
            "1.0.0",
        )?;

        let info_path = disc_root.join("DISC_INFO.txt");
        assert!(info_path.exists());

        let content = fs::read_to_string(&info_path)?;
        assert!(content.contains("Disc-ID: 2024-BD-001"));
        assert!(content.contains("Test disc"));
        assert!(content.contains("/tmp/test1"));

        Ok(())
    }
}
