// LogSleuth - platform/fs.rs
//
// Filesystem abstraction traits.
// Enables testing core logic without real filesystem access.
//
// Implementation: next increment (trait + real implementation).

use std::io;
use std::path::Path;

/// Read the first N lines of a file for format detection.
///
/// Returns up to `max_lines` lines from the start of the file.
/// Handles encoding errors gracefully (replaces invalid UTF-8).
pub fn read_first_lines(path: &Path, max_lines: usize) -> io::Result<Vec<String>> {
    use std::io::BufRead;
    let file = std::fs::File::open(path)?;
    let reader = io::BufReader::new(file);

    let mut lines = Vec::with_capacity(max_lines);
    for line_result in reader.lines().take(max_lines) {
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

/// Read the full content of a file as a string.
///
/// For files with invalid UTF-8, uses lossy conversion.
pub fn read_file_lossy(path: &Path) -> io::Result<String> {
    let bytes = std::fs::read(path)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
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
