// LogSleuth - app/scan.rs
//
// Scan lifecycle management. Orchestrates discovery and parsing on
// background threads, sending progress messages to the UI thread
// via channels.
//
// Full implementation: next increment.

use crate::core::model::ScanProgress;
use std::path::PathBuf;
use std::sync::mpsc;

/// Manages a scan operation on a background thread.
pub struct ScanManager {
    /// Channel receiver for the UI to poll progress.
    pub progress_rx: Option<mpsc::Receiver<ScanProgress>>,
}

impl ScanManager {
    pub fn new() -> Self {
        Self { progress_rx: None }
    }

    /// Start a scan of the given directory.
    ///
    /// Spawns a background thread for discovery + parsing.
    /// Returns immediately; progress is communicated via the channel.
    pub fn start_scan(&mut self, _root: PathBuf) {
        let (tx, rx) = mpsc::channel();
        self.progress_rx = Some(rx);

        // TODO: Implement background scanning in next increment.
        // For now, immediately signal completion with empty results.
        let _ = tx.send(ScanProgress::DiscoveryStarted);
        let _ = tx.send(ScanProgress::DiscoveryCompleted { total_files: 0 });
        let _ = tx.send(ScanProgress::ParsingCompleted {
            summary: crate::core::model::ScanSummary::default(),
        });

        tracing::info!("Scan started (stub implementation)");
    }

    /// Poll for progress messages (non-blocking).
    pub fn poll_progress(&self) -> Vec<ScanProgress> {
        let mut messages = Vec::new();
        if let Some(ref rx) = self.progress_rx {
            while let Ok(msg) = rx.try_recv() {
                messages.push(msg);
            }
        }
        messages
    }
}

impl Default for ScanManager {
    fn default() -> Self {
        Self::new()
    }
}
