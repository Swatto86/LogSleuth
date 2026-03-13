// LogSleuth - core/export.rs
//
// CSV and JSON export of filtered log entries.
// Core layer: writes to any Write trait object.
//
// Implementation: next increment.

use crate::core::model::LogEntry;
use crate::util::error::ExportError;
use std::io::Write;
use std::path::Path;

/// Export filtered entries to CSV format.
///
/// Writes: timestamp, severity, source_file, line_number, thread, component, message
///
/// Accepts an iterator of entry references so callers can stream entries
/// without collecting them into a temporary `Vec`.
pub fn export_csv<'a, W: Write>(
    entries: impl Iterator<Item = &'a LogEntry>,
    writer: W,
    _export_path: &Path,
) -> Result<usize, ExportError> {
    let mut csv_writer = csv::Writer::from_writer(writer);

    // Header
    csv_writer
        .write_record([
            "timestamp",
            "severity",
            "source_file",
            "line",
            "thread",
            "component",
            "message",
        ])
        .map_err(|e| ExportError::Csv {
            path: _export_path.to_path_buf(),
            source: e,
        })?;

    let mut count = 0;
    for entry in entries {
        let ts = entry.timestamp.map(|t| t.to_rfc3339()).unwrap_or_default();

        csv_writer
            .write_record([
                &ts,
                entry.severity.label(),
                &entry.source_file.display().to_string(),
                &entry.line_number.to_string(),
                entry.thread.as_deref().unwrap_or(""),
                entry.component.as_deref().unwrap_or(""),
                &entry.message,
            ])
            .map_err(|e| ExportError::Csv {
                path: _export_path.to_path_buf(),
                source: e,
            })?;
        count += 1;
    }

    csv_writer.flush().map_err(|e| ExportError::Io {
        path: _export_path.to_path_buf(),
        source: e,
    })?;

    Ok(count)
}

/// Export filtered entries to JSON format (array of objects).
///
/// Streams entries one at a time to avoid requiring all entries in memory
/// simultaneously.  Produces the same pretty-printed JSON array output as
/// the previous slice-based implementation.
pub fn export_json<'a, W: Write>(
    entries: impl Iterator<Item = &'a LogEntry>,
    mut writer: W,
    export_path: &Path,
) -> Result<usize, ExportError> {
    writer.write_all(b"[\n").map_err(|e| ExportError::Io {
        path: export_path.to_path_buf(),
        source: e,
    })?;
    let mut count = 0;
    for entry in entries {
        if count > 0 {
            writer.write_all(b",\n").map_err(|e| ExportError::Io {
                path: export_path.to_path_buf(),
                source: e,
            })?;
        }
        serde_json::to_writer_pretty(&mut writer, entry).map_err(|e| ExportError::Json {
            path: export_path.to_path_buf(),
            source: e,
        })?;
        count += 1;
    }
    writer.write_all(b"\n]\n").map_err(|e| ExportError::Io {
        path: export_path.to_path_buf(),
        source: e,
    })?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::Severity;
    use std::path::PathBuf;

    fn make_entry(id: u64, message: &str) -> LogEntry {
        LogEntry {
            id,
            timestamp: None,
            severity: Severity::Error,
            source_file: PathBuf::from("test.log"),
            line_number: id,
            thread: Some("1".to_string()),
            component: Some("test".to_string()),
            message: message.to_string(),
            raw_text: message.to_string(),
            profile_id: "test".to_string(),
            file_modified: None,
        }
    }

    #[test]
    fn test_csv_export() {
        let entries = vec![make_entry(1, "Error one"), make_entry(2, "Error two")];
        let mut buf = Vec::new();
        let count = export_csv(entries.iter(), &mut buf, &PathBuf::from("out.csv")).unwrap();
        assert_eq!(count, 2);

        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("timestamp,severity"));
        assert!(output.contains("Error one"));
        assert!(output.contains("Error two"));
    }

    #[test]
    fn test_json_export() {
        let entries = vec![make_entry(1, "Test message")];
        let mut buf = Vec::new();
        let count = export_json(entries.iter(), &mut buf, &PathBuf::from("out.json")).unwrap();
        assert_eq!(count, 1);

        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("Test message"));
    }
}
