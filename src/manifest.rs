use anyhow::{Context, Result};
use sha2::{Sha256, Digest};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

/// File metadata for a file in the archive.
#[derive(Debug, Clone)]
pub struct FileMetadata {
    pub rel_path: PathBuf,
    pub size: u64,
    pub mtime: String,  // ISO 8601 format
    pub sha256: String,
}

/// Generate manifest and SHA256 sums for a directory.
pub fn generate_manifest_and_sums(
    root_dir: &Path,
    base_path: Option<&Path>,
) -> Result<Vec<FileMetadata>> {
    let base = base_path.unwrap_or(root_dir);
    let mut files = Vec::new();

    debug!("Generating manifest for directory: {}", root_dir.display());

    walk_directory(root_dir, base, &mut files)?;

    info!("Generated manifest with {} files", files.len());
    Ok(files)
}

/// Recursively walk directory and collect file metadata.
fn walk_directory(
    dir: &Path,
    base: &Path,
    files: &mut Vec<FileMetadata>,
) -> Result<()> {
    let entries = fs::read_dir(dir)
        .with_context(|| format!("Failed to read directory: {}", dir.display()))?;

    for entry in entries {
        let entry = entry.context("Failed to read directory entry")?;
        let path = entry.path();

        if path.is_dir() {
            walk_directory(&path, base, files)?;
        } else if path.is_file() {
            let metadata = generate_file_metadata(&path, base)?;
            files.push(metadata);
        }
    }

    Ok(())
}

/// Generate metadata for a single file.
fn generate_file_metadata(file_path: &Path, base: &Path) -> Result<FileMetadata> {
    let rel_path = crate::paths::make_relative(file_path, base)?;

    let metadata = fs::metadata(file_path)
        .with_context(|| format!("Failed to read file metadata: {}", file_path.display()))?;

    let size = metadata.len();
    let mtime = metadata
        .modified()
        .context("Failed to get modification time")?;

    let mtime_str = format_timestamp(mtime);

    let sha256 = calculate_sha256(file_path)?;

    Ok(FileMetadata {
        rel_path,
        size,
        mtime: mtime_str,
        sha256,
    })
}

/// Calculate SHA256 hash of a file.
fn calculate_sha256(file_path: &Path) -> Result<String> {
    let mut file = fs::File::open(file_path)
        .with_context(|| format!("Failed to open file: {}", file_path.display()))?;

    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024]; // 64KB buffer

    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    let hash = hasher.finalize();
    Ok(hex::encode(hash))
}

/// Format timestamp as ISO 8601 string.
fn format_timestamp(time: std::time::SystemTime) -> String {
    // For now, use a simple format; in production you might want a proper date library
    match time.duration_since(std::time::UNIX_EPOCH) {
        Ok(duration) => {
            let secs = duration.as_secs();
            // Simple ISO 8601 format (UTC approximation)
            // For proper formatting, consider using a date library
            format_timestamp_simple(secs)
        }
        Err(_) => "1970-01-01T00:00:00Z".to_string(),
    }
}

/// Simple timestamp formatting (approximate UTC).
fn format_timestamp_simple(secs: u64) -> String {
    // This is a simplified formatter; for production use a proper date library
    // Using Unix epoch calculations
    let days = secs / 86400;
    let secs_in_day = secs % 86400;
    
    // Approximate years since 1970
    let year = 1970 + (days / 365);
    let day_of_year = days % 365;
    let month = 1 + (day_of_year / 30);
    let day = 1 + (day_of_year % 30);
    
    let hours = secs_in_day / 3600;
    let mins = (secs_in_day % 3600) / 60;
    let secs_remainder = secs_in_day % 60;
    
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", year, month, day, hours, mins, secs_remainder)
}

/// Write manifest file (one path per line).
pub fn write_manifest_file(manifest_path: &Path, files: &[FileMetadata]) -> Result<()> {
    let mut manifest = String::new();
    for file in files {
        let path_str = file.rel_path.to_string_lossy();
        manifest.push_str(&path_str);
        manifest.push('\n');
    }

    fs::write(manifest_path, manifest)
        .with_context(|| format!("Failed to write manifest file: {}", manifest_path.display()))?;

    debug!("Wrote manifest file: {} ({} entries)", manifest_path.display(), files.len());
    Ok(())
}

