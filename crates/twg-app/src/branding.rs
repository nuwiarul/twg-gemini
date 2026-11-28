use bevy_egui::egui;

include!(concat!(env!("OUT_DIR"), "/brand_config.rs"));

/// Compiled-in public server URL configured at packaging time (`None` in generic development builds).
pub fn public_server_url() -> Option<&'static str> {
    let trimmed = PUBLIC_SERVER_URL.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

pub fn has_public_server() -> bool {
    public_server_url().is_some()
}

pub fn org_name() -> Option<&'static str> {
    let trimmed = ORG_NAME.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

pub fn org_subtitle() -> Option<&'static str> {
    let trimmed = ORG_SUBTITLE.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

pub fn app_title() -> &'static str {
    let trimmed = APP_TITLE.trim();
    if trimmed.is_empty() {
        "Tactical Wall Game 3D"
    } else {
        trimmed
    }
}

pub fn window_title() -> String {
    if let Some(org) = org_name() {
        format!("{} — {}", app_title(), org)
    } else {
        app_title().to_string()
    }
}

pub fn has_logos() -> bool {
    !BRAND_LOGOS_PNG.is_empty()
}

#[derive(Clone)]
struct CachedBrandLogos {
    items: Vec<(egui::TextureHandle, [f32; 2])>,
}

/// Lazily decodes embedded `1.png..3.png` bytes into egui textures once per egui context.
fn load_brand_logos(ctx: &egui::Context) -> Vec<(egui::TextureHandle, [f32; 2])> {
    if BRAND_LOGOS_PNG.is_empty() {
        return Vec::new();
    }
    let cache_id = egui::Id::new("twg_embedded_brand_logos_v1");
    if let Some(cached) = ctx.data(|d| d.get_temp::<CachedBrandLogos>(cache_id)) {
        return cached.items;
    }

    let mut items = Vec::with_capacity(BRAND_LOGOS_PNG.len());
    for (idx, png_bytes) in BRAND_LOGOS_PNG.iter().enumerate() {
        if let Ok(img) = image::load_from_memory(png_bytes) {
            let rgba = img.into_rgba8();
            let (w, h) = rgba.dimensions();
            if w == 0 || h == 0 {
                continue;
            }
            let color_img =
                egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], rgba.as_raw());
            // Always call `load_texture` outside `ctx.data` / `ctx.data_mut` to avoid deadlock.
            let tex = ctx.load_texture(
                format!("twg_brand_logo_{}", idx + 1),
                color_img,
                egui::TextureOptions::LINEAR,
            );
            items.push((tex, [w as f32, h as f32]));
        }
    }

    ctx.data_mut(|d| {
        d.insert_temp(
            cache_id,
            CachedBrandLogos {
                items: items.clone(),
            },
        );
    });
    items
}

/// Computes `(width, height)` preserving each logo's original aspect ratio at `target_h`.
fn fitted_logo_size(dims: [f32; 2], target_h: f32) -> egui::Vec2 {
    let aspect = (dims[0] / dims[1].max(1.0)).clamp(0.45, 2.4);
    egui::vec2((target_h * aspect).round(), target_h)
}

/// Draws up to 3 embedded branding logos left-to-right (`1.png`, `2.png`, `3.png`), centered inside `ui`.
/// Returns `true` if at least one logo was rendered.
pub fn draw_brand_logos_centered(ui: &mut egui::Ui, logo_height: f32, spacing: f32) -> bool {
    let logos = load_brand_logos(ui.ctx());
    if logos.is_empty() {
        return false;
    }

    let sizes: Vec<egui::Vec2> = logos
        .iter()
        .map(|(_, dims)| fitted_logo_size(*dims, logo_height))
        .collect();
    let total_w: f32 =
        sizes.iter().map(|s| s.x).sum::<f32>() + spacing * (sizes.len().saturating_sub(1) as f32);

    ui.allocate_ui_with_layout(
        egui::vec2(total_w, logo_height),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = spacing;
            for ((tex, _), sz) in logos.iter().zip(sizes.iter()) {
                ui.add(egui::Image::new(egui::load::SizedTexture::new(
                    tex.id(),
                    *sz,
                )));
            }
        },
    );
    true
}

/// Compact pill badge displaying the institution name (`ORG_NAME`) for top navigation bars.
pub fn draw_org_nav_badge(ui: &mut egui::Ui) {
    let Some(org) = org_name() else {
        return;
    };
    egui::Frame::new()
        .fill(egui::Color32::from_rgba_unmultiplied(245, 158, 11, 28))
        .stroke(egui::Stroke::new(
            1.0,
            egui::Color32::from_rgba_unmultiplied(251, 191, 36, 130),
        ))
        .corner_radius(4.0)
        .inner_margin(egui::Margin::symmetric(7, 2))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(org)
                    .size(10.5)
                    .strong()
                    .color(egui::Color32::from_rgb(253, 224, 71)),
            );
        });
}

/// Draws the top-right institutional logos (`1.png..3.png`) cleanly on **Layar Besar**
/// without any background box, title, or subtitle — positioned above the North Compass HUD.
/// Stores its measured height in egui temp data so the compass HUD below never overlaps it.
pub fn draw_layar_besar_brand_hud(ctx: &egui::Context, clip_rect: egui::Rect) {
    let height_id = egui::Id::new("layar_besar_brand_hud_height");
    if !has_logos() {
        ctx.data_mut(|d| d.insert_temp::<f32>(height_id, 0.0));
        return;
    }

    let anchor_pos = egui::pos2(clip_rect.right() - 16.0, clip_rect.top() + 12.0);
    let resp = egui::Area::new(egui::Id::new("twg_layar_besar_brand_hud"))
        .order(egui::Order::Foreground)
        .interactable(false)
        .fixed_pos(anchor_pos)
        .pivot(egui::Align2::RIGHT_TOP)
        .show(ctx, |ui| {
            let _ = draw_brand_logos_centered(ui, 46.0, 12.0);
        });

    let measured_h = resp.response.rect.height();
    ctx.data_mut(|d| d.insert_temp::<f32>(height_id, measured_h));
}

/// Returns the vertical pixel offset needed to place the Layar Besar North Compass HUD
/// directly beneath the top-right institutional logos without overlap.
pub fn layar_besar_compass_top_offset(ctx: &egui::Context) -> f32 {
    if !has_logos() {
        return 0.0;
    }
    let height_id = egui::Id::new("layar_besar_brand_hud_height");
    let measured = ctx
        .data(|d| d.get_temp::<f32>(height_id))
        .unwrap_or(46.0);
    if measured > 1.0 { measured + 8.0 } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_brand_logos_decode_and_cache_without_deadlock() {
        let ctx = egui::Context::default();
        for _ in 0..2 {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    let _ = draw_brand_logos_centered(ui, 44.0, 12.0);
                });
            });
            out.textures_delta.clear();
        }
        let logos = load_brand_logos(&ctx);
        assert_eq!(logos.len(), BRAND_LOGOS_PNG.len());
    }
}
