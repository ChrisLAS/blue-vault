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
#[allow(dead_code)]
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
#[allow(dead_code)]
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
pub fn copy_directory_recursive(source: &Path, dest: &Path) -> Result<()> {
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

/// Represents a directory entry with size information for layout planning
#[derive(Debug, Clone)]
pub struct DirectoryEntry {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub is_file: bool,
    pub children: Vec<DirectoryEntry>,
}

/// Analyze directory structure for multi-disc planning
pub fn analyze_directory_structure(root_path: &Path) -> Result<DirectoryEntry> {
    fn analyze_recursive(path: &Path) -> Result<DirectoryEntry> {
        let metadata = fs::metadata(path)
            .with_context(|| format!("Failed to read metadata for: {}", path.display()))?;

        if metadata.is_file() {
            return Ok(DirectoryEntry {
                path: path.to_path_buf(),
                size_bytes: metadata.len(),
                is_file: true,
                children: Vec::new(),
            });
        }

        let mut total_size = 0u64;
        let mut children = Vec::new();

        let entries = fs::read_dir(path)
            .with_context(|| format!("Failed to read directory: {}", path.display()))?;

        for entry in entries {
            let entry = entry.context("Failed to read directory entry")?;
            let child_path = entry.path();
            let child_entry = analyze_recursive(&child_path)?;
            total_size += child_entry.size_bytes;
            children.push(child_entry);
        }

        // Sort children by size (largest first) for better packing
        children.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));

        Ok(DirectoryEntry {
            path: path.to_path_buf(),
            size_bytes: total_size,
            is_file: false,
            children,
        })
    }

    analyze_recursive(root_path)
}

/// Plan disc layout to minimize directory splits across discs
pub fn plan_disc_layout(
    source_folders: &[PathBuf],
    disc_capacity_bytes: u64,
) -> Result<Vec<DiscPlan>> {
    plan_disc_layout_with_progress(source_folders, disc_capacity_bytes, |_| {})
}

/// Plan disc layout with progress callback for UI feedback
pub fn plan_disc_layout_with_progress<F>(
    source_folders: &[PathBuf],
    disc_capacity_bytes: u64,
    mut progress_callback: F,
) -> Result<Vec<DiscPlan>>
where
    F: FnMut(&str) -> (),
{
    let mut all_entries = Vec::new();

    progress_callback("🔍 Analyzing source directories...");

    // Analyze all source directories and flatten their children as packable entries
    for (i, folder) in source_folders.iter().enumerate() {
        if folder.exists() {
            progress_callback(&format!("📂 Analyzing folder {}/{}: {}", i + 1, source_folders.len(), folder.display()));
            let structure = analyze_directory_structure(folder)?;

            // If this is a directory with children, add the children as packable entries
            // Otherwise, add the structure itself
            if !structure.is_file && !structure.children.is_empty() {
                all_entries.extend(structure.children);
            } else {
                all_entries.push(structure);
            }
        }
    }

    progress_callback(&format!("📊 Found {} items to pack across discs", all_entries.len()));

    // Sort using intelligent bin-packing strategy
    progress_callback("🧠 Sorting items with intelligent bin-packing algorithm...");
    all_entries = sort_for_bin_packing(all_entries, disc_capacity_bytes);

    let mut discs = Vec::new();
    let current_disc = DiscPlan::new(discs.len() + 1, disc_capacity_bytes);
    discs.push(current_disc);

    progress_callback("🎯 Starting disc packing algorithm...");

    // Use a greedy bin-packing approach that prefers keeping directories together
    for (i, entry) in all_entries.iter().enumerate() {
        if i % 50 == 0 && i > 0 {
            progress_callback(&format!("📦 Packed {}/{} items ({} discs so far)", i, all_entries.len(), discs.len()));
        }

        if !try_add_to_disc(&mut discs, &entry, disc_capacity_bytes) {
            // If we couldn't fit the entire entry, try to fit its children individually
            if !entry.is_file && !entry.children.is_empty() {
                // Sort children by size (largest first) for better packing
                let mut children = entry.children.clone();
                children.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));

                for child in children {
                    if !try_add_to_disc(&mut discs, &child, disc_capacity_bytes) {
                        // If child doesn't fit anywhere, create a new disc for it
                        let mut new_disc = DiscPlan::new(discs.len() + 1, disc_capacity_bytes);
                        if !new_disc.try_add_entry(&child) {
                            // If child still doesn't fit, split it
                            split_directory_across_discs(&mut discs, child, disc_capacity_bytes);
                        } else {
                            discs.push(new_disc);
                        }
                    }
                }
            } else {
                // Entry is a file or has no children - try to put it on a new disc
                let mut new_disc = DiscPlan::new(discs.len() + 1, disc_capacity_bytes);
                if !new_disc.try_add_entry(&entry) {
                    // If it still doesn't fit, we have a problem (file too big)
                    warn!("Entry too large for any disc: {} ({} bytes)", entry.path.display(), entry.size_bytes);
                } else {
                    discs.push(new_disc);
                }
            }
        }
    }

    progress_callback(&format!("✅ Planning complete! Created {} discs for {} items", discs.len(), all_entries.len()));
    Ok(discs)
}

