// LogSleuth - core/mod.rs
//
// Core business logic layer.
// Dependencies: standard library only.
// Must NOT depend on: ui, platform, app, or any I/O crate directly.

pub mod discovery;
#[cfg(target_os = "windows")]
pub mod evtx_parser;
pub mod export;
pub mod filter;
pub mod model;
pub mod multi_search;
pub mod parser;
pub mod profile;
