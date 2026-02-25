/// gen_icons — Icon asset generator for LogSleuth.
///
/// Reads `assets/icon.svg`, renders it at multiple resolutions using `resvg`,
/// and writes:
///   - `assets/icon_32.png`   (egui 32-px window icon fallback)
///   - `assets/icon_48.png`   (taskbar / dock medium)
///   - `assets/icon_256.png`  (installer / large display)
///   - `assets/icon_512.png`  (high-DPI display)
///   - `assets/icon.png`      (canonical full-size copy, same as 512)
///   - `assets/icon.ico`      (Windows multi-res ICO: 16/32/48/64/128/256 px)
///
/// Run with:
///   cargo run --example gen_icons
use ico::{IconDir, IconDirEntry, IconImage, ResourceType};
use resvg::{tiny_skia, usvg};
use std::fs;
use std::path::Path;
use std::sync::Arc;

fn main() {
    // Locate assets/ relative to the workspace root (where cargo is run from).
    let svg_path = Path::new("assets/icon.svg");
    if !svg_path.exists() {
        eprintln!("ERROR: assets/icon.svg not found. Run from the workspace root.");
        std::process::exit(1);
    }

    let svg_data = fs::read(svg_path).expect("Failed to read assets/icon.svg");

    // Load system fonts so that text elements in the SVG render correctly.
    // usvg 0.44: fontdb lives inside Options as an Arc<Database>.
    let mut opt = usvg::Options::default();
    Arc::make_mut(&mut opt.fontdb).load_system_fonts();

    let tree = usvg::Tree::from_data(&svg_data, &opt).expect("Failed to parse SVG");

    let svg_w = tree.size().width();
    let svg_h = tree.size().height();

    println!("SVG size: {svg_w} x {svg_h}");

    // --- Render a single PNG at the requested pixel size ---
    let render = |size: u32| -> Vec<u8> {
        let mut pixmap = tiny_skia::Pixmap::new(size, size)
            .unwrap_or_else(|| panic!("Failed to create {size}x{size} pixmap"));
        let scale_x = size as f32 / svg_w;
        let scale_y = size as f32 / svg_h;
        let transform = tiny_skia::Transform::from_scale(scale_x, scale_y);
        resvg::render(&tree, transform, &mut pixmap.as_mut());
        pixmap.take() // raw RGBA bytes, row-major
    };

    let render_png_bytes = |size: u32| -> Vec<u8> {
        let mut pixmap = tiny_skia::Pixmap::new(size, size)
            .unwrap_or_else(|| panic!("Failed to create {size}x{size} pixmap"));
        let scale_x = size as f32 / svg_w;
        let scale_y = size as f32 / svg_h;
        let transform = tiny_skia::Transform::from_scale(scale_x, scale_y);
        resvg::render(&tree, transform, &mut pixmap.as_mut());
        pixmap.encode_png().expect("Failed to encode PNG")
    };

    // --- Write PNG assets ---
    let png_targets: &[(u32, &str)] = &[
        (32, "assets/icon_32.png"),
        (48, "assets/icon_48.png"),
        (256, "assets/icon_256.png"),
        (512, "assets/icon_512.png"),
        (512, "assets/icon.png"),
    ];

    for &(size, path) in png_targets {
        let data = render_png_bytes(size);
        fs::write(path, &data).unwrap_or_else(|e| panic!("Failed to write {path}: {e}"));
        println!("Wrote {path}  ({size}x{size})");
    }

    // --- Build multi-resolution ICO (16, 32, 48, 64, 128, 256) ---
    let ico_sizes: &[u32] = &[16, 32, 48, 64, 128, 256];
    let mut icon_dir = IconDir::new(ResourceType::Icon);

    for &size in ico_sizes {
        let rgba = render(size);
        // ico 0.3: from_rgba_data returns IconImage directly (no Result).
        let img = IconImage::from_rgba_data(size, size, rgba);
        let entry = IconDirEntry::encode(&img)
            .unwrap_or_else(|e| panic!("Failed to encode ICO entry at {size}px: {e}"));
        icon_dir.add_entry(entry);
        println!("ICO layer: {size}x{size}");
    }

    let ico_path = "assets/icon.ico";
    let ico_file =
        fs::File::create(ico_path).unwrap_or_else(|e| panic!("Failed to create {ico_path}: {e}"));
    icon_dir
        .write(ico_file)
        .unwrap_or_else(|e| panic!("Failed to write ICO: {e}"));
    println!("Wrote {ico_path}  (multi-res: {ico_sizes:?})");

    println!("Done — all icon assets regenerated.");
}
