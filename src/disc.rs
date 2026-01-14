use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::debug;
use rusqlite::params;

/// Generate a disc ID in the format YYYY-BD-#.
pub fn generate_disc_id() -> String {
    let year = get_current_year();
    let number = get_next_disc_number(&year).unwrap_or(1);
    format!("{:04}-BD-{}", year, number)
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
/// For now, just return None to always start from 1.
fn get_next_disc_number(year: &u32) -> Option<u32> {
    // Query database for existing discs with this year prefix
    let db_path = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("bdarchive")
        .join("archive.db");

    if !db_path.exists() {
        // Database doesn't exist yet, start from 1
        return Some(1);
    }

    let conn = match rusqlite::Connection::open(&db_path) {
        Ok(conn) => conn,
        Err(_) => return Some(1), // If we can't open DB, start from 1
    };

    let year_prefix = format!("{:04}-BD-", year);
    let query = "SELECT disc_id FROM discs WHERE disc_id LIKE ?1 ORDER BY disc_id DESC LIMIT 1";

    match conn.query_row(query, params![format!("{}%", year_prefix)], |row| {
        let disc_id: String = row.get(0)?;
        // Extract the number from disc_id like "2026-BD-005"
        if let Some(num_str) = disc_id.strip_prefix(&year_prefix) {
            Ok(num_str.parse::<u32>().ok())
        } else {
            Ok(None)
        }
    }) {
        Ok(Some(last_num)) => Some(last_num + 1),
        _ => Some(1), // No existing discs for this year, start from 1
    }
}

/// Generate volume label from disc ID.
pub fn generate_volume_label(disc_id: &str) -> String {
    // Convert to uppercase and replace hyphens with underscores
    disc_id.to_uppercase().replace("-", "_")
}

/// Generate volume label for multi-disc sets.
/// Ensures labels fit within filesystem constraints (typically 32 chars max).
pub fn generate_multi_disc_volume_label(base_id: &str, sequence_num: u32, total_discs: u32) -> String {
    // For multi-disc sets, create labels like: "BDARCHIVE_2024_1_OF_3"
    // This clearly shows the disc position and total count

    // Extract year from base_id if it's in the format "YYYY-BD-XXX"
    let year_part = if base_id.len() >= 4 && base_id.chars().take(4).all(|c| c.is_ascii_digit()) {
        format!("_{}", &base_id[0..4])
    } else {
        String::new()
    };

    let label = format!("BDARCHIVE{}D{}_OF_{}", year_part, sequence_num, total_discs);

    // Ensure it fits within typical filesystem limits (32 chars is common)
    if label.len() > 32 {
        // Fallback to shorter format if needed
        format!("BD{}_{}_{}", &base_id[0..4], sequence_num, total_discs)
    } else {
        label
    }
}

/// Generate disc ID for a specific sequence in a multi-disc set.
/// For multi-disc sets, generates IDs like "2024-BD-ARCHIVE-1", "2024-BD-ARCHIVE-2", etc.
pub fn generate_multi_disc_id(base_id: &str, sequence_num: u32) -> String {
    format!("{}-{}", base_id, sequence_num)
}

/// Validate that a disc ID is valid for use in filenames and volume labels.
pub fn validate_disc_id(disc_id: &str) -> Result<(), String> {
    if disc_id.is_empty() {
        return Err("Disc ID cannot be empty".to_string());
    }

    if disc_id.len() > 50 {
        return Err("Disc ID too long (max 50 characters)".to_string());
    }

    // Check for invalid characters (filesystem unsafe)
    let invalid_chars = ['/', '\\', ':', '*', '?', '"', '<', '>', '|', '\0', '\n', '\r'];
    if let Some(invalid_char) = disc_id.chars().find(|c| invalid_chars.contains(c)) {
        return Err(format!("Invalid character '{}' in disc ID", invalid_char));
    }

    // Check for reserved names (Windows system files, etc.)
    let lower_id = disc_id.to_lowercase();
    let reserved_names = ["con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9"];
    if reserved_names.contains(&lower_id.as_str()) {
        return Err(format!("'{}' is a reserved system name", disc_id));
    }

    Ok(())
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
    set_id: Option<&str>,
    sequence_number: Option<u32>,
    total_discs: Option<u32>,
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

    // Add multi-disc information if available
    if let (Some(set_id), Some(seq), Some(total)) = (set_id, sequence_number, total_discs) {
        info.push_str(&format!("Multi-Disc Set: {}\n", set_id));
        info.push_str(&format!("Disc Sequence: {} of {}\n", seq, total));
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
            None, // set_id
            None, // sequence_number
            None, // total_discs
        )?;

        let info_path = disc_root.join("DISC_INFO.txt");
        assert!(info_path.exists());

        let content = fs::read_to_string(&info_path)?;
        assert!(content.contains("Disc-ID: 2024-BD-001"));
        assert!(content.contains("Test disc"));
        assert!(content.contains("/tmp/test1"));

        Ok(())
    }

    #[test]
    fn test_write_disc_info_multi_disc() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let disc_root = temp_dir.path();

        let source_roots = vec![PathBuf::from("/tmp/test1"), PathBuf::from("/tmp/test2")];
        write_disc_info(
            disc_root,
            "2024-BD-ARCHIVE-002",
            Some("Second disc of backup set"),
            &source_roots,
            "1.0.0",
            Some("SET-20240115103000"),
            Some(2),
            Some(5),
        )?;

        let info_path = disc_root.join("DISC_INFO.txt");
        assert!(info_path.exists());

        let content = fs::read_to_string(&info_path)?;
        assert!(content.contains("Disc-ID: 2024-BD-ARCHIVE-002"));
        assert!(content.contains("Second disc of backup set"));
        assert!(content.contains("Multi-Disc Set: SET-20240115103000"));
        assert!(content.contains("Disc Sequence: 2 of 5"));
        assert!(content.contains("/tmp/test1"));

        Ok(())
    }

    #[test]
    fn test_generate_multi_disc_id() {
        let base_id = "2024-BD-ARCHIVE";
        assert_eq!(generate_multi_disc_id(base_id, 1), "2024-BD-ARCHIVE-1");
        assert_eq!(generate_multi_disc_id(base_id, 15), "2024-BD-ARCHIVE-15");
        assert_eq!(generate_multi_disc_id(base_id, 123), "2024-BD-ARCHIVE-123");
    }

    #[test]
    fn test_generate_multi_disc_volume_label() {
        // Test normal case
        let label = generate_multi_disc_volume_label("2024-BD-ARCHIVE", 1, 3);
        assert_eq!(label, "BDARCHIVE_2024D1_OF_3");

        // Test longer base ID (should still fit)
        let label = generate_multi_disc_volume_label("2024-BD-VERY-LONG-ARCHIVE-NAME", 5, 12);
        assert!(label.len() <= 32); // Should fit within filesystem limits
        assert!(label.contains("5"));
        assert!(label.contains("12"));
    }

    #[test]
    fn test_validate_disc_id() {
        // Valid IDs
        assert!(validate_disc_id("2024-BD-001").is_ok());
        assert!(validate_disc_id("MY_ARCHIVE_001").is_ok());
        assert!(validate_disc_id("test-disc").is_ok());

        // Invalid: empty
        assert!(validate_disc_id("").is_err());

        // Invalid: too long
        assert!(validate_disc_id(&"A".repeat(51)).is_err());

        // Invalid: bad characters
        assert!(validate_disc_id("test/disc").is_err());
        assert!(validate_disc_id("test\\disc").is_err());
        assert!(validate_disc_id("test:disc").is_err());
        assert!(validate_disc_id("test*disc").is_err());
        assert!(validate_disc_id("test?disc").is_err());
        assert!(validate_disc_id("test\"disc").is_err());
        assert!(validate_disc_id("test<disc").is_err());
        assert!(validate_disc_id("test>disc").is_err());
        assert!(validate_disc_id("test|disc").is_err());

        // Invalid: reserved names
        assert!(validate_disc_id("con").is_err());
        assert!(validate_disc_id("CON").is_err());
        assert!(validate_disc_id("nul").is_err());
        assert!(validate_disc_id("com1").is_err());
        assert!(validate_disc_id("lpt1").is_err());
    }
}
