use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// Stage files from source folders to disc layout in staging directory.
pub fn stage_files(
    disc_root: &Path,
    source_folders: &[PathBuf],
    use_rsync: bool,
    dry_run: bool,
) -> Result<Vec<PathBuf>> {
    let archive_dir = disc_root.join("ARCHIVE");
    fs::create_dir_all(&archive_dir)?;

    let mut staged_paths = Vec::new();

    for source in source_folders {
        if !source.exists() {
            warn!("Source folder does not exist: {}", source.display());
            continue;
        }

        if !source.is_dir() {
            warn!("Source is not a directory: {}", source.display());
            continue;
        }

        let folder_name = source
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        let dest = archive_dir.join(folder_name);

        if use_rsync {
            stage_with_rsync(source, &dest, dry_run)?;
        } else {
            stage_with_copy(source, &dest, dry_run)?;
        }

        staged_paths.push(dest);
    }

    info!("Staged {} folders", staged_paths.len());
    Ok(staged_paths)
}

/// Stage files using rsync.
fn stage_with_rsync(source: &Path, dest: &Path, dry_run: bool) -> Result<()> {
    debug!("Staging with rsync: {} -> {}", source.display(), dest.display());

    let source_str = format!("{}/", source.display());
    let dest_str = dest.display().to_string();
    let args = vec![
        "-av",
        "--delete",
        &source_str,
        &dest_str,
    ];

    if dry_run {
        println!("[DRY RUN] Would run: rsync {}", args.join(" "));
        return Ok(());
    }

    crate::commands::execute_command("rsync", &args, dry_run)
        .context("rsync failed")?;

    Ok(())
}

/// Stage files using standard copy.
fn stage_with_copy(source: &Path, dest: &Path, dry_run: bool) -> Result<()> {
    debug!("Staging with copy: {} -> {}", source.display(), dest.display());

    if dry_run {
        println!("[DRY RUN] Would copy: {} -> {}", source.display(), dest.display());
        return Ok(());
    }

    copy_directory_recursive(source, dest)?;
    Ok(())
}

/// Recursively copy directory.
fn copy_directory_recursive(source: &Path, dest: &Path) -> Result<()> {
    fs::create_dir_all(dest)?;

    let entries = fs::read_dir(source)
        .with_context(|| format!("Failed to read source directory: {}", source.display()))?;

    for entry in entries {
        let entry = entry.context("Failed to read directory entry")?;
        let path = entry.path();
        let file_name = entry.file_name();
        let dest_path = dest.join(&file_name);

        if path.is_dir() {
            copy_directory_recursive(&path, &dest_path)?;
        } else {
            fs::copy(&path, &dest_path)
                .with_context(|| {
                    format!(
                        "Failed to copy file: {} -> {}",
                        path.display(),
                        dest_path.display()
                    )
                })?;
        }
    }

    Ok(())
}

/// Calculate total size of files in a directory.
pub fn calculate_directory_size(path: &Path) -> Result<u64> {
    let mut total = 0u64;

    if path.is_file() {
        return Ok(
            fs::metadata(path)
                .with_context(|| format!("Failed to read file metadata: {}", path.display()))?
                .len(),
        );
    }

    let entries = fs::read_dir(path)
        .with_context(|| format!("Failed to read directory: {}", path.display()))?;

    for entry in entries {
        let entry = entry.context("Failed to read directory entry")?;
        let path = entry.path();
        total += calculate_directory_size(&path)?;
    }

    Ok(total)
}

/// Check if total size exceeds capacity.
pub fn check_capacity(source_folders: &[PathBuf], capacity_bytes: u64) -> Result<(u64, bool)> {
    let mut total_size = 0u64;

    for folder in source_folders {
        if folder.exists() {
            total_size += calculate_directory_size(folder)?;
        }
    }

    let exceeds = total_size > capacity_bytes;
    Ok((total_size, exceeds))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    #[test]
    fn test_stage_with_copy() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let source = temp_dir.path().join("source");
        let dest = temp_dir.path().join("dest");

        fs::create_dir_all(&source)?;
        fs::write(source.join("file.txt"), "test content")?;

        stage_with_copy(&source, &dest, false)?;

        assert!(dest.join("file.txt").exists());
        let content = fs::read_to_string(dest.join("file.txt"))?;
        assert_eq!(content, "test content");

        Ok(())
    }

    #[test]
    fn test_calculate_directory_size() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let test_dir = temp_dir.path();

        fs::write(test_dir.join("file1.txt"), "content1")?;
        fs::write(test_dir.join("file2.txt"), "content2")?;

        let size = calculate_directory_size(test_dir)?;
        assert!(size >= 14); // At least the content size

        Ok(())
    }

    #[test]
    fn test_check_capacity() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let test_dir = temp_dir.path();

        fs::write(test_dir.join("file.txt"), "test")?;

        let folders = vec![test_dir.to_path_buf()];
        let (size, exceeds) = check_capacity(&folders, 1000)?;

        assert!(size < 1000);
        assert!(!exceeds);

        Ok(())
    }
}

