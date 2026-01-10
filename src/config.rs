use crate::paths;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub struct Config {
    /// Blu-ray device path (auto-detected, defaults to /dev/sr0)
    #[serde(default = "default_device")]
    pub device: String,

    /// Staging directory for building ISO
    pub staging_dir: Option<String>,

    /// Database path (defaults to data_dir/archive.db)
    pub database_path: Option<String>,

    /// Default disc capacity in GB (25 or 50)
    #[serde(default = "default_capacity_gb")]
    pub default_capacity_gb: u64,

    /// Verification settings
    #[serde(default)]
    pub verification: VerificationConfig,

    /// Burn configuration
    #[serde(default)]
    pub burn: BurnConfig,

    /// Optional tools configuration
    #[serde(default)]
    pub optional_tools: OptionalToolsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationConfig {
    /// Automatically verify disc after burning
    #[serde(default)]
    pub auto_verify_after_burn: bool,

    /// Automatically mount disc when verifying
    #[serde(default)]
    pub auto_mount: bool,
}

impl Default for VerificationConfig {
    fn default() -> Self {
        Self {
            auto_verify_after_burn: false,
            auto_mount: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BurnConfig {
    /// Burn method: "iso" (create ISO first) or "direct" (burn directory directly)
    #[serde(default = "default_burn_method")]
    pub method: String,
}

impl Default for BurnConfig {
    fn default() -> Self {
        Self {
            method: default_burn_method(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionalToolsConfig {
    /// Use qrencode for QR code generation
    #[serde(default = "default_true")]
    pub use_qrencode: bool,

    /// Use rsync for staging files
    #[serde(default = "default_true")]
    pub use_rsync: bool,

    /// Use Midnight Commander for folder selection
    #[serde(default = "default_true")]
    pub use_mc: bool,
}

impl Default for OptionalToolsConfig {
    fn default() -> Self {
        Self {
            use_qrencode: true,
            use_rsync: true,
            use_mc: true,
        }
    }
}

fn default_device() -> String {
    // Try to auto-detect the optical drive, fall back to /dev/sr0
    crate::paths::detect_optical_drive().unwrap_or_else(|| "/dev/sr0".to_string())
}

fn default_capacity_gb() -> u64 {
    25
}

fn default_true() -> bool {
    true
}

fn default_burn_method() -> String {
    "direct".to_string() // Default to direct method for space efficiency
}

impl Default for Config {
    fn default() -> Self {
        Self {
            device: default_device(),
            staging_dir: None,
            database_path: None,
            default_capacity_gb: default_capacity_gb(),
            verification: VerificationConfig::default(),
            burn: BurnConfig::default(),
            optional_tools: OptionalToolsConfig::default(),
        }
    }
}

impl Config {
    /// Load config from file, or return default if file doesn't exist.
    pub fn load() -> Result<Self> {
        let config_path = Self::config_file_path()?;

        if !config_path.exists() {
            // Return default config
            return Ok(Self::default());
        }

        let contents = std::fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read config file: {}", config_path.display()))?;

        let config: Config = toml::from_str(&contents).context("Failed to parse config file")?;

        Ok(config)
    }

    /// Save config to file.
    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_file_path()?;
        paths::ensure_config_dir()?;

        let contents = toml::to_string_pretty(self).context("Failed to serialize config")?;

        std::fs::write(&config_path, contents)
            .with_context(|| format!("Failed to write config file: {}", config_path.display()))?;

        Ok(())
    }

    /// Get the config file path.
    pub fn config_file_path() -> Result<PathBuf> {
        Ok(paths::config_dir()?.join("config.toml"))
    }

    /// Get the staging directory, creating default if not set.
    pub fn staging_dir(&self) -> Result<PathBuf> {
        match &self.staging_dir {
            Some(dir) => Ok(paths::expand_tilde(dir)),
            None => {
                // Default to system temp directory
                let default = std::env::temp_dir().join("bdarchive_staging");
                Ok(default)
            }
        }
    }

    /// Get the database path, creating default if not set.
    pub fn database_path(&self) -> Result<PathBuf> {
        match &self.database_path {
            Some(path) => Ok(paths::expand_tilde(path)),
            None => paths::default_database_path(),
        }
    }

    /// Get the default disc capacity in bytes.
    pub fn default_capacity_bytes(&self) -> u64 {
        self.default_capacity_gb * 1024 * 1024 * 1024
    }

    /// Validate the configuration.
    pub fn validate(&mut self) -> Result<()> {
        // Validate device path - try auto-detection if default doesn't work
        let device_path = Path::new(&self.device);
        if device_path.exists() {
            paths::validate_device(device_path).with_context(|| {
                // Suggest auto-detected device if validation fails
                let suggestion = paths::detect_optical_drive()
                    .filter(|d| d != &self.device)
                    .map(|d| format!("\n\n💡 Suggestion: Use auto-detected drive: {}", d))
                    .unwrap_or_default();
                format!("Invalid device path: {}{}", self.device, suggestion)
            })?;
        } else {
            // Device doesn't exist - try auto-detection
            if let Some(auto_device) = paths::detect_optical_drive() {
                info!(
                    "Auto-detected optical drive: {} (instead of {})",
                    auto_device, self.device
                );
                self.device = auto_device;
            } else {
                return Err(anyhow::anyhow!(
                    "No optical drive found at {} and auto-detection found no drives.\n\
                     Please ensure you have an optical drive connected and accessible.",
                    self.device
                ));
            }
        }

        // Validate staging directory exists or can be created
        let staging = self.staging_dir()?;
        if staging.exists() {
            paths::validate_dir(&staging)?;
        } else {
            paths::ensure_dir(&staging)?;
        }

        // Validate database path parent directory exists
        let db_path = self.database_path()?;
        if let Some(parent) = db_path.parent() {
            paths::ensure_dir(parent)?;
        }

        // Validate capacity
        if self.default_capacity_gb != 25 && self.default_capacity_gb != 50 {
            anyhow::bail!("Default capacity must be 25 or 50 GB");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.device, "/dev/sr0");
        assert_eq!(config.default_capacity_gb, 25);
        assert_eq!(config.default_capacity_bytes(), 25 * 1024 * 1024 * 1024);
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        assert!(toml_str.contains("device"));
        assert!(toml_str.contains("/dev/sr0"));
    }

    #[test]
    fn test_config_deserialization() {
        let toml_str = r#"
device = "/dev/sr1"
default_capacity_gb = 50
[verification]
auto_verify_after_burn = true
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.device, "/dev/sr1");
        assert_eq!(config.default_capacity_gb, 50);
        assert!(config.verification.auto_verify_after_burn);
    }

    #[test]
    fn test_staging_dir_default() {
        let config = Config::default();
        let staging = config.staging_dir().unwrap();
        assert!(staging.to_string_lossy().contains("bdarchive_staging"));
    }

    #[test]
    fn test_database_path() -> Result<()> {
        let config = Config::default();
        let db_path = config.database_path()?;
        assert!(db_path.to_string_lossy().contains("archive.db"));
        Ok(())
    }
}
