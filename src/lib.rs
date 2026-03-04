// LogSleuth - lib.rs
//
// Library entry point, exposing all non-GUI modules for integration testing
// and potential future programmatic use.
//
// The GUI-specific `gui` module lives in `main.rs` and is not part of the
// library surface.

pub mod app;
pub mod core;
pub mod platform;
pub mod ui;
pub mod util;
