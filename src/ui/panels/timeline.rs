// LogSleuth - ui/panels/timeline.rs
//
// Virtual-scrolling unified timeline view.
//
// Uses egui's `ScrollArea::show_rows` which renders only the rows currently
// visible in the viewport, giving O(1) rendering cost regardless of entry count.
// Rule 16 compliance: selection is always valid; row clicks update state directly.
//
// Text contrast: each row is rendered with a LayoutJob that colours only the
// severity badge prefix ([CRIT], [ERR ], etc.) with the severity-specific hue,
// while the timestamp / filename / message body uses `theme::row_text_colour`
// (white in dark mode, near-black in light mode).  This guarantees that text
// remains readable even when a Critical or Error severity background tint is
// applied to the row (red-on-red contrast is avoided).

use crate::app::state::AppState;
use crate::core::filter::FilterState;
use crate::ui::theme;
use egui::text::{LayoutJob, TextFormat};

/// Render the timeline panel (central area).
pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    let filtered = state.filtered_indices.len();

    if filtered == 0 {
        ui.centered_and_justified(|ui| {
            // Three distinct empty states, each with its own guidance:
            //   1. No scan started yet              -> welcome screen
            //   2. Files discovered, nothing ticked -> prompt to tick
            //   3. Files ticked, no entries match   -> filter hint
            if state.discovered_files.is_empty() {
                // ---- State 1: no scan started yet ----
                ui.vertical_centered(|ui| {
                    ui.add_space(80.0);
                    ui.label(
                        egui::RichText::new("\u{1f50d}")
                            .size(48.0)
                            .color(egui::Color32::from_rgb(107, 114, 128)),
                    );
                    ui.add_space(12.0);
                    ui.label(
                        egui::RichText::new("Welcome to LogSleuth")
                            .size(20.0)
                            .strong(),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(
                            "Point at a directory of log files and LogSleuth will\n\
                             discover, parse, and merge them into a single timeline.\n\n\
                             To get started:\n\
                             \u{2022} Use File \u{2192} Open Directory to scan a folder\n\
                             \u{2022} Use File \u{2192} Open Log(s) to open specific files",
                        )
                        .color(egui::Color32::from_rgb(156, 163, 175)),
                    );
                });
            } else if state.filter_state.hide_all_sources {
                // ---- State 2: files discovered/loaded but nothing selected ----
                let n = state.discovered_files.len();
                let word = if n == 1 { "file" } else { "files" };
                ui.vertical_centered(|ui| {
                    ui.add_space(80.0);
                    ui.label(
                        egui::RichText::new("\u{2610}") // ballot box (empty checkbox)
                            .size(48.0)
                            .color(egui::Color32::from_rgb(107, 114, 128)),
                    );
                    ui.add_space(12.0);
                    ui.label(
                        egui::RichText::new(format!("{n} {word} discovered"))
                            .size(18.0)
                            .strong(),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(
                            "Tick files in the Files tab to load and view their entries.\n\
                             Use \u{201c}Select all\u{201d} to load everything at once.",
                        )
                        .color(egui::Color32::from_rgb(156, 163, 175)),
                    );
                });
            } else {
                // ---- State 3: files selected but nothing matches current filters ----
                ui.vertical_centered(|ui| {
                    ui.add_space(80.0);
                    ui.label(
                        egui::RichText::new("No entries match the current filters.")
                            .size(16.0)
                            .color(egui::Color32::from_rgb(156, 163, 175)),
                    );
                    ui.add_space(8.0);
                    // Surface which filters are currently active so the user
                    // understands why the timeline is empty.
                    let mut active_filters: Vec<String> = Vec::new();
                    let f = &state.filter_state;
                    if !f.severity_levels.is_empty() {
                        active_filters.push("Severity".to_string());
                    }
                    if !f.text_search.trim().is_empty() || !f.exclude_text.trim().is_empty() {
                        active_filters.push("Text search".to_string());
                    }
                    if f.regex_search.is_some() {
                        active_filters.push("Regex".to_string());
                    }
                    if f.relative_time_secs.is_some()
                        || f.time_start.is_some()
                        || f.time_end.is_some()
                    {
                        active_filters.push("Time range".to_string());
                    }
                    if !f.source_files.is_empty() {
                        active_filters.push("File filter".to_string());
                    }
                    if f.bookmarks_only {
                        active_filters.push("Bookmarks".to_string());
                    }
                    if f.hide_no_timestamp {
                        active_filters.push("Timestamped only".to_string());
                    }
                    if f.dedup_mode != crate::core::filter::DedupMode::Off {
                        active_filters.push("Dedup".to_string());
                    }
                    if let Some(secs) = state.activity_window_secs {
                        let label = if secs < 60 {
                            format!("Activity window: {}s", secs)
                        } else if secs < 3_600 {
                            format!("Activity window: {}m", secs / 60)
                        } else {
                            format!("Activity window: {}h", secs / 3_600)
                        };
                        active_filters.push(label);
                    }

                    if active_filters.is_empty() {
                        ui.label(
                            egui::RichText::new(
                                "Try adjusting severity, text, time range, or file filters\n\
                                 in the Filters tab on the left.",
                            )
                            .small()
                            .color(egui::Color32::from_rgb(107, 114, 128)),
                        );
                    } else {
                        ui.label(
                            egui::RichText::new(format!(
                                "All entries are hidden by: {}",
                                active_filters.join(" • ")
                            ))
                            .small()
                            .color(egui::Color32::from_rgb(107, 114, 128)),
                        );
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            if ui
                                .button("Clear filters")
                                .on_hover_text(
                                    "Reset severity, text, time, file, and regex filters",
                                )
                                .clicked()
                            {
                                state.filter_state = FilterState::default();
                                state.apply_filters();
                            }
                            if state.activity_window_secs.is_some()
                                && ui
                                    .button("Turn off activity window")
                                    .on_hover_text(
                                        "Show entries from all files, not just recent ones",
                                    )
                                    .clicked()
                            {
                                state.activity_window_secs = None;
                                state.activity_window_input.clear();
                                state.apply_filters();
                            }
                        });
                    }
                });
            }
        });
        return;
    }

    let font_size = state.ui_font_size;
    let row_height = theme::row_height(font_size);

    // Sort order toolbar -- compact single-line bar above the scroll area.
    ui.horizontal(|ui| {
        let (sort_label, sort_hint) = if state.sort_descending {
            (
                "\u{2193} Newest first",
                "Currently showing newest entries at the top. Click to switch to oldest-first order.",
            )
        } else {
            (
                "\u{2191} Oldest first",
                "Currently showing oldest entries at the top. Click to switch to newest-first order.",
            )
        };
        if ui
            .small_button(sort_label)
            .on_hover_text(sort_hint)
            .clicked()
        {
            state.toggle_sort_direction();
        }
    });
    ui.separator();

    // Ascending mode: stick to the bottom so the newest entry (at the end) stays
    // in view as the tail appends rows.  Descending mode: newest entry is already
    // at display_idx 0 (the top), so bottom-sticking is not wanted; instead we
    // snap to the top whenever `scroll_top_requested` is set by the tail loop.
    let stick = state.tail_active && state.tail_auto_scroll && !state.sort_descending;

    // Bookmark toggle and correlation refresh are collected here and applied
    // after show_rows so we do not mutable-borrow `state` while `entry` still
    // holds an immutable reference into `state.entries`.
    let mut bookmark_toggle: Option<u64> = None;
    let mut correlation_update_needed = false;

    // Deferred multi-select actions collected during show_rows and applied after.
    let mut click_action: Option<(usize, bool, bool)> = None; // (actual_idx, ctrl, shift)
    let mut context_menu_copy = false;

    // Build the scroll area; optionally snap to the top for descending mode.
    let snap_top = state.scroll_top_requested && state.sort_descending && state.tail_auto_scroll;
    // Bug fix: always clear the flag after reading it. Previously the flag
    // was only cleared when snap_top was true (sort_descending && tail_auto_scroll).
    // If the user toggled either setting between the frame that set the flag
    // and the next render, the flag persisted indefinitely and caused an
    // unexpected scroll-to-top when the original conditions were re-enabled.
    state.scroll_top_requested = false;
    let mut scroll_area = egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .stick_to_bottom(stick);
    if snap_top {
        scroll_area = scroll_area.scroll_offset(egui::vec2(0.0, 0.0));
    }
    scroll_area.show_rows(ui, row_height, filtered, |ui, row_range| {
        for display_idx in row_range {
            // When sort_descending the display positions are reversed:
            // display_idx 0 maps to the last element of filtered_indices
            // (the newest entry) and display_idx n-1 maps to the first.
            let actual_idx = if state.sort_descending {
                filtered.saturating_sub(1).saturating_sub(display_idx)
            } else {
                display_idx
            };
            let Some(&entry_idx) = state.filtered_indices.get(actual_idx) else {
                continue;
            };
            let Some(entry) = state.entries.get(entry_idx) else {
                continue;
            };

            let is_selected = state.selected_index == Some(actual_idx)
                || state.selected_indices.contains(&actual_idx);
            let sev_colour = theme::severity_colour(&entry.severity, state.dark_mode);
            let file_colour = state.colour_for_file(&entry.source_file);
            let entry_id = entry.id;
            let is_bookmarked = state.is_bookmarked(entry_id);
            let is_correlated = state.correlated_ids.contains(&entry_id);

            // Build a LayoutJob so the severity badge ([CRIT], [ERR ], etc.)
            // keeps its severity-specific hue while the rest of the row
            // (timestamp, filename, message) uses a high-contrast foreground
            // colour: white in dark mode, near-black in light mode.
            // This prevents red-on-red readability problems on Critical/Error
            // rows that carry a severity background tint.
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

            let font = egui::FontId::monospace(font_size);
            let body_colour = theme::row_text_colour(state.dark_mode);

            let mut row_job = LayoutJob::default();
            row_job.append(
                &format!("[{:<4}] ", entry.severity.short_label()),
                0.0,
                TextFormat {
                    font_id: font.clone(),
                    color: sev_colour,
                    ..Default::default()
                },
            );
            row_job.append(
                &format!("{} | {} | {}", ts, file_name, first_line),
                0.0,
                TextFormat {
                    font_id: font.clone(),
                    color: body_colour,
                    ..Default::default()
                },
            );
            // Dedup count badge: when dedup is active and this entry represents
            // a group of duplicates, append a purple "(xN)" suffix.
            if let Some(info) = state.dedup_info.get(&entry_idx) {
                if info.count > 1 {
                    row_job.append(
                        &format!(" (x{})", info.count),
                        0.0,
                        TextFormat {
                            font_id: font,
                            color: egui::Color32::from_rgb(168, 85, 247), // purple
                            ..Default::default()
                        },
                    );
                }
            }

            // Severity accent underline — a 2 px strip at the bottom of the
            // row in the severity colour.  Gives a clear visual cue without
            // washing out the entire row background.
            // Only Critical / Error / Warning get an underline.
            let show_severity_accent = matches!(
                entry.severity,
                crate::core::model::Severity::Critical
                    | crate::core::model::Severity::Error
                    | crate::core::model::Severity::Warning
            );

            // Save cursor position and available width BEFORE the row is
            // laid out, so we can paint the severity underline AFTER the
            // row content (fixing z-order: underline on top of selection).
            let row_top = ui.cursor().min;
            let full_width = ui.available_width();

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

            // Subtle blue tint on multi-selected rows (distinct from the
            // primary selection highlight managed by egui's selectable_label).
            let is_multi_selected = state.selected_indices.contains(&actual_idx);
            if is_multi_selected && state.selected_index != Some(actual_idx) {
                let tint_rect = egui::Rect::from_min_size(
                    ui.cursor().min,
                    egui::vec2(ui.available_width(), row_height),
                );
                ui.painter().rect_filled(
                    tint_rect,
                    0.0,
                    egui::Color32::from_rgba_premultiplied(59, 130, 246, 35),
                );
            }

            // Each row: 4 px coloured file stripe | star button | selectable label
            let response = ui
                .horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    // Coloured left stripe — visual CMTrace-style file indicator.
                    let (bar_rect, _) =
                        ui.allocate_exact_size(egui::vec2(4.0, row_height), egui::Sense::hover());
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
                                egui::RichText::new(star_char)
                                    .color(star_colour)
                                    .size((font_size * 0.85).round()),
                            )
                            .small()
                            .frame(false)
                            .min_size(egui::vec2((font_size * 1.1).round(), row_height)),
                        )
                        .on_hover_text(if is_bookmarked {
                            "Remove bookmark"
                        } else {
                            "Bookmark this entry"
                        });
                    if star_btn.clicked() {
                        bookmark_toggle = Some(entry_id);
                    }

                    ui.selectable_label(is_selected, row_job)
                })
                .inner;

            if response.clicked() {
                let modifiers = ui.input(|i| i.modifiers);
                click_action = Some((
                    actual_idx,
                    modifiers.ctrl || modifiers.mac_cmd,
                    modifiers.shift,
                ));
            }

            // Right-click context menu for copy operations.
            response.context_menu(|ui| {
                let has_multi = !state.selected_indices.is_empty();
                let count = state.selected_indices.len();
                let label = if has_multi {
                    format!("Copy {count} Selected Lines")
                } else {
                    "Copy This Line".to_string()
                };
                if ui.button(label).clicked() {
                    if has_multi {
                        context_menu_copy = true;
                    } else {
                        // No multi-select: copy just this row's raw text.
                        ui.ctx().copy_text(entry.raw_text.clone());
                    }
                    ui.close_menu();
                }
            });

            // Show full path + timestamp + severity as tooltip on hover.
            response.on_hover_ui(|ui| {
                ui.label(
                    egui::RichText::new(entry.severity.label())
                        .strong()
                        .color(sev_colour),
                );
                if let Some(ts_full) = entry.timestamp {
                    ui.label(ts_full.format("%Y-%m-%d %H:%M:%S UTC").to_string());
                }
                ui.label(
                    egui::RichText::new(entry.source_file.display().to_string())
                        .monospace()
                        .small(),
                );
                if let Some(ref thread) = entry.thread {
                    ui.label(
                        egui::RichText::new(format!("Thread: {thread}"))
                            .small()
                            .weak(),
                    );
                }
                ui.label(
                    egui::RichText::new(
                        "Click to select | Ctrl+Click to multi-select | Shift+Click for range",
                    )
                    .small()
                    .weak()
                    .italics(),
                );
            });

            // Paint severity accent underline AFTER the row content so it
            // renders on top of the selection/hover highlight (z-order fix).
            if show_severity_accent {
                let underline_rect = egui::Rect::from_min_size(
                    egui::pos2(row_top.x, row_top.y + row_height - 2.0),
                    egui::vec2(full_width, 2.0),
                );
                ui.painter().rect_filled(underline_rect, 0.0, sev_colour);
            }
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

    // Apply deferred multi-select click action.
    //
    // Modifiers:
    //   Plain click   -> single-select (clears multi-select)
    //   Ctrl+Click    -> toggle individual entry in multi-select
    //   Shift+Click   -> range-select from last selected_index to clicked row
    if let Some((actual_idx, ctrl, shift)) = click_action {
        if ctrl {
            // Toggle the clicked entry in the multi-select set.
            if state.selected_indices.contains(&actual_idx) {
                state.selected_indices.remove(&actual_idx);
            } else {
                state.selected_indices.insert(actual_idx);
            }
            // Update primary selection to the clicked entry for detail pane.
            state.selected_index = Some(actual_idx);
            correlation_update_needed = true;
        } else if shift {
            // Range select: from the anchor (selected_index) to the clicked row.
            if let Some(anchor) = state.selected_index {
                let lo = anchor.min(actual_idx);
                let hi = anchor.max(actual_idx);
                for i in lo..=hi {
                    state.selected_indices.insert(i);
                }
            } else {
                // No anchor: treat as single select.
                state.selected_indices.clear();
                state.selected_indices.insert(actual_idx);
                state.selected_index = Some(actual_idx);
            }
            correlation_update_needed = true;
        } else {
            // Plain click: single-select, clear multi-select.
            state.selected_indices.clear();
            state.selected_index = Some(actual_idx);
            correlation_update_needed = true;
        }
    }

    // Copy multi-selected entries to clipboard (deferred from context menu).
    if context_menu_copy {
        let report = state.selected_entries_report();
        ui.ctx().copy_text(report);
        let n = state.selected_indices.len();
        state.status_message = format!("Copied {n} selected entries to clipboard.");
    }

    // Recompute the correlation window for the newly selected entry (if any).
    // This is deferred from the click handler above so the &mut self call does
    // not conflict with the immutable entry borrow inside show_rows.
    if correlation_update_needed {
        state.update_correlation();
    }
}
