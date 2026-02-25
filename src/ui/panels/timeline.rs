// LogSleuth - ui/panels/timeline.rs
//
// Virtual-scrolling unified timeline view.
//
// Uses egui's `ScrollArea::show_rows` which renders only the rows currently
// visible in the viewport, giving O(1) rendering cost regardless of entry count.
// Rule 16 compliance: selection is always valid; row clicks update state directly.

use crate::app::state::AppState;
use crate::ui::theme;

/// Render the timeline panel (central area).
pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    let filtered = state.filtered_indices.len();

    if filtered == 0 {
        ui.centered_and_justified(|ui| {
            if state.entries.is_empty() {
                ui.label(
                    "No log entries loaded.\nOpen a directory via File \u{2192} Open Directory.",
                );
            } else {
                ui.label("No entries match the current filters.");
            }
        });
        return;
    }

    let row_height = theme::ROW_HEIGHT;

    // Stick to the bottom while live tail + auto-scroll are both active so
    // new entries scroll into view immediately as they arrive.
    let stick = state.tail_active && state.tail_auto_scroll;
    // After one frame of sticking, clear the one-shot flag.
    if state.tail_scroll_to_bottom {
        state.tail_scroll_to_bottom = false;
    }

    // Bookmark toggle and correlation refresh are collected here and applied
    // after show_rows so we do not mutable-borrow `state` while `entry` still
    // holds an immutable reference into `state.entries`.
    let mut bookmark_toggle: Option<u64> = None;
    let mut correlation_update_needed = false;

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .stick_to_bottom(stick)
        .show_rows(ui, row_height, filtered, |ui, row_range| {
            for display_idx in row_range {
                let Some(&entry_idx) = state.filtered_indices.get(display_idx) else {
                    continue;
                };
                let Some(entry) = state.entries.get(entry_idx) else {
                    continue;
                };

                let is_selected = state.selected_index == Some(display_idx);
                let sev_colour = theme::severity_colour(&entry.severity);
                let file_colour = state.colour_for_file(&entry.source_file);
                let entry_id = entry.id;
                let is_bookmarked = state.is_bookmarked(entry_id);
                let is_correlated = state.correlated_ids.contains(&entry_id);

                // Format: [SEV ] HH:MM:SS | filename.log | first line of message
                let ts = entry
                    .timestamp
                    .map(|t| t.format("%H:%M:%S").to_string())
                    .unwrap_or_else(|| "--:--:--".to_string());

                let file_name = entry
                    .source_file
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("?");

                let first_line = entry.message.lines().next().unwrap_or(&entry.message);

                let row_text = egui::RichText::new(format!(
                    "[{:<4}] {} | {:>16} | {}",
                    entry.severity.short_label(),
                    ts,
                    truncate_filename(file_name, 16),
                    first_line,
                ))
                .color(sev_colour)
                .monospace()
                .size(12.0);

                // Severity background tint — lowest priority layer, drawn first so
                // that the correlated (teal) and bookmarked (gold) tints visually
                // override it.  Only Critical / Error / Warning produce a tint;
                // Info, Debug, and Unknown return None and paint nothing.
                if let Some(sev_bg) = theme::severity_bg_colour(&entry.severity) {
                    let tint_rect = egui::Rect::from_min_size(
                        ui.cursor().min,
                        egui::vec2(ui.available_width(), row_height),
                    );
                    ui.painter().rect_filled(tint_rect, 0.0, sev_bg);
                }

                // Teal tint on correlated rows (drawn first so that the gold
                // bookmark tint on bookmarked+correlated rows takes visual priority).
                if is_correlated {
                    let tint_rect = egui::Rect::from_min_size(
                        ui.cursor().min,
                        egui::vec2(ui.available_width(), row_height),
                    );
                    ui.painter().rect_filled(
                        tint_rect,
                        0.0,
                        egui::Color32::from_rgba_premultiplied(20, 184, 166, 28),
                    );
                }

                // Subtle gold background tint on bookmarked rows.
                if is_bookmarked {
                    let tint_rect = egui::Rect::from_min_size(
                        ui.cursor().min,
                        egui::vec2(ui.available_width(), row_height),
                    );
                    ui.painter().rect_filled(
                        tint_rect,
                        0.0,
                        egui::Color32::from_rgba_premultiplied(251, 191, 36, 18),
                    );
                }

                // Each row: 4 px coloured file stripe | star button | selectable label
                let response = ui
                    .horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        // Coloured left stripe — visual CMTrace-style file indicator.
                        let (bar_rect, _) = ui
                            .allocate_exact_size(egui::vec2(4.0, row_height), egui::Sense::hover());
                        ui.painter().rect_filled(bar_rect, 0.0, file_colour);

                        // Bookmark star: gold when bookmarked, dim outline when not.
                        let star_char = if is_bookmarked {
                            "\u{2605}"
                        } else {
                            "\u{2606}"
                        };
                        let star_colour = if is_bookmarked {
                            egui::Color32::from_rgb(251, 191, 36) // amber
                        } else {
                            ui.style().visuals.weak_text_color()
                        };
                        let star_btn = ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new(star_char).color(star_colour).size(11.0),
                                )
                                .small()
                                .frame(false)
                                .min_size(egui::vec2(14.0, row_height)),
                            )
                            .on_hover_text(if is_bookmarked {
                                "Remove bookmark"
                            } else {
                                "Bookmark this entry"
                            });
                        if star_btn.clicked() {
                            bookmark_toggle = Some(entry_id);
                        }

                        ui.selectable_label(is_selected, row_text)
                    })
                    .inner;

                if response.clicked() {
                    state.selected_index = Some(display_idx);
                    // Flag for correlation recompute; must happen outside show_rows
                    // because update_correlation() takes &mut self which conflicts
                    // with the immutable borrow of state.entries (via `entry`).
                    correlation_update_needed = true;
                }

                // Show full path + timestamp as tooltip on hover.
                response.on_hover_ui(|ui| {
                    if let Some(ts_full) = entry.timestamp {
                        ui.label(ts_full.format("%Y-%m-%d %H:%M:%S UTC").to_string());
                    }
                    ui.label(
                        egui::RichText::new(entry.source_file.display().to_string())
                            .monospace()
                            .small(),
                    );
                });
            }
        });

    // Apply any pending bookmark toggle after the scroll area releases `state`.
    if let Some(id) = bookmark_toggle {
        state.toggle_bookmark(id);
        // In bookmarks-only mode, removing a bookmark must refresh the
        // timeline immediately so the entry disappears from view.
        if state.filter_state.bookmarks_only {
            state.apply_filters();
        }
    }

    // Recompute the correlation window for the newly selected entry (if any).
    // This is deferred from the click handler above so the &mut self call does
    // not conflict with the immutable entry borrow inside show_rows.
    if correlation_update_needed {
        state.update_correlation();
    }
}

/// Return the last `max` characters of `s`, right-aligned.
fn truncate_filename(s: &str, max: usize) -> String {
    // Truncate from the LEFT so the extension is always visible
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        format!("{:>width$}", s, width = max)
    } else {
        chars[chars.len() - max..].iter().collect()
    }
}
