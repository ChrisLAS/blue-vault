use anyhow::{Context, Result};
use std::path::Path;
use tracing::{debug, info};
use crate::commands;

/// Create an ISO image from a directory using xorriso.
pub fn create_iso(
    source_dir: &Path,
    output_iso: &Path,
    volume_label: &str,
    dry_run: bool,
) -> Result<()> {
    info!(
        "Creating ISO image: {} -> {} (volume: {})",
        source_dir.display(),
        output_iso.display(),
        volume_label
    );

    // Validate source directory
    crate::paths::validate_dir(source_dir)
        .context("Source directory validation failed")?;

    // Ensure output directory exists
    if let Some(parent) = output_iso.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Build xorriso command
    // Using mkisofs compatible mode for better compatibility
    let output_iso_str = output_iso.to_string_lossy().to_string();
    let source_dir_str = source_dir.to_string_lossy().to_string();
    let args = vec![
        "-as", "mkisofs",  // Use mkisofs compatible mode
        "-r",              // Rock Ridge (Unix file names and permissions)
        "-J",              // Joliet (Windows compatibility)
        "-V", volume_label, // Volume label
        "-o", &output_iso_str, // Output file
        &source_dir_str,        // Source directory
    ];

    let output = commands::execute_command("xorriso", &args, dry_run)?;

    if !output.success {
        anyhow::bail!(
            "xorriso failed: {}\n{}",
            output.stderr,
            output.stdout
        );
    }

    debug!("ISO image created: {}", output_iso.display());
    Ok(())
}

/// Get ISO file size in bytes.
pub fn get_iso_size(iso_path: &Path) -> Result<u64> {
    let metadata = std::fs::metadata(iso_path)
        .with_context(|| format!("Failed to read ISO metadata: {}", iso_path.display()))?;
    Ok(metadata.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    #[test]
    fn test_create_iso_dry_run() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let source = temp_dir.path().join("source");
        let output = temp_dir.path().join("output.iso");

        fs::create_dir_all(&source)?;
        fs::write(source.join("test.txt"), "test")?;

        // Should not fail in dry run mode
        create_iso(&source, &output, "TEST_LABEL", true)?;
        Ok(())
    }

    #[test]
    fn test_get_iso_size() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let iso_path = temp_dir.path().join("test.iso");
        fs::write(&iso_path, "test content")?;

        let size = get_iso_size(&iso_path)?;
        assert!(size > 0);
        Ok(())
    }
}

