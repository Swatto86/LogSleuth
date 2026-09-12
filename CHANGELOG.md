# Changelog

## 1.1.3

- Reconcile the 36 saved review findings and pending Windows/Linux fixes.
- Keep Troubleshoot Mode parsing new files and starting Live Tail; manual Stop Tail cancels pending startup.
- Confirm rescans and bookmark clearing before discarding data. Fix keyboard selection,
  text-field Escape handling, invalid-regex state and empty-session tail controls.
- Export CSV/JSON in an owned background worker using exclusive atomic temporary files.
- Align directory-watch exclusions and resumed date filters with scanning. Report Event
  Viewer and file-manager failures, and bound external profile reads.
- Improve generated profiles, timestamp precision, CSV formula protection and portable regression tests.
- Harden release tag handling, validate downloaded AppImage tools and publish SHA256SUMS.
- Derive local Windows installer versions from Cargo.toml.

### Compatibility

CSV's source line header is now `line_number`, matching JSON. Update consumers that
look for `line`; consumers supporting both names can read old and new exports.
Existing exports are unchanged. Version 1.1.2 remains available for rollback.

Unix configuration is application-specific: Linux uses
`$XDG_CONFIG_HOME/logsleuth/config.toml` (default `~/.config/logsleuth/config.toml`),
macOS uses `~/Library/Application Support/LogSleuth/config.toml`.
A shared parent `config.toml` is no longer loaded. Copy only LogSleuth's settings into
the application directory; keep the original file for rollback. Unsupported example
configuration keys were removed; supported settings retain their meaning.

### Runtime and verification

Windows artifacts target x64 Windows 10/11 and statically link the MSVC CRT.
Linux artifacts need a compatible desktop/glibc, GTK 3 and graphics libraries;
AppImage may need FUSE 2 or `APPIMAGE_EXTRACT_AND_RUN=1`. macOS artifacts target the
architecture of GitHub's macos-latest runner and are unsigned. See README prerequisites.
Windows native launch and live append behavior are tested locally; Linux/macOS build
and automated test coverage run in CI. Native Linux/macOS launch is not locally verified.
