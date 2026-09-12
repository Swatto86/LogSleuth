# Architecture

See [ATLAS.md](ATLAS.md) for the wider code map. These are the main ownership boundaries:

- `src/main.rs` loads platform configuration, profiles and startup diagnostics.
- `src/app/state.rs` owns session and UI requests. `src/gui.rs` consumes requests,
  coordinates scans/watchers and owns the background export worker through shutdown.
- `src/app/scan.rs` runs discovery and parsing. Normal discovery profiles files until
  selected; Troubleshoot Mode opts into automatic parsing and initial tail startup.
  Manual Stop Tail cancels pending automatic startup.
- `src/core/discovery.rs` shares directory exclusion semantics with `src/app/dir_watcher.rs`.
  Watch start/resume use the same date, depth and pattern configuration.
- `src/platform/fs.rs` provides exclusive same-directory temporary files for atomic
  session and export replacement. `src/core/export.rs` owns CSV/JSON serialization.
- `src/app/profile_mgr.rs` bounds external profile reads and rejects non-regular files.
  `src/platform/config.rs` owns application-specific configuration locations.
- `src/ui/panels` render state and queue operations; rescan confirmation and row selection
  are shared state operations so keyboard and mouse follow the same rules.

`src/ui/workspace.rs` renders the action bar, welcome flow and timeline search.
It emits actions for file dialogs/export; `gui.rs` handles those external effects.
Both sidebar and timeline retain virtual scrolling. Source checkboxes reflect
whether a discovered file has actually been parsed, including metadata-only scans.
