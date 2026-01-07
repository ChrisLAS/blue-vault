use anyhow::Result;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};
use crate::commands;
use crate::dependencies;

/// Generate a QR code for a disc ID.
pub fn generate_qrcode(
    disc_id: &str,
    output_dir: &Path,
    format: QrCodeFormat,
    dry_run: bool,
) -> Result<PathBuf> {
    // Check if qrencode is available
    let qrencode_path_str = match dependencies::get_optional_command("qrencode") {
        Some(path) => path.to_string_lossy().to_string(),
        None => {
            warn!("qrencode not found, skipping QR code generation");
            return Err(anyhow::anyhow!("qrencode not available"));
        }
    };

    info!("Generating QR code for disc ID: {}", disc_id);

    // Ensure output directory exists
    std::fs::create_dir_all(output_dir)?;

    let extension = match format {
        QrCodeFormat::PNG => "png",
        QrCodeFormat::SVG => "svg",
        QrCodeFormat::ASCII => "txt",
    };

    let output_path = output_dir.join(format!("{}.{}", disc_id, extension));

    let output_path_str = output_path.to_string_lossy().to_string();
    let mut args = vec![String::new(); 4]; // Pre-allocate with placeholders
    
    match format {
        QrCodeFormat::PNG => {
            args[0] = "-t".to_string();
            args[1] = "PNG".to_string();
            args[2] = "-o".to_string();
            args[3] = output_path_str;
        }
        QrCodeFormat::SVG => {
            args[0] = "-t".to_string();
            args[1] = "SVG".to_string();
            args[2] = "-o".to_string();
            args[3] = output_path_str;
        }
        QrCodeFormat::ASCII => {
            args[0] = "-t".to_string();
            args[1] = "ANSI".to_string();
            args[2] = "-o".to_string();
            args[3] = output_path_str;
        }
    }
    
    let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let args_with_id: Vec<&str> = [args_str.as_slice(), &[disc_id]].concat();
    let output = commands::execute_command(qrencode_path_str.as_str(), args_with_id.as_slice(), dry_run)?;

    if !output.success {
        anyhow::bail!("qrencode failed: {}", output.stderr);
    }

    debug!("QR code generated: {}", output_path.display());
    Ok(output_path)
}

/// Generate and display ASCII QR code in terminal.
pub fn generate_ascii_qrcode(disc_id: &str, dry_run: bool) -> Result<String> {
    let qrencode_path = match dependencies::get_optional_command("qrencode") {
        Some(path) => path.to_string_lossy().to_string(),
        None => {
            return Err(anyhow::anyhow!("qrencode not available"));
        }
    };

    let args: &[&str] = &["-t", "ANSIUTF8", disc_id];

    if dry_run {
        println!("[DRY RUN] Would generate ASCII QR code for: {}", disc_id);
        return Ok(String::new());
    }

    let output = commands::execute_command_capture_stdout(qrencode_path.as_str(), args, dry_run)?;
    Ok(output)
}

#[derive(Debug, Clone, Copy)]
pub enum QrCodeFormat {
    PNG,
    SVG,
    ASCII,
}

impl QrCodeFormat {
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "png" => QrCodeFormat::PNG,
            "svg" => QrCodeFormat::SVG,
            _ => QrCodeFormat::PNG, // Default
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qrcode_format_from_extension() {
        assert!(matches!(
            QrCodeFormat::from_extension("png"),
            QrCodeFormat::PNG
        ));
        assert!(matches!(
            QrCodeFormat::from_extension("svg"),
            QrCodeFormat::SVG
        ));
    }
}