/// Try to add an entry to existing discs using intelligent bin-packing
/// Uses Best Fit Decreasing (BFD) algorithm for optimal space utilization
fn try_add_to_disc(discs: &mut Vec<DiscPlan>, entry: &DirectoryEntry, disc_capacity: u64) -> bool {
    // First try to add to existing discs without splitting using Best Fit
    if let Some(best_disc_idx) = find_best_fit_disc(discs, entry, disc_capacity) {
        let disc = &mut discs[best_disc_idx];
        if disc.try_add_entry(entry) {
            return true;
        }
    }

    // If that didn't work, try splitting if it's a directory
    if !entry.is_file {
        if let Some(best_disc_idx) = find_best_fit_for_partial_directory(discs, entry, disc_capacity) {
            let disc = &mut discs[best_disc_idx];
            if disc.try_add_partial_directory(entry, disc_capacity) {
                return true;
            }
        }
    }

    false
}

/// Find the best disc to fit an entry using Best Fit Decreasing algorithm
/// Returns the index of the disc with least remaining space that can fit the item
fn find_best_fit_disc(discs: &[DiscPlan], entry: &DirectoryEntry, _disc_capacity: u64) -> Option<usize> {
    let mut best_fit_idx = None;
    let mut best_wasted_space = u64::MAX;

    for (i, disc) in discs.iter().enumerate() {
        let remaining_space = disc.capacity_bytes.saturating_sub(disc.used_bytes);
        if entry.size_bytes <= remaining_space {
            let wasted_space = remaining_space - entry.size_bytes;
            // Prefer discs with less wasted space (tighter fit)
            if wasted_space < best_wasted_space {
                best_wasted_space = wasted_space;
                best_fit_idx = Some(i);
            }
        }
    }

    best_fit_idx
}

/// Find the best disc for partial directory placement
fn find_best_fit_for_partial_directory(discs: &[DiscPlan], entry: &DirectoryEntry, disc_capacity: u64) -> Option<usize> {
    let mut best_fit_idx = None;
    let mut best_utilization = 0.0;

    for (i, disc) in discs.iter().enumerate() {
        let available_space = disc.capacity_bytes - disc.used_bytes;
        if available_space < disc_capacity / 10 {
            // Don't bother with less than 10% of disc space
            continue;
        }

        // Calculate potential utilization if we add part of this directory
        let mut potential_size = 0u64;
        for child in &entry.children {
            if potential_size + child.size_bytes <= available_space {
                potential_size += child.size_bytes;
            } else {
                break;
            }
        }

        if potential_size > 0 {
            let utilization = potential_size as f64 / available_space as f64;
            // Prefer higher utilization
            if utilization > best_utilization {
                best_utilization = utilization;
                best_fit_idx = Some(i);
            }
        }
    }

    best_fit_idx
}

/// Sort entries for optimal bin-packing using multiple heuristics
fn sort_for_bin_packing(entries: Vec<DirectoryEntry>, disc_capacity: u64) -> Vec<DirectoryEntry> {
    // First, separate files and directories
    let (files, directories): (Vec<_>, Vec<_>) = entries.into_iter()
        .partition(|e| e.is_file);

    // Sort files by size (largest first) - classic FFD for files
    let mut sorted_files = files;
    sorted_files.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));

    // For directories, use a more sophisticated approach
    let mut sorted_directories = directories;
    sorted_directories = sort_directories_for_packing(sorted_directories, disc_capacity);

    // Combine: directories first (for better cohesion), then files
    sorted_directories.extend(sorted_files);
    sorted_directories
}

