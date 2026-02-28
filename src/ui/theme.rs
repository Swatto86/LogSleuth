// LogSleuth - ui/theme.rs
//
// Colour scheme, severity colour mapping, and layout constants.
// No dependencies on app state or business logic.

use crate::core::model::Severity;
use egui::Color32;

/// Colour for a given severity level.
///
/// Dark mode uses bright, high-contrast colours visible on a dark background.
/// Light mode uses dark, saturated colours visible on a light background.
pub fn severity_colour(severity: &Severity, dark_mode: bool) -> Color32 {
    if dark_mode {
        match severity {
            // Bright coral-red: most alarming, clearly critical
            Severity::Critical => Color32::from_rgb(255, 99, 99),
            // Red 400: clearly red and readable on dark
            Severity::Error => Color32::from_rgb(248, 113, 113),
            // Orange 300: warm amber distinct from red, highly visible
            Severity::Warning => Color32::from_rgb(253, 186, 116),
            // Gray 300: soft white-gray, readable on dark
            Severity::Info => Color32::from_rgb(209, 213, 219),
            // Gray 400: slightly lighter than the old Gray 500
            Severity::Debug => Color32::from_rgb(156, 163, 175),
            // Gray 500: muted, lowest visual priority
            Severity::Unknown => Color32::from_rgb(107, 114, 128),
        }
    } else {
        match severity {
            // Red 800: strong red on white
            Severity::Critical => Color32::from_rgb(185, 28, 28),
            // Red 900: slightly darker to distinguish from Critical
            Severity::Error => Color32::from_rgb(127, 29, 29),
            // Amber 900: dark, warm, readable on white
            Severity::Warning => Color32::from_rgb(120, 53, 15),
            // Gray 700: dark enough to read on a light background
            Severity::Info => Color32::from_rgb(55, 65, 81),
            // Gray 600: slightly lighter than Info
            Severity::Debug => Color32::from_rgb(75, 85, 99),
            // Gray 500
            Severity::Unknown => Color32::from_rgb(107, 114, 128),
        }
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

/// Text colour used for the body of timeline rows.
///
/// In dark mode this returns pure white so every row — including those with a
/// Critical/Error severity background tint — remains fully readable.
/// In light mode this returns near-black (Slate 950) for the same reason.
///
/// The severity badge prefix `[CRIT]` etc. is still rendered with
/// `severity_colour` for visual distinctiveness; this function controls the
/// timestamp, filename, and message text that follow the badge.
pub fn row_text_colour(dark_mode: bool) -> Color32 {
    if dark_mode {
        Color32::WHITE
    } else {
        Color32::from_rgb(15, 23, 42) // Slate 950 — near-black
    }
}

/// Return the nth colour from the file palette (wraps for > 24 files).
pub fn file_colour(index: usize) -> Color32 {
    FILE_COLOUR_PALETTE[index % FILE_COLOUR_PALETTE.len()]
}

/// Layout constants.
pub const SIDEBAR_WIDTH: f32 = 460.0;
pub const DETAIL_PANE_HEIGHT: f32 = 200.0;
pub const STATUS_BAR_HEIGHT: f32 = 28.0;

/// Compute the row height for virtual-scrolled lists based on the current
/// font size.  Produces a consistent single-line row height that scales
/// proportionally with the user's font-size preference.
///
/// At the default 14 pt this returns 22 px; at the old hardcoded 12 pt it
/// returns 20 px (matching the previous constant).
pub fn row_height(font_size: f32) -> f32 {
    (font_size + 8.0).round()
}
