use anyhow::{Context, Result};
use rusqlite::{params, Connection, Transaction};
use std::path::Path;
use tracing::{debug, info};

/// Database schema version
const SCHEMA_VERSION: u32 = 1;

/// Initialize the database and run migrations if needed.
pub fn init_database(db_path: &Path) -> Result<Connection> {
    // Ensure parent directory exists
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!("Failed to create database directory: {}", parent.display())
        })?;
    }

    let mut conn = Connection::open(db_path)
        .with_context(|| format!("Failed to open database: {}", db_path.display()))?;

    // Enable foreign keys
    conn.execute("PRAGMA foreign_keys = ON", [])?;

    // Run migrations
    migrate_database(&mut conn)?;

    info!("Database initialized at: {}", db_path.display());
    Ok(conn)
}

/// Migrate database to the latest schema version.
fn migrate_database(conn: &mut Connection) -> Result<()> {
    let current_version = get_schema_version(conn)?;

    if current_version < SCHEMA_VERSION {
        info!(
            "Migrating database from version {} to {}",
            current_version, SCHEMA_VERSION
        );

        let tx = conn.transaction()?;
        if current_version == 0 {
            create_schema(&tx)?;
        }
        // Future migrations would go here:
        // if current_version == 1 {
        //     migrate_v1_to_v2(&tx)?;
        // }
        set_schema_version(&tx, SCHEMA_VERSION)?;
        tx.commit()?;

        info!("Database migration completed");
    } else if current_version > SCHEMA_VERSION {
        anyhow::bail!(
            "Database schema version {} is newer than supported version {}",
            current_version,
            SCHEMA_VERSION
        );
    }

    Ok(())
}

/// Get the current schema version.
fn get_schema_version(conn: &Connection) -> Result<u32> {
    // Check if version table exists
    let table_exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='schema_version')",
            [],
            |row| row.get(0),
        )
        .unwrap_or(false);

    if !table_exists {
        return Ok(0);
    }

    let version: u32 = conn
        .query_row("SELECT version FROM schema_version", [], |row| row.get(0))
        .unwrap_or(0);

    Ok(version)
}

/// Set the schema version.
fn set_schema_version(tx: &Transaction, version: u32) -> Result<()> {
    tx.execute(
        "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER)",
        [],
    )?;
    tx.execute("DELETE FROM schema_version", [])?;
    tx.execute(
        "INSERT INTO schema_version (version) VALUES (?1)",
        params![version],
    )?;
    Ok(())
}

/// Create the initial database schema.
fn create_schema(tx: &Transaction) -> Result<()> {
    // Discs table
    tx.execute(
        "CREATE TABLE IF NOT EXISTS discs (
            disc_id TEXT PRIMARY KEY,
            volume_label TEXT NOT NULL,
            created_at TEXT NOT NULL,
            notes TEXT,
            iso_size INTEGER,
            burn_device TEXT,
            checksum_manifest_hash TEXT,
            qr_path TEXT,
            source_roots TEXT,
            tool_version TEXT
        )",
        [],
    )?;

    tx.execute(
        "CREATE INDEX IF NOT EXISTS idx_discs_created_at ON discs(created_at)",
        [],
    )?;

    // Files table
    tx.execute(
        "CREATE TABLE IF NOT EXISTS files (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            disc_id TEXT NOT NULL,
            rel_path TEXT NOT NULL,
            sha256 TEXT NOT NULL,
            size INTEGER NOT NULL,
            mtime TEXT NOT NULL,
            added_at TEXT NOT NULL,
            FOREIGN KEY (disc_id) REFERENCES discs(disc_id) ON DELETE CASCADE,
            UNIQUE(disc_id, rel_path)
        )",
        [],
    )?;

    tx.execute(
        "CREATE INDEX IF NOT EXISTS idx_files_disc_id ON files(disc_id)",
        [],
    )?;

    tx.execute(
        "CREATE INDEX IF NOT EXISTS idx_files_rel_path ON files(rel_path)",
        [],
    )?;

    tx.execute(
        "CREATE INDEX IF NOT EXISTS idx_files_sha256 ON files(sha256)",
        [],
    )?;

    tx.execute(
        "CREATE INDEX IF NOT EXISTS idx_files_disc_path ON files(disc_id, rel_path)",
        [],
    )?;

    // Verification runs table
    tx.execute(
        "CREATE TABLE IF NOT EXISTS verification_runs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            disc_id TEXT NOT NULL,
            verified_at TEXT NOT NULL,
            mountpoint TEXT,
            device TEXT,
            success INTEGER NOT NULL,
            error_message TEXT,
            files_checked INTEGER,
            files_failed INTEGER,
            FOREIGN KEY (disc_id) REFERENCES discs(disc_id) ON DELETE CASCADE
        )",
        [],
    )?;

    tx.execute(
        "CREATE INDEX IF NOT EXISTS idx_verification_disc_id ON verification_runs(disc_id)",
        [],
    )?;

    tx.execute(
        "CREATE INDEX IF NOT EXISTS idx_verification_verified_at ON verification_runs(verified_at)",
        [],
    )?;

    debug!("Database schema created");
    Ok(())
}

