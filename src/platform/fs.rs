// LogSleuth - platform/fs.rs
//
// Platform-specific filesystem helpers used by the scan and tail layers.
// Each function is a thin wrapper around std::fs with a focused contract
// (encoding-safe line reading, full-file reading, file-manager reveal).
// These are free functions rather than a trait because the only consumer
// is the application binary itself, and real-filesystem E2E tests are
// sufficient coverage without an abstraction boundary here.

use std::io;
use std::path::Path;

/// Read the first N lines of a file for format detection.
///
/// Returns up to `max_lines` lines from the start of the file.
/// Handles encoding errors gracefully (replaces invalid UTF-8).
/// I/O buffer size for network-efficient reads (128 KB reduces SMB round-trips
/// by 16x compared to the default 8 KB BufReader buffer).
const IO_BUFFER_SIZE: usize = 128 * 1024;

pub fn read_first_lines(path: &Path, max_lines: usize) -> io::Result<Vec<String>> {
    use std::io::BufRead;
    let file = std::fs::File::open(path)?;
    let reader = io::BufReader::with_capacity(IO_BUFFER_SIZE, file);

    let mut lines = Vec::with_capacity(max_lines);
    // Manual loop instead of `.take(max_lines)` so that encoding-error lines
    // do not count toward the budget.  With `.take()`, skipped InvalidData
    // lines still consumed an iteration, reducing the number of usable sample
    // lines available for profile auto-detection (Bug fix).
    for line_result in reader.lines() {
        if lines.len() >= max_lines {
            break;
        }
        match line_result {
            Ok(line) => lines.push(line),
            Err(e) if e.kind() == io::ErrorKind::InvalidData => {
                // Skip lines with encoding errors
                tracing::debug!(path = %path.display(), "Skipping line with encoding error");
            }
            Err(e) => return Err(e),
        }
    }
    Ok(lines)
}

/// Ensure a directory exists, creating it if necessary.
///
/// Returns `Ok(())` if the directory already exists or was successfully
/// created, or an `io::Error` if creation fails.
pub fn ensure_dir_exists(dir: &Path) -> io::Result<()> {
    std::fs::create_dir_all(dir)
}

/// Open a directory in the platform file manager.
///
/// Unlike [`reveal_in_file_manager`] (which highlights a specific file),
/// this opens the directory itself so the user lands inside the folder.
pub fn open_directory(dir: &Path) {
    #[cfg(target_os = "windows")]
    {
        if let Err(e) = std::process::Command::new("explorer.exe").arg(dir).spawn() {
            tracing::warn!(
                dir = %dir.display(),
                error = %e,
                "Failed to open directory in Explorer"
            );
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Err(e) = std::process::Command::new("open").arg(dir).spawn() {
            tracing::warn!(
                dir = %dir.display(),
                error = %e,
                "Failed to open directory in Finder"
            );
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Err(e) = std::process::Command::new("xdg-open").arg(dir).spawn() {
            tracing::warn!(
                dir = %dir.display(),
                error = %e,
                "Failed to open directory in file manager"
            );
        }
    }
}

/// Open the system file manager and highlight `path` within it.
///
/// Platform behaviour:
/// - **Windows**: `explorer.exe /select,"<path>"` — opens Explorer with the
///   file pre-selected in its parent folder.
/// - **macOS**: `open -R "<path>"` — reveals the file in Finder.
/// - **Linux**: `xdg-open "<parent>"` — opens the parent directory (most
///   Linux file managers do not support per-file selection via a standard
///   command-line API).
///
/// The subprocess is spawned detached; any launch failure is logged at WARN
/// level but never propagated so the UI never blocks.
pub fn reveal_in_file_manager(path: &Path) {
    #[cfg(target_os = "windows")]
    {
        // `/select,<path>` must be a single argument — no space after comma.
        let arg = format!("/select,{}", path.display());
        if let Err(e) = std::process::Command::new("explorer").arg(arg).spawn() {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "Failed to reveal file in Explorer"
            );
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Err(e) = std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn()
        {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "Failed to reveal file in Finder"
            );
        }
    }
    #[cfg(target_os = "linux")]
    {
        // Best available fallback: open the parent directory.
        let parent = path.parent().unwrap_or(path);
        if let Err(e) = std::process::Command::new("xdg-open").arg(parent).spawn() {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "Failed to open parent directory in file manager"
            );
        }
    }
}
