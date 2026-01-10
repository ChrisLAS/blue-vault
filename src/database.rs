use anyhow::{Context, Result};
use rusqlite::{params, Connection, Transaction};
use std::path::Path;
use tracing::{debug, info};

/// Database schema version
const SCHEMA_VERSION: u32 = 2;

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
        if current_version == 1 {
            migrate_v1_to_v2(&tx)?;
        }
        // Future migrations would go here:
        // if current_version == 2 {
        //     migrate_v2_to_v3(&tx)?;
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

/// Migrate from schema version 1 to version 2 (add multi-disc support).
fn migrate_v1_to_v2(tx: &Transaction) -> Result<()> {
    info!("Migrating database to version 2: adding multi-disc support");

    // Create disc_sets table
    tx.execute(
        "CREATE TABLE IF NOT EXISTS disc_sets (
            set_id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT,
            total_size INTEGER NOT NULL,
            disc_count INTEGER NOT NULL,
            created_at TEXT NOT NULL,
            source_roots TEXT
        )",
        [],
    )?;

    tx.execute(
        "CREATE INDEX IF NOT EXISTS idx_disc_sets_created_at ON disc_sets(created_at)",
        [],
    )?;

    // Add set_id and sequence_number to discs table
    tx.execute(
        "ALTER TABLE discs ADD COLUMN set_id TEXT",
        [],
    )?;

    tx.execute(
        "ALTER TABLE discs ADD COLUMN sequence_number INTEGER",
        [],
    )?;

    // Create index for the new columns
    tx.execute(
        "CREATE INDEX IF NOT EXISTS idx_discs_set_id ON discs(set_id)",
        [],
    )?;

    tx.execute(
        "CREATE INDEX IF NOT EXISTS idx_discs_set_sequence ON discs(set_id, sequence_number)",
        [],
    )?;

    // Add foreign key constraint (SQLite doesn't support adding FK constraints to existing tables,
    // but we can add the index and handle constraints in application code)

    info!("Migration to version 2 completed");
    Ok(())
}

/// Create the initial database schema.
fn create_schema(tx: &Transaction) -> Result<()> {
    // Disc sets table (for multi-disc archives)
    tx.execute(
        "CREATE TABLE IF NOT EXISTS disc_sets (
            set_id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT,
            total_size INTEGER NOT NULL,
            disc_count INTEGER NOT NULL,
            created_at TEXT NOT NULL,
            source_roots TEXT
        )",
        [],
    )?;

    tx.execute(
        "CREATE INDEX IF NOT EXISTS idx_disc_sets_created_at ON disc_sets(created_at)",
        [],
    )?;

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
            tool_version TEXT,
            set_id TEXT,
            sequence_number INTEGER,
            FOREIGN KEY (set_id) REFERENCES disc_sets(set_id) ON DELETE SET NULL
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

/// Disc set record structure (for multi-disc archives)
#[derive(Debug, Clone)]
pub struct DiscSet {
    pub set_id: String,
    pub name: String,
    pub description: Option<String>,
    pub total_size: u64,
    pub disc_count: u32,
    pub created_at: String,
    pub source_roots: Option<String>,
}

