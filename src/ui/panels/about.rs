// LogSleuth - ui/panels/about.rs
//
// About dialog: shown when the user clicks the ⓘ button in the menu bar.
// Rendered as a centred, non-resizable, non-collapsible modal window.

use crate::app::state::AppState;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const REPO_URL: &str = "https://github.com/swatto86/LogSleuth";

/// Render the About dialog (if `state.show_about` is true).
pub fn render(ctx: &egui::Context, state: &mut AppState) {
    if !state.show_about {
        return;
    }

    let mut open = true;
    egui::Window::new("About LogSleuth")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .min_width(360.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.add_space(8.0);

            // Large app name
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new("\u{1f50d}  LogSleuth")
                        .size(28.0)
                        .strong(),
                );
                ui.add_space(4.0);
                ui.label(egui::RichText::new(format!("v{VERSION}")).size(14.0).weak());
            });

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);

            ui.vertical_centered(|ui| {
                ui.label("A cross-platform log file viewer and analyser");
                ui.label("with extensible format profiles and live tail.");
            });

            ui.add_space(10.0);

            ui.vertical_centered(|ui| {
                ui.hyperlink_to(REPO_URL, REPO_URL);
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(6.0);

            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new("MIT License \u{00b7} \u{00a9} 2026 Swatto")
                        .small()
                        .weak(),
                );
                ui.label(egui::RichText::new("Built with Rust & egui").small().weak());
            });

            ui.add_space(8.0);
        });

    if !open {
        state.show_about = false;
    }
}
