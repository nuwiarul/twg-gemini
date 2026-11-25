use bevy::{
    ecs::system::NonSendMarker,
    prelude::*,
    winit::WINIT_WINDOWS,
};
use bevy_egui::egui;

const APP_ICON_64_PNG: &[u8] = include_bytes!("../../../../packaging/icons/twg-app-64.png");
const APP_ICON_256_PNG: &[u8] = include_bytes!("../../../../packaging/icons/twg-app-256.png");

fn decode_winit_icon(png_bytes: &[u8]) -> Option<winit::window::Icon> {
    let rgba = image::load_from_memory(png_bytes).ok()?.into_rgba8();
    let (w, h) = rgba.dimensions();
    winit::window::Icon::from_rgba(rgba.into_raw(), w, h).ok()
}

/// Applies the embedded TWG-3D icon to the OS window titlebar & taskbar and maximizes the window once created.
pub fn set_winit_window_icon_system(
    _main_thread: NonSendMarker,
    mut applied: Local<bool>,
) {
    if *applied {
        return;
    }

    let did_apply = WINIT_WINDOWS.with_borrow(|winit_windows| {
        if winit_windows.windows.is_empty() {
            return false;
        }
        let icon_small = decode_winit_icon(APP_ICON_64_PNG);
        let icon_large = decode_winit_icon(APP_ICON_256_PNG);

        for window in winit_windows.windows.values() {
            if let Some(icon) = icon_small.clone().or_else(|| icon_large.clone()) {
                window.set_window_icon(Some(icon));
            }
            #[cfg(target_os = "windows")]
            {
                use winit::platform::windows::WindowExtWindows;
                if let Some(icon) = icon_large.clone().or_else(|| icon_small.clone()) {
                    window.set_taskbar_icon(Some(icon));
                }
            }
            window.set_maximized(true);
        }
        true
    });

    if did_apply {
        *applied = true;
    }
}

/// Lazily uploads the embedded 64x64 TWG-3D PNG into egui's texture cache and draws it at `size x size`.
pub fn draw_app_brand_icon(ui: &mut egui::Ui, size: f32) {
    let tex_id = egui::Id::new("twg_app_brand_icon_texture");
    let handle = if let Some(existing) =
        ui.ctx().data(|data| data.get_temp::<egui::TextureHandle>(tex_id))
    {
        Some(existing)
    } else if let Ok(img) = image::load_from_memory(APP_ICON_64_PNG) {
        let rgba = img.into_rgba8();
        let (w, h) = rgba.dimensions();
        let color_img =
            egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], rgba.as_raw());
        // IMPORTANT: call `load_texture` outside `ctx.data`/`ctx.data_mut` to avoid re-locking egui's RwLock.
        let tex = ui.ctx().load_texture(
            "twg_app_brand_icon",
            color_img,
            egui::TextureOptions::LINEAR,
        );
        ui.ctx().data_mut(|data| {
            data.insert_temp(tex_id, tex.clone());
        });
        Some(tex)
    } else {
        None
    };

    if let Some(tex) = handle {
        ui.add(
            egui::Image::new(egui::load::SizedTexture::new(
                tex.id(),
                egui::vec2(size, size),
            ))
            .corner_radius(egui::CornerRadius::same((size * 0.2).round() as u8)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brand_icon_loads_and_caches_without_deadlock() {
        let ctx = egui::Context::default();
        for _ in 0..2 {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    draw_app_brand_icon(ui, 24.0);
                });
            });
            out.textures_delta.clear();
        }
        let cached = ctx.data(|d| {
            d.get_temp::<egui::TextureHandle>(egui::Id::new("twg_app_brand_icon_texture"))
                .is_some()
        });
        assert!(cached, "brand icon texture should be cached in egui context");
    }
}
