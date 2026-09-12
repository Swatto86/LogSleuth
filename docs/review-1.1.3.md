# Saved review reconciliation: 1.1.3

The original 36 findings were reconciled with the 17 retained local commits and
unfinished Windows/Omarchy changes. Numbers retain the saved report's order.
Findings 27 and 28 describe the same installer version defect.

| # | Resolution | Acceptance evidence |
|---|---|---|
| 1 | Release tags are environment data, validated before PowerShell use; build jobs have read-only permissions | `tests/release_workflow.py` executes the real guard with valid and hostile tags |
| 2 | Generated profiles retain required schema keys even with low-confidence input | `e2e_generated_profile` runs the generator and compiles/parses its output |
| 3 | Cancelled add-file scans emit a terminal cancellation | `cancelled_files_scan_reports_cancelled` |
| 4 | Pending scan preserves Troubleshoot tail intent; newly discovered files are parsed | Pending-scan/automatic-filter/stop tests; native initial error and later appended error appeared with LIVE active |
| 5 | Arrow navigation follows display order and safely handles the first row | Descending-order and headless egui keyboard tests |
| 6 | All three file pickers expose EVTX, common rotations and All files | Shared extension list and caller inspection |
| 7 | Date changes, F5 and Rescan request confirmation before data loss | State regression; native F5 retained two entries while showing confirmation |
| 8 | Portable fixtures cover required behavior; corpus-only tests explicitly ignored | Integration suite; six D:\\Logs tests remain explicit opt-in checks |
| 9 | Release builds retain unwinding for parser panic containment | Release profile inspection and optimized build |
| 10 | README download names match release outputs | Workflow/table reconciliation; publication asset validation |
| 11 | Configuration example contains supported keys | Example/specification compared with configuration structs |
| 12 | Generator and scan use the same 20-line default; threshold comments corrected | Generator integration and constant/caller inspection |
| 13 | Watcher directory exclusions use discovery's component predicate | Wildcard exclusion regression observed failing before fix, passing after |
| 14 | Invalid regex clears the previous compiled expression | Valid-then-invalid regression observed failing before fix, passing after |
| 15 | Event Viewer failures and partial access problems reach visible diagnostics | Startup/GUI error paths inspected; installed app visibly reports inaccessible EVTX files |
| 16 | Text input consumes Escape; dialogs close before filters reset | Headless egui focus and dialog regressions |
| 17 | Export formatting/writing runs in an owned background thread | Export worker completion/content test; shutdown joins worker |
| 18 | Watch resume preserves the original date filter | Shared watch-config date/depth/pattern test |
| 19 | Discovery respects configured include/exclude patterns | Configuration/discovery regression coverage |
| 20 | Windows configuration and profile directory use the application root | Separate parent/child configuration regression on Windows |
| 21 | File-manager path/spawn failures return actionable UI errors | Missing-path regression and all three UI caller checks |
| 22 | Stop Tail remains available without files/entries | Actual egui panel render test finds Stop Tail |
| 23 | Bookmark clearing requires a second click | `clear_bookmarks_requires_a_second_click` |
| 24 | Mouse and keyboard share row selection; Shift/Ctrl and copy work | Headless egui keyboard selection and clipboard-output tests |
| 25 | Maximum-file test uses deterministic data and checks newest files/warning | `e2e_max_files_truncates_file_list` |
| 26 | Detection test requires content evidence beyond filename bonus | VBR profile confidence regression |
| 27 | Local installer version comes from Cargo.toml | PowerShell 5.1 helper built NSIS installer 1.1.3 |
| 28 | Duplicate of 27; direct NSIS use requires explicit version | NSIS no longer has a stale fallback version |
| 29 | External profile reads reject non-regular/oversized input and enforce a byte cap | Directory, oversized and valid-file regressions |
| 30 | Session saves use exclusive same-directory temporary files | Pre-existing-temp regression observed failing before fix, passing after; round-trip tests |
| 31 | CSV header is `line_number`, matching JSON | Export assertion; migration and rollback documented in CHANGELOG |
| 32 | Multiline parser test asserts parsed entries instead of input | Plain-text fallback parser regression |
| 33 | Unix loads only application-specific configuration, warns on shared parent file | Unix canonical/shared configuration regressions run in Linux/macOS CI |
| 34 | Font Reset tooltip derives from DEFAULT_FONT_SIZE | Shared constant/caller inspection |
| 35 | Removed the unused contradictory debounce constant | Real filter delay retains one 150 ms constant |
| 36 | Required fixture absence fails the test | Removed early success return; portable integration suite |

Additional imported changes preserve timestamp fractional precision and epoch boundaries,
avoid Unicode digit byte-index panics, and prevent CSV formula execution (including leading
whitespace and metadata). Export temporary-file symlink regressions run on Unix.
The Windows AppImage change pins and checks appimagetool before execution; the actual
download hash was checked, and `tests/appimage-checksum.sh` tests both acceptance and
rejection with a harmless mocked tool.

Local full gate: 212 library, 14 binary and 25 integration tests passed; two benchmark
tests and six private-corpus tests are intentionally ignored. Formatter, Clippy with
warnings denied, release boundary tests and AppImage checksum integration passed.
Native Windows verification used a disposable log and restored the original session.
Linux/macOS native launch is not verified locally; their automated gates run in CI.
