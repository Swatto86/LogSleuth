// LogSleuth - core/export.rs
//
// CSV and JSON export of filtered log entries.
// Core layer: writes to any Write trait object.
//
// Implementation: next increment.

use crate::core::model::LogEntry;
use crate::util::error::ExportError;
use std::io::Write;
use std::path::PathBuf;

/// Export filtered entries to CSV format.
///
/// Writes: timestamp, severity, source_file, line_number, thread, component, message
pub fn export_csv<W: Write>(
    entries: &[LogEntry],
    writer: W,
    _export_path: &PathBuf,
) -> Result<usize, ExportError> {
    let mut csv_writer = csv::Writer::from_writer(writer);

    // Header
    csv_writer
        .write_record(["timestamp", "severity", "source_file", "line", "thread", "component", "message"])
        .map_err(|e| ExportError::Csv {
            path: _export_path.clone(),
            source: e,
        })?;

    let mut count = 0;
    for entry in entries {
        let ts = entry
            .timestamp
            .map(|t| t.to_rfc3339())
            .unwrap_or_default();

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
                path: _export_path.clone(),
                source: e,
            })?;
        count += 1;
    }

    csv_writer.flush().map_err(|e| ExportError::Io {
        path: _export_path.clone(),
        source: e,
    })?;

    Ok(count)
}

/// Export filtered entries to JSON format (array of objects).
pub fn export_json<W: Write>(
    entries: &[LogEntry],
    writer: W,
    export_path: &PathBuf,
) -> Result<usize, ExportError> {
    serde_json::to_writer_pretty(writer, entries).map_err(|e| ExportError::Json {
        path: export_path.clone(),
        source: e,
    })?;
    Ok(entries.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::Severity;

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
        }
    }

    #[test]
    fn test_csv_export() {
        let entries = vec![
            make_entry(1, "Error one"),
            make_entry(2, "Error two"),
        ];
        let mut buf = Vec::new();
        let count = export_csv(&entries, &mut buf, &PathBuf::from("out.csv")).unwrap();
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
        let count = export_json(&entries, &mut buf, &PathBuf::from("out.json")).unwrap();
        assert_eq!(count, 1);

        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("Test message"));
    }
}