/// Sort directories using multiple bin-packing heuristics
fn sort_directories_for_packing(directories: Vec<DirectoryEntry>, disc_capacity: u64) -> Vec<DirectoryEntry> {
    if directories.is_empty() {
        return directories;
    }

    // Calculate packing scores for each directory
    let mut scored_directories: Vec<(DirectoryEntry, f64)> = directories.into_iter()
        .map(|dir| {
            let score = calculate_packing_score(&dir, disc_capacity);
            (dir, score)
        })
        .collect();

    // Sort by score (higher scores first - better packing candidates)
    scored_directories.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    scored_directories.into_iter().map(|(dir, _)| dir).collect()
}

/// Calculate a packing score for a directory based on multiple heuristics
fn calculate_packing_score(dir: &DirectoryEntry, disc_capacity: u64) -> f64 {
    let mut score = 0.0;

    // Heuristic 1: Size efficiency (prefer directories that fit well on discs)
    // Directories that are 40-80% of disc capacity score highest
    let size_ratio = dir.size_bytes as f64 / disc_capacity as f64;
    if size_ratio >= 0.4 && size_ratio <= 0.8 {
        score += 100.0; // Perfect fit bonus
    } else if size_ratio > 0.8 {
        score += 50.0; // Large but may need splitting
    } else if size_ratio >= 0.2 {
        score += 75.0; // Medium size, flexible
    } else {
        score += 25.0; // Small, can fill gaps
    }

    // Heuristic 2: Child distribution (prefer directories with varied child sizes)
    // This helps with better bin utilization
    if !dir.children.is_empty() {
        let child_sizes: Vec<f64> = dir.children.iter()
            .map(|c| c.size_bytes as f64)
            .collect();

        if child_sizes.len() > 1 {
            let avg_size = child_sizes.iter().sum::<f64>() / child_sizes.len() as f64;
            let variance = child_sizes.iter()
                .map(|&size| (size - avg_size).powi(2))
                .sum::<f64>() / child_sizes.len() as f64;

            // Higher variance means more varied sizes = better packing potential
            let normalized_variance = variance.sqrt() / avg_size;
            score += normalized_variance * 20.0;
        }
    }

    // Heuristic 3: Directory depth (prefer shallower structures)
    // Deeper directory structures are harder to split efficiently
    let depth_penalty = (calculate_directory_depth(dir) as f64) * 5.0;
    score -= depth_penalty;

    // Heuristic 4: Large child bonus (directories with large children get priority)
    // Large children should be placed first in bin-packing
    if let Some(largest_child) = dir.children.iter()
        .map(|c| c.size_bytes)
        .max() {
        let large_child_ratio = largest_child as f64 / disc_capacity as f64;
        if large_child_ratio > 0.3 {
            score += large_child_ratio * 30.0;
        }
    }

    score
}

/// Calculate the maximum depth of a directory structure
fn calculate_directory_depth(entry: &DirectoryEntry) -> usize {
    if entry.children.is_empty() {
        0
    } else {
        1 + entry.children.iter()
            .map(calculate_directory_depth)
            .max()
            .unwrap_or(0)
    }
}

/// Calculate how well a directory entry fits in available space
/// Higher scores mean better fits
fn calculate_fit_score(entry: &DirectoryEntry, available_space: u64) -> f64 {
    if entry.size_bytes > available_space {
        return 0.0; // Can't fit at all
    }

    let utilization = entry.size_bytes as f64 / available_space as f64;
    let waste = available_space - entry.size_bytes;

    // Prefer high utilization (close to 1.0) but penalize very small waste
    // This avoids leaving tiny unusable gaps
    let mut score = utilization * 100.0;

    // Small waste penalty (gaps under 1MB are heavily penalized)
    if waste > 0 && waste < 1024 * 1024 {
        score -= 50.0;
    } else if waste > 0 && waste < 10 * 1024 * 1024 {
        score -= 20.0;
    }

    // Bonus for items that use significant portion of space
    if utilization > 0.8 {
        score += 20.0;
    } else if utilization > 0.6 {
        score += 10.0;
    }

    score
}

