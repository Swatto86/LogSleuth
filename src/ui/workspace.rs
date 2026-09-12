//! Everyday workspace controls. File dialogs and exports are handled by the GUI owner.
use crate::app::state::AppState;
use egui::{RichText, Ui};

pub enum Action {
    OpenFolder,
    AddFiles,
    Export(bool),
}

pub fn toolbar(ui: &mut Ui, state: &mut AppState, exporting: bool) -> Option<Action> {
    let mut action = None;
    ui.horizontal_wrapped(|ui| {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
        ui.label(RichText::new("LogSleuth").size(20.0).strong());
        ui.separator();
        if ui
            .add_enabled(!state.scan_in_progress, egui::Button::new("Open folder…"))
            .on_hover_text("Add a folder to this workspace · Ctrl+O")
            .clicked()
        {
            action = Some(Action::OpenFolder);
        }
        if ui
            .add_enabled(!state.scan_in_progress, egui::Button::new("Add log files…"))
            .on_hover_text("Add individual files without replacing your current workspace")
            .clicked()
        {
            action = Some(Action::AddFiles);
        }
        ui.separator();
        live_controls(ui, state);
        ui.separator();
        ui.add_enabled_ui(!exporting && !state.filtered_indices.is_empty(), |ui| {
            ui.menu_button("Export results", |ui| {
                ui.label(format!("{} matching entries", state.filtered_indices.len()));
                for (label, json) in [("CSV spreadsheet…", false), ("JSON data…", true)] {
                    if ui.button(label).clicked() {
                        action = Some(Action::Export(json));
                        ui.close_menu();
                    }
                }
            });
            if ui.button("Summary").clicked() {
                state.show_log_summary = true;
            }
        });
        if ui.button("Settings").clicked() {
            state.show_options = true;
        }
    });
    action
}

pub fn live_controls(ui: &mut Ui, state: &mut AppState) {
    if state.tail_active {
        if ui
            .button("Stop live tail")
            .on_hover_text("Stop capturing new lines; keep the entries already loaded")
            .clicked()
        {
            state.request_stop_tail = true;
        }
        ui.checkbox(&mut state.tail_auto_scroll, "Follow newest");
    } else if ui
        .add_enabled(
            !state.scan_in_progress && !state.entries.is_empty(),
            egui::Button::new("Start live tail"),
        )
        .on_hover_text("Load at least one log, then watch new lines as they arrive")
        .clicked()
    {
        state.request_start_tail = true;
    }
}

pub fn search(ui: &mut Ui, state: &mut AppState) {
    ui.horizontal_wrapped(|ui| {
        ui.heading("Timeline");
        ui.label(
            RichText::new(format!(
                "{} matching / {} loaded",
                state.filtered_indices.len(),
                state.entries.len()
            ))
            .weak(),
        );
    });
    ui.horizontal(|ui| {
        let width = (ui.available_width() - 130.0).max(120.0);
        let search = ui.add_sized(
            [width, 30.0],
            egui::TextEdit::singleline(&mut state.filter_state.text_search)
                .hint_text("Search messages…  Ctrl+F"),
        );
        if state.request_focus_text_search {
            state.request_focus_text_search = false;
            search.request_focus();
        }
        if search.changed() {
            state.filter_dirty_at = Some(std::time::Instant::now());
        }
        if ui
            .checkbox(&mut state.filter_state.fuzzy, "Fuzzy")
            .on_hover_text("Match characters in order even when other characters separate them")
            .changed()
        {
            state.apply_filters();
        }
        if !state.filter_state.text_search.is_empty() && ui.button("Clear").clicked() {
            state.filter_state.text_search.clear();
            state.apply_filters();
        }
    });
    if !state.filter_state.is_empty() || state.activity_window_secs.is_some() {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(state.filter_description()).small().weak());
            if ui.small_button("Edit filters").clicked() {
                state.sidebar_tab = 1;
            }
        });
    }
    ui.add_space(8.0);
}

pub fn welcome(ui: &mut Ui, state: &mut AppState) -> Option<Action> {
    let mut action = None;
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.add_space((ui.available_height() * 0.12).min(75.0));
        ui.heading(RichText::new("Find the story in your logs.").size(28.0));
        ui.add_space(8.0);
        ui.label("Bring your logs together, find what matters, and follow the details.");
        ui.add_space(24.0);
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(!state.scan_in_progress, egui::Button::new("Open a folder…").min_size(egui::vec2(170.0, 44.0))).clicked() {
                action = Some(Action::OpenFolder);
            }
            if ui.add_enabled(!state.scan_in_progress, egui::Button::new("Choose log files…").min_size(egui::vec2(170.0, 44.0))).clicked() {
                action = Some(Action::AddFiles);
            }
        });
        ui.add_space(28.0);
        for (title, description) in [
            ("1   Choose your sources", "Open a folder or add files. Use the Sources list to choose which logs are loaded."),
            ("2   Find what matters", "Search messages or use Filters to narrow by severity, time and more."),
            ("3   Inspect and share", "Select an entry to read its full message. Bookmark useful lines or export your results."),
        ] {
            egui::Frame::group(ui.style()).inner_margin(14).show(ui, |ui| {
                ui.set_min_width((ui.available_width() - 2.0).max(0.0));
                ui.label(RichText::new(title).strong());
                ui.label(RichText::new(description).weak());
            });
            ui.add_space(8.0);
        }
        if state.scan_in_progress { ui.spinner(); ui.label("Discovering logs… Your sources will appear on the left."); }
    });
    action
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_focus_request_accepts_typing_and_schedules_filtering() {
        let ctx = egui::Context::default();
        let mut state = AppState::new(vec![], false);
        state.request_focus_text_search = true;
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| search(ui, &mut state));
        });
        assert!(!state.request_focus_text_search);
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Text("timeout".into())],
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| search(ui, &mut state));
            },
        );
        assert_eq!(state.filter_state.text_search, "timeout");
        assert!(state.filter_dirty_at.is_some());
    }
}