impl DiscSet {
    /// Insert a new disc set record.
    pub fn insert(conn: &mut Connection, disc_set: &DiscSet) -> Result<()> {
        conn.execute(
            "INSERT INTO disc_sets (
                set_id, name, description, total_size, disc_count, created_at, source_roots
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                disc_set.set_id,
                disc_set.name,
                disc_set.description,
                disc_set.total_size,
                disc_set.disc_count,
                disc_set.created_at,
                disc_set.source_roots
            ],
        )?;
        Ok(())
    }

    /// Get a disc set by ID.
    pub fn get(conn: &Connection, set_id: &str) -> Result<Option<DiscSet>> {
        let mut stmt = conn.prepare(
            "SELECT set_id, name, description, total_size, disc_count, created_at, source_roots
             FROM disc_sets WHERE set_id = ?1",
        )?;

        let disc_set = stmt.query_row(params![set_id], |row| {
            Ok(DiscSet {
                set_id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                total_size: row.get(3)?,
                disc_count: row.get(4)?,
                created_at: row.get(5)?,
                source_roots: row.get(6)?,
            })
        });

        match disc_set {
            Ok(ds) => Ok(Some(ds)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Get all discs in this set, ordered by sequence number.
    pub fn get_discs(conn: &Connection, set_id: &str) -> Result<Vec<Disc>> {
        let mut stmt = conn.prepare(
            "SELECT disc_id, volume_label, created_at, notes, iso_size, burn_device,
                    checksum_manifest_hash, qr_path, source_roots, tool_version, set_id, sequence_number
             FROM discs WHERE set_id = ?1 ORDER BY sequence_number",
        )?;

        let disc_iter = stmt.query_map(params![set_id], |row| {
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
                set_id: row.get(10)?,
                sequence_number: row.get(11)?,
            })
        })?;

        let mut discs = Vec::new();
        for disc in disc_iter {
            discs.push(disc?);
        }

        Ok(discs)
    }
}

/// Generate a unique set ID for a multi-disc archive
pub fn generate_set_id() -> String {
    use crate::disc::format_timestamp_now;
    format!("SET-{}", format_timestamp_now().replace([':', '-'], ""))
}

/// Helper functions for multi-disc operations
pub struct MultiDiscOps;

impl MultiDiscOps {
    /// Create a new disc set and get the set ID
    pub fn create_disc_set(
        conn: &mut Connection,
        name: &str,
        description: Option<&str>,
        total_size: u64,
        disc_count: u32,
        source_roots: Option<&str>,
    ) -> Result<String> {
        let set_id = generate_set_id();
        let created_at = crate::disc::format_timestamp_now();

        let disc_set = DiscSet {
            set_id: set_id.clone(),
            name: name.to_string(),
            description: description.map(|s| s.to_string()),
            total_size,
            disc_count,
            created_at,
            source_roots: source_roots.map(|s| s.to_string()),
        };

        DiscSet::insert(conn, &disc_set)?;
        Ok(set_id)
    }

    /// Add a disc to an existing set
    pub fn add_disc_to_set(
        conn: &mut Connection,
        disc: &mut Disc,
        set_id: &str,
        sequence_number: u32,
    ) -> Result<()> {
        disc.set_id = Some(set_id.to_string());
        disc.sequence_number = Some(sequence_number);
        Disc::insert(conn, disc)?;
        Ok(())
    }

    /// Check if a disc is part of a multi-disc set
    pub fn is_part_of_set(conn: &Connection, disc_id: &str) -> Result<Option<String>> {
        let disc = Disc::get(conn, disc_id)?;
        Ok(disc.and_then(|d| d.set_id))
    }

    /// Get all discs in the same set as the given disc
    pub fn get_related_discs(conn: &Connection, disc_id: &str) -> Result<Vec<Disc>> {
        if let Some(set_id) = Self::is_part_of_set(conn, disc_id)? {
            DiscSet::get_discs(conn, &set_id)
        } else {
            Ok(Vec::new())
        }
    }
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
    pub set_id: Option<String>,
    pub sequence_number: Option<u32>,
}

