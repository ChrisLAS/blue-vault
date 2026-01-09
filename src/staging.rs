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
    stage_files_with_progress(disc_root, source_folders, use_rsync, dry_run, None)
}

/// Stage files with progress callback.
pub fn stage_files_with_progress(
    disc_root: &Path,
    source_folders: &[PathBuf],
    use_rsync: bool,
    dry_run: bool,
    mut progress_callback: Option<Box<dyn FnMut(&str) + Send>>,
) -> Result<Vec<PathBuf>> {
    let archive_dir = disc_root.join("ARCHIVE");
    fs::create_dir_all(&archive_dir)?;

    let mut staged_paths = Vec::new();

    // Count total files and size for progress reporting
    let mut total_files = 0;
    let mut processed_files = 0;
    let mut total_size_bytes = 0u64;

    // First pass: count files and estimate total size
    for source in source_folders {
        if source.exists() && source.is_dir() {
            if let Ok(count) = count_files_and_size(source) {
                total_files += count.0;
                total_size_bytes += count.1;
            }
        }
    }

    if let Some(ref mut callback) = progress_callback {
        let size_mb = total_size_bytes / (1024 * 1024);
        callback(&format!("📁 Preparing to stage {} files ({}MB) from {} folders",
                         total_files, size_mb, source_folders.len()));
    }

    for (i, source) in source_folders.iter().enumerate() {
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

        if let Some(ref mut callback) = progress_callback {
            callback(&format!("📂 Staging folder {}/{}: {} ({} files processed so far)",
                             i + 1, source_folders.len(), folder_name, processed_files));
        }

        let dest = archive_dir.join(folder_name);

    // Enhanced staging with file-by-file progress
    if use_rsync {
        stage_with_rsync_progress(source, &dest, dry_run, &mut progress_callback, &mut processed_files)?;
    } else {
        stage_with_copy_progress(source, &dest, dry_run, &mut progress_callback, &mut processed_files)?;
    }

        staged_paths.push(dest);
    }

    if let Some(ref mut callback) = progress_callback {
        callback(&format!("✅ Staging complete: {} folders, {} files processed", staged_paths.len(), processed_files));
    }

    info!("Staged {} folders, {} files", staged_paths.len(), processed_files);
    Ok(staged_paths)
}

/// Count files and total size in a directory tree.
fn count_files_and_size(dir: &Path) -> Result<(usize, u64)> {
    let mut file_count = 0;
    let mut total_size = 0u64;

    fn walk_dir(path: &Path, file_count: &mut usize, total_size: &mut u64) -> Result<()> {
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries {
                if let Ok(entry) = entry {
                    let path = entry.path();
                    if path.is_file() {
                        *file_count += 1;
                        if let Ok(metadata) = entry.metadata() {
                            *total_size += metadata.len();
                        }
                    } else if path.is_dir() {
                        walk_dir(&path, file_count, total_size)?;
                    }
                }
            }
        }
        Ok(())
    }

    walk_dir(dir, &mut file_count, &mut total_size)?;
    Ok((file_count, total_size))
}

/// Stage files using rsync with progress reporting.
fn stage_with_rsync_progress(
    source: &Path,
    dest: &Path,
    dry_run: bool,
    progress_callback: &mut Option<Box<dyn FnMut(&str) + Send>>,
    processed_files: &mut usize,
) -> Result<usize> {
    debug!(
        "Staging with rsync: {} -> {} (dry_run: {})",
        source.display(),
        dest.display(),
        dry_run
    );

    // For rsync, we can't easily track individual file progress,
    // so we'll just show the folder being processed
    let source_str = format!("{}/", source.display());
    let dest_str = dest.display().to_string();
    let args = vec!["-av", "--delete", &source_str, &dest_str];

    if dry_run {
        info!("[DRY RUN] Would run: rsync {}", args.join(" "));
        // Estimate files processed for dry run
        if let Ok((count, _)) = count_files_and_size(source) {
            *processed_files += count;
        }
        return Ok(0);
    }

    if let Some(ref mut callback) = progress_callback {
        callback(&format!("🔄 Running rsync: {} -> {}", source.display(), dest.display()));
    }

    crate::commands::execute_command("rsync", &args, dry_run).context("rsync failed")?;

    // Count files that were actually processed
    let file_count = if let Ok((count, _)) = count_files_and_size(dest) {
        count
    } else {
        0
    };
    *processed_files += file_count;

    Ok(file_count)
}

