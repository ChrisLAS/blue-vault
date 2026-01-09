use anyhow::Result;
use std::path::PathBuf;
use tracing::{debug, info, warn};

/// Check if a command is available in PATH.
pub fn check_command(command: &str) -> Option<PathBuf> {
    which::which(command).ok()
}

/// Required dependencies for the application.
pub const REQUIRED_COMMANDS: &[&str] = &[
    "xorriso", // For ISO creation and burning
    "sha256sum",
    "mount",
    "umount",
];

/// Optional dependencies.
pub const OPTIONAL_COMMANDS: &[&str] = &["qrencode", "rsync", "mc"];

/// Check all dependencies and return missing required ones.
pub fn check_dependencies() -> DependencyStatus {
    let mut missing_required = Vec::new();
    let mut missing_optional = Vec::new();
    let mut found_optional = Vec::new();

    // Check required commands
    for cmd in REQUIRED_COMMANDS {
        match check_command(cmd) {
            Some(path) => {
                debug!("Found required command: {} at {}", cmd, path.display());
            }
            None => {
                warn!("Missing required command: {}", cmd);
                missing_required.push(cmd.to_string());
            }
        }
    }

    // Check optional commands
    for cmd in OPTIONAL_COMMANDS {
        match check_command(cmd) {
            Some(path) => {
                debug!("Found optional command: {} at {}", cmd, path.display());
                found_optional.push((cmd.to_string(), path));
            }
            None => {
                debug!("Missing optional command: {} (not critical)", cmd);
                missing_optional.push(cmd.to_string());
            }
        }
    }

    DependencyStatus {
        missing_required,
        missing_optional,
        found_optional,
    }
}

/// Verify all required dependencies are present.
pub fn verify_dependencies() -> Result<()> {
    let status = check_dependencies();

    if !status.missing_required.is_empty() {
        let mut error_msg = format!(
            "Missing required dependencies: {}\n",
            status.missing_required.join(", ")
        );
        error_msg.push_str("\nPlease install the missing tools:\n");

        for cmd in &status.missing_required {
            match installation_hint(cmd) {
                Some(hint) => error_msg.push_str(&format!("  {}: {}\n", cmd, hint)),
                None => error_msg.push_str(&format!("  {}: Please install this tool\n", cmd)),
            }
        }

        anyhow::bail!("{}", error_msg);
    }

    info!("All required dependencies are available");
    Ok(())
}

/// Get installation hints for common Linux distributions.
fn installation_hint(command: &str) -> Option<&'static str> {
    match command {
        "xorriso" => Some("sudo apt install xorriso (Debian/Ubuntu) or sudo dnf install xorriso (Fedora/RHEL)"),
        "growisofs" => Some("sudo apt install dvd+rw-tools (Debian/Ubuntu) or sudo dnf install dvd+rw-tools (Fedora/RHEL)"),
        "sha256sum" => Some("Usually included in coreutils, try: sudo apt install coreutils"),
        "mount" => Some("Usually included in util-linux, try: sudo apt install util-linux"),
        "umount" => Some("Usually included in util-linux, try: sudo apt install util-linux"),
        "qrencode" => Some("sudo apt install qrencode (Debian/Ubuntu) or sudo dnf install qrencode (Fedora/RHEL)"),
        "rsync" => Some("sudo apt install rsync (Debian/Ubuntu) or sudo dnf install rsync (Fedora/RHEL)"),
        "mc" => Some("sudo apt install mc (Debian/Ubuntu) or sudo dnf install mc (Fedora/RHEL)"),
        _ => None,
    }
}

/// Get the path to an optional command, or None if not found.
pub fn get_optional_command(command: &str) -> Option<PathBuf> {
    check_command(command)
}

#[derive(Debug, Clone)]
pub struct DependencyStatus {
    pub missing_required: Vec<String>,
    pub missing_optional: Vec<String>,
    pub found_optional: Vec<(String, PathBuf)>,
}

impl DependencyStatus {
    /// Check if all required dependencies are present.
    pub fn all_required_present(&self) -> bool {
        self.missing_required.is_empty()
    }

    /// Print a summary of dependency status.
    pub fn print_summary(&self) {
        if self.missing_required.is_empty() {
            println!("✓ All required dependencies are available");
        } else {
            println!("✗ Missing required dependencies:");
            for cmd in &self.missing_required {
                println!("  - {}", cmd);
                if let Some(hint) = installation_hint(cmd) {
                    println!("    Hint: {}", hint);
                }
            }
        }

        if !self.found_optional.is_empty() {
            println!("\nOptional dependencies found:");
            for (cmd, path) in &self.found_optional {
                println!("  ✓ {} at {}", cmd, path.display());
            }
        }

        if !self.missing_optional.is_empty() {
            println!("\nOptional dependencies not found (not critical):");
            for cmd in &self.missing_optional {
                println!("  - {}", cmd);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_command() {
        // These should exist on most systems
        assert!(check_command("sh").is_some());
        assert!(check_command("cat").is_some());
        // This probably doesn't exist
        assert!(check_command("nonexistent_command_xyz123").is_none());
    }
}
