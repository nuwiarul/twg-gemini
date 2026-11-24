pub mod asset_preview;
pub mod basic_mesh;
pub mod buildings;
pub mod gizmo;
pub mod lines;
pub mod markers;
pub mod media_preview;
pub mod playback;
pub mod polygon_edit;
pub mod skybox;

pub fn parse_hex_color(hex: &str, alpha: f32) -> bevy::prelude::Color {
    let clean = hex.trim().trim_start_matches('#');
    if clean.len() == 6
        && let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&clean[0..2], 16),
            u8::from_str_radix(&clean[2..4], 16),
            u8::from_str_radix(&clean[4..6], 16),
        )
    {
        return bevy::prelude::Color::srgba(
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            alpha,
        );
    }
    bevy::prelude::Color::srgba(0.22, 0.74, 0.97, alpha)
}
