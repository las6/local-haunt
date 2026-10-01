//! Packaging uses the compiled theme, so icon export cannot pick a stale color.
use resvg::{tiny_skia, usvg};
use std::path::Path;

pub fn export_iconset(directory: &Path) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(directory)?;
    let tree = usvg::Tree::from_str(&crate::theme::ghost_svg(), &usvg::Options::default())?;
    for points in [16, 32, 128, 256, 512] {
        for scale in [1, 2] {
            let pixels = points * scale;
            let mut image =
                tiny_skia::Pixmap::new(pixels, pixels).ok_or("Could not allocate icon image")?;
            resvg::render(
                &tree,
                tiny_skia::Transform::from_scale(
                    pixels as f32 / tree.size().width(),
                    pixels as f32 / tree.size().height(),
                ),
                &mut image.as_mut(),
            );
            let suffix = if scale == 2 { "@2x" } else { "" };
            image.save_png(directory.join(format!("icon_{points}x{points}{suffix}.png")))?;
        }
    }
    Ok(())
}
