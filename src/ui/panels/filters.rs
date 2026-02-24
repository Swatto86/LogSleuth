// LogSleuth - ui/panels/filters.rs
//
// Filter controls sidebar.
// Implementation: next increment.

use crate::app::state::AppState;
use crate::core::model::Severity;

/// Render the filter controls.
pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("Filters");
    ui.separator();

    // Quick filters
    if ui.button("Errors Only").clicked() {
        state.filter_state = crate::core::filter::FilterState::errors_only();
        state.apply_filters();
    }
    if ui.button("Errors + Warnings").clicked() {
        state.filter_state = crate::core::filter::FilterState::errors_and_warnings();
        state.apply_filters();
    }
    if ui.button("Clear Filters").clicked() {
        state.filter_state = crate::core::filter::FilterState::default();
        state.apply_filters();
    }

    ui.separator();

    // Severity checkboxes
    ui.label("Severity:");
    let mut changed = false;
    for severity in Severity::all() {
        let mut checked = state.filter_state.severity_levels.contains(severity);
        if ui.checkbox(&mut checked, severity.label()).changed() {
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

    ui.separator();

    // Text search
    ui.label("Text search:");
    let text_response = ui.text_edit_singleline(&mut state.filter_state.text_search);
    if text_response.changed() {
        // TODO: debounce in next increment
        state.apply_filters();
    }
}
