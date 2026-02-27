// LogSleuth - ui/panels/filters.rs
//
// Filter controls sidebar panel.
// Rule 16 compliance: controls disabled when their action is invalid;
// filter application is immediate on change.

use crate::app::state::AppState;
use crate::core::model::Severity;
use crate::ui::theme;

/// Render the filter controls sidebar section.
pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("Filters");
    ui.separator();

    // Single row — severity presets + utility actions combined.
    ui.horizontal_wrapped(|ui| {
        let fuzzy = state.filter_state.fuzzy;
        if ui.small_button("Errors only").clicked() {
            let current_files = std::mem::take(&mut state.filter_state.source_files);
            let hide_all = state.filter_state.hide_all_sources;
            state.filter_state = crate::core::filter::FilterState::errors_only_from(fuzzy);
            state.filter_state.source_files = current_files;
            state.filter_state.hide_all_sources = hide_all;
            state.apply_filters();
        }
        if ui.small_button("Errors + Warn").clicked() {
            let current_files = std::mem::take(&mut state.filter_state.source_files);
            let hide_all = state.filter_state.hide_all_sources;
            state.filter_state = crate::core::filter::FilterState::errors_and_warnings_from(fuzzy);
            state.filter_state.source_files = current_files;
            state.filter_state.hide_all_sources = hide_all;
            state.apply_filters();
        }
        // Combined troubleshooting preset: severity Error+Warning + last 15-minute rolling window.
        // Single click brings up the most recent problem signals across all loaded files,
        // and continues to show live tail entries that fall within the advancing window.
        if ui
            .small_button("Err+Warn+15m")
            .on_hover_text(
                "Show only Error / Warning entries from the last 15 minutes.\n\
                 When Live Tail is active the window advances automatically.",
            )
            .clicked()
        {
            let current_files = std::mem::take(&mut state.filter_state.source_files);
            let hide_all = state.filter_state.hide_all_sources;
            state.filter_state = crate::core::filter::FilterState::errors_and_warnings_from(fuzzy);
            state.filter_state.source_files = current_files;
            state.filter_state.hide_all_sources = hide_all;
            state.filter_state.relative_time_secs = Some(15 * 60);
            state.filter_state.relative_time_input.clear();
            state.apply_filters();
        }
        if ui.small_button("Clear").clicked() {
            state.filter_state = crate::core::filter::FilterState {
                fuzzy,
                ..Default::default()
            };
            state.apply_filters();
        }

        // Summary shortcut (disabled when no filtered entries yet)
        let has_entries = !state.filtered_indices.is_empty();
        ui.add_enabled_ui(has_entries, |ui| {
            if ui.small_button("Summary").clicked() {
                state.show_log_summary = true;
            }
        });

        // Bookmarks toggle: shows only bookmarked entries when active.
        let bm_count = state.bookmark_count();
        let bm_active = state.filter_state.bookmarks_only;
        let bm_label = if bm_count > 0 {
            format!("\u{2605} Bookmarks ({bm_count})")
        } else {
            "\u{2606} Bookmarks".to_string()
        };
        let bm_colour = if bm_active {
            egui::Color32::from_rgb(251, 191, 36) // amber when active
        } else {
            ui.style().visuals.text_color()
        };
        if ui
            .add(egui::Button::new(
                egui::RichText::new(&bm_label).small().color(bm_colour),
            ))
            .on_hover_text("Show only bookmarked entries")
            .clicked()
        {
            state.filter_state.bookmarks_only = !bm_active;
            state.apply_filters();
        }
        // Clear-all bookmarks button (only when bookmarks exist)
        if bm_count > 0
            && ui
                .add(
                    egui::Button::new(
                        egui::RichText::new("\u{d7} clear bm")
                            .small()
                            .color(egui::Color32::from_rgb(156, 163, 175)),
                    )
                    .frame(false),
                )
                .on_hover_text("Remove all bookmarks")
                .clicked()
        {
            state.clear_bookmarks();
        }
    });

    ui.add_space(6.0);
    ui.separator();

    // Severity checkboxes with severity-coloured labels
    ui.label("Severity:");
    let mut changed = false;
    for severity in Severity::all() {
        let colour = theme::severity_colour(severity, state.dark_mode);
        let label = egui::RichText::new(severity.label()).color(colour);
        let mut checked = state.filter_state.severity_levels.contains(severity);
        if ui.checkbox(&mut checked, label).changed() {
            if checked {
                state.filter_state.severity_levels.insert(*severity);
            } else {
                state.filter_state.severity_levels.remove(severity);
            }
            changed = true;
        }
    }
    if changed {
        state.apply_filters();
    }

    ui.add_space(6.0);
    ui.separator();

    // Text search (substring or fuzzy depending on mode toggle)
    ui.label("Text search:");
    ui.horizontal(|ui| {
        if ui
            .text_edit_singleline(&mut state.filter_state.text_search)
            .changed()
        {
            state.apply_filters();
        }
        // Fuzzy mode toggle button: lights up when active
        let fuzzy_colour = if state.filter_state.fuzzy {
            egui::Color32::from_rgb(96, 165, 250) // blue-ish when active
        } else {
            ui.style().visuals.text_color()
        };
        let fuzzy_btn = ui.add(
            egui::Button::new(egui::RichText::new("~").color(fuzzy_colour))
                .small()
                .min_size(egui::vec2(18.0, 0.0)),
        );
        if fuzzy_btn
            .on_hover_text("Toggle fuzzy (subsequence) matching")
            .clicked()
        {
            state.filter_state.fuzzy = !state.filter_state.fuzzy;
            state.apply_filters();
        }
    });
    // Mode label under the search box
    if !state.filter_state.text_search.is_empty() {
        ui.label(
            egui::RichText::new(if state.filter_state.fuzzy {
                "\u{223c} fuzzy"
            } else {
                "= exact"
            })
            .small()
            .weak(),
        );
    }

    ui.add_space(4.0);

    // Regex search with compile-error feedback
    ui.label("Regex:");
    let re_changed = ui
        .text_edit_singleline(&mut state.filter_state.regex_pattern)
        .changed();
    if re_changed {
        let pattern = state.filter_state.regex_pattern.clone();
        let _ = state.filter_state.set_regex(&pattern);
        state.apply_filters();
    }
    // Feedback indicator -- only show when there is a non-empty pattern
    if !state.filter_state.regex_pattern.is_empty() {
        if state.filter_state.regex_search.is_some() {
            ui.colored_label(
                egui::Color32::from_rgb(74, 222, 128),
                "\u{2713} regex valid",
            );
        } else {
            ui.colored_label(
                egui::Color32::from_rgb(248, 113, 113),
                "\u{2717} invalid regex",
            );
        }
    }

    ui.add_space(6.0);
    ui.separator();

    // -------------------------------------------------------------------------
    // Time range filter
    // -------------------------------------------------------------------------
    ui.label("Time range:");

    // Quick-select buttons (toggle: click active button to clear it)
    ui.horizontal_wrapped(|ui| {
        for &(label, secs) in &[
            ("1m", 60u64),
            ("15m", 15 * 60),
            ("1h", 3_600),
            ("6h", 21_600),
            ("24h", 86_400),
        ] {
            let active = state.filter_state.relative_time_secs == Some(secs);
            if ui.selectable_label(active, label).clicked() {
                if active {
                    state.filter_state.relative_time_secs = None;
                    state.filter_state.time_start = None;
                } else {
                    state.filter_state.relative_time_secs = Some(secs);
                    state.filter_state.relative_time_input.clear();
                }
                state.apply_filters();
            }
        }
    });

    // Custom minutes text input
    ui.horizontal(|ui| {
        ui.label("Last");
        let resp = ui.add(
            egui::TextEdit::singleline(&mut state.filter_state.relative_time_input)
                .desired_width(42.0)
                .hint_text("min"),
        );
        ui.label("min");

        // Apply the filter when the field loses focus (click away or Tab/Enter).
        // Previously this required both lost_focus AND key_pressed(Enter), which
        // silently discarded a typed value when the user clicked away without
        // pressing Enter, leaving the rolling window unset.
        let committed = resp.lost_focus();
        if committed {
            let trimmed = state.filter_state.relative_time_input.trim().to_string();
            if let Ok(mins) = trimmed.parse::<u64>() {
                if mins > 0 {
                    state.filter_state.relative_time_secs = Some(mins * 60);
                    state.apply_filters();
                }
            }
        }

        // Clear button when a relative window is set
        if state.filter_state.relative_time_secs.is_some() && ui.small_button("\u{2715}").clicked()
        {
            state.filter_state.relative_time_secs = None;
            state.filter_state.time_start = None;
            state.filter_state.relative_time_input.clear();
            state.apply_filters();
        }
    });

    // Show computed window start as feedback
    if let Some(start) = state.filter_state.time_start {
        ui.label(
            egui::RichText::new(format!("After {}", start.format("%H:%M:%S")))
                .small()
                .color(egui::Color32::from_rgb(96, 165, 250)),
        );
    }
    // When Live Tail is active and a rolling window is set, confirm the window is
    // continuously advancing with each frame so the user knows new tail entries
    // entering the window will appear automatically.
    if state.tail_active && state.filter_state.relative_time_secs.is_some() {
        ui.label(
            egui::RichText::new("\u{25cf} Rolling window (live)")
                .small()
                .color(egui::Color32::from_rgb(34, 197, 94)),
        );
    }

    // -------------------------------------------------------------------------
    // Time correlation overlay controls
    // -------------------------------------------------------------------------
    // Only shown once entries are loaded — the feature requires a selection.
    if !state.entries.is_empty() {
        ui.add_space(6.0);
        ui.separator();
        ui.label("Correlation:");

        let corr_active = state.correlation_active;
        let has_selection = state.selected_index.is_some();
        let teal = egui::Color32::from_rgb(20, 184, 166);

        // Toggle button: teal when active, dim when off.
        ui.horizontal(|ui| {
            let toggle_text = if corr_active {
                format!("\u{22c4} +/-{}s", state.correlation_window_secs)
            } else {
                "\u{22c4} Off".to_string()
            };
            let toggle_colour = if corr_active {
                teal
            } else {
                ui.style().visuals.text_color()
            };
            let hover_msg = if corr_active {
                "Click to disable the correlation highlight"
            } else if has_selection {
                "Click to highlight all entries near the selected entry"
            } else {
                "Select a timeline entry first, then enable correlation"
            };
            if ui
                .add(egui::Button::new(
                    egui::RichText::new(&toggle_text)
                        .small()
                        .color(toggle_colour),
                ))
                .on_hover_text(hover_msg)
                .clicked()
            {
                state.correlation_active = !corr_active;
                state.update_correlation();
            }

            // Entry count badge — only visible when the overlay is populated.
            if corr_active && !state.correlated_ids.is_empty() {
                let n = state.correlated_ids.len();
                ui.label(
                    egui::RichText::new(format!("{n} entries"))
                        .small()
                        .color(teal),
                );
            }
        });

        // Window size input — always editable so the user can set it before
        // enabling the overlay, matching the relative-time UX pattern.
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Window:").small());
            let input_resp = ui.add(
                egui::TextEdit::singleline(&mut state.correlation_window_input)
                    .desired_width(40.0)
                    .hint_text("sec"),
            );
            ui.label(egui::RichText::new("sec").small());

            // Commit on Enter (same pattern as relative-time custom input).
            let committed = input_resp.lost_focus()
                && input_resp.ctx.input(|i| i.key_pressed(egui::Key::Enter));
            if committed {
                if let Ok(secs) = state.correlation_window_input.trim().parse::<i64>() {
                    let clamped = secs.clamp(
                        crate::util::constants::MIN_CORRELATION_WINDOW_SECS,
                        crate::util::constants::MAX_CORRELATION_WINDOW_SECS,
                    );
                    state.correlation_window_secs = clamped;
                    state.correlation_window_input = clamped.to_string();
                    state.update_correlation();
                } else {
                    // Reset to the current valid value on bad input.
                    state.correlation_window_input = state.correlation_window_secs.to_string();
                }
            }
        });
    }

    // Entry-count summary and "Copy Filtered" action at the bottom of the filter section.
    if !state.entries.is_empty() {
        ui.add_space(6.0);
        ui.separator();
        let total = state.entries.len();
        let filtered = state.filtered_indices.len();
        ui.horizontal(|ui| {
            if filtered == total {
                ui.label(format!("{total} entries"));
            } else {
                ui.label(format!("{filtered} / {total} entries"));
            }
            // Disabled when filtered set is empty (Rule 16: controls reflect valid actions).
            ui.add_enabled_ui(filtered > 0, |ui| {
                if ui
                    .add(egui::Button::new(
                        egui::RichText::new("\u{1f4cb} Copy").small(),
                    ))
                    .on_hover_text("Copy all filtered entries to clipboard as a plain-text report")
                    .clicked()
                {
                    let report = state.filtered_results_report();
                    ui.ctx().copy_text(report);
                    state.status_message =
                        format!("Copied {filtered} filtered entries to clipboard.");
                }
            });
        });
    }
}
