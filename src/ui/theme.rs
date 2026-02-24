// LogSleuth - ui/theme.rs
//
// Colour scheme, severity colour mapping, and layout constants.
// No dependencies on app state or business logic.

use crate::core::model::Severity;
use egui::Color32;

/// Colour for a given severity level.
pub fn severity_colour(severity: &Severity) -> Color32 {
    match severity {
        Severity::Critical => Color32::from_rgb(220, 38, 38),   // Red 600
        Severity::Error => Color32::from_rgb(185, 28, 28),      // Red 800
        Severity::Warning => Color32::from_rgb(217, 119, 6),    // Amber 600
        Severity::Info => Color32::from_rgb(209, 213, 219),     // Gray 300
        Severity::Debug => Color32::from_rgb(107, 114, 128),    // Gray 500
        Severity::Unknown => Color32::from_rgb(75, 85, 99),     // Gray 600
    }
}

/// Background highlight colour for a severity (subtle, for row backgrounds).
pub fn severity_bg_colour(severity: &Severity) -> Option<Color32> {
    match severity {
        Severity::Critical => Some(Color32::from_rgba_premultiplied(220, 38, 38, 25)),
        Severity::Error => Some(Color32::from_rgba_premultiplied(185, 28, 28, 20)),
        Severity::Warning => Some(Color32::from_rgba_premultiplied(217, 119, 6, 15)),
        _ => None,
    }
}

/// Status bar colours.
pub const STATUS_BG: Color32 = Color32::from_rgb(31, 41, 55);      // Gray 800
pub const STATUS_TEXT: Color32 = Color32::from_rgb(209, 213, 219);  // Gray 300

/// Layout constants.
pub const SIDEBAR_WIDTH: f32 = 250.0;
pub const DETAIL_PANE_HEIGHT: f32 = 200.0;
pub const ROW_HEIGHT: f32 = 20.0;
pub const STATUS_BAR_HEIGHT: f32 = 28.0;
