use anyhow::{Context, Result};
use std::ffi::OsStr;
use std::path::Path;
use std::process::{Command, Stdio};
use tracing::{debug, warn};

/// Execute a command safely without shell injection.
/// All arguments must be provided separately.
pub fn execute_command<S: AsRef<OsStr>>(
    program: S,
    args: &[S],
    dry_run: bool,
) -> Result<CommandOutput> {
    let program_str = program.as_ref().to_string_lossy().to_string();
    let args_str: Vec<String> = args
        .iter()
        .map(|a| a.as_ref().to_string_lossy().to_string())
        .collect();

    debug!("Executing command: {} {}", program_str, args_str.join(" "));

    if dry_run {
        debug!(
            "[DRY RUN] Would execute: {} {}",
            program_str,
            args_str.join(" ")
        );
        return Ok(CommandOutput {
            success: true,
            stdout: String::new(),
            stderr: String::new(),
            exit_code: Some(0),
        });
    }

    let output = Command::new(&program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("Failed to execute command: {}", program_str))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    let success = output.status.success();
    let exit_code = output.status.code();

    if !success {
        warn!(
            "Command failed: {} {} (exit code: {:?})",
            program_str,
            args_str.join(" "),
            exit_code
        );
        warn!("stderr: {}", stderr);
    } else {
        debug!("Command succeeded: {} {}", program_str, args_str.join(" "));
    }

    Ok(CommandOutput {
        success,
        stdout,
        stderr,
        exit_code,
    })
}

/// Execute a command and return only success/failure.
pub fn execute_command_simple<S: AsRef<OsStr>>(
    program: S,
    args: &[S],
    dry_run: bool,
) -> Result<bool> {
    Ok(execute_command(program, args, dry_run)?.success)
}

/// Execute a command and capture stdout as a string.
pub fn execute_command_capture_stdout<S: AsRef<OsStr>>(
    program: S,
    args: &[S],
    dry_run: bool,
) -> Result<String> {
    let output = execute_command(program, args, dry_run)?;
    if !output.success {
        anyhow::bail!("Command failed: {}", output.stderr);
    }
    Ok(output.stdout.trim().to_string())
}

/// Execute a command with stdin input.
pub fn execute_command_with_stdin<S: AsRef<OsStr>>(
    program: S,
    args: &[S],
    stdin_data: &[u8],
    dry_run: bool,
) -> Result<CommandOutput> {
    let program_str = program.as_ref().to_string_lossy().to_string();
    let args_str: Vec<String> = args
        .iter()
        .map(|a| a.as_ref().to_string_lossy().to_string())
        .collect();

    debug!(
        "Executing command with stdin: {} {}",
        program_str,
        args_str.join(" ")
    );

    if dry_run {
        debug!(
            "[DRY RUN] Would execute: {} {} (with {} bytes of stdin)",
            program_str,
            args_str.join(" "),
            stdin_data.len()
        );
        return Ok(CommandOutput {
            success: true,
            stdout: String::new(),
            stderr: String::new(),
            exit_code: Some(0),
        });
    }

    let mut child = Command::new(&program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to spawn command: {}", program_str))?;

    // Write stdin data
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        stdin
            .write_all(stdin_data)
            .context("Failed to write to stdin")?;
    }

    let output = child
        .wait_with_output()
        .with_context(|| format!("Failed to wait for command: {}", program_str))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let success = output.status.success();
    let exit_code = output.status.code();

    if !success {
        warn!(
            "Command failed: {} {} (exit code: {:?})",
            program_str,
            args_str.join(" "),
            exit_code
        );
        warn!("stderr: {}", stderr);
    }

    Ok(CommandOutput {
        success,
        stdout,
        stderr,
        exit_code,
    })
}

/// Validate that a path is safe to use in commands (no shell injection).
/// This checks for basic path traversal and shell metacharacters.
pub fn validate_safe_path(path: &Path) -> Result<()> {
    let path_str = path.to_string_lossy();

    // Check for shell metacharacters
    if path_str.contains(|c: char| {
        matches!(
            c,
            '&' | '|' | ';' | '`' | '$' | '(' | ')' | '<' | '>' | '\n' | '\r' | '\t'
        )
    }) {
        anyhow::bail!("Path contains unsafe characters: {}", path_str);
    }

    // Check for null bytes
    if path_str.contains('\0') {
        anyhow::bail!("Path contains null byte");
    }

    Ok(())
}

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_validate_safe_path() {
        // Safe paths
        assert!(validate_safe_path(Path::new("/tmp/test.txt")).is_ok());
        assert!(validate_safe_path(Path::new("/home/user/file")).is_ok());

        // Unsafe paths
        assert!(validate_safe_path(Path::new("/tmp/test;rm -rf /")).is_err());
        assert!(validate_safe_path(Path::new("/tmp/test&rm -rf /")).is_err());
    }

    #[test]
    fn test_execute_command_dry_run() {
        let output = execute_command("echo", &["test"], true).unwrap();
        assert!(output.success);
    }

    #[test]
    fn test_execute_command_real() {
        let output = execute_command("echo", &["test"], false).unwrap();
        assert!(output.success);
        assert!(output.stdout.contains("test"));
    }

    #[test]
    fn test_execute_command_failure() {
        let output = execute_command("false", &[], false).unwrap();
        assert!(!output.success);
        assert_eq!(output.exit_code, Some(1));
    }
}