/// Split a large directory across multiple discs
fn split_directory_across_discs(
    discs: &mut Vec<DiscPlan>,
    entry: DirectoryEntry,
    disc_capacity: u64,
) {
    if entry.is_file {
        // For files that are too big (shouldn't happen with Blu-ray, but handle gracefully)
        // This would require file splitting, which we're avoiding per requirements
        warn!("File too large for any disc: {} ({} bytes)", entry.path.display(), entry.size_bytes);
        return;
    }

    // Sort children using intelligent bin-packing strategy
    let mut remaining_children = entry.children;
    remaining_children = sort_for_bin_packing(remaining_children, disc_capacity);

    let mut part_num = 1;
    let dir_name = entry.path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");

    while !remaining_children.is_empty() {
        // Find or create a disc with space
        let disc_idx = discs.iter().position(|d| d.used_bytes < disc_capacity)
            .unwrap_or_else(|| {
                discs.push(DiscPlan::new(discs.len() + 1, disc_capacity));
                discs.len() - 1
            });

        let disc = &mut discs[disc_idx];

        // Create a split directory entry for this disc
        let mut split_children = Vec::new();
        let mut split_size = 0u64;

        // Try to fit as many children as possible
        remaining_children.retain(|child| {
            if split_size + child.size_bytes <= disc_capacity - disc.used_bytes {
                split_size += child.size_bytes;
                split_children.push(child.clone());
                false // Remove from remaining
            } else {
                true // Keep for next disc
            }
        });

        if !split_children.is_empty() {
            // Create the split directory path
            let split_dir_name = format!("{}_part{}", dir_name, part_num);
            let split_path = entry.path.with_file_name(split_dir_name);

            let split_entry = DirectoryEntry {
                path: split_path,
                size_bytes: split_size,
                is_file: false,
                children: split_children,
            };

            disc.add_entry(split_entry);
            part_num += 1;
        } else {
            // No more children could fit, avoid infinite loop
            break;
        }
    }
}

/// Represents a planned disc with its contents
#[derive(Debug, Clone)]
pub struct DiscPlan {
    pub disc_number: usize,
    pub capacity_bytes: u64,
    pub used_bytes: u64,
    pub entries: Vec<DirectoryEntry>,
    pub split_directories: Vec<String>, // Names of directories split across discs
}

impl DiscPlan {
    pub fn new(disc_number: usize, capacity_bytes: u64) -> Self {
        Self {
            disc_number,
            capacity_bytes,
            used_bytes: 0,
            entries: Vec::new(),
            split_directories: Vec::new(),
        }
    }

    /// Try to add an entire entry to this disc
    pub fn try_add_entry(&mut self, entry: &DirectoryEntry) -> bool {
        if self.used_bytes + entry.size_bytes <= self.capacity_bytes {
            self.used_bytes += entry.size_bytes;
            self.entries.push(entry.clone());
            true
        } else {
            false
        }
    }

    /// Try to add part of a directory to this disc
    pub fn try_add_partial_directory(&mut self, entry: &DirectoryEntry, max_size: u64) -> bool {
        if entry.is_file {
            return false;
        }

        let available_space = self.capacity_bytes - self.used_bytes;
        if available_space < max_size / 10 {
            // Don't bother with less than 10% of disc space
            return false;
        }

    // Try to fit some children using intelligent selection
    let mut added_size = 0u64;
    let mut added_children = Vec::new();

    // Sort children by packing priority for this specific disc space
    let mut sorted_children = entry.children.clone();
    sorted_children.sort_by(|a, b| {
        // Prefer children that fit well in remaining space
        let a_fit_score = calculate_fit_score(a, available_space - added_size);
        let b_fit_score = calculate_fit_score(b, available_space - added_size);
        b_fit_score.partial_cmp(&a_fit_score).unwrap_or(std::cmp::Ordering::Equal)
    });

    for child in sorted_children {
        if added_size + child.size_bytes <= available_space {
            added_size += child.size_bytes;
            added_children.push(child);
        } else {
            // If this child doesn't fit, check if smaller children might
            // This prevents leaving small gaps
            break;
        }
    }

        if !added_children.is_empty() {
            // Create a partial directory entry
            let partial_entry = DirectoryEntry {
                path: entry.path.clone(),
                size_bytes: added_size,
                is_file: false,
                children: added_children,
            };

            self.used_bytes += added_size;
            self.entries.push(partial_entry);
            self.split_directories.push(entry.path.display().to_string());
            true
        } else {
            false
        }
    }

    /// Force add an entry (used internally after planning)
    pub fn add_entry(&mut self, entry: DirectoryEntry) {
        self.used_bytes += entry.size_bytes;
        self.entries.push(entry);
    }

    /// Get utilization percentage
    pub fn utilization_percent(&self) -> f64 {
        (self.used_bytes as f64 / self.capacity_bytes as f64) * 100.0
    }
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

    #[test]
    fn test_analyze_directory_structure() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let root_dir = temp_dir.path().join("root");
        fs::create_dir_all(&root_dir)?;

        // Create test structure:
        // root/
        //   file1.txt (10 bytes)
        //   subdir/
        //     file2.txt (15 bytes)
        //   another_file.txt (20 bytes)

