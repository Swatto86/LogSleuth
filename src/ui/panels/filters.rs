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

    // Quick-filter buttons
    ui.horizontal_wrapped(|ui| {
        let fuzzy = state.filter_state.fuzzy;
        if ui.small_button("Errors only").clicked() {
            let current_files = std::mem::take(&mut state.filter_state.source_files);
            state.filter_state = crate::core::filter::FilterState::errors_only_from(fuzzy);
            state.filter_state.source_files = current_files;
            state.apply_filters();
        }
        if ui.small_button("Errors + Warn").clicked() {
            let current_files = std::mem::take(&mut state.filter_state.source_files);
            state.filter_state = crate::core::filter::FilterState::errors_and_warnings_from(fuzzy);
            state.filter_state.source_files = current_files;
            state.apply_filters();
        }
        if ui.small_button("Clear").clicked() {
            state.filter_state = crate::core::filter::FilterState {
                fuzzy,
                ..Default::default()
            };
            state.apply_filters();
        }
    });

    ui.add_space(6.0);
    ui.separator();

    // Severity checkboxes with severity-coloured labels
    ui.label("Severity:");
    let mut changed = false;
    for severity in Severity::all() {
        let colour = theme::severity_colour(severity);
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
            ("15m", 15u64 * 60),
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

        let committed = resp.lost_focus() && resp.ctx.input(|i| i.key_pressed(egui::Key::Enter));
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

    // -------------------------------------------------------------------------
    // Source file filter (only shown once files have been discovered)
    // -------------------------------------------------------------------------
    if !state.discovered_files.is_empty() {
        ui.add_space(6.0);
        ui.separator();

        // Pre-collect (path, name) so we can borrow mutably later
        let file_paths: Vec<(std::path::PathBuf, String)> = state
            .discovered_files
            .iter()
            .map(|f| {
                let name = f
                    .path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("?")
                    .to_string();
                (f.path.clone(), name)
            })
            .collect();

        let total = file_paths.len();
        let active_count = state.filter_state.source_files.len();

        // Header row: label + active count + global reset
        ui.horizontal(|ui| {
            if active_count == 0 {
                ui.label(egui::RichText::new(format!("Files ({total}):")).strong());
            } else {
                ui.label(
                    egui::RichText::new(format!("{active_count}/{total} files"))
                        .strong()
                        .color(egui::Color32::from_rgb(96, 165, 250)),
                );
            }
            if active_count > 0 && ui.small_button("All").clicked() {
                state.filter_state.source_files.clear();
                state.apply_filters();
            }
        });

        // Search box -- only render when there are enough files to warrant it
        if total > 8 {
            ui.add(
                egui::TextEdit::singleline(&mut state.file_list_search)
                    .hint_text("\u{1f50d} search files\u{2026}")
                    .desired_width(f32::INFINITY),
            );
        }

        // Apply search to build the visible subset
        let search_lower = state.file_list_search.to_lowercase();
        let visible: Vec<&(std::path::PathBuf, String)> = file_paths
            .iter()
            .filter(|(_, name)| {
                search_lower.is_empty() || name.to_lowercase().contains(&search_lower)
            })
            .collect();

        let visible_count = visible.len();

        // Select All / None operating on the VISIBLE subset
        if visible_count > 1 {
            ui.horizontal(|ui| {
                if ui.small_button("Select all").clicked() {
                    // Remove all visible paths from the exclusion set (include them)
                    for (path, _) in &visible {
                        state.filter_state.source_files.remove(path);
                    }
                    // If the set is now empty every file is included -- "all pass"
                    state.apply_filters();
                }
                if ui.small_button("None").clicked() {
                    // Seed the whitelist with every file, then remove the visible ones
                    if state.filter_state.source_files.is_empty() {
                        for (path, _) in &file_paths {
                            state.filter_state.source_files.insert(path.clone());
                        }
                    }
                    for (path, _) in &visible {
                        state.filter_state.source_files.remove(path);
                    }
                    state.apply_filters();
                }
                if !search_lower.is_empty() {
                    ui.label(
                        egui::RichText::new(format!("{visible_count}/{total}"))
                            .small()
                            .weak(),
                    );
                }
            });
        }

        // Fixed-height scrollable checklist (180 px regardless of item count)
        egui::ScrollArea::vertical()
            .id_salt("filter_file_list")
            .max_height(180.0)
            .show(ui, |ui| {
                for (path, name) in &visible {
                    // Empty source_files == all pass, so show as checked
                    let mut checked = state.filter_state.source_files.is_empty()
                        || state.filter_state.source_files.contains(path);

                    ui.horizontal(|ui| {
                        // Coloured dot matching timeline file stripe.
                        let dot_colour = state.colour_for_file(path);
                        let (dot_rect, _) =
                            ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                        ui.painter()
                            .circle_filled(dot_rect.center(), 4.0, dot_colour);

                        if ui
                            .checkbox(&mut checked, egui::RichText::new(name.as_str()).small())
                            .changed()
                        {
                            if !checked {
                                if state.filter_state.source_files.is_empty() {
                                    for (other_path, _) in &file_paths {
                                        state.filter_state.source_files.insert(other_path.clone());
                                    }
                                }
                                state.filter_state.source_files.remove(path);
                            } else {
                                state.filter_state.source_files.insert((*path).clone());
                                if state.filter_state.source_files.len() == total {
                                    state.filter_state.source_files.clear();
                                }
                            }
                            state.apply_filters();
                        }

                        // Solo button.
                        let already_solo = state.filter_state.source_files.len() == 1
                            && state.filter_state.source_files.contains(path);
                        let solo_colour = if already_solo {
                            egui::Color32::from_rgb(96, 165, 250)
                        } else {
                            egui::Color32::from_rgb(107, 114, 128)
                        };
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("solo").small().color(solo_colour),
                                )
                                .small()
                                .frame(false),
                            )
                            .on_hover_text("Show only this file")
                            .clicked()
                        {
                            if already_solo {
                                state.filter_state.source_files.clear();
                            } else {
                                state.filter_state.source_files.clear();
                                state.filter_state.source_files.insert((*path).clone());
                            }
                            state.apply_filters();
                        }
                    });
                }

                if visible.is_empty() {
                    ui.label(egui::RichText::new("No files match.").small().weak());
                }
            });
    }

    // Entry-count summary at the bottom of the filter section
    if !state.entries.is_empty() {
        ui.add_space(6.0);
        ui.separator();
        let total = state.entries.len();
        let filtered = state.filtered_indices.len();
        if filtered == total {
            ui.label(format!("{total} entries"));
        } else {
            ui.label(format!("{filtered} / {total} entries"));
        }
    }
}
