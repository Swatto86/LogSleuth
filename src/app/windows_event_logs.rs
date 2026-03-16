// LogSleuth - app/windows_event_logs.rs
//
// Windows Event Viewer log file discovery helpers.

#[cfg(windows)]
#[derive(Debug, Clone)]
pub struct EventViewerLogSelection {
    pub dir: std::path::PathBuf,
    pub files: Vec<std::path::PathBuf>,
    pub access_denied: usize,
    pub unreadable: usize,
}

#[cfg(windows)]
pub fn collect_event_viewer_log_files() -> Result<EventViewerLogSelection, String> {
    let dir = default_event_log_dir()
        .ok_or_else(|| "Could not resolve %SystemRoot% for Event Viewer logs.".to_string())?;
    if !dir.is_dir() {
        return Err(format!(
            "Event Viewer log directory not found: {}",
            dir.display()
        ));
    }

    let mut files = Vec::new();
    let mut access_denied = 0usize;
    let mut unreadable = 0usize;
    let entries =
        std::fs::read_dir(&dir).map_err(|e| format!("Failed to read {}: {e}", dir.display()))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to enumerate Event Viewer logs: {e}"))?;
        let path = entry.path();
        let is_evtx = path
            .extension()
            .map(|ext| ext.to_string_lossy().eq_ignore_ascii_case("evtx"))
            .unwrap_or(false);
        if !is_evtx {
            continue;
        }

        match std::fs::File::open(&path) {
            Ok(_) => files.push(path),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                access_denied += 1;
            }
            Err(_) => {
                unreadable += 1;
            }
        }
    }

    files.sort();
    Ok(EventViewerLogSelection {
        dir,
        files,
        access_denied,
        unreadable,
    })
}

#[cfg(windows)]
pub fn default_event_log_dir() -> Option<std::path::PathBuf> {
    let root = std::env::var_os("SystemRoot").or_else(|| std::env::var_os("WINDIR"))?;
    let mut path = std::path::PathBuf::from(root);
    path.push("System32");
    path.push("winevt");
    path.push("Logs");
    Some(path)
}
