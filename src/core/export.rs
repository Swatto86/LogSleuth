// LogSleuth - core/export.rs
//
// CSV and JSON export of filtered log entries.
// Core layer: writes to any Write trait object.

use crate::core::model::LogEntry;
use crate::util::error::ExportError;
use std::io::Write;
use std::path::Path;

/// Metadata written into every export file header so the recipient knows how
/// the data was generated.
pub struct ExportMetadata<'a> {
    pub scan_path: Option<&'a Path>,
    pub filter_description: &'a str,
    pub entry_count: usize,
}

/// Export filtered entries to CSV format.
///
/// Writes a metadata comment block followed by: timestamp, severity,
/// source_file, line_number, thread, component, message.
///
/// Accepts an iterator of entry references so callers can stream entries
/// without collecting them into a temporary `Vec`.
pub fn export_csv<'a, W: Write>(
    entries: impl Iterator<Item = &'a LogEntry>,
    mut writer: W,
    export_path: &Path,
    metadata: &ExportMetadata<'_>,
) -> Result<usize, ExportError> {
    // EXP-03: metadata header as CSV comment lines
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
    let scan = metadata
        .scan_path
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "(individual files)".to_string());
    for line in [
        "# LogSleuth Export".to_string(),
        format!("# Exported: {now}"),
        format!("# Scan path: {scan}"),
        format!("# Filter: {}", metadata.filter_description),
        format!("# Entries: {}", metadata.entry_count),
        String::new(),
    ] {
        writeln!(writer, "{line}").map_err(|e| ExportError::Io {
            path: export_path.to_path_buf(),
            source: e,
        })?;
    }

    let mut csv_writer = csv::Writer::from_writer(writer);

    // Column header
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
            path: export_path.to_path_buf(),
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
                path: export_path.to_path_buf(),
                source: e,
            })?;
        count += 1;
    }

    csv_writer.flush().map_err(|e| ExportError::Io {
        path: export_path.to_path_buf(),
        source: e,
    })?;

    Ok(count)
}

/// Export filtered entries to JSON format.
///
/// Produces a JSON object with `metadata` and `entries` fields.  The
/// metadata object contains scan path, filter criteria, export timestamp,
/// and entry count per EXP-03.
pub fn export_json<'a, W: Write>(
    entries: impl Iterator<Item = &'a LogEntry>,
    mut writer: W,
    export_path: &Path,
    metadata: &ExportMetadata<'_>,
) -> Result<usize, ExportError> {
    let now = chrono::Utc::now()
        .format("%Y-%m-%d %H:%M:%S UTC")
        .to_string();
    let scan = metadata
        .scan_path
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "(individual files)".to_string());
    // EXP-03: wrap entries in a metadata envelope
    write!(
        writer,
        "{{\n  \"metadata\": {{\n    \"exported\": \"{now}\",\n    \"scan_path\": {},\n    \"filter\": {},\n    \"entry_count\": {}\n  }},\n  \"entries\": [\n",
        serde_json::to_string(&scan).unwrap_or_else(|_| "null".to_string()),
        serde_json::to_string(metadata.filter_description).unwrap_or_else(|_| "null".to_string()),
        metadata.entry_count,
    )
    .map_err(|e| ExportError::Io {
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
    writer
        .write_all(b"\n  ]\n}\n")
        .map_err(|e| ExportError::Io {
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
        let meta = ExportMetadata {
            scan_path: Some(Path::new("/tmp/logs")),
            filter_description: "No filter (all entries)",
            entry_count: 2,
        };
        let count = export_csv(entries.iter(), &mut buf, &PathBuf::from("out.csv"), &meta).unwrap();
        assert_eq!(count, 2);

        let output = String::from_utf8(buf).unwrap();
        // Metadata header
        assert!(output.contains("# LogSleuth Export"));
        assert!(output.contains("# Scan path: /tmp/logs"));
        assert!(output.contains("# Filter: No filter (all entries)"));
        assert!(output.contains("# Entries: 2"));
        // Data
        assert!(output.contains("timestamp,severity"));
        assert!(output.contains("Error one"));
        assert!(output.contains("Error two"));
    }

    #[test]
    fn test_json_export() {
        let entries = vec![make_entry(1, "Test message")];
        let mut buf = Vec::new();
        let meta = ExportMetadata {
            scan_path: None,
            filter_description: "Severity: Error",
            entry_count: 1,
        };
        let count =
            export_json(entries.iter(), &mut buf, &PathBuf::from("out.json"), &meta).unwrap();
        assert_eq!(count, 1);

        let output = String::from_utf8(buf).unwrap();
        // Metadata envelope
        assert!(output.contains("\"metadata\""));
        assert!(output.contains("\"exported\""));
        assert!(output.contains("\"filter\""));
        assert!(output.contains("\"entry_count\": 1"));
        // Data
        assert!(output.contains("Test message"));
    }
}
