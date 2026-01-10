use anyhow::Result;
use rusqlite::{Connection, Row};

/// Map a database row to SearchResult.
fn map_row(row: &Row) -> rusqlite::Result<SearchResult> {
    Ok(SearchResult {
        disc_id: row.get(0)?,
        rel_path: row.get(1)?,
        size: row.get(2)?,
        mtime: row.get(3)?,
        sha256: row.get(4)?,
    })
}

/// Search query parameters.
#[derive(Debug, Clone)]
pub struct SearchQuery {
    pub path_substring: Option<String>,
    pub exact_filename: Option<String>,
    pub sha256: Option<String>,
    pub regex: Option<String>, // Nice-to-have, not implemented yet
}

/// Search result.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub disc_id: String,
    pub rel_path: String,
    pub size: u64,
    pub mtime: String,
    pub sha256: String,
}

/// Search files in the database.
pub fn search_files(conn: &Connection, query: &SearchQuery) -> Result<Vec<SearchResult>> {
    let mut results = Vec::new();

    // Build SQL query and parameters based on search criteria
    let (sql, param): (String, Option<String>) = if let Some(ref sha256) = query.sha256 {
        // SHA256 search (exact match)
        let sql = "SELECT disc_id, rel_path, size, mtime, sha256 FROM files WHERE sha256 = ?1 ORDER BY rel_path LIMIT 1000";
        (sql.to_string(), Some(sha256.clone()))
    } else if let Some(ref path_substring) = query.path_substring {
        // Path substring search
        let pattern = format!("%{}%", path_substring);
        let sql = "SELECT disc_id, rel_path, size, mtime, sha256 FROM files WHERE rel_path LIKE ?1 ORDER BY rel_path LIMIT 1000";
        (sql.to_string(), Some(pattern))
    } else if let Some(ref exact_filename) = query.exact_filename {
        // Exact filename search
        let pattern = format!("%/{}", exact_filename);
        let sql = "SELECT disc_id, rel_path, size, mtime, sha256 FROM files WHERE rel_path LIKE ?1 ORDER BY rel_path LIMIT 1000";
        (sql.to_string(), Some(pattern))
    } else {
        // No filters, return all
        let sql =
            "SELECT disc_id, rel_path, size, mtime, sha256 FROM files ORDER BY rel_path LIMIT 1000";
        (sql.to_string(), None)
    };

    let mut stmt = conn.prepare(&sql)?;

    let rows: rusqlite::MappedRows<_> = if let Some(p) = param {
        stmt.query_map(rusqlite::params![p], map_row)?
    } else {
        stmt.query_map([], map_row)?
    };

    for row in rows {
        results.push(row?);
    }

    Ok(results)
}

/// Format file size for display.
pub fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", bytes, UNITS[unit_index])
    } else {
        format!("{:.2} {}", size, UNITS[unit_index])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database;
    use tempfile::TempDir;

    #[test]
    fn test_search_files() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let db_path = temp_dir.path().join("test.db");
        let mut conn = database::init_database(&db_path)?;

        // Insert test disc first (required for foreign key constraint)
        let disc = crate::database::Disc {
            disc_id: "2024-BD-001".to_string(),
            volume_label: "TEST_DISC".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            notes: Some("Test disc".to_string()),
            iso_size: Some(1024),
            burn_device: Some("/dev/sr0".to_string()),
            checksum_manifest_hash: None,
            qr_path: None,
            source_roots: None,
            tool_version: None,
            set_id: None,
            sequence_number: None,
        };
        crate::database::Disc::insert(&mut conn, &disc)?;

        // Insert test file
        let file = crate::database::FileRecord {
            id: None,
            disc_id: "2024-BD-001".to_string(),
            rel_path: "ARCHIVE/test/file.txt".to_string(),
            sha256: "abc123".to_string(),
            size: 100,
            mtime: "2024-01-01T00:00:00Z".to_string(),
            added_at: "2024-01-01T00:00:00Z".to_string(),
        };

        crate::database::FileRecord::insert(&mut conn, &file)?;

        // Search by substring
        let query = SearchQuery {
            path_substring: Some("test".to_string()),
            exact_filename: None,
            sha256: None,
            regex: None,
        };

        let results = search_files(&conn, &query)?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].rel_path, "ARCHIVE/test/file.txt");

        Ok(())
    }

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(1024), "1.00 KB");
        assert_eq!(format_size(1048576), "1.00 MB");
        assert_eq!(format_size(1073741824), "1.00 GB");
    }
}
