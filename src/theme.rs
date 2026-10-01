//! Shared design tokens. Views choose roles instead of embedding color values.
use gpui_kit::{Hsla, rgb};
use std::time::Duration;

// Change this one value to recolor links, selection, indicators, and detail borders.
// pub const ACCENT: u32 = 0x7259db;
pub const ACCENT: u32 = 0xf7920c;
// Dark tint over the native macOS blur; lower values reveal more of the backdrop.
pub const WINDOW_TINT_OPACITY: f32 = 0.8;
// Extra tint for controls and panels over the window; keeps text and icons fully opaque.
pub const ELEMENT_TINT_OPACITY: f32 = 0.35;
pub const FONT_FAMILY: &str = ".SystemUIFont";
pub const REFRESH_INTERVAL: Duration = Duration::from_secs(10);
pub const ROW_TRANSITION: Duration = Duration::from_millis(850);
pub const TRANSITION_RETENTION: Duration = Duration::from_millis(900);

#[derive(Clone, Copy)]
pub struct Palette {
    pub background: Hsla,
    pub surface: Hsla,
    pub row_background: Hsla,
    pub row_alternate: Hsla,
    pub row_hover: Hsla,
    pub button_hover: Hsla,
    pub text: Hsla,
    pub secondary: Hsla,
    pub muted: Hsla,
    pub border: Hsla,
    pub accent: Hsla,
    pub selected: Hsla,
    pub danger: Hsla,
    pub error: Hsla,
    pub danger_surface: Hsla,
}
pub fn palette() -> Palette {
    let accent = rgb(ACCENT).blend(rgb(0xffffff).alpha(0.45)).into();
    Palette {
        background: rgb(0x20242b).alpha(WINDOW_TINT_OPACITY).into(),
        surface: rgb(0x252a32).alpha(ELEMENT_TINT_OPACITY).into(),
        // Let alternate rows show the root tint without applying it twice.
        row_background: rgb(0x20242b).alpha(0.0).into(),
        // A light overlay keeps striping subtle without obscuring the blurred backdrop.
        row_alternate: rgb(0xffffff).alpha(0.035).into(),
        row_hover: rgb(0xffffff).alpha(0.08).into(),
        button_hover: rgb(0x444b59).alpha(ELEMENT_TINT_OPACITY).into(),
        text: rgb(0xdce0e7).into(),
        secondary: rgb(0xaab1bd).into(),
        muted: rgb(0x8b93a2).into(),
        border: rgb(0x343a45).into(),
        accent,
        selected: rgb(0x20242b)
            .blend(rgb(ACCENT).alpha(0.16))
            .alpha(ELEMENT_TINT_OPACITY)
            .into(),
        danger: rgb(0xe8a0a0).into(),
        error: rgb(0xffaaaa).into(),
        danger_surface: rgb(0x3b2c34).alpha(ELEMENT_TINT_OPACITY).into(),
    }
}

pub struct Metrics {
    pub text: f32,
    pub small: f32,
    pub caption: f32,
    pub row: f32,
    pub header: f32,
    pub column_header: f32,
    pub port: f32,
    pub process: f32,
    pub pid: f32,
    pub actions: f32,
    pub ghost: f32,
    pub disclosure: f32,
    pub child_indent: f32,
    pub details_inset: f32,
    pub nested_details_inset: f32,
    pub traffic_lights_inset: f32,
}
impl Metrics {
    pub fn at_zoom(zoom: i8) -> Self {
        let zoom = zoom as f32;
        Self {
            text: 12.0 + zoom,
            small: 11.0 + zoom,
            caption: 10.0 + zoom,
            row: 28.0 + zoom * 2.0,
            header: 44.0 + zoom * 2.0,
            column_header: 26.0 + zoom * 2.0,
            port: 70.0 + zoom * 4.0,
            process: 110.0 + zoom * 6.0,
            pid: 85.0 + zoom * 5.0,
            actions: 136.0 + zoom * 8.0,
            ghost: 18.0 + zoom,
            disclosure: 12.0,
            child_indent: 16.0,
            details_inset: 32.0,
            nested_details_inset: 48.0,
            traffic_lights_inset: 84.0,
        }
    }
}

/// Recolor the embedded artwork from the same accent token as the interface.
/// Packaging uses the same artwork for the Dock icon.
pub fn ghost_image() -> std::sync::Arc<gpui_kit::Image> {
    let svg = ghost_svg();
    std::sync::Arc::new(gpui_kit::Image::from_bytes(
        gpui_kit::ImageFormat::Svg,
        svg.into_bytes(),
    ))
}

/// Shared by the live interface and the build-time Dock icon exporter.
pub fn ghost_svg() -> String {
    include_str!("../assets/ghost.svg").replace("#7259db", &format!("#{ACCENT:06x}"))
}
