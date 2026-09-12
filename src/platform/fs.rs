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

/// Exclusively create an unpredictable temporary file beside its destination.
/// Keep this owner alive through `persist` so failure cleans up only our file.
pub fn create_atomic_temp(dest: &Path) -> io::Result<tempfile::NamedTempFile> {
    let parent = dest
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    tempfile::NamedTempFile::new_in(parent)
}

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

/// Launch the platform file manager, reporting path and spawn failures to the caller.
pub fn open_directory(dir: &Path) -> io::Result<()> {
    if !dir.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("Directory '{}' does not exist", dir.display()),
        ));
    }
    #[cfg(windows)]
    let mut command = std::process::Command::new("explorer.exe");
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(target_os = "linux")]
    let mut command = std::process::Command::new("xdg-open");
    command.arg(dir).spawn().map(|_| ())
}

/// Reveal a file without interpreting its path as shell source.
pub fn reveal_in_file_manager(path: &Path) -> io::Result<()> {
    if !path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("Path '{}' does not exist", path.display()),
        ));
    }
    #[cfg(windows)]
    {
        std::process::Command::new("explorer.exe")
            .arg(format!("/select,{}", path.display()))
            .spawn()
            .map(|_| ())
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn()
            .map(|_| ())
    }
    #[cfg(target_os = "linux")]
    {
        open_directory(path.parent().unwrap_or(path))
    }
}

#[cfg(test)]
mod file_manager_tests {
    use super::*;
    #[test]
    fn missing_paths_return_actionable_errors() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing");
        assert!(open_directory(&missing)
            .unwrap_err()
            .to_string()
            .contains("missing"));
        assert!(reveal_in_file_manager(&missing)
            .unwrap_err()
            .to_string()
            .contains("missing"));
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::io::Write;
    use std::os::unix::fs::symlink;

    #[test]
    fn csv_export_temp_leaves_planted_symlink_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let sentinel = dir.path().join("sentinel");
        std::fs::write(&sentinel, b"original evidence").unwrap();
        let planted = dir.path().join("report.csv.tmp");
        symlink(&sentinel, &planted).unwrap();
        let dest = dir.path().join("report.csv");
        let mut file = create_atomic_temp(&dest).unwrap();
        file.write_all(b"exported CSV").unwrap();
        file.persist(&dest).unwrap();
        assert_eq!(std::fs::read(&sentinel).unwrap(), b"original evidence");
        assert!(std::fs::symlink_metadata(&planted)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(std::fs::read_link(&planted).unwrap(), sentinel);
        assert_eq!(std::fs::read(&dest).unwrap(), b"exported CSV");
    }
    #[test]
    fn json_export_temp_leaves_planted_symlink_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let sentinel = dir.path().join("sentinel");
        std::fs::write(&sentinel, b"original evidence").unwrap();
        let planted = dir.path().join("report.json.tmp");
        symlink(&sentinel, &planted).unwrap();
        let dest = dir.path().join("report.json");
        let mut file = create_atomic_temp(&dest).unwrap();
        file.write_all(b"exported JSON").unwrap();
        file.persist(&dest).unwrap();
        assert_eq!(std::fs::read(&sentinel).unwrap(), b"original evidence");
        assert!(std::fs::symlink_metadata(&planted)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(std::fs::read_link(&planted).unwrap(), sentinel);
        assert_eq!(std::fs::read(&dest).unwrap(), b"exported JSON");
    }
}
