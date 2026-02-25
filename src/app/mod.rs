// LogSleuth - app/mod.rs
//
// Application layer: orchestration, state management, profile loading.
// Dependencies: core layer.
// Must NOT depend on: ui, platform specifics.

pub mod profile_mgr;
pub mod scan;
pub mod session;
pub mod state;
pub mod tail;
