// LogSleuth - ui/theme.rs
//
// Colour scheme, severity colour mapping, and layout constants.
// No dependencies on app state or business logic.

use crate::core::model::Severity;
use egui::Color32;

/// Colour for a given severity level.
pub fn severity_colour(severity: &Severity) -> Color32 {
    match severity {
        Severity::Critical => Color32::from_rgb(220, 38, 38), // Red 600
        Severity::Error => Color32::from_rgb(185, 28, 28),    // Red 800
        Severity::Warning => Color32::from_rgb(217, 119, 6),  // Amber 600
        Severity::Info => Color32::from_rgb(209, 213, 219),   // Gray 300
        Severity::Debug => Color32::from_rgb(107, 114, 128),  // Gray 500
        Severity::Unknown => Color32::from_rgb(75, 85, 99),   // Gray 600
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

/// A palette of 24 visually distinct colours assigned to source files in the
/// merged timeline. Chosen to differentiate files from one another and from
/// the severity colour set above. Wraps around for scans with > 24 files.
const FILE_COLOUR_PALETTE: &[Color32] = &[
    // First 12 — primary set
    Color32::from_rgb(56, 189, 248),  // Sky 400
    Color32::from_rgb(163, 230, 53),  // Lime 400
    Color32::from_rgb(251, 191, 36),  // Amber 400
    Color32::from_rgb(192, 132, 252), // Purple 400
    Color32::from_rgb(251, 113, 133), // Rose 400
    Color32::from_rgb(52, 211, 153),  // Emerald 400
    Color32::from_rgb(251, 146, 60),  // Orange 400
    Color32::from_rgb(129, 140, 248), // Indigo 400
    Color32::from_rgb(45, 212, 191),  // Teal 400
    Color32::from_rgb(249, 168, 212), // Pink 300
    Color32::from_rgb(253, 224, 71),  // Yellow 300
    Color32::from_rgb(94, 234, 212),  // Teal 300
    // Second 12 — extended set (different hues/shades for files 13-24)
    Color32::from_rgb(34, 211, 238),  // Cyan 400
    Color32::from_rgb(59, 130, 246),  // Blue 500
    Color32::from_rgb(139, 92, 246),  // Violet 500
    Color32::from_rgb(232, 121, 249), // Fuchsia 400
    Color32::from_rgb(74, 222, 128),  // Green 400
    Color32::from_rgb(252, 165, 165), // Red 300 (soft, distinct from severity red)
    Color32::from_rgb(125, 211, 252), // Light Blue 300
    Color32::from_rgb(253, 230, 138), // Amber 200
    Color32::from_rgb(15, 118, 110),  // Teal 700
    Color32::from_rgb(126, 34, 206),  // Purple 700
    Color32::from_rgb(101, 163, 13),  // Lime 600
    Color32::from_rgb(236, 72, 153),  // Pink 500
];

/// Return the nth colour from the file palette (wraps for > 24 files).
pub fn file_colour(index: usize) -> Color32 {
    FILE_COLOUR_PALETTE[index % FILE_COLOUR_PALETTE.len()]
}

/// Status bar colours.
pub const STATUS_BG: Color32 = Color32::from_rgb(31, 41, 55); // Gray 800
pub const STATUS_TEXT: Color32 = Color32::from_rgb(209, 213, 219); // Gray 300

/// Layout constants.
pub const SIDEBAR_WIDTH: f32 = 290.0;
pub const DETAIL_PANE_HEIGHT: f32 = 200.0;
pub const ROW_HEIGHT: f32 = 20.0;
pub const STATUS_BAR_HEIGHT: f32 = 28.0;