impl Disc {
    /// Insert a new disc record.
    pub fn insert(conn: &mut Connection, disc: &Disc) -> Result<()> {
        conn.execute(
            "INSERT INTO discs (
                disc_id, volume_label, created_at, notes, iso_size, burn_device,
                checksum_manifest_hash, qr_path, source_roots, tool_version, set_id, sequence_number
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
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
                disc.tool_version,
                disc.set_id,
                disc.sequence_number
            ],
        )?;
        Ok(())
    }

    /// Get a disc by ID.
    pub fn get(conn: &Connection, disc_id: &str) -> Result<Option<Disc>> {
        let mut stmt = conn.prepare(
            "SELECT disc_id, volume_label, created_at, notes, iso_size, burn_device,
                    checksum_manifest_hash, qr_path, source_roots, tool_version, set_id, sequence_number
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
                set_id: row.get(10)?,
                sequence_number: row.get(11)?,
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
                    checksum_manifest_hash, qr_path, source_roots, tool_version, set_id, sequence_number
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
                set_id: row.get(10)?,
                sequence_number: row.get(11)?,
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
            set_id: None,
            sequence_number: None,
        };

        Disc::insert(&mut conn, &disc)?;
        let retrieved = Disc::get(&conn, "2024-BD-001")?;

        assert!(retrieved.is_some());
        let d = retrieved.unwrap();
        assert_eq!(d.disc_id, "2024-BD-001");
        assert_eq!(d.notes, Some("Test disc".to_string()));

        Ok(())
    }

    #[test]
    fn test_disc_set_operations() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let db_path = temp_dir.path().join("test.db");
        let mut conn = init_database(&db_path)?;

        // Create a disc set
        let set_id = MultiDiscOps::create_disc_set(
            &mut conn,
            "Test Multi-Disc Archive",
            Some("A test archive spanning multiple discs"),
            500 * 1024 * 1024, // 500MB total
            2, // 2 discs
            Some("/home/user/data"),
        )?;

        // Create discs for the set
        let mut disc1 = Disc {
            disc_id: "2024-BD-001".to_string(),
            volume_label: "BDARCHIVE_2024_BD_001".to_string(),
            created_at: "2024-01-15T10:30:00Z".to_string(),
            notes: Some("First disc of set".to_string()),
            iso_size: Some(250 * 1024 * 1024),
            burn_device: Some("/dev/sr0".to_string()),
            checksum_manifest_hash: None,
            qr_path: None,
            source_roots: None,
            tool_version: None,
            set_id: None,
            sequence_number: None,
        };

        let mut disc2 = Disc {
            disc_id: "2024-BD-002".to_string(),
            volume_label: "BDARCHIVE_2024_BD_002".to_string(),
            created_at: "2024-01-15T11:00:00Z".to_string(),
            notes: Some("Second disc of set".to_string()),
            iso_size: Some(250 * 1024 * 1024),
            burn_device: Some("/dev/sr0".to_string()),
            checksum_manifest_hash: None,
            qr_path: None,
            source_roots: None,
            tool_version: None,
            set_id: None,
            sequence_number: None,
        };

        // Add discs to the set
        MultiDiscOps::add_disc_to_set(&mut conn, &mut disc1, &set_id, 1)?;
        MultiDiscOps::add_disc_to_set(&mut conn, &mut disc2, &set_id, 2)?;

        // Verify the set was created
        let retrieved_set = DiscSet::get(&conn, &set_id)?;
        assert!(retrieved_set.is_some());
        let set = retrieved_set.unwrap();
        assert_eq!(set.name, "Test Multi-Disc Archive");
        assert_eq!(set.disc_count, 2);
        assert_eq!(set.total_size, 500 * 1024 * 1024);

        // Verify discs are in the set
        let set_discs = DiscSet::get_discs(&conn, &set_id)?;
        assert_eq!(set_discs.len(), 2);
        assert_eq!(set_discs[0].disc_id, "2024-BD-001");
        assert_eq!(set_discs[0].sequence_number, Some(1));
        assert_eq!(set_discs[1].disc_id, "2024-BD-002");
        assert_eq!(set_discs[1].sequence_number, Some(2));

        // Test relationship queries
        assert_eq!(MultiDiscOps::is_part_of_set(&conn, "2024-BD-001")?, Some(set_id.clone()));
        let related_discs = MultiDiscOps::get_related_discs(&conn, "2024-BD-001")?;
        assert_eq!(related_discs.len(), 2);

        Ok(())
    }
}
