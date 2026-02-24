/// build.rs — LogSleuth build script.
///
/// On Windows targets: embeds assets/icon.ico into the compiled executable so
/// that the OS displays the correct icon in the titlebar, taskbar, Alt+Tab
/// switcher, and Windows Explorer — without requiring any runtime loading.
///
/// Uses CARGO_CFG_TARGET_OS rather than cfg!(target_os) so that cross-
/// compilation scenarios are handled correctly.
///
/// On non-Windows targets this script is a no-op (the icon is set at runtime
/// via eframe's NativeOptions viewport builder instead).
fn main() {
    // Rerun the build script whenever these assets change.
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=assets/app.manifest");
    println!("cargo:rerun-if-changed=assets/icon.svg");

    // Only embed the Windows resource when compiling FOR Windows.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "windows" {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.compile()
            .expect("Failed to compile Windows resources (winres). \
                     Ensure a C compiler (MSVC or MinGW) is available.");
    }
}
