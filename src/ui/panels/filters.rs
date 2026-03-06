// LogSleuth - ui/panels/filters.rs
//
// Filter controls sidebar panel.
// Rule 16 compliance: controls disabled when their action is invalid;
// filter application is immediate on change.

use crate::app::state::AppState;
use crate::core::filter::DedupMode;
use crate::core::model::Severity;
use crate::core::multi_search::{MultiSearch, MultiSearchMode};
use crate::ui::theme;

/// Render the filter controls sidebar section.
pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("Filters")
        .on_hover_text("Narrow down which log entries are shown in the timeline. Filters are combined: an entry must match all active filters to appear.");
    ui.separator();

    // Troubleshoot mode warning banner — shown above all filter controls so the
    // user is aware that the dataset is pre-filtered at ingestion time and NOT
    // all severity levels are present in memory.
    if state.troubleshoot_mode {
        egui::Frame::new()
            .fill(egui::Color32::from_rgba_premultiplied(80, 20, 20, 200))
            .inner_margin(egui::Margin::same(6))
            .corner_radius(4.0)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new("\u{26a0} Troubleshoot Mode")
                            .strong()
                            .color(egui::Color32::from_rgb(248, 113, 113)),
                    );
                });
                ui.label(
                    egui::RichText::new(
                        "Only Critical and Error entries have been captured. \
                         Info, Warning, Debug, and Unknown entries were \
                         discarded at load time to reduce memory usage.",
                    )
                    .small()
                    .color(egui::Color32::from_rgb(252, 165, 165)),
                );
            });
        ui.add_space(4.0);
    }

    // Single row -- severity presets + utility actions combined.
    ui.horizontal_wrapped(|ui| {
        let fuzzy = state.filter_state.fuzzy;
        if ui
            .small_button("Errors only")
            .on_hover_text("Show only Critical and Error entries, hiding everything else")
            .clicked()
        {
            let current_files = std::mem::take(&mut state.filter_state.source_files);
            let hide_all = state.filter_state.hide_all_sources;
            state.filter_state = crate::core::filter::FilterState::errors_only_from(fuzzy);
            state.filter_state.source_files = current_files;
            state.filter_state.hide_all_sources = hide_all;
            state.apply_filters();
        }
        if ui
            .small_button("Errors + Warn")
            .on_hover_text("Show Critical, Error, and Warning entries")
            .clicked()
        {
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
        if ui
            .small_button("Clear")
            .on_hover_text("Remove all active filters and show every entry")
            .clicked()
        {
            state.filter_state = crate::core::filter::FilterState {
                fuzzy,
                ..Default::default()
            };
            state.apply_filters();
        }

        // Summary shortcut (disabled when no filtered entries yet)
        let has_entries = !state.filtered_indices.is_empty();
        ui.add_enabled_ui(has_entries, |ui| {
            if ui
                .small_button("Summary")
                .on_hover_text(
                    "Open a severity breakdown and message preview of the filtered entries",
                )
                .clicked()
            {
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
    ui.label("Severity:").on_hover_text(
        "Check the severity levels you want to see. Unchecked levels are hidden from the timeline.",
    );
    let mut changed = false;
    for severity in Severity::all() {
        let colour = theme::severity_colour(severity, state.dark_mode);
        let label = egui::RichText::new(severity.label()).color(colour);
        let mut checked = state.filter_state.severity_levels.contains(severity);
        let tooltip = match severity {
            Severity::Critical => "Fatal errors that crash or halt a service",
            Severity::Error => "Failures requiring attention but not necessarily fatal",
            Severity::Warning => "Potential problems or degraded conditions",
            Severity::Info => "Normal operational messages",
            Severity::Debug => "Verbose diagnostic output for developers",
            Severity::Unknown => "Entries whose severity could not be determined",
        };
        if ui
            .checkbox(&mut checked, label)
            .on_hover_text(tooltip)
            .changed()
        {
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
    ui.label("Text search:")
        .on_hover_text("Filter entries whose message contains this text. Toggle the ~ button for fuzzy (non-contiguous) matching.");
    ui.horizontal(|ui| {
        if ui
            .text_edit_singleline(&mut state.filter_state.text_search)
            .on_hover_text(
                "Type to search. Matches anywhere in the log message (case-insensitive).",
            )
            .changed()
        {
            // Debounce: mark filter dirty rather than calling apply_filters()
            // immediately.  The render loop fires apply_filters() once the text
            // has been unchanged for FILTER_DEBOUNCE_MS ms, preventing an O(n)
            // filter pass on every individual keystroke.
            state.filter_dirty_at = Some(std::time::Instant::now());
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
            .on_hover_text(
                "Toggle fuzzy matching.\n\
                 When ON, your search term is treated as a sequence of characters \
                 that must all appear in order, but not necessarily adjacent \
                 — e.g. \"cnerr\" matches \"Connection error\".\n\
                 When OFF, only exact substring matches are shown.",
            )
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
    ui.label("Regex:")
        .on_hover_text(r"Filter entries using a regular expression. Examples: ^ERROR, timeout|refused, \d{3}\.\d{3}");
    let re_changed = ui
        .text_edit_singleline(&mut state.filter_state.regex_pattern)
        .on_hover_text("Regex applied to the full message text (case-insensitive). Invalid patterns are highlighted in red.")
        .changed();
    if re_changed {
        // Compile the regex immediately so the valid/invalid indicator updates
        // on every keystroke without waiting for the debounce to fire.
        let pattern = state.filter_state.regex_pattern.clone();
        let _ = state.filter_state.set_regex(&pattern);
        // Defer the O(n) filter rebuild until the user pauses typing.
        state.filter_dirty_at = Some(std::time::Instant::now());
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

    ui.add_space(4.0);

    // -------------------------------------------------------------------------
    // Exclusion (NOT) filter
    // -------------------------------------------------------------------------
    ui.label("Exclude:")
        .on_hover_text("Entries whose message, thread, or component contains this text are HIDDEN (case-insensitive). Useful for removing known noise such as 'heartbeat' or 'health check'.");
    ui.horizontal(|ui| {
        if ui
            .text_edit_singleline(&mut state.filter_state.exclude_text)
            .on_hover_text(
                "NOT-match filter: entries containing this text in any field are hidden.\n\
                 Complements the include text filter above.",
            )
            .changed()
        {
            state.filter_dirty_at = Some(std::time::Instant::now());
        }
        // Clear button
        if !state.filter_state.exclude_text.is_empty()
            && ui
                .add(
                    egui::Button::new(
                        egui::RichText::new("\u{d7}")
                            .small()
                            .color(egui::Color32::from_rgb(156, 163, 175)),
                    )
                    .frame(false),
                )
                .on_hover_text("Clear exclusion filter")
                .clicked()
        {
            state.filter_state.exclude_text.clear();
            state.apply_filters();
        }
    });
    if !state.filter_state.exclude_text.is_empty() {
        ui.label(
            egui::RichText::new("NOT active")
                .small()
                .color(egui::Color32::from_rgb(248, 113, 113)),
        )
        .on_hover_text("Entries containing the exclusion term are hidden");
    }

    // -------------------------------------------------------------------------
    // Deduplication mode
    // -------------------------------------------------------------------------
    ui.add_space(4.0);
    ui.label("Deduplicate:").on_hover_text(
        "Collapse duplicate log messages so only the latest occurrence per file is shown.\n\n\
             Off: show all entries as-is.\n\
             Exact match: group entries with identical message text.\n\
             Normalized: group entries after replacing variable data (IPs, GUIDs, hex, numbers)\n\
             with tokens, so messages differing only in variable parts are treated as duplicates.",
    );
    let current_mode = state.filter_state.dedup_mode;
    egui::ComboBox::from_id_salt("dedup_mode")
        .selected_text(current_mode.label())
        .width(110.0)
        .show_ui(ui, |ui| {
            for &mode in DedupMode::all() {
                let tooltip = match mode {
                    DedupMode::Off => "Show all entries without deduplication",
                    DedupMode::Exact => "Group entries with exactly the same message text (per file)",
                    DedupMode::Normalized => {
                        "Group entries after normalising IPs, GUIDs, hex strings, and numbers (per file)"
                    }
                };
                if ui
                    .selectable_value(&mut state.filter_state.dedup_mode, mode, mode.label())
                    .on_hover_text(tooltip)
                    .changed()
                {
                    state.apply_filters();
                }
            }
        });
    // Show active indicator when dedup is on
    if current_mode != DedupMode::Off {
        let deduped_count = state.dedup_info.len();
        let total_hidden: usize = state
            .dedup_info
            .values()
            .map(|d| d.count.saturating_sub(1))
            .sum();
        ui.label(
            egui::RichText::new(format!(
                "{deduped_count} unique ({total_hidden} duplicates hidden)"
            ))
            .small()
            .color(egui::Color32::from_rgb(168, 85, 247)),
        )
        .on_hover_text(
            "Number of unique message groups shown and how many duplicate entries are collapsed",
        );
    }

    ui.add_space(6.0);
    ui.separator();

    // -------------------------------------------------------------------------
    // Multi-term search
    // -------------------------------------------------------------------------
    render_multi_search(ui, state);

    ui.add_space(6.0);
    ui.separator();

    // -------------------------------------------------------------------------
    // Time range filter
    // -------------------------------------------------------------------------
    ui.label("Time range:")
        .on_hover_text("Show only entries from within a rolling time window. The window moves forward with the clock so Live Tail entries stay visible.");

    // Quick-select buttons (toggle: click active button to clear it)
    ui.horizontal_wrapped(|ui| {
        for &(label, secs) in &[
            ("1m", 60u64),
            ("15m", 15 * 60),
            ("1h", 3_600),
            ("6h", 21_600),
            ("24h", 86_400),
        ] {
            let tooltip = match label {
                "1m" => "Show entries from the last 1 minute. Click again to clear.",
                "15m" => "Show entries from the last 15 minutes. Click again to clear.",
                "1h" => "Show entries from the last 1 hour. Click again to clear.",
                "6h" => "Show entries from the last 6 hours. Click again to clear.",
                "24h" => "Show entries from the last 24 hours. Click again to clear.",
                _ => "Show entries from this rolling time window.",
            };
            let active = state.filter_state.relative_time_secs == Some(secs);
            if ui
                .selectable_label(active, label)
                .on_hover_text(tooltip)
                .clicked()
            {
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
        let resp = ui
            .add(
                egui::TextEdit::singleline(&mut state.filter_state.relative_time_input)
                    .desired_width(42.0)
                    .hint_text("min"),
            )
            .on_hover_text(
                "Type a whole number and press Enter or Tab to set a custom rolling window.",
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
                // Bug fix: use checked_mul to prevent u64 overflow (silent
                // wrapping in release / panic in debug) for very large inputs.
                // Bug fix: cap to MAX_CUSTOM_TIME_MINUTES so users cannot set
                // meaningless multi-million-year windows that bypass the filter.
                if mins > 0 && mins <= crate::util::constants::MAX_CUSTOM_TIME_MINUTES {
                    if let Some(secs) = mins.checked_mul(60) {
                        state.filter_state.relative_time_secs = Some(secs);
                        state.apply_filters();
                    }
                } else {
                    // Out-of-range (0 or exceeds cap): reset the field to the
                    // currently applied value so it doesn't display stale text
                    // that no longer matches the active filter.  Mirrors the
                    // reset pattern used by the correlation window input.
                    state.filter_state.relative_time_input = state
                        .filter_state
                        .relative_time_secs
                        .map(|s| (s / 60).to_string())
                        .unwrap_or_default();
                }
            } else if !trimmed.is_empty() {
                // Non-numeric text: reset to the currently applied value.
                state.filter_state.relative_time_input = state
                    .filter_state
                    .relative_time_secs
                    .map(|s| (s / 60).to_string())
                    .unwrap_or_default();
            }
        }

        // Clear button when a relative window is set
        if state.filter_state.relative_time_secs.is_some()
            && ui
                .small_button("\u{2715}")
                .on_hover_text("Clear the rolling time window")
                .clicked()
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
        )
        .on_hover_text("Only entries timestamped after this time are shown");
    }
    // When Live Tail is active and a rolling window is set, confirm the window is
    // continuously advancing with each frame so the user knows new tail entries
    // entering the window will appear automatically.
    if state.tail_active && state.filter_state.relative_time_secs.is_some() {
        ui.label(
            egui::RichText::new("\u{25cf} Rolling window (live)")
                .small()
                .color(egui::Color32::from_rgb(34, 197, 94)),
        )
        .on_hover_text("The time window advances every frame so Live Tail entries stay visible.");
    }

    // -------------------------------------------------------------------------
    // Absolute date / time range
    // -------------------------------------------------------------------------
    // Provides pinned investigation windows (e.g. "show everything from the
    // 2-hour incident window on 2026-01-15").  Setting an absolute bound clears
    // the rolling relative window — they are mutually exclusive.
    // Accepted formats: "YYYY-MM-DD HH:MM:SS", "YYYY-MM-DD HH:MM", "YYYY-MM-DD"
    // All interpreted as local time and converted to UTC internally.
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Absolute range:").small().weak())
        .on_hover_text(
            "Pin an exact investigation window using local date/time.\n\
             Formats: YYYY-MM-DD  or  YYYY-MM-DD HH:MM  or  YYYY-MM-DD HH:MM:SS\n\
             Setting an absolute bound clears the rolling time window.",
        );

    // -- From / To -- laid out in a two-column grid so the labels share a
    // fixed-width column and both input fields start at the same horizontal
    // position regardless of label length.
    egui::Grid::new("abs_time_grid")
        .num_columns(3)
        .spacing([4.0, 4.0])
        .show(ui, |ui| {
            // --- From row ---
            ui.label(egui::RichText::new("From:").small());
            let from_resp = ui
                .add(
                    egui::TextEdit::singleline(&mut state.filter_state.abs_time_start_input)
                        .desired_width(150.0)
                        .hint_text("YYYY-MM-DD HH:MM"),
                )
                .on_hover_text(
                    "Earliest log time to show (inclusive). Tab or click away to apply.\n\
                     Clears the rolling time window when set.",
                );
            // Commit when focus leaves the field (Tab / click-away / Enter).
            if from_resp.lost_focus() {
                let s = state.filter_state.abs_time_start_input.clone();
                if s.trim().is_empty() {
                    state.filter_state.time_start = None;
                    state.apply_filters();
                } else if let Some(dt) = crate::app::state::parse_filter_datetime(&s) {
                    state.filter_state.relative_time_secs = None;
                    state.filter_state.relative_time_input.clear();
                    state.filter_state.time_start = Some(dt);
                    state.apply_filters();
                } else {
                    state.filter_state.abs_time_start_input = state
                        .filter_state
                        .time_start
                        .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
                        .unwrap_or_default();
                }
            }
            // Validity indicator (occupies column 3).
            if !state.filter_state.abs_time_start_input.is_empty() {
                let valid = crate::app::state::parse_filter_datetime(
                    &state.filter_state.abs_time_start_input,
                )
                .is_some();
                if valid {
                    ui.label(
                        egui::RichText::new("\u{2713}")
                            .small()
                            .color(egui::Color32::from_rgb(74, 222, 128)),
                    );
                } else {
                    ui.label(
                        egui::RichText::new("\u{2717}")
                            .small()
                            .color(egui::Color32::from_rgb(248, 113, 113)),
                    );
                }
            } else {
                ui.label(""); // keep column 3 present so grid stays stable
            }
            ui.end_row();

            // --- To row ---
            ui.label(egui::RichText::new("To:").small());
            let to_resp = ui
                .add(
                    egui::TextEdit::singleline(&mut state.filter_state.abs_time_end_input)
                        .desired_width(150.0)
                        .hint_text("YYYY-MM-DD HH:MM"),
                )
                .on_hover_text(
                    "Latest log time to show (inclusive). Tab or click away to apply.\n\
                     Clears the rolling time window when set.",
                );
            if to_resp.lost_focus() {
                let s = state.filter_state.abs_time_end_input.clone();
                if s.trim().is_empty() {
                    state.filter_state.time_end = None;
                    state.apply_filters();
                } else if let Some(dt) = crate::app::state::parse_filter_datetime(&s) {
                    state.filter_state.relative_time_secs = None;
                    state.filter_state.relative_time_input.clear();
                    state.filter_state.time_end = Some(dt);
                    state.apply_filters();
                } else {
                    state.filter_state.abs_time_end_input = state
                        .filter_state
                        .time_end
                        .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
                        .unwrap_or_default();
                }
            }
            if !state.filter_state.abs_time_end_input.is_empty() {
                let valid = crate::app::state::parse_filter_datetime(
                    &state.filter_state.abs_time_end_input,
                )
                .is_some();
                if valid {
                    ui.label(
                        egui::RichText::new("\u{2713}")
                            .small()
                            .color(egui::Color32::from_rgb(74, 222, 128)),
                    );
                } else {
                    ui.label(
                        egui::RichText::new("\u{2717}")
                            .small()
                            .color(egui::Color32::from_rgb(248, 113, 113)),
                    );
                }
            } else {
                ui.label("");
            }
            ui.end_row();
        });

    // Clear absolute bounds button (only shown when at least one bound is set
    // via an absolute input -- i.e. the rolling window is not responsible).
    let abs_active = state.filter_state.relative_time_secs.is_none()
        && (state.filter_state.time_start.is_some() || state.filter_state.time_end.is_some());
    if abs_active
        && ui
            .small_button("\u{2715} Clear abs. range")
            .on_hover_text("Remove the pinned date/time range")
            .clicked()
    {
        state.filter_state.time_start = None;
        state.filter_state.time_end = None;
        state.filter_state.abs_time_start_input.clear();
        state.filter_state.abs_time_end_input.clear();
        state.apply_filters();
    }

    // "Hide rows with no timestamp" checkbox -- always visible in the time section.
    // Entries whose source text contains no parseable date/time are excluded when
    // this is checked, even if no time-range bounds are active.
    ui.add_space(4.0);
    let mut hide_no_ts = state.filter_state.hide_no_timestamp;
    if ui
        .checkbox(&mut hide_no_ts, "Hide rows with no timestamp")
        .on_hover_text(
            "When checked, entries that have no date/time in their log text are removed\n\
             from the timeline. This includes lines that fall back to the file's last-\n\
             modified time as an estimate.\n\
             Useful when you need precise event times and do not want approximate rows\n\
             mixed in.",
        )
        .changed()
    {
        state.filter_state.hide_no_timestamp = hide_no_ts;
        state.apply_filters();
    }

    // -------------------------------------------------------------------------
    // Time correlation overlay controls
    // -------------------------------------------------------------------------
    // Only shown once entries are loaded — the feature requires a selection.
    if !state.entries.is_empty() {
        ui.add_space(6.0);
        ui.separator();
        ui.label("Correlation:")
            .on_hover_text("Highlight entries from other files that occurred within a time window around the selected entry. Useful for cross-log troubleshooting.");

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
            ui.label(egui::RichText::new("Window:").small())
                .on_hover_text(
                    "How many seconds either side of the selected entry to highlight",
                );
            let input_resp = ui
                .add(
                    egui::TextEdit::singleline(&mut state.correlation_window_input)
                        .desired_width(40.0)
                        .hint_text("sec"),
                )
                .on_hover_text(
                    "Type a number of seconds and press Enter or Tab to apply. \
                     Entries within this window on either side of the current selection are highlighted in teal.",
                );
            ui.label(egui::RichText::new("sec").small());

            // Commit when focus leaves the field (same fix as the relative-time
            // custom input: previously this required both lost_focus AND
            // key_pressed(Enter), which silently discarded a value typed by the
            // user who clicked away without pressing Enter).
            let committed = input_resp.lost_focus();
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
        // -------------------------------------------------------------------------
        // Component filter (only shown when entries contain component data)
        // -------------------------------------------------------------------------
        let component_values: Vec<String> = state.unique_component_values.clone();
        if !component_values.is_empty() {
            ui.add_space(6.0);
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Component:").on_hover_text(
                    "Show only entries from selected components or modules.\n\
                     When nothing is checked all components are shown.",
                );
                if !state.filter_state.component_filter.is_empty()
                    && ui
                        .small_button("\u{d7} clear")
                        .on_hover_text("Remove component filter (show all components)")
                        .clicked()
                {
                    state.filter_state.component_filter.clear();
                    state.apply_filters();
                }
            });
            let mut comp_changed = false;
            for c in &component_values {
                let mut checked = state.filter_state.component_filter.contains(c.as_str());
                if ui
                    .checkbox(&mut checked, c)
                    .on_hover_text(format!("Show only entries from component '{c}'"))
                    .changed()
                {
                    if checked {
                        state.filter_state.component_filter.insert(c.clone());
                    } else {
                        state.filter_state.component_filter.remove(c.as_str());
                    }
                    comp_changed = true;
                }
            }
            if comp_changed {
                state.apply_filters();
            }
        }

        ui.add_space(6.0);
        ui.separator();
        let total = state.entries.len();
        let filtered = state.filtered_indices.len();
        ui.horizontal(|ui| {
            if filtered == total {
                ui.label(format!("{total} entries"))
                    .on_hover_text("Total number of log entries loaded in this session");
            } else {
                ui.label(format!("{filtered} / {total} entries"))
                    .on_hover_text(format!(
                        "{filtered} entries match the current filters out of {total} total"
                    ));
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

/// Render the multi-term search section inside the filter panel.
///
/// Provides a collapsible UI for entering multiple search terms (one per
/// line or comma-separated), choosing match mode (ANY/ALL), and toggling
/// per-term options (case, whole word, regex).  Feeds into
/// `FilterState::multi_search` via debounced recompilation.
fn render_multi_search(ui: &mut egui::Ui, state: &mut AppState) {
    let ms = &state.filter_state.multi_search;
    let active = ms.is_active();
    let has_error = ms.compile_error.is_some();
    let term_count = ms.include_terms.len() + ms.exclude_terms.len();

    // Section heading with active indicator
    let heading_text = if active {
        format!("Multi-term Search ({term_count})")
    } else {
        "Multi-term Search".to_string()
    };
    let heading_colour = if active {
        egui::Color32::from_rgb(96, 165, 250) // blue when active
    } else if has_error {
        egui::Color32::from_rgb(248, 113, 113) // red on error
    } else {
        ui.style().visuals.text_color()
    };

    let id = ui.make_persistent_id("multi_search_section");
    egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false)
        .show_header(ui, |ui| {
            ui.label(egui::RichText::new(heading_text).color(heading_colour))
                .on_hover_text(
                    "Search for multiple terms simultaneously.\n\n\
                     Enter one term per line, or separate with commas.\n\
                     Prefix a term with - or ! to exclude entries containing it.\n\n\
                     Examples:\n  error\n  timeout\n  -heartbeat\n  !noise",
                );
        })
        .body(|ui| {
            // Multiline text input for terms
            ui.label(egui::RichText::new("Terms (one per line or comma-separated):").small())
                .on_hover_text(
                    "Enter search terms, one per line or separated by commas.\n\
                     Prefix with - or ! to make a NOT (exclusion) term.\n\n\
                     Examples:\n  error, timeout, refused\n  -heartbeat\n  !healthcheck",
                );

            let text_resp = ui.add(
                egui::TextEdit::multiline(&mut state.multi_search_input)
                    .desired_rows(4)
                    .desired_width(f32::INFINITY)
                    .hint_text("error, timeout\n-heartbeat")
                    .font(egui::TextStyle::Monospace),
            );
            if text_resp
                .on_hover_text(
                    "Type search terms here. One per line, or comma-separated.\n\
                     Prefix with - or ! to exclude matching entries.",
                )
                .changed()
            {
                recompile_multi_search(state);
                state.filter_dirty_at = Some(std::time::Instant::now());
            }

            ui.add_space(2.0);

            // Mode selector + min match on the same row
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Mode:").small())
                    .on_hover_text(
                        "ANY (OR): show entries matching at least one include term.\n\
                         ALL (AND): show only entries matching every include term.",
                    );
                let prev_mode = state.filter_state.multi_search.mode;
                egui::ComboBox::from_id_salt("ms_mode")
                    .selected_text(state.filter_state.multi_search.mode.label())
                    .width(90.0)
                    .show_ui(ui, |ui| {
                        for &mode in MultiSearchMode::all() {
                            let tip = match mode {
                                MultiSearchMode::Any => {
                                    "OR logic: entries matching ANY one of the include terms are shown"
                                }
                                MultiSearchMode::All => {
                                    "AND logic: only entries matching ALL include terms are shown"
                                }
                            };
                            ui.selectable_value(
                                &mut state.filter_state.multi_search.mode,
                                mode,
                                mode.label(),
                            )
                            .on_hover_text(tip);
                        }
                    });
                if state.filter_state.multi_search.mode != prev_mode {
                    // Mode changed but patterns haven't — no recompile needed,
                    // just re-evaluate filters.
                    state.filter_dirty_at = Some(std::time::Instant::now());
                }
            });

            // Min-match threshold (only meaningful for ANY mode with 2+ terms)
            let include_count = state.filter_state.multi_search.include_terms.len();
            if include_count >= 2 {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Min match:").small())
                        .on_hover_text(
                            "Minimum number of include terms that must match for an entry to pass.\n\
                             Default: 1 for ANY mode, all for ALL mode.\n\
                             Set to 2+ for threshold matching (e.g. 'at least 3 of 5 terms').",
                        );

                    let max = include_count;
                    let mut val = state
                        .filter_state
                        .multi_search
                        .min_match
                        .unwrap_or(match state.filter_state.multi_search.mode {
                            MultiSearchMode::Any => 1,
                            MultiSearchMode::All => max,
                        });

                    let slider = egui::Slider::new(&mut val, 1..=max)
                        .clamping(egui::SliderClamping::Always)
                        .integer();
                    if ui
                        .add(slider)
                        .on_hover_text(format!(
                            "Require at least this many of the {include_count} include terms to match.\n\
                             Currently: {val} of {include_count}.",
                        ))
                        .changed()
                    {
                        state.filter_state.multi_search.min_match = Some(val);
                        state.filter_dirty_at = Some(std::time::Instant::now());
                    }
                });
            }

            ui.add_space(2.0);

            // Option toggles in a horizontal row
            ui.horizontal_wrapped(|ui| {
                // Case sensitivity toggle
                let mut case_i = state.filter_state.multi_search.case_insensitive;
                if ui
                    .checkbox(&mut case_i, "Ignore case")
                    .on_hover_text(
                        "When checked, matching is case-insensitive (e.g. 'Error' matches 'error').\n\
                         When unchecked, case must match exactly.",
                    )
                    .changed()
                {
                    state.filter_state.multi_search.case_insensitive = case_i;
                    recompile_multi_search(state);
                    state.filter_dirty_at = Some(std::time::Instant::now());
                }

                // Whole word toggle
                let mut ww = state.filter_state.multi_search.whole_word;
                if ui
                    .checkbox(&mut ww, "Whole word")
                    .on_hover_text(
                        "When checked, terms only match at word boundaries.\n\
                         'error' matches 'an error occurred' but NOT 'errors' or 'myerror'.\n\
                         Uses \\b (word boundary) anchors around each term.",
                    )
                    .changed()
                {
                    state.filter_state.multi_search.whole_word = ww;
                    recompile_multi_search(state);
                    state.filter_dirty_at = Some(std::time::Instant::now());
                }

                // Regex mode toggle
                let mut rx = state.filter_state.multi_search.regex_mode;
                if ui
                    .checkbox(&mut rx, "Regex")
                    .on_hover_text(
                        "When checked, terms are treated as regular expressions.\n\
                         When unchecked, terms are treated as literal text (special regex characters are escaped).\n\n\
                         Example regex terms:\n  \\d{3}\\.\\d{3}\n  timeout|refused\n  ^ERROR",
                    )
                    .changed()
                {
                    state.filter_state.multi_search.regex_mode = rx;
                    recompile_multi_search(state);
                    state.filter_dirty_at = Some(std::time::Instant::now());
                }
            });

            // Clear button
            if !state.multi_search_input.is_empty() {
                ui.horizontal(|ui| {
                    if ui
                        .small_button("\u{d7} Clear terms")
                        .on_hover_text("Remove all multi-search terms and reset to defaults")
                        .clicked()
                    {
                        state.multi_search_input.clear();
                        state.filter_state.multi_search = MultiSearch::default();
                        state.apply_filters();
                    }
                });
            }

            // Compile error feedback
            if let Some(ref err) = state.filter_state.multi_search.compile_error {
                ui.colored_label(
                    egui::Color32::from_rgb(248, 113, 113),
                    format!("\u{2717} {err}"),
                )
                .on_hover_text(
                    "One or more search terms failed to compile. Check for invalid regex syntax.",
                );
            } else if active {
                let inc = state.filter_state.multi_search.include_terms.len();
                let exc = state.filter_state.multi_search.exclude_terms.len();
                let mode = state.filter_state.multi_search.mode.label();
                let summary = if exc > 0 {
                    format!("\u{2713} {inc} include, {exc} exclude ({mode})")
                } else {
                    format!("\u{2713} {inc} terms ({mode})")
                };
                ui.colored_label(egui::Color32::from_rgb(74, 222, 128), summary)
                    .on_hover_text(
                        "Multi-term search is active. Entries are filtered according to the terms and mode above.",
                    );
            }
        });
}

/// Re-parse and recompile the multi-search terms from the raw input buffer.
fn recompile_multi_search(state: &mut AppState) {
    let (include, exclude) = MultiSearch::parse_terms(&state.multi_search_input);
    state.filter_state.multi_search.include_terms = include;
    state.filter_state.multi_search.exclude_terms = exclude;
    state.filter_state.multi_search.compile();
}
