use bevy_egui::egui;
use twg_shared::{
    particle_anim::{
        ParticleAnimKind, is_particle_anim, parse_particle_anim_config, particle_anim_position,
    },
    route_formation::{
        anim_bound_marker_ids, anim_is_playable, is_multi_route_anim, parse_multi_route_config,
    },
    shoot_anim::{is_water_cannon_anim, parse_water_cannon_config},
};
use uuid::Uuid;

use crate::{
    map::camera::TacticalCamera,
    net::ws_client::WsClientBridge,
    scene::playback::route_total_length_m,
    state::{AnimCategory, AnimRunStatus, AppState, PlaybackState},
    ui::{
        spinner::{SpinnerConfig, numeric_spinner_f32_w},
        topbar::broadcast_playback_cmd,
    },
};

/// Dedicated Animation Control Panel shown during `Play Skenario` mode while Editor panels
/// (Layers, 3D Catalog, Basic Mesh, Inspector) are temporarily hidden.
pub fn draw_play_animation_panel(
    viewport_ui: &mut egui::Ui,
    state: &mut AppState,
    tcam: &mut TacticalCamera,
    playback: &mut PlaybackState,
    ws: &WsClientBridge,
) -> f32 {
    egui::Panel::left("play_animation_panel")
        .resizable(true)
        .default_size(330.0)
        .min_size(275.0)
        .max_size(460.0)
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(13, 21, 32))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(16, 185, 129)))
                .inner_margin(egui::Margin::same(10)),
        )
        .show(viewport_ui, |ui| {
            // Header
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Panel Animasi")
                        .size(14.5)
                        .strong()
                        .color(egui::Color32::WHITE),
                );
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(6, 78, 59))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(16, 185, 129)))
                    .corner_radius(4.0)
                    .inner_margin(egui::Margin::symmetric(6, 2))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new("PLAY SKENARIO")
                                .size(9.5)
                                .strong()
                                .color(egui::Color32::from_rgb(167, 243, 208)),
                        );
                    });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let exit_btn = egui::Button::new(
                        egui::RichText::new("Keluar")
                            .size(11.0)
                            .strong()
                            .color(egui::Color32::WHITE),
                    )
                    .fill(egui::Color32::from_rgb(185, 28, 28));
                    if ui
                        .add(exit_btn)
                        .on_hover_text("Keluar dari Play Skenario dan tampilkan kembali panel Editor")
                        .clicked()
                    {
                        playback.stop_play_test();
                        state.status_banner = Some("Kembali ke Mode Editor".into());
                        broadcast_playback_cmd(state, playback, ws, "stop_anim");
                    }
                });
            });

            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(
                    "Toolbar, panel editor, dan garis rute disembunyikan sementara. Kontrol setiap animasi di bawah.",
                )
                .size(10.5)
                .color(egui::Color32::from_rgb(148, 163, 184)),
            );
            ui.add_space(6.0);

            // Master toolbar (Play Semua / Stop Semua / Speed Multiplier)
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(18, 28, 44))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(38, 52, 74)))
                .corner_radius(6.0)
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let has_playable = state
                            .anims
                            .iter()
                            .any(|a| anim_is_playable(a, &state.markers));
                        let play_all_btn = egui::Button::new(
                            egui::RichText::new("Play Semua")
                                .size(11.0)
                                .strong()
                                .color(egui::Color32::WHITE),
                        )
                        .fill(egui::Color32::from_rgb(5, 150, 105))
                        .min_size(egui::vec2(88.0, 24.0));
                        if ui.add_enabled(has_playable, play_all_btn).clicked() {
                            playback.play_all_routes(&state.anims);
                            broadcast_playback_cmd(state, playback, ws, "play_anim");
                        }

                        let stop_all_btn = egui::Button::new(
                            egui::RichText::new("Stop Semua")
                                .size(11.0)
                                .strong()
                                .color(egui::Color32::WHITE),
                        )
                        .fill(egui::Color32::from_rgb(153, 27, 27))
                        .min_size(egui::vec2(84.0, 24.0));
                        if ui.add_enabled(has_playable, stop_all_btn).clicked() {
                            playback.stop_all_routes(&state.anims);
                            broadcast_playback_cmd(state, playback, ws, "stop_anim");
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            numeric_spinner_f32_w(
                                ui,
                                "play_panel_speed_mult",
                                &mut playback.speed_multiplier,
                                78.0,
                                SpinnerConfig {
                                    min: 0.25,
                                    max: 4.0,
                                    step: 0.25,
                                    drag_speed: 0.05,
                                    decimals: 2,
                                    suffix: "x",
                                    wrap_around: false,
                                },
                            );
                        });
                    });
                });

            ui.add_space(6.0);

            // Category Filter Tabs (scalable: Semua | Rute | Shoot | Spawn & Hide)
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                draw_cat_pill(
                    ui,
                    "Semua",
                    playback.play_panel_category.is_none(),
                    &mut playback.play_panel_category,
                    None,
                );
                for &cat in AnimCategory::ALL {
                    draw_cat_pill(
                        ui,
                        cat.label(),
                        playback.play_panel_category == Some(cat),
                        &mut playback.play_panel_category,
                        Some(cat),
                    );
                }
            });

            ui.add_space(4.0);
            ui.separator();
            ui.add_space(4.0);

            let mut fly_to: Option<(f64, f64)> = None;
            let mut action_play: Option<Uuid> = None;
            let mut action_pause: Option<Uuid> = None;
            let mut action_resume: Option<Uuid> = None;
            let mut action_stop: Option<Uuid> = None;

            let selected_cat = playback.play_panel_category;
            let mut sorted_all: Vec<twg_shared::models::Anim> = state.anims.clone();
            sorted_all.sort_by(|a, b| {
                a.sort_order
                    .cmp(&b.sort_order)
                    .then_with(|| a.created_at.cmp(&b.created_at))
                    .then_with(|| a.id.cmp(&b.id))
            });
            let filtered_anims: Vec<(usize, twg_shared::models::Anim)> = sorted_all
                .into_iter()
                .enumerate()
                .filter(|(_, a)| {
                    selected_cat.is_none_or(|cat| AnimCategory::from_anim_type(&a.anim_type) == cat)
                })
                .collect();

            egui::ScrollArea::vertical()
                .id_salt("play_panel_anims_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if let Some(cat) = selected_cat
                        && !matches!(
                            cat,
                            AnimCategory::Route | AnimCategory::Shoot | AnimCategory::Particle
                        )
                    {
                        egui::Frame::new()
                            .fill(egui::Color32::from_rgb(20, 29, 44))
                            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(44, 58, 82)))
                            .corner_radius(6.0)
                            .inner_margin(egui::Margin::same(12))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.label(
                                    egui::RichText::new(format!("Kategori '{}' — Segera Hadir", cat.label()))
                                        .size(12.0)
                                        .strong()
                                        .color(egui::Color32::from_rgb(253, 230, 138)),
                                );
                                ui.add_space(4.0);
                                ui.label(
                                    egui::RichText::new(
                                        "Kategori animasi ini disiapkan untuk tahap pengembangan berikutnya.",
                                    )
                                    .size(11.0)
                                    .color(egui::Color32::from_rgb(148, 163, 184)),
                                );
                            });
                        return;
                    }

                    if filtered_anims.is_empty() {
                        egui::Frame::new()
                            .fill(egui::Color32::from_rgb(20, 29, 44))
                            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(44, 58, 82)))
                            .corner_radius(6.0)
                            .inner_margin(egui::Margin::same(12))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.label(
                                    egui::RichText::new("Belum ada animasi rute di skenario ini.")
                                        .size(12.0)
                                        .strong()
                                        .color(egui::Color32::from_rgb(226, 232, 240)),
                                );
                                ui.add_space(4.0);
                                ui.label(
                                    egui::RichText::new(
                                        "Klik 'Keluar' lalu buka tab 'Animasi' -> 'Rute' -> '+ Buat Rute' untuk membuat animasi rute baru.",
                                    )
                                    .size(11.0)
                                    .color(egui::Color32::from_rgb(148, 163, 184)),
                                );
                            });
                        return;
                    }

                    for (global_idx, anim) in &filtered_anims {
                        let is_wc = is_water_cannon_anim(anim);
                        let is_particle = is_particle_anim(anim);
                        let is_multi = is_multi_route_anim(anim);
                        let wc_cfg = if is_wc {
                            Some(parse_water_cannon_config(anim))
                        } else {
                            None
                        };
                        let p_cfg = if is_particle {
                            Some(parse_particle_anim_config(anim))
                        } else {
                            None
                        };
                        let status = playback.track_status(anim.id);
                        let track = playback.tracks.get(&anim.id);
                        let (dist_m, total_m, progress) = if let Some(ref wcfg) = wc_cfg {
                            let elapsed = track.map(|t| t.distance_m).unwrap_or(0.0);
                            if wcfg.continuous {
                                let p = if status == AnimRunStatus::Playing { 1.0 } else { 0.0 };
                                (elapsed, 0.0, p)
                            } else {
                                let dur = anim.duration_sec.max(0.5);
                                (elapsed.clamp(0.0, dur), dur, (elapsed / dur).clamp(0.0, 1.0) as f32)
                            }
                        } else if let Some(ref pcfg) = p_cfg {
                            let elapsed = track.map(|t| t.distance_m).unwrap_or(0.0);
                            if pcfg.continuous {
                                let p = if status == AnimRunStatus::Playing { 1.0 } else { 0.0 };
                                (elapsed, 0.0, p)
                            } else {
                                let dur = anim.duration_sec.max(0.5);
                                (elapsed.clamp(0.0, dur), dur, (elapsed / dur).clamp(0.0, 1.0) as f32)
                            }
                        } else {
                            let total = track
                                .map(|t| t.total_length_m)
                                .filter(|&v| v > 0.0)
                                .unwrap_or_else(|| route_total_length_m(&anim.geom));
                            let dist = track
                                .map(|t| t.distance_m)
                                .unwrap_or(0.0)
                                .clamp(0.0, total.max(0.01));
                            let prog = if total > 0.01 {
                                (dist / total).clamp(0.0, 1.0) as f32
                            } else {
                                0.0
                            };
                            (dist, total, prog)
                        };

                        let bound_ids = anim_bound_marker_ids(anim);
                        let bound_names: Vec<String> = bound_ids
                            .iter()
                            .filter_map(|mid| {
                                state
                                    .markers
                                    .iter()
                                    .find(|m| m.id == *mid)
                                    .map(|m| m.name.clone())
                            })
                            .collect();
                        let can_play = anim_is_playable(anim, &state.markers);

                        let (status_txt, status_bg, status_fg, card_stroke) = match status {
                            AnimRunStatus::Stopped => (
                                if is_wc || is_particle {
                                    "STANDBY"
                                } else {
                                    "IDLE"
                                },
                                egui::Color32::from_rgb(30, 41, 59),
                                egui::Color32::from_rgb(148, 163, 184),
                                egui::Color32::from_rgb(44, 58, 82),
                            ),
                            AnimRunStatus::Playing => {
                                if let Some(ref pcfg) = p_cfg {
                                    match pcfg.kind {
                                        ParticleAnimKind::Fire => (
                                            "MENYALA",
                                            egui::Color32::from_rgb(124, 45, 18),
                                            egui::Color32::from_rgb(253, 186, 116),
                                            egui::Color32::from_rgb(249, 115, 22),
                                        ),
                                        ParticleAnimKind::Smoke => (
                                            "BERASAP",
                                            egui::Color32::from_rgb(51, 65, 85),
                                            egui::Color32::from_rgb(226, 232, 240),
                                            egui::Color32::from_rgb(148, 163, 184),
                                        ),
                                        ParticleAnimKind::Explosion => (
                                            "MELEDAK",
                                            egui::Color32::from_rgb(127, 29, 29),
                                            egui::Color32::from_rgb(254, 202, 202),
                                            egui::Color32::from_rgb(239, 68, 68),
                                        ),
                                    }
                                } else if is_wc {
                                    (
                                        "MENYEMPROT",
                                        egui::Color32::from_rgb(12, 74, 110),
                                        egui::Color32::from_rgb(125, 211, 252),
                                        egui::Color32::from_rgb(56, 189, 248),
                                    )
                                } else {
                                    (
                                        "BERJALAN",
                                        egui::Color32::from_rgb(6, 78, 59),
                                        egui::Color32::from_rgb(110, 231, 183),
                                        egui::Color32::from_rgb(16, 185, 129),
                                    )
                                }
                            }
                            AnimRunStatus::Paused => (
                                if is_wc || is_particle {
                                    "DIJEDA"
                                } else {
                                    "DIJEDA (IDLE)"
                                },
                                egui::Color32::from_rgb(120, 53, 15),
                                egui::Color32::from_rgb(253, 230, 138),
                                egui::Color32::from_rgb(245, 158, 11),
                            ),
                            AnimRunStatus::Finished => (
                                if is_wc || is_particle {
                                    "SELESAI"
                                } else {
                                    "SELESAI (IDLE)"
                                },
                                egui::Color32::from_rgb(30, 58, 138),
                                egui::Color32::from_rgb(191, 219, 254),
                                egui::Color32::from_rgb(56, 189, 248),
                            ),
                        };

                        egui::Frame::new()
                            .fill(egui::Color32::from_rgb(20, 29, 44))
                            .stroke(egui::Stroke::new(1.2, card_stroke))
                            .corner_radius(6.0)
                            .inner_margin(egui::Margin::same(9))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());

                                // Card Row 1: #Order + [TAG] + Name + Status + Go
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new(format!("#{}", global_idx + 1))
                                            .size(10.0)
                                            .strong()
                                            .color(egui::Color32::from_rgb(125, 211, 252)),
                                    );
                                    let (badge_txt, badge_col) = if let Some(ref pcfg) = p_cfg {
                                        match pcfg.kind {
                                            ParticleAnimKind::Fire => (
                                                "[API]",
                                                egui::Color32::from_rgb(251, 146, 60),
                                            ),
                                            ParticleAnimKind::Smoke => (
                                                "[ASAP]",
                                                egui::Color32::from_rgb(203, 213, 225),
                                            ),
                                            ParticleAnimKind::Explosion => (
                                                "[LEDAKAN]",
                                                egui::Color32::from_rgb(248, 113, 113),
                                            ),
                                        }
                                    } else if is_wc {
                                        ("[WATER CANNON]", egui::Color32::from_rgb(248, 113, 113))
                                    } else if is_multi {
                                        ("[FORMASI]", egui::Color32::from_rgb(56, 189, 248))
                                    } else {
                                        ("[RUTE]", egui::Color32::from_rgb(52, 211, 153))
                                    };
                                    ui.label(
                                        egui::RichText::new(badge_txt)
                                            .size(9.5)
                                            .strong()
                                            .color(badge_col),
                                    );
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(&anim.name)
                                                .size(12.5)
                                                .strong()
                                                .color(egui::Color32::WHITE),
                                        )
                                        .truncate(),
                                    );
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            let go_target = anim
                                                .geom
                                                .first()
                                                .map(|p| (p[0], p[1]))
                                                .or_else(|| {
                                                    anim.marker_id
                                                        .and_then(|mid| {
                                                            state
                                                                .markers
                                                                .iter()
                                                                .find(|m| m.id == mid)
                                                        })
                                                        .map(|m| (m.lng, m.lat))
                                                });
                                            if ui
                                                .add_enabled(
                                                    go_target.is_some(),
                                                    egui::Button::new(
                                                        egui::RichText::new("Go")
                                                            .size(10.0)
                                                            .color(egui::Color32::from_rgb(
                                                                125, 211, 252,
                                                            )),
                                                    )
                                                    .small()
                                                    .frame(false),
                                                )
                                                .on_hover_text("Fokus kamera ke lokasi animasi")
                                                .clicked()
                                                && let Some((lng, lat)) = go_target
                                            {
                                                fly_to = Some((lng, lat));
                                            }
                                            egui::Frame::new()
                                                .fill(status_bg)
                                                .corner_radius(3.0)
                                                .inner_margin(egui::Margin::symmetric(5, 1))
                                                .show(ui, |ui| {
                                                    ui.label(
                                                        egui::RichText::new(status_txt)
                                                            .size(9.0)
                                                            .strong()
                                                            .color(status_fg),
                                                    );
                                                });
                                        },
                                    );
                                });

                                ui.add_space(3.0);

                                // Target 3D Marker(s) / Particle Location info
                                if let Some(ref pcfg) = p_cfg {
                                    let pos_line = if !anim.geom.is_empty() {
                                        let [lng, lat, alt] = particle_anim_position(anim);
                                        format!(
                                            "{}  •  Alt {:.1} m  •  ({lat:.4}, {lng:.4})",
                                            pcfg.kind.label(),
                                            alt
                                        )
                                    } else {
                                        "Lokasi partikel belum ditentukan".to_string()
                                    };
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(pos_line)
                                                .size(11.0)
                                                .color(egui::Color32::from_rgb(253, 186, 116)),
                                        )
                                        .truncate(),
                                    );
                                } else if !bound_names.is_empty() {
                                    let target_line = if is_multi {
                                        let cfg = parse_multi_route_config(anim);
                                        let preview_names = if bound_names.len() <= 3 {
                                            bound_names.join(", ")
                                        } else {
                                            format!(
                                                "{}, {}, +{} lagi",
                                                bound_names[0],
                                                bound_names[1],
                                                bound_names.len() - 2
                                            )
                                        };
                                        format!(
                                            "{} ({} Unit): {preview_names}",
                                            cfg.formation.short_label(),
                                            bound_names.len()
                                        )
                                    } else {
                                        format!("Unit 3D: {}", bound_names[0])
                                    };
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(target_line)
                                                .size(11.0)
                                                .color(egui::Color32::from_rgb(186, 230, 253)),
                                        )
                                        .truncate(),
                                    );
                                } else {
                                    ui.label(
                                        egui::RichText::new(
                                            "Unit 3D belum dipilih (ikat unit di Mode Editor)",
                                        )
                                        .size(10.5)
                                        .color(egui::Color32::from_rgb(248, 113, 113)),
                                    );
                                }

                                // Speed / distance OR Water Cannon OR Particle metadata
                                let meta_line = if let Some(ref pcfg) = p_cfg {
                                    if pcfg.continuous {
                                        format!(
                                            "Radius: {:.1} m  •  Tinggi: {:.1} m  •  Terus-menerus",
                                            pcfg.radius_m, pcfg.height_m
                                        )
                                    } else {
                                        format!(
                                            "Radius: {:.1} m  •  Tinggi: {:.1} m  •  {:.1} / {:.1} dtk{}",
                                            pcfg.radius_m,
                                            pcfg.height_m,
                                            dist_m,
                                            total_m,
                                            if anim.replay { " (Repeat)" } else { "" }
                                        )
                                    }
                                } else if let Some(ref wcfg) = wc_cfg {
                                    if wcfg.continuous {
                                        format!(
                                            "Jarak: {:.0} m  •  Tekanan: {:.1} bar  •  Terus-menerus",
                                            wcfg.range_m, wcfg.pressure_bar
                                        )
                                    } else {
                                        format!(
                                            "Jarak: {:.0} m  •  {:.1} bar  •  {:.1} / {:.1} dtk{}",
                                            wcfg.range_m,
                                            wcfg.pressure_bar,
                                            dist_m,
                                            total_m,
                                            if anim.replay { " (Repeat)" } else { "" }
                                        )
                                    }
                                } else {
                                    format!(
                                        "Speed: {:.1} m/s  •  Jarak: {:.1} / {:.1} m  •  Repeat: {}",
                                        anim.speed,
                                        dist_m,
                                        total_m,
                                        if anim.replay { "Aktif" } else { "Tidak" }
                                    )
                                };
                                ui.label(
                                    egui::RichText::new(meta_line)
                                        .size(10.5)
                                        .color(egui::Color32::from_rgb(148, 163, 184)),
                                );

                                ui.add_space(5.0);

                                // Clean progress bar
                                let bar_h = 5.0;
                                let (bar_rect, _) = ui.allocate_exact_size(
                                    egui::vec2(ui.available_width(), bar_h),
                                    egui::Sense::hover(),
                                );
                                let p = ui.painter();
                                p.rect_filled(bar_rect, 2.5, egui::Color32::from_rgb(12, 18, 30));
                                if progress > 0.0 {
                                    let fill_w = (bar_rect.width() * progress).max(4.0);
                                    let fill_rect = egui::Rect::from_min_size(
                                        bar_rect.min,
                                        egui::vec2(fill_w, bar_h),
                                    );
                                    let bar_col = match status {
                                        AnimRunStatus::Playing => egui::Color32::from_rgb(16, 185, 129),
                                        AnimRunStatus::Paused => egui::Color32::from_rgb(245, 158, 11),
                                        _ => egui::Color32::from_rgb(56, 189, 248),
                                    };
                                    p.rect_filled(fill_rect, 2.5, bar_col);
                                }

                                ui.add_space(7.0);

                                // EXACT per-route state machine buttons:
                                // - Stopped -> [Play]
                                // - Playing -> [Pause] [Stop]
                                // - Paused  -> [Resume] [Stop]
                                // - Finished -> [Play] [Stop (Reset Awal)]
                                match status {
                                    AnimRunStatus::Stopped => {
                                        let play_btn = egui::Button::new(
                                            egui::RichText::new("Play")
                                                .size(12.0)
                                                .strong()
                                                .color(egui::Color32::WHITE),
                                        )
                                        .fill(if can_play {
                                            egui::Color32::from_rgb(5, 150, 105)
                                        } else {
                                            egui::Color32::from_rgb(30, 41, 59)
                                        })
                                        .min_size(egui::vec2(ui.available_width(), 28.0));
                                        if ui.add_enabled(can_play, play_btn).clicked() {
                                            action_play = Some(anim.id);
                                        }
                                    }
                                    AnimRunStatus::Playing => {
                                        ui.columns(2, |cols| {
                                            let pause_btn = egui::Button::new(
                                                egui::RichText::new("Pause")
                                                    .size(11.5)
                                                    .strong()
                                                    .color(egui::Color32::WHITE),
                                            )
                                            .fill(egui::Color32::from_rgb(180, 83, 9))
                                            .min_size(egui::vec2(cols[0].available_width(), 28.0));
                                            if cols[0]
                                                .add(pause_btn)
                                                .on_hover_text(
                                                    "Jeda animasi di posisi terakhir dan kembalikan unit ke Idle",
                                                )
                                                .clicked()
                                            {
                                                action_pause = Some(anim.id);
                                            }

                                            let stop_btn = egui::Button::new(
                                                egui::RichText::new("Stop")
                                                    .size(11.5)
                                                    .strong()
                                                    .color(egui::Color32::WHITE),
                                            )
                                            .fill(egui::Color32::from_rgb(185, 28, 28))
                                            .min_size(egui::vec2(cols[1].available_width(), 28.0));
                                            if cols[1]
                                                .add(stop_btn)
                                                .on_hover_text(
                                                    "Hentikan animasi & kembalikan unit ke titik awal rute (Idle)",
                                                )
                                                .clicked()
                                            {
                                                action_stop = Some(anim.id);
                                            }
                                        });
                                    }
                                    AnimRunStatus::Paused => {
                                        ui.columns(2, |cols| {
                                            let resume_btn = egui::Button::new(
                                                egui::RichText::new("Resume")
                                                    .size(11.5)
                                                    .strong()
                                                    .color(egui::Color32::WHITE),
                                            )
                                            .fill(egui::Color32::from_rgb(5, 150, 105))
                                            .min_size(egui::vec2(cols[0].available_width(), 28.0));
                                            if cols[0]
                                                .add(resume_btn)
                                                .on_hover_text(
                                                    "Lanjutkan perjalanan dari posisi terakhir (Walk)",
                                                )
                                                .clicked()
                                            {
                                                action_resume = Some(anim.id);
                                            }

                                            let stop_btn = egui::Button::new(
                                                egui::RichText::new("Stop")
                                                    .size(11.5)
                                                    .strong()
                                                    .color(egui::Color32::WHITE),
                                            )
                                            .fill(egui::Color32::from_rgb(185, 28, 28))
                                            .min_size(egui::vec2(cols[1].available_width(), 28.0));
                                            if cols[1]
                                                .add(stop_btn)
                                                .on_hover_text(
                                                    "Hentikan animasi & kembalikan unit ke titik awal rute (Idle)",
                                                )
                                                .clicked()
                                            {
                                                action_stop = Some(anim.id);
                                            }
                                        });
                                    }
                                    AnimRunStatus::Finished => {
                                        ui.columns(2, |cols| {
                                            let replay_btn = egui::Button::new(
                                                egui::RichText::new("Play")
                                                    .size(11.5)
                                                    .strong()
                                                    .color(egui::Color32::WHITE),
                                            )
                                            .fill(egui::Color32::from_rgb(5, 150, 105))
                                            .min_size(egui::vec2(cols[0].available_width(), 28.0));
                                            if cols[0]
                                                .add(replay_btn)
                                                .on_hover_text("Jalankan lagi dari titik awal")
                                                .clicked()
                                            {
                                                action_play = Some(anim.id);
                                            }

                                            let stop_btn = egui::Button::new(
                                                egui::RichText::new("Stop")
                                                    .size(11.5)
                                                    .strong()
                                                    .color(egui::Color32::WHITE),
                                            )
                                            .fill(egui::Color32::from_rgb(185, 28, 28))
                                            .min_size(egui::vec2(cols[1].available_width(), 28.0));
                                            if cols[1]
                                                .add(stop_btn)
                                                .on_hover_text(
                                                    "Kembalikan unit ke titik awal rute (Idle)",
                                                )
                                                .clicked()
                                            {
                                                action_stop = Some(anim.id);
                                            }
                                        });
                                    }
                                }
                            });
                        ui.add_space(6.0);
                    }
                });

            // Apply deferred playback commands
            if let Some(aid) = action_play {
                playback.play_anim(aid);
                broadcast_playback_cmd(state, playback, ws, "play_anim");
            } else if let Some(aid) = action_pause {
                playback.pause_anim(aid);
                broadcast_playback_cmd(state, playback, ws, "pause_anim");
            } else if let Some(aid) = action_resume {
                playback.resume_anim(aid);
                broadcast_playback_cmd(state, playback, ws, "play_anim");
            } else if let Some(aid) = action_stop {
                playback.stop_anim(aid);
                broadcast_playback_cmd(state, playback, ws, "stop_anim");
            }

            if let Some((lng, lat)) = fly_to {
                let local = state.geo_anchor.lng_lat_alt_to_local(lng, lat, 0.0);
                tcam.target_local = bevy::math::Vec2::new(local[0], local[2]);
            }
        })
        .response
        .rect
        .width()
}

fn draw_cat_pill(
    ui: &mut egui::Ui,
    label: &str,
    active: bool,
    target: &mut Option<AnimCategory>,
    value: Option<AnimCategory>,
) {
    let btn = egui::Button::new(
        egui::RichText::new(label)
            .size(10.5)
            .strong()
            .color(if active {
                egui::Color32::WHITE
            } else {
                egui::Color32::from_rgb(148, 163, 184)
            }),
    )
    .fill(if active {
        egui::Color32::from_rgb(14, 116, 144)
    } else {
        egui::Color32::from_rgb(20, 29, 44)
    })
    .corner_radius(4.0)
    .min_size(egui::vec2(0.0, 22.0));
    if ui.add(btn).clicked() {
        *target = value;
    }
}