/// Stage files using copy with detailed progress reporting.
fn stage_with_copy_progress(
    source: &Path,
    dest: &Path,
    dry_run: bool,
    progress_callback: &mut Option<Box<dyn FnMut(&str) + Send>>,
    processed_files: &mut usize,
) -> Result<usize> {
    debug!(
        "Staging with copy: {} -> {} (dry_run: {})",
        source.display(),
        dest.display(),
        dry_run
    );

    if dry_run {
        info!("[DRY RUN] Would copy: {} -> {}", source.display(), dest.display());
        // Estimate files processed for dry run
        if let Ok((count, _)) = count_files_and_size(source) {
            *processed_files += count;
        }
        return Ok(0);
    }

    fs::create_dir_all(dest)?;

    let mut files_copied = 0;

    fn copy_recursive(
        src: &Path,
        dst: &Path,
        progress_callback: &mut Option<Box<dyn FnMut(&str) + Send>>,
        files_copied: &mut usize,
    ) -> Result<()> {
        if let Ok(entries) = fs::read_dir(src) {
            for entry in entries {
                if let Ok(entry) = entry {
                    let src_path = entry.path();
                    let file_name = src_path.file_name().unwrap_or_default();
                    let dst_path = dst.join(file_name);

                    if src_path.is_file() {
                        // Copy file
                        fs::copy(&src_path, &dst_path)?;
                        *files_copied += 1;

                        // Report progress for larger files or every 10 files
                        if *files_copied % 10 == 0 || src_path.metadata()?.len() > 10 * 1024 * 1024 {
                            if let Some(ref mut callback) = progress_callback {
                                let size_mb = src_path.metadata()?.len() / (1024 * 1024);
                                callback(&format!("📄 Copied: {} ({}MB) - {} files total",
                                                 file_name.to_string_lossy(), size_mb, files_copied));
                            }
                        }
                    } else if src_path.is_dir() {
                        // Create directory and recurse
                        fs::create_dir_all(&dst_path)?;
                        copy_recursive(&src_path, &dst_path, progress_callback, files_copied)?;
                    }
                }
            }
        }
        Ok(())
    }

    if let Some(ref mut callback) = progress_callback {
        callback(&format!("📋 Starting copy: {} -> {}", source.display(), dest.display()));
    }

    copy_recursive(source, dest, progress_callback, &mut files_copied)?;
    *processed_files += files_copied;

    Ok(files_copied)
}

/// Stage files using rsync.
fn stage_with_rsync(source: &Path, dest: &Path, dry_run: bool) -> Result<()> {
    debug!(
        "Staging with rsync: {} -> {} (dry_run: {})",
        source.display(),
        dest.display(),
        dry_run
    );

    let source_str = format!("{}/", source.display());
    let dest_str = dest.display().to_string();
    let args = vec!["-av", "--delete", &source_str, &dest_str];

    if dry_run {
        info!("[DRY RUN] Would run: rsync {}", args.join(" "));
        return Ok(());
    }

    crate::commands::execute_command("rsync", &args, dry_run).context("rsync failed")?;

    Ok(())
}

/// Stage files using standard copy.
fn stage_with_copy(source: &Path, dest: &Path, dry_run: bool) -> Result<()> {
    debug!(
        "Staging with copy: {} -> {}",
        source.display(),
        dest.display()
    );

    if dry_run {
        info!(
            "[DRY RUN] Would copy: {} -> {}",
            source.display(),
            dest.display()
        );
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
            fs::copy(&path, &dest_path).with_context(|| {
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
        return Ok(fs::metadata(path)
            .with_context(|| format!("Failed to read file metadata: {}", path.display()))?
            .len());
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
    use std::fs;
    use tempfile::TempDir;

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

