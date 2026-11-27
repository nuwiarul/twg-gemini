use bevy_egui::egui;

use crate::{
    net::{asset_sync::AssetSyncBridge, client::ApiClientBridge, ws_client::WsClientBridge},
    scene::benchmark::PerformanceBenchmarkState,
    settings::{
        LocalPerformanceSettings, PerformancePreset, resolve_performance_settings_path,
        save_performance_settings,
    },
    state::{AppScreen, AppState, PlaybackState},
    ui::nav_bar::{draw_global_top_bar, enter_layar_besar},
};

#[allow(clippy::too_many_arguments)]
pub fn draw_graphics_screen(
    viewport_ui: &mut egui::Ui,
    state: &mut AppState,
    playback: &mut PlaybackState,
    bench: &mut PerformanceBenchmarkState,
    api: &ApiClientBridge,
    ws: &WsClientBridge,
    sync_bridge: &AssetSyncBridge,
) {
    let is_dedicated_big_screen = state.is_big_screen_only_role();
    if is_dedicated_big_screen {
        egui::Panel::top("graphics_big_screen_top_bar")
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(15, 23, 42))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 41, 59)))
                    .inner_margin(egui::Margin::symmetric(14, 8)),
            )
            .show(viewport_ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Pengaturan Grafis & Performa (Layar Besar)")
                            .size(14.0)
                            .strong()
                            .color(egui::Color32::from_rgb(56, 189, 248)),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("< Kembali ke Layar Besar")
                                        .strong()
                                        .color(egui::Color32::WHITE),
                                )
                                .fill(egui::Color32::from_rgb(14, 116, 144)),
                            )
                            .clicked()
                        {
                            enter_layar_besar(state, playback, ws);
                        }
                    });
                });
            });
    } else {
        draw_global_top_bar(viewport_ui, state, playback, api, ws, sync_bridge);
    }

    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(11, 17, 29))
                .inner_margin(egui::Margin::symmetric(24, 18)),
        )
        .show(viewport_ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("graphics_settings_page_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_max_width(860.0);

                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new("Pengaturan Grafis & Performa 3D")
                                    .size(20.0)
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            );
                            ui.label(
                                egui::RichText::new(
                                    "Tersimpan otomatis di perangkat ini dan langsung aktif di Plot Editor, Play Skenario Tactical, maupun Layar Besar.",
                                )
                                .size(12.0)
                                .color(egui::Color32::from_rgb(148, 163, 184)),
                            );
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if state.active_skenario.is_some()
                                && !is_dedicated_big_screen
                                && ui
                                    .add(
                                        egui::Button::new(
                                            egui::RichText::new("< Kembali ke Peta 3D")
                                                .strong()
                                                .color(egui::Color32::WHITE),
                                        )
                                        .fill(egui::Color32::from_rgb(14, 116, 144)),
                                    )
                                    .clicked()
                            {
                                state.screen = AppScreen::PlotSkenario;
                            }
                        });
                    });

                    ui.add_space(14.0);

                    let mut cur = bench.to_local_settings();
                    let active_preset = cur.detect_matching_preset();
                    let mut changed = false;

                    // Section 1: 3 One-Click Presets
                    ui.label(
                        egui::RichText::new("1. Pilih Preset Cepat")
                            .size(13.5)
                            .strong()
                            .color(egui::Color32::from_rgb(186, 230, 253)),
                    );
                    ui.add_space(6.0);

                    ui.columns(3, |cols| {
                        let presets = [
                            (
                                PerformancePreset::Low,
                                "Ringan (Laptop)",
                                "Cocok untuk laptop / GPU standar & ribuan objek.",
                                "LOD2 >= 80m • Far Cull 500m • Anim 150m",
                                egui::Color32::from_rgb(217, 119, 6),
                            ),
                            (
                                PerformancePreset::Balanced,
                                "Seimbang (Default)",
                                "Keseimbangan terbaik detail 3D & target >= 30 FPS.",
                                "LOD2 >= 120m • Far Cull OFF • Anim 260m",
                                egui::Color32::from_rgb(14, 116, 144),
                            ),
                            (
                                PerformancePreset::High,
                                "Tinggi (PC RTX)",
                                "Detail LOD0 lebih jauh untuk PC Layar Besar ber-GPU tinggi.",
                                "LOD2 >= 220m • Far Cull OFF • Anim 450m",
                                egui::Color32::from_rgb(5, 150, 105),
                            ),
                        ];

                        for (idx, (preset, title, desc, spec, accent)) in
                            presets.into_iter().enumerate()
                        {
                            let selected = active_preset == preset;
                            egui::Frame::new()
                                .fill(if selected {
                                    egui::Color32::from_rgb(18, 35, 56)
                                } else {
                                    egui::Color32::from_rgb(16, 24, 38)
                                })
                                .stroke(egui::Stroke::new(
                                    if selected { 1.8 } else { 1.0 },
                                    if selected {
                                        accent
                                    } else {
                                        egui::Color32::from_rgb(38, 52, 74)
                                    },
                                ))
                                .corner_radius(8.0)
                                .inner_margin(egui::Margin::same(12))
                                .show(&mut cols[idx], |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new(title)
                                                .size(13.0)
                                                .strong()
                                                .color(egui::Color32::WHITE),
                                        );
                                        if selected {
                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    egui::Frame::new()
                                                        .fill(accent)
                                                        .corner_radius(4.0)
                                                        .inner_margin(egui::Margin::symmetric(6, 1))
                                                        .show(ui, |ui| {
                                                            ui.label(
                                                                egui::RichText::new("AKTIF")
                                                                    .size(9.0)
                                                                    .strong()
                                                                    .color(egui::Color32::WHITE),
                                                            );
                                                        });
                                                },
                                            );
                                        }
                                    });
                                    ui.add_space(4.0);
                                    ui.label(
                                        egui::RichText::new(desc)
                                            .size(11.0)
                                            .color(egui::Color32::from_rgb(203, 213, 225)),
                                    );
                                    ui.add_space(4.0);
                                    ui.label(
                                        egui::RichText::new(spec)
                                            .size(10.0)
                                            .color(egui::Color32::from_rgb(148, 163, 184)),
                                    );
                                    ui.add_space(8.0);
                                    if ui
                                        .add_sized(
                                            [ui.available_width(), 28.0],
                                            egui::Button::new(
                                                egui::RichText::new(if selected {
                                                    "Terpilih"
                                                } else {
                                                    "Pakai Preset Ini"
                                                })
                                                .size(11.0)
                                                .strong()
                                                .color(egui::Color32::WHITE),
                                            )
                                            .fill(if selected {
                                                accent
                                            } else {
                                                egui::Color32::from_rgb(30, 41, 59)
                                            }),
                                        )
                                        .clicked()
                                    {
                                        let keep_fps = cur.show_corner_fps;
                                        cur = LocalPerformanceSettings::for_preset(preset);
                                        cur.show_corner_fps = keep_fps;
                                        changed = true;
                                    }
                                });
                        }
                    });

                    ui.add_space(16.0);

                    // Section 2: Fine-Grained Controls
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("2. Pengaturan Detail LOD, Skinning & Culling")
                                .size(13.5)
                                .strong()
                                .color(egui::Color32::from_rgb(186, 230, 253)),
                        );
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(format!("Mode saat ini: {}", active_preset.label()))
                                .size(11.0)
                                .color(egui::Color32::from_rgb(110, 231, 183)),
                        );
                    });
                    ui.add_space(6.0);

                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(16, 24, 38))
                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(38, 52, 74)))
                        .corner_radius(8.0)
                        .inner_margin(egui::Margin::same(14))
                        .show(ui, |ui| {
                            if ui
                                .checkbox(
                                    &mut cur.auto_lod_enabled,
                                    egui::RichText::new(
                                        "Auto Distance LOD (Zoom dekat = LOD0 penuh, Zoom jauh = *_idle_lod2.glb)",
                                    )
                                    .size(12.0)
                                    .strong(),
                                )
                                .changed()
                            {
                                changed = true;
                            }
                            ui.label(
                                egui::RichText::new(
                                    "Otomatis mengganti model 3D ke versi ringan LOD2 saat kamera menjauh agar ratusan hingga ribuan unit tetap lancar >= 30 FPS.",
                                )
                                .size(10.5)
                                .color(egui::Color32::from_rgb(148, 163, 184)),
                            );
                            if cur.auto_lod_enabled {
                                ui.add_space(6.0);
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("Batas jarak LOD0 -> LOD2:").size(11.5));
                                    if ui
                                        .add(
                                            egui::Slider::new(
                                                &mut cur.lod_switch_distance_m,
                                                40.0..=400.0,
                                            )
                                            .suffix(" m")
                                            .fixed_decimals(0),
                                        )
                                        .changed()
                                    {
                                        changed = true;
                                    }
                                });
                                ui.add_space(4.0);
                                if ui
                                    .checkbox(
                                        &mut cur.strip_lod2_skinning,
                                        "Tanpa GPU Skinning di LOD2 (Render *_idle_lod2.glb sebagai mesh statis untuk GPU Instancing)",
                                    )
                                    .changed()
                                {
                                    changed = true;
                                }
                            }

                            ui.add_space(10.0);
                            ui.separator();
                            ui.add_space(8.0);

                            if ui
                                .checkbox(
                                    &mut cur.lod_far_cull_enabled,
                                    egui::RichText::new(
                                        "Sembunyikan unit sangat jauh (Far Cull)",
                                    )
                                    .size(12.0)
                                    .strong(),
                                )
                                .changed()
                            {
                                changed = true;
                            }
                            ui.label(
                                egui::RichText::new(
                                    "Menyembunyikan objek yang berada di luar radius jauh tertentu (disarankan aktif hanya untuk skenario masif 5.000-10.000 unit di laptop standar).",
                                )
                                .size(10.5)
                                .color(egui::Color32::from_rgb(148, 163, 184)),
                            );
                            if cur.lod_far_cull_enabled {
                                ui.add_space(6.0);
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("Batas jarak Far Cull:").size(11.5));
                                    if ui
                                        .add(
                                            egui::Slider::new(
                                                &mut cur.lod_far_cull_distance_m,
                                                200.0..=1500.0,
                                            )
                                            .suffix(" m")
                                            .fixed_decimals(0),
                                        )
                                        .changed()
                                    {
                                        changed = true;
                                    }
                                });
                            }

                            ui.add_space(10.0);
                            ui.separator();
                            ui.add_space(8.0);

                            if ui
                                .checkbox(
                                    &mut cur.anim_culling_enabled,
                                    egui::RichText::new(
                                        "Distance Animation Culling (Jeda animasi tulang pada unit jauh)",
                                    )
                                    .size(12.0)
                                    .strong(),
                                )
                                .changed()
                            {
                                changed = true;
                            }
                            if cur.anim_culling_enabled {
                                ui.add_space(4.0);
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("Batas jarak animasi aktif:").size(11.5));
                                    if ui
                                        .add(
                                            egui::Slider::new(
                                                &mut cur.anim_cull_distance_m,
                                                40.0..=1500.0,
                                            )
                                            .suffix(" m")
                                            .fixed_decimals(0),
                                        )
                                        .changed()
                                    {
                                        changed = true;
                                    }
                                });
                            }

                            ui.add_space(8.0);
                            if ui
                                .checkbox(
                                    &mut cur.lazy_walk_spawn,
                                    egui::RichText::new(
                                        "Lazy-Spawn Rig Walk (Muat model jalan hanya saat unit bergerak — hemat 50% entity tulang)",
                                    )
                                    .size(12.0),
                                )
                                .changed()
                            {
                                changed = true;
                            }

                            ui.add_space(8.0);
                            if ui
                                .checkbox(
                                    &mut cur.show_corner_fps,
                                    egui::RichText::new(
                                        "Tampilkan indikator Mini FPS di pojok kanan atas peta 3D",
                                    )
                                    .size(12.0),
                                )
                                .changed()
                            {
                                changed = true;
                            }
                        });

                    if changed {
                        cur.preset = cur.detect_matching_preset();
                        bench.apply_local_settings(&cur);
                        match save_performance_settings(&cur) {
                            Ok(_) => {
                                state.status_banner = Some(format!(
                                    "Pengaturan Grafis ({}) disimpan ke perangkat ini.",
                                    cur.preset.label()
                                ));
                            }
                            Err(err) => {
                                state.status_banner = Some(err);
                            }
                        }
                    }

                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        let path = resolve_performance_settings_path();
                        ui.label(
                            egui::RichText::new(format!("File lokal: {}", path.display()))
                                .size(10.5)
                                .color(egui::Color32::from_rgb(100, 116, 139)),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .button("Reset Default (Seimbang)")
                                .clicked()
                            {
                                let def = LocalPerformanceSettings::default();
                                bench.apply_local_settings(&def);
                                let _ = save_performance_settings(&def);
                                state.status_banner = Some(
                                    "Pengaturan Grafis dikembalikan ke default Seimbang.".into(),
                                );
                            }
                        });
                    });

                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(
                            "Tips: Tekan F9 saat berada di Peta 3D (Plot Editor / Play Skenario / Layar Besar) untuk membuka Panel Uji Benchmark Stress Test (1.000 / 5.000 / 10.000 objek).",
                        )
                        .size(10.5)
                        .color(egui::Color32::from_rgb(148, 163, 184)),
                    );
                });
        });
}
