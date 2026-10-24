# LogSleuth

Cross-platform log file viewer. Point it at a directory and it discovers log files, matches them to
TOML format profiles (built-ins in `profiles/`, user overrides in the external profiles directory)
and shows one filterable merged timeline, with live tail, directory watch, bookmarks, time
correlation, saved sessions and CSV/JSON export. On Windows it also reads `.evtx`. One Rust 2021
crate on eframe/egui 0.31, released for Windows x64, Linux and macOS. Public repository
`Swatto86/LogSleuth`, MIT, single branch `main`.

## Build, test, verify

- No verify script. CI (`ci.yml`, Ubuntu, Windows and macOS) runs `cargo build --locked`,
  `cargo fmt -- --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings` and
  `cargo test --locked --workspace --all-targets`. Windows also runs
  `python -m unittest discover -s tests -p '*.py'` (PyYAML 6.0.3, PowerShell 7); Linux also runs
  `bash tests/appimage-checksum.sh`. Linux builds need the GTK 3, xcb and xkbcommon dev packages
  listed in `ci.yml`.
- The generated-profile integration test invokes Windows PowerShell. Ignored tests that read
  `D:\Logs` need a private local corpus; fixtures in `tests/fixtures/` cover the same invariants.
- Run: `cargo run --release -- /path/to/logs` (`--debug` or `RUST_LOG=debug` for logging).
- Release: install and launch a locally built release, wait for all three CI jobs on the exact
  commit, then push its `vX.Y.Z` tag. `release.yml` builds `LogSleuth.exe` (static CRT), an NSIS
  installer from `.github/installer.nsi`, a macOS binary and DMG, a Linux binary and AppImage, and
  publishes them with `SHA256SUMS`. README's Download table names those assets; change both
  together.
- Local Windows installer: `powershell -File scripts/build-installer.ps1` (version from
  `Cargo.toml`; it builds `installer/windows/logsleuth.nsi`, not the CI script).
- `update-application.ps1` and `update-application.sh` bump the version, run `cargo update` and
  the gates, commit, tag, push, then delete every older `v*.*.*` tag and its GitHub Release. Run
  them only when the owner asks.

## Where things are

- `src/main.rs`: configuration, profiles, startup diagnostics. `src/app/state.rs`: session and UI
  requests. `src/gui.rs`: consumes requests, runs scans and watchers, owns the export worker.
- `src/app/scan.rs`: discovery and parsing; `src/core/discovery.rs` shares exclusion rules with
  `src/app/dir_watcher.rs`.
- `src/core/`: parser, profiles, filters, export, `.evtx` parser. `src/platform/config.rs`:
  per-OS config, profile and data paths; `src/platform/fs.rs`: exclusive temp files for atomic
  replacement.
- `src/ui/workspace.rs` (action bar, welcome, timeline search) emits dialog and export actions to
  `gui.rs`; `src/ui/panels/` render state and queue operations.
- `scripts/New-LogSleuthProfile.ps1`: generates a profile from a sample log directory.

## Constraints

- Never modify, delete or exclusively lock a source log file.
- The UI thread never blocks on file I/O or parsing; threads talk over channels.
- `core` has no UI, platform or I/O dependency; `ui` does no direct I/O; limits and defaults live
  in `src/util/constants.rs`; user regexes are compiled with size limits.
- Session and config formats are stable (1.2.0 kept them). CSV's line column is `line_number`.
- `release.yml` passes the tag as validated data and only `create-release` may write;
  `tests/release_workflow.py` enforces both.
- `.evtx` support compiles only on Windows (`evtx` is a Windows-only dependency).
- User data: `config.toml`, `profiles/` and `session.json` under the `directories` paths resolved
  in `src/platform/config.rs` (config and profiles: Windows `%APPDATA%\LogSleuth\`, Linux
  `~/.config/logsleuth/`; Linux session: `~/.local/share/logsleuth/`). A shared parent `config.toml` is no longer loaded.

## Open questions

- Native Linux and macOS launch is not verified locally; CI only builds and tests them.
- README may be stale in two places: it says Options are not kept across restarts, but
  `src/app/session.rs` saves them; and it puts the Windows session at
  `%APPDATA%\LogSleuth\session.json`, while `config.rs` drops the trailing `config` folder only
  for config, not for the data directory. Check on Windows.

## Where longer material lives

- `README.md`: downloads, features, options, profiles, build and usage.
- `ARCHITECTURE.md`: ownership boundaries. `CONTEXT.md`: verification, release steps, 1.2.0 state.
- `ATLAS.md`: wider code map and invariants (last updated 2026-04-04, before 1.1.3 and 1.2.0).
- `LogSleuth-Specification.md`, `CHANGELOG.md`, `docs/review-1.1.3.md`, `config.example.toml`.
- `PROGRESS.md`: the increment log (history).
