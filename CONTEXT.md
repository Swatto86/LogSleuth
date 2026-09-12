# Project context

LogSleuth is a Rust/egui desktop log viewer. Work on `main`; preserve unfinished
work before combining checkouts. See [ARCHITECTURE.md](ARCHITECTURE.md) for code ownership.

Verification: `cargo fmt --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`,
and `cargo test --locked --workspace --all-targets`. Windows also runs
`python -m unittest discover -s tests -p '*.py'` (PyYAML 6.0.3) and the generated-profile
integration test invokes Windows PowerShell. Linux runs `bash tests/appimage-checksum.sh`.
The explicitly ignored D:\\Logs tests require a private local corpus; portable fixtures
cover the corresponding discovery invariants.

Before publishing, install and launch a locally built release, wait for all three OS
CI jobs on the exact commit, then push its version tag. The release workflow builds
portable binaries plus installers and publishes SHA256SUMS. Local Windows installer
builds use `powershell -File scripts/build-installer.ps1`, deriving the version from Cargo.toml.

Version 1.1.3 reconciles the saved review with both sets of unfinished local fixes;
see [docs/review-1.1.3.md](docs/review-1.1.3.md) and [CHANGELOG.md](CHANGELOG.md).

The 1.2.0 workspace keeps everyday actions on a wrapping toolbar and search above
its timeline. Sources show actual load status; advanced filters and scan controls
are expandable. The bottom inspector is allocated only for a selected entry.
The UI uses the existing request/worker paths; session and configuration formats
are unchanged. `ui/workspace.rs` emits dialog/export actions to the GUI owner.
