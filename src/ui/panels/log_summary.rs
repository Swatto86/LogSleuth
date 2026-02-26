// LogSleuth - ui/panels/log_summary.rs
//
// Log-entry summarisation panel.
//
// Enumerates the currently-filtered log entries and produces a structured
// summary of errors and warnings (or whichever severities are present).
// Colour coding mirrors the timeline's `theme::severity_colour` mapping.
//
// The window is modal-ish (floated, always on top) and is opened via
// View -> Log Summary or the "Summary" button in the Filters sidebar.
//
// Rule 16: the close button is always enabled.
// Rule 11: no unbounded allocation — at most MAX_PREVIEW_ROWS rows are shown
//          per severity group.

use crate::app::state::AppState;
use crate::core::model::Severity;
use crate::ui::theme;

/// Maximum number of message preview rows shown per severity group.
const MAX_PREVIEW_ROWS: usize = 50;

/// Maximum number of characters shown per message in the preview.
/// Binary or extremely long lines are truncated to keep the grid layout stable.
const MAX_MSG_CHARS: usize = 140;

/// Render the log-entry summary window (if `state.show_log_summary` is true).
pub fn render(ctx: &egui::Context, state: &mut AppState) {
    if !state.show_log_summary {
        return;
    }

    // Collect statistics over currently-filtered entries.
    // We iterate once: build per-severity counts and collect up to
    // MAX_PREVIEW_ROWS representative messages for actionable severities.
    let total_filtered = state.filtered_indices.len();

    // Ordered severities used when building per-group previews.
    let display_order = [
        Severity::Critical,
        Severity::Error,
        Severity::Warning,
        Severity::Info,
        Severity::Debug,
        Severity::Unknown,
    ];

    // Per-severity entry count.
    let mut counts: std::collections::HashMap<Severity, usize> = std::collections::HashMap::new();

    // Per-severity preview rows: (timestamp_str, source_file, first_line_of_message).
    let mut previews: std::collections::HashMap<Severity, Vec<(String, String, String)>> =
        std::collections::HashMap::new();

    for &idx in &state.filtered_indices {
        let Some(entry) = state.entries.get(idx) else {
            continue;
        };
        let sev = entry.severity;
        *counts.entry(sev).or_insert(0) += 1;

        let rows = previews.entry(sev).or_default();
        if rows.len() < MAX_PREVIEW_ROWS {
            let ts = entry
                .timestamp
                .map(|t| t.format("%H:%M:%S").to_string())
                .unwrap_or_else(|| "--:--:--".to_string());
            let file = entry.source_file.display().to_string();
            let msg = sanitise_preview(entry.message.lines().next().unwrap_or(&entry.message));
            rows.push((ts, file, msg));
        }
    }

    // Mirror the open flag so the built-in title-bar × also closes the window.
    // `open` is borrowed mutably by Window::open(); a separate `close_clicked`
    // flag carries the body Close button result out of the closure to avoid a
    // second mutable borrow of `open` inside the same expression.
    let mut open = state.show_log_summary;
    let mut close_clicked = false;

    egui::Window::new("Log Summary")
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .min_width(520.0)
        .min_height(200.0)
        .default_width(680.0)
        .default_height(420.0)
        .default_pos([
            ctx.screen_rect().width() * 0.5 - 340.0,
            48.0, // sit just below the menu bar, never off the top edge
        ])
        .show(ctx, |ui| {
            // ----------------------------------------------------------------
            // Header: overall counts line
            // ----------------------------------------------------------------
            ui.horizontal(|ui| {
                ui.strong(format!("Showing {total_filtered} filtered entries"));
                if total_filtered < state.entries.len() {
                    ui.label(
                        egui::RichText::new(format!(
                            "({} total, {} hidden by filters)",
                            state.entries.len(),
                            state.entries.len() - total_filtered
                        ))
                        .weak()
                        .small(),
                    );
                }
            });

            ui.add_space(4.0);

            // ----------------------------------------------------------------
            // Severity breakdown table
            // ----------------------------------------------------------------
            egui::Grid::new("log_summary_counts")
                .num_columns(3)
                .spacing([20.0, 3.0])
                .show(ui, |ui| {
                    ui.strong("Severity");
                    ui.strong("Count");
                    ui.strong("Share");
                    ui.end_row();

                    for sev in &display_order {
                        let count = counts.get(sev).copied().unwrap_or(0);
                        if count == 0 {
                            continue;
                        }
                        let colour = theme::severity_colour(sev, state.dark_mode);
                        let pct = if total_filtered > 0 {
                            (count as f64 / total_filtered as f64) * 100.0
                        } else {
                            0.0
                        };

                        ui.colored_label(colour, sev.label());
                        ui.colored_label(colour, count.to_string());
                        ui.label(egui::RichText::new(format!("{pct:.1}%")).weak().small());
                        ui.end_row();
                    }
                });

            ui.add_space(4.0);
            ui.separator();

            // ----------------------------------------------------------------
            // Per-severity message preview sections
            // Only show sections for severities that have entries.
            // Actionable severities (Critical, Error, Warning) are expanded
            // by default; Info/Debug/Unknown are collapsed.
            // ----------------------------------------------------------------
            // Reserve height for the header rows + separator + Close button so
            // the scroll area never pushes them off the bottom of the window.
            let reserved_bottom: f32 = 80.0;
            let scroll_max_height = (ui.available_height() - reserved_bottom).max(120.0);
            egui::ScrollArea::vertical()
                .id_salt("log_summary_scroll")
                .max_height(scroll_max_height)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for sev in &display_order {
                        let Some(rows) = previews.get(sev) else {
                            continue;
                        };
                        if rows.is_empty() {
                            continue;
                        }
                        let count = counts.get(sev).copied().unwrap_or(0);
                        let colour = theme::severity_colour(sev, state.dark_mode);

                        // Default-open for actionable severities.
                        let default_open = matches!(
                            sev,
                            Severity::Critical | Severity::Error | Severity::Warning
                        );

                        // Use CollapsingState directly with a stable plain-string ID.
                        // CollapsingHeader::new() derives its persistent ID from the
                        // rendered label text; when the label is RichText the derived ID
                        // can differ between frames, which prevents the open/close state
                        // from being found and resets it every render cycle.
                        // CollapsingState with an explicit stable ID avoids the problem.
                        let header_id = ui.make_persistent_id(format!(
                            "log_summary_section_{}",
                            sev.label()
                        ));
                        let section = egui::containers::collapsing_header::CollapsingState::load_with_default_open(
                            ui.ctx(),
                            header_id,
                            default_open,
                        );

                        section
                            .show_header(ui, |ui| {
                                ui.colored_label(
                                    colour,
                                    egui::RichText::new(format!("{} ({})", sev.label(), count))
                                        .strong(),
                                );
                            })
                            .body(|ui| {
                                // Table: timestamp | file | message
                                egui::Grid::new(format!("log_summary_grid_{}", sev.label()))
                                    .num_columns(3)
                                    .spacing([8.0, 2.0])
                                    .striped(true)
                                    .show(ui, |ui| {
                                        for (ts, file, msg) in rows {
                                            ui.label(
                                                egui::RichText::new(ts)
                                                    .monospace()
                                                    .size(11.5)
                                                    .weak(),
                                            );
                                            // Show just the filename; full path in tooltip.
                                            let display_name =
                                                std::path::Path::new(file.as_str())
                                                    .file_name()
                                                    .and_then(|n| n.to_str())
                                                    .unwrap_or(file.as_str());
                                            ui.label(
                                                egui::RichText::new(display_name)
                                                    .monospace()
                                                    .size(11.5)
                                                    .weak(),
                                            )
                                            .on_hover_text(file.as_str());
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(msg)
                                                        .color(colour)
                                                        .size(11.5),
                                                )
                                                .truncate(),
                                            );
                                            ui.end_row();
                                        }
                                    });

                                if count > MAX_PREVIEW_ROWS {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "... and {} more (export to CSV/JSON to see all)",
                                            count - MAX_PREVIEW_ROWS
                                        ))
                                        .weak()
                                        .small()
                                        .italics(),
                                    );
                                }
                            });
                    }

                    if total_filtered == 0 {
                        ui.centered_and_justified(|ui| {
                            ui.label("No entries match the current filters.");
                        });
                    }
                });

            ui.add_space(6.0);
            ui.separator();
            if ui.button("Close").clicked() {
                close_clicked = true;
            }
        });

    // Write back: honour both the title-bar × and the body Close button.
    state.show_log_summary = open && !close_clicked;
}

/// Sanitise a raw log message fragment for safe display in the preview grid.
///
/// Binary log files (CBS.log, some Windows Update logs) can contain thousands
/// of non-printable bytes forming a single "line" with no newline characters.
/// When rendered in an `egui::Grid` these overflow the column width, push the
/// window wider than the scroll area, and displace the collapsing-header click
/// region outside egui's clip rect — making the section impossible to collapse.
///
/// This function:
/// 1. Replaces non-printable / control characters (except tab) with \u{FFFD}.
/// 2. Hard-truncates to MAX_MSG_CHARS, appending `\u{2026}` (ellipsis) if cut.
///
/// The result is always valid UTF-8, ASCII-printable safe, and short enough
/// that `Label::truncate()` in the grid can clip any remainder at column width.
fn sanitise_preview(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| {
            if c == '\t' || (!c.is_control() && c != '\r') {
                c
            } else {
                '\u{FFFD}'
            }
        })
        .collect();

    let chars: Vec<char> = cleaned.chars().collect();
    if chars.len() <= MAX_MSG_CHARS {
        cleaned
    } else {
        let mut s: String = chars[..MAX_MSG_CHARS].iter().collect();
        s.push('\u{2026}'); // ellipsis
        s
    }
}