        fs::write(root_dir.join("file1.txt"), "0123456789")?; // 10 bytes
        fs::create_dir_all(root_dir.join("subdir"))?;
        fs::write(root_dir.join("subdir").join("file2.txt"), "012345678901234")?; // 15 bytes
        fs::write(root_dir.join("another_file.txt"), "01234567890123456789")?; // 20 bytes

        let structure = analyze_directory_structure(&root_dir)?;

        assert_eq!(structure.size_bytes, 45); // 10 + 15 + 20
        assert!(!structure.is_file);
        assert_eq!(structure.children.len(), 3); // 2 files + 1 directory

        // Check that children are sorted by size (largest first)
        assert!(structure.children[0].size_bytes >= structure.children[1].size_bytes);
        assert!(structure.children[1].size_bytes >= structure.children[2].size_bytes);

        Ok(())
    }

    #[test]
    fn test_disc_plan_basic() {
        let capacity = 100 * 1024 * 1024; // 100MB
        let mut plan = DiscPlan::new(1, capacity);

        let entry = DirectoryEntry {
            path: PathBuf::from("/test/dir"),
            size_bytes: 50 * 1024 * 1024, // 50MB
            is_file: false,
            children: Vec::new(),
        };

        assert!(plan.try_add_entry(&entry));
        assert_eq!(plan.used_bytes, 50 * 1024 * 1024);
        assert_eq!(plan.entries.len(), 1);
        assert!((plan.utilization_percent() - 50.0).abs() < 0.1);
    }

    #[test]
    fn test_disc_plan_capacity_exceeded() {
        let capacity = 50 * 1024 * 1024; // 50MB
        let mut plan = DiscPlan::new(1, capacity);

        let entry = DirectoryEntry {
            path: PathBuf::from("/test/dir"),
            size_bytes: 100 * 1024 * 1024, // 100MB (too big)
            is_file: false,
            children: Vec::new(),
        };

        assert!(!plan.try_add_entry(&entry));
        assert_eq!(plan.used_bytes, 0);
        assert_eq!(plan.entries.len(), 0);
    }

    #[test]
    fn test_plan_disc_layout_single_disc() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let source_dir = temp_dir.path().join("source");
        fs::create_dir_all(&source_dir)?;

        // Create files totaling 30MB (should fit on one 100MB disc)
        fs::write(source_dir.join("file1.txt"), vec![0u8; 10 * 1024 * 1024])?; // 10MB
        fs::write(source_dir.join("file2.txt"), vec![0u8; 20 * 1024 * 1024])?; // 20MB

        let disc_capacity = 100 * 1024 * 1024; // 100MB
        let plans = plan_disc_layout(&[source_dir], disc_capacity)?;

        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].disc_number, 1);
        assert!(plans[0].used_bytes > 0);
        assert!(plans[0].used_bytes <= disc_capacity);

        Ok(())
    }

    #[test]
    fn test_plan_disc_layout_multiple_discs() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let source_dir = temp_dir.path().join("source");
        fs::create_dir_all(&source_dir)?;

        // Create large directories that will require multiple discs
        let big_dir1 = source_dir.join("bigdir1");
        let big_dir2 = source_dir.join("bigdir2");
        fs::create_dir_all(&big_dir1)?;
        fs::create_dir_all(&big_dir2)?;

        // Create files totaling 250MB across two directories
        fs::write(big_dir1.join("file1.txt"), vec![0u8; 100 * 1024 * 1024])?; // 100MB
        fs::write(big_dir1.join("file2.txt"), vec![0u8; 50 * 1024 * 1024])?;  // 50MB
        fs::write(big_dir2.join("file3.txt"), vec![0u8; 80 * 1024 * 1024])?;  // 80MB
        fs::write(big_dir2.join("file4.txt"), vec![0u8; 20 * 1024 * 1024])?;  // 20MB

        let disc_capacity = 150 * 1024 * 1024; // 150MB discs

        let plans = plan_disc_layout(&[source_dir], disc_capacity)?;

        assert!(plans.len() >= 2); // Should need at least 2 discs for 250MB

        // Check that total used space is reasonable
        let total_used: u64 = plans.iter().map(|p| p.used_bytes).sum();
        assert_eq!(total_used, 250 * 1024 * 1024);

        // Check that no disc exceeds capacity
        for plan in &plans {
            assert!(plan.used_bytes <= disc_capacity);
        }

        Ok(())
    }
}

