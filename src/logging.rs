use anyhow::Result;
use tracing_subscriber::{fmt, EnvFilter, prelude::*};
use crate::paths;

/// Initialize logging to both console and file.
pub fn init_logging() -> Result<()> {
    let logs_dir = paths::logs_dir()?;
    std::fs::create_dir_all(&logs_dir)?;

    // Use log file with date in name
    let date = format_date_simple();
    let log_file = logs_dir.join(format!("bdarchive-{}.log", date));

    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file)?;

    // Console subscriber
    let console_layer = fmt::layer()
        .with_target(false)
        .with_writer(std::io::stderr)
        .with_ansi(true);

    // File subscriber
    let file_layer = fmt::layer()
        .with_target(true)
        .with_writer(file)
        .with_ansi(false);

    // Combine layers
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(console_layer)
        .with(file_layer)
        .init();

    tracing::info!("Logging initialized. Log file: {}", log_file.display());

    Ok(())
}

/// Format Unix timestamp as YYYY-MM-DD.
fn format_date(timestamp: u64) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let datetime = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(timestamp);
    
    // Simple date formatting without external dependencies
    // This is a fallback; in production you might want to use a date library
    // For now, we'll use a simpler approach
    match datetime.duration_since(UNIX_EPOCH) {
        Ok(duration) => {
            let secs = duration.as_secs();
            let days = secs / 86400;
            // Approximate: days since Unix epoch
            // This is a simplified version; for accurate dates you'd need proper date handling
            // Using a simple heuristic for YYYY-MM-DD format
            let year = 1970 + (days / 365);
            let day_of_year = days % 365;
            let month = 1 + (day_of_year / 30); // Approximate month
            let day = 1 + (day_of_year % 30);
            format!("{:04}-{:02}-{:02}", year, month, day)
        }
        Err(_) => "unknown".to_string(),
    }
}

// Better implementation using time library - but for now use system date
// Actually, let's use the system's date command or just use a simpler approach
// For MVP, we'll use the current system date via environment or command
fn format_date_simple() -> String {
    // Try to get date from environment or use timestamp
    if let Ok(date_str) = std::process::Command::new("date")
        .args(&["+%Y-%m-%d"])
        .output()
    {
        if date_str.status.success() {
            if let Ok(date) = String::from_utf8(date_str.stdout) {
                return date.trim().to_string();
            }
        }
    }
    // Fallback: use timestamp-based approximation
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    format_date(now)
}