/// Disc record structure
#[derive(Debug, Clone)]
pub struct Disc {
    pub disc_id: String,
    pub volume_label: String,
    pub created_at: String,
    pub notes: Option<String>,
    pub iso_size: Option<u64>,
    pub burn_device: Option<String>,
    pub checksum_manifest_hash: Option<String>,
    pub qr_path: Option<String>,
    pub source_roots: Option<String>,
    pub tool_version: Option<String>,
}

impl Disc {
    /// Insert a new disc record.
    pub fn insert(conn: &mut Connection, disc: &Disc) -> Result<()> {
        conn.execute(
            "INSERT INTO discs (
                disc_id, volume_label, created_at, notes, iso_size, burn_device,
                checksum_manifest_hash, qr_path, source_roots, tool_version
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                disc.disc_id,
                disc.volume_label,
                disc.created_at,
                disc.notes,
                disc.iso_size,
                disc.burn_device,
                disc.checksum_manifest_hash,
                disc.qr_path,
                disc.source_roots,
                disc.tool_version
            ],
        )?;
        Ok(())
    }

    /// Get a disc by ID.
    pub fn get(conn: &Connection, disc_id: &str) -> Result<Option<Disc>> {
        let mut stmt = conn.prepare(
            "SELECT disc_id, volume_label, created_at, notes, iso_size, burn_device,
                    checksum_manifest_hash, qr_path, source_roots, tool_version
             FROM discs WHERE disc_id = ?1",
        )?;

        let disc = stmt.query_row(params![disc_id], |row| {
            Ok(Disc {
                disc_id: row.get(0)?,
                volume_label: row.get(1)?,
                created_at: row.get(2)?,
                notes: row.get(3)?,
                iso_size: row.get(4)?,
                burn_device: row.get(5)?,
                checksum_manifest_hash: row.get(6)?,
                qr_path: row.get(7)?,
                source_roots: row.get(8)?,
                tool_version: row.get(9)?,
            })
        });

        match disc {
            Ok(d) => Ok(Some(d)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// List all discs.
    pub fn list_all(conn: &Connection) -> Result<Vec<Disc>> {
        let mut stmt = conn.prepare(
            "SELECT disc_id, volume_label, created_at, notes, iso_size, burn_device,
                    checksum_manifest_hash, qr_path, source_roots, tool_version
             FROM discs ORDER BY created_at DESC",
        )?;

        let discs = stmt.query_map([], |row| {
            Ok(Disc {
                disc_id: row.get(0)?,
                volume_label: row.get(1)?,
                created_at: row.get(2)?,
                notes: row.get(3)?,
                iso_size: row.get(4)?,
                burn_device: row.get(5)?,
                checksum_manifest_hash: row.get(6)?,
                qr_path: row.get(7)?,
                source_roots: row.get(8)?,
                tool_version: row.get(9)?,
            })
        })?;

        let mut result = Vec::new();
        for disc in discs {
            result.push(disc?);
        }
        Ok(result)
    }
}

/// File record structure
#[derive(Debug, Clone)]
pub struct FileRecord {
    pub id: Option<i64>,
    pub disc_id: String,
    pub rel_path: String,
    pub sha256: String,
    pub size: u64,
    pub mtime: String,
    pub added_at: String,
}

impl FileRecord {
    /// Insert a file record.
    pub fn insert(conn: &Connection, file: &FileRecord) -> Result<()> {
        conn.execute(
            "INSERT INTO files (disc_id, rel_path, sha256, size, mtime, added_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(disc_id, rel_path) DO UPDATE SET
                sha256 = ?3, size = ?4, mtime = ?5, added_at = ?6",
            params![
                file.disc_id,
                file.rel_path,
                file.sha256,
                file.size,
                file.mtime,
                file.added_at
            ],
        )?;
        Ok(())
    }

    /// Insert multiple file records in a transaction.
    pub fn insert_batch(conn: &mut Connection, files: &[FileRecord]) -> Result<()> {
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO files (disc_id, rel_path, sha256, size, mtime, added_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(disc_id, rel_path) DO UPDATE SET
                    sha256 = ?3, size = ?4, mtime = ?5, added_at = ?6",
            )?;

            for file in files {
                stmt.execute(params![
                    file.disc_id,
                    file.rel_path,
                    file.sha256,
                    file.size,
                    file.mtime,
                    file.added_at
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }
}

/// Verification run record
#[derive(Debug, Clone)]
pub struct VerificationRun {
    pub id: Option<i64>,
    pub disc_id: String,
    pub verified_at: String,
    pub mountpoint: Option<String>,
    pub device: Option<String>,
    pub success: bool,
    pub error_message: Option<String>,
    pub files_checked: Option<u32>,
    pub files_failed: Option<u32>,
}

impl VerificationRun {
    /// Insert a verification run record.
    pub fn insert(conn: &Connection, run: &VerificationRun) -> Result<i64> {
        conn.execute(
            "INSERT INTO verification_runs (
                disc_id, verified_at, mountpoint, device, success,
                error_message, files_checked, files_failed
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                run.disc_id,
                run.verified_at,
                run.mountpoint,
                run.device,
                if run.success { 1 } else { 0 },
                run.error_message,
                run.files_checked,
                run.files_failed
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_database_creation() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let db_path = temp_dir.path().join("test.db");
        let conn = init_database(&db_path)?;

        // Verify tables exist
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table'")?
            .query_map([], |row| row.get(0))?
            .collect::<Result<_, _>>()?;

        assert!(tables.contains(&"discs".to_string()));
        assert!(tables.contains(&"files".to_string()));
        assert!(tables.contains(&"verification_runs".to_string()));

        Ok(())
    }

    #[test]
    fn test_disc_insert_and_get() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let db_path = temp_dir.path().join("test.db");
        let mut conn = init_database(&db_path)?;

        let disc = Disc {
            disc_id: "2024-BD-001".to_string(),
            volume_label: "BDARCHIVE_2024_BD_001".to_string(),
            created_at: "2024-01-15T10:30:00Z".to_string(),
            notes: Some("Test disc".to_string()),
            iso_size: Some(1024),
            burn_device: Some("/dev/sr0".to_string()),
            checksum_manifest_hash: None,
            qr_path: None,
            source_roots: None,
            tool_version: None,
        };

        Disc::insert(&mut conn, &disc)?;
        let retrieved = Disc::get(&conn, "2024-BD-001")?;

        assert!(retrieved.is_some());
        let d = retrieved.unwrap();
        assert_eq!(d.disc_id, "2024-BD-001");
        assert_eq!(d.notes, Some("Test disc".to_string()));

        Ok(())
    }
}