/// Write SHA256SUMS file (sha256sum format).
pub fn write_sha256sums_file(sums_path: &Path, files: &[FileMetadata]) -> Result<()> {
    let mut sums = String::new();
    for file in files {
        let path_str = file.rel_path.to_string_lossy();
        sums.push_str(&format!("{}  {}\n", file.sha256, path_str));
    }

    fs::write(sums_path, sums)
        .with_context(|| format!("Failed to write SHA256SUMS file: {}", sums_path.display()))?;

    debug!("Wrote SHA256SUMS file: {} ({} entries)", sums_path.display(), files.len());
    Ok(())
}

/// Calculate total size of all files.
pub fn calculate_total_size(files: &[FileMetadata]) -> u64 {
    files.iter().map(|f| f.size).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    #[test]
    fn test_manifest_generation() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();

        // Create test files
        let file1 = root.join("file1.txt");
        fs::write(&file1, "test content 1")?;

        let subdir = root.join("subdir");
        fs::create_dir_all(&subdir)?;
        let file2 = subdir.join("file2.txt");
        fs::write(&file2, "test content 2")?;

        let files = generate_manifest_and_sums(root, None)?;
        assert_eq!(files.len(), 2);

        // Check that paths are relative
        assert!(files.iter().any(|f| f.rel_path == PathBuf::from("file1.txt")));
        assert!(files.iter().any(|f| f.rel_path == PathBuf::from("subdir/file2.txt")));

        // Check that SHA256 hashes are present
        for file in &files {
            assert_eq!(file.sha256.len(), 64); // SHA256 hex is 64 chars
            assert!(file.size > 0);
        }

        Ok(())
    }

    #[test]
    fn test_write_manifest_file() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let manifest_path = temp_dir.path().join("MANIFEST.txt");

        let files = vec![
            FileMetadata {
                rel_path: PathBuf::from("file1.txt"),
                size: 100,
                mtime: "2024-01-01T00:00:00Z".to_string(),
                sha256: "abc123".repeat(10).chars().take(64).collect(),
            },
            FileMetadata {
                rel_path: PathBuf::from("subdir/file2.txt"),
                size: 200,
                mtime: "2024-01-02T00:00:00Z".to_string(),
                sha256: "def456".repeat(10).chars().take(64).collect(),
            },
        ];

        write_manifest_file(&manifest_path, &files)?;

        let content = fs::read_to_string(&manifest_path)?;
        assert!(content.contains("file1.txt"));
        assert!(content.contains("subdir/file2.txt"));

        Ok(())
    }

    #[test]
    fn test_write_sha256sums_file() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let sums_path = temp_dir.path().join("SHA256SUMS.txt");

        let files = vec![
            FileMetadata {
                rel_path: PathBuf::from("file1.txt"),
                size: 100,
                mtime: "2024-01-01T00:00:00Z".to_string(),
                sha256: "abc123".repeat(10).chars().take(64).collect(),
            },
        ];

        write_sha256sums_file(&sums_path, &files)?;

        let content = fs::read_to_string(&sums_path)?;
        assert!(content.contains("abc123"));
        assert!(content.contains("file1.txt"));

        Ok(())
    }

    #[test]
    fn test_calculate_total_size() {
        let files = vec![
            FileMetadata {
                rel_path: PathBuf::from("file1.txt"),
                size: 100,
                mtime: "2024-01-01T00:00:00Z".to_string(),
                sha256: "abc123".to_string(),
            },
            FileMetadata {
                rel_path: PathBuf::from("file2.txt"),
                size: 200,
                mtime: "2024-01-02T00:00:00Z".to_string(),
                sha256: "def456".to_string(),
            },
        ];

        assert_eq!(calculate_total_size(&files), 300);
    }
}

