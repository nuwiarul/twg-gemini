use bevy_egui::egui;
use twg_shared::{
    group_anim::{GroupAnimMode, SequenceTriggerMode, is_group_anim, parse_group_anim_config},
    particle_anim::{
        ParticleAnimKind, is_particle_anim, parse_particle_anim_config, particle_anim_position,
    },
    pointer_anim::{is_pointer_arrow_anim, parse_pointer_arrow_config},
    route_formation::{
        RouteFormationKind, anim_bound_marker_ids, anim_is_playable, is_multi_route_anim,
        parse_multi_route_config,
    },
    shoot_anim::{
        ShootAnimKind, parse_apar_spray_config, parse_bullet_shoot_config,
        parse_stone_throw_config, parse_tear_gas_config, parse_water_cannon_config,
        shoot_anim_is_continuous, shoot_anim_kind,
    },
    spawn_hide_anim::{SpawnHideAction, is_spawn_hide_anim, parse_spawn_hide_config},
    traffic_anim::{
        TrafficLightColor, is_traffic_anim, is_traffic_flow_or_signal, is_traffic_signal_anim,
        parse_traffic_anim_config,
    },
    traffic_diversion::{is_traffic_diversion_anim, parse_traffic_diversion_config},
    traffic_queue::{
        QueueManualAction, is_traffic_gridlock_anim, is_traffic_manual_anim,
        is_traffic_queue_anim, parse_traffic_queue_config,
    },
};
use uuid::Uuid;

use crate::{
    map::camera::TacticalCamera,
    net::{
        client::ApiClientBridge,
        ws_client::{WsClientBridge, WsOutCommand},
    },
    scene::{anim_primary_focus_lnglat, play_rts::PlayRtsState, playback::route_total_length_m},
    state::{
        AnimCategory, AnimRunStatus, AppScreen, AppState, PlayPanelTab, PlaySessionMode,
        PlaybackState,
    },
    ui::{
        nav_bar::{open_tactical_skenario, trigger_load_tactical_operasis},
        spinner::{SpinnerConfig, numeric_spinner_f32_w},
        topbar::{
            broadcast_tactical_anim_cmd, broadcast_tactical_diversion_toggle_cmd,
            broadcast_tactical_queue_clear_cmd, broadcast_tactical_queue_manual_cmd,
        },
    },
};
use twg_shared::ws::{TacticalCommand, WsClientMode};

/// Dedicated Animation Control Panel shown during `Play Skenario` mode while Editor panels
/// (Layers, 3D Catalog, Basic Mesh, Inspector) are temporarily hidden.
#[allow(clippy::too_many_arguments)]
pub fn draw_play_animation_panel(
    viewport_ui: &mut egui::Ui,
    state: &mut AppState,
    tcam: &mut TacticalCamera,
    playback: &mut PlaybackState,
    play_rts: &mut PlayRtsState,
    api: &ApiClientBridge,
    ws: &WsClientBridge,
) -> f32 {
    let is_tactical_live = state.is_tactical_live();
    let can_big_screen = is_tactical_live && state.can_control_active_big_screen();

    egui::Panel::left("play_animation_panel")
        .resizable(true)
        .default_size(336.0)
        .min_size(280.0)
        .max_size(460.0)
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(13, 21, 32))
                .stroke(egui::Stroke::new(
                    1.0,
                    if is_tactical_live {
                        egui::Color32::from_rgb(16, 185, 129)
                    } else {
                        egui::Color32::from_rgb(56, 189, 248)
                    },
                ))
                .inner_margin(egui::Margin::same(10)),
        )
        .show(viewport_ui, |ui| {
            super::audio_anim::draw_output_toggle(ui, state);
            // Header
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(if is_tactical_live {
                        "Tactical Play"
                    } else {
                        "Panel Animasi"
                    })
                    .size(14.5)
                    .strong()
                    .color(egui::Color32::WHITE),
                );
                egui::Frame::new()
                    .fill(if is_tactical_live {
                        egui::Color32::from_rgb(6, 78, 59)
                    } else {
                        egui::Color32::from_rgb(12, 74, 110)
                    })
                    .stroke(egui::Stroke::new(
                        1.0,
                        if is_tactical_live {
                            egui::Color32::from_rgb(16, 185, 129)
                        } else {
                            egui::Color32::from_rgb(56, 189, 248)
                        },
                    ))
                    .corner_radius(4.0)
                    .inner_margin(egui::Margin::symmetric(6, 2))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(if is_tactical_live {
                                "TACTICAL LIVE"
                            } else {
                                "PREVIEW LOKAL"
                            })
                            .size(9.5)
                            .strong()
                            .color(if is_tactical_live {
                                egui::Color32::from_rgb(167, 243, 208)
                            } else {
                                egui::Color32::from_rgb(186, 230, 253)
                            }),
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
                        .on_hover_text(if is_tactical_live {
                            "Keluar dari Tactical Live dan kembali ke Halaman Tactical"
                        } else {
                            "Keluar dari Preview Play Skenario dan kembali ke Plot Editor"
                        })
                        .clicked()
                    {
                        playback.stop_play_test();
                        play_rts.reset_all();
                        if is_tactical_live {
                            state.play_session_mode = PlaySessionMode::LocalPreview;
                            state.screen = AppScreen::Tactical;
                            ws.send(WsOutCommand::Subscribe {
                                skenario_id: None,
                                client_mode: WsClientMode::Idle,
                            });
                            trigger_load_tactical_operasis(state, api);
                            state.status_banner = Some("Kembali ke Halaman Tactical".into());
                        } else {
                            state.status_banner = Some("Kembali ke Mode Editor".into());
                        }
                    }
                });
            });

            ui.add_space(4.0);
            if is_tactical_live {
                let live_info = if can_big_screen {
                    "Mode Live: Animasi, RTS, Gizmo, Kamera & Load/Reload sinkron ke Layar Besar."
                } else {
                    "Mode Live: Animasi, RTS & Gizmo sinkron ke peserta lain & Layar Besar (kamera hanya lokal)."
                };
                ui.label(
                    egui::RichText::new(live_info)
                        .size(10.5)
                        .color(if can_big_screen {
                            egui::Color32::from_rgb(254, 240, 138)
                        } else {
                            egui::Color32::from_rgb(148, 163, 184)
                        }),
                );

                // Two-tab switcher in Tactical Live: [Animasi] | [Skenario]
                ui.add_space(6.0);
                ui.columns(2, |cols| {
                    let anim_active = state.tactical_page.play_panel_tab == PlayPanelTab::Animasi;
                    let sk_active = state.tactical_page.play_panel_tab == PlayPanelTab::Skenario;
                    if cols[0]
                        .add(
                            egui::Button::new(
                                egui::RichText::new(format!("Animasi ({})", state.anims.len()))
                                    .size(11.5)
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            )
                            .fill(if anim_active {
                                egui::Color32::from_rgb(14, 116, 144)
                            } else {
                                egui::Color32::from_rgb(20, 29, 44)
                            })
                            .min_size(egui::vec2(cols[0].available_width(), 26.0)),
                        )
                        .clicked()
                    {
                        state.tactical_page.play_panel_tab = PlayPanelTab::Animasi;
                    }
                    if cols[1]
                        .add(
                            egui::Button::new(
                                egui::RichText::new(format!(
                                    "Skenario ({})",
                                    state.tactical_page.active_operasi_skenarios.len()
                                ))
                                .size(11.5)
                                .strong()
                                .color(egui::Color32::WHITE),
                            )
                            .fill(if sk_active {
                                egui::Color32::from_rgb(14, 116, 144)
                            } else {
                                egui::Color32::from_rgb(20, 29, 44)
                            })
                            .min_size(egui::vec2(cols[1].available_width(), 26.0)),
                        )
                        .clicked()
                    {
                        state.tactical_page.play_panel_tab = PlayPanelTab::Skenario;
                    }
                });
            } else {
                state.tactical_page.play_panel_tab = PlayPanelTab::Animasi;
                ui.label(
                    egui::RichText::new(
                        "Preview Lokal Plot Editor — tidak mempengaruhi aplikasi lain atau Layar Besar.",
                    )
                    .size(10.5)
                    .color(egui::Color32::from_rgb(148, 163, 184)),
                );
            }
            ui.add_space(6.0);

            // Tab 2 (Tactical Live only): Skenario switcher & Reload Skenario
            if is_tactical_live && state.tactical_page.play_panel_tab == PlayPanelTab::Skenario {
                draw_tactical_skenario_tab(ui, state, playback, play_rts, api, ws);
                return;
            }

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
                            broadcast_tactical_anim_cmd(
                                state,
                                ws,
                                "play_all_anim",
                                None,
                                true,
                                playback.speed_multiplier,
                            );
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
                            broadcast_tactical_anim_cmd(
                                state,
                                ws,
                                "stop_all_anim",
                                None,
                                true,
                                playback.speed_multiplier,
                            );
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let prev_speed = playback.speed_multiplier;
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
                            if (playback.speed_multiplier - prev_speed).abs() > 1e-4 {
                                broadcast_tactical_anim_cmd(
                                    state,
                                    ws,
                                    "set_speed",
                                    None,
                                    false,
                                    playback.speed_multiplier,
                                );
                            }
                        });
                    });

                    ui.add_space(5.0);
                    ui.horizontal(|ui| {
                        let has_moves = play_rts.has_runtime_changes();
                        let reset_pos_btn = egui::Button::new(
                            egui::RichText::new("Reset Posisi")
                                .size(10.5)
                                .strong()
                                .color(if has_moves {
                                    egui::Color32::WHITE
                                } else {
                                    egui::Color32::from_rgb(100, 116, 139)
                                }),
                        )
                        .fill(if has_moves {
                            egui::Color32::from_rgb(180, 83, 9)
                        } else {
                            egui::Color32::from_rgb(24, 34, 52)
                        })
                        .min_size(egui::vec2(88.0, 22.0));
                        if ui
                            .add_enabled(has_moves, reset_pos_btn)
                            .on_hover_text(
                                "Kembalikan semua unit RTS & peralatan (Gizmo Move) ke posisi awal Plot Editor",
                            )
                            .clicked()
                        {
                            play_rts.reset_positions();
                            if state.is_tactical_live()
                                && !state.offline_preview
                                && let Some(sk) = &state.active_skenario
                            {
                                ws.send(WsOutCommand::SendCommand(TacticalCommand {
                                    operasi_id: sk.operasi_id,
                                    skenario_id: sk.id,
                                    marker_id: None,
                                    command: "reset_positions".to_string(),
                                    payload: serde_json::json!({}),
                                    save_history: false,
                                }));
                            }
                            state.status_banner = Some(
                                "Posisi unit & peralatan dikembalikan ke posisi awal Editor.".into(),
                            );
                        }

                        if !play_rts.selected_unit_ids.is_empty() {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} unit dipilih",
                                    play_rts.selected_unit_ids.len()
                                ))
                                .size(10.5)
                                .strong()
                                .color(egui::Color32::from_rgb(52, 211, 153)),
                            );
                        } else if play_rts.selected_gizmo_marker_id.is_some() {
                            ui.label(
                                egui::RichText::new("Gizmo Move")
                                    .size(10.5)
                                    .strong()
                                    .color(egui::Color32::from_rgb(253, 224, 71)),
                            );
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.checkbox(&mut state.trackpad_mode, "Trackpad")
                                .on_hover_text(
                                    "Mode Trackpad (MacBook / Laptop):\n\
                                     • Scroll 2 jari = Pan peta 360°\n\
                                     • Pinch 2 jari / Ctrl+Scroll = Zoom\n\
                                     • Alt/Option + Drag Kiri (atau Scroll 2 jari) = Orbit / Pitch\n\
                                     • Tap 2 jari = Klik Kanan (Gerak Formasi)\n\
                                     • Space + Drag Kiri = Pan alternatif",
                                );
                        });
                    });
                });

            ui.add_space(6.0);

            // Search bar + Reset pencarian button
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let reset_w = 50.0;
                ui.add(
                    egui::TextEdit::singleline(&mut playback.play_panel_search)
                        .hint_text("Cari animasi / unit...")
                        .desired_width((ui.available_width() - reset_w - 4.0).max(100.0)),
                );
                let has_search = !playback.play_panel_search.is_empty();
                let reset_btn = egui::Button::new(
                    egui::RichText::new("Reset")
                        .size(10.5)
                        .color(if has_search {
                            egui::Color32::WHITE
                        } else {
                            egui::Color32::from_rgb(100, 116, 139)
                        }),
                )
                .fill(if has_search {
                    egui::Color32::from_rgb(51, 65, 85)
                } else {
                    egui::Color32::from_rgb(20, 29, 44)
                })
                .min_size(egui::vec2(reset_w, 22.0));
                if ui
                    .add_enabled(has_search, reset_btn)
                    .on_hover_text("Reset pencarian (hapus kata kunci)")
                    .clicked()
                {
                    playback.play_panel_search.clear();
                }
            });

            ui.add_space(5.0);

            // Category Filter Tabs (scalable: Semua | Rute | Shoot | Particle | Spawn & Hide | Lainnya)
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

            let mut fly_to: Option<(f64, f64)> = None;
            let mut action_play: Option<Uuid> = None;
            let mut action_pause: Option<Uuid> = None;
            let mut action_resume: Option<Uuid> = None;
            let mut action_stop: Option<Uuid> = None;
            let mut action_queue_clear: Option<Uuid> = None;
            let mut action_queue_manual: Option<(Uuid, QueueManualAction)> = None;
            let mut action_diversion_toggle: Option<(Uuid, bool)> = None;

            let selected_cat = playback.play_panel_category;
            let search_query = playback.play_panel_search.trim().to_lowercase();
            let mut sorted_all: Vec<twg_shared::models::Anim> = state.anims.clone();
            sorted_all.sort_by(|a, b| {
                a.sort_order
                    .cmp(&b.sort_order)
                    .then_with(|| a.created_at.cmp(&b.created_at))
                    .then_with(|| a.id.cmp(&b.id))
            });
            let mut cat_total = 0usize;
            let filtered_anims: Vec<(usize, twg_shared::models::Anim)> = sorted_all
                .into_iter()
                .enumerate()
                .filter(|(_, a)| {
                    if !selected_cat
                        .is_none_or(|cat| AnimCategory::from_anim_type(&a.anim_type) == cat)
                    {
                        return false;
                    }
                    cat_total += 1;
                    anim_matches_play_search(a, &state.anims, &state.markers, &search_query)
                })
                .collect();

            if !search_query.is_empty() {
                ui.add_space(3.0);
                ui.label(
                    egui::RichText::new(format!(
                        "Hasil pencarian: {} dari {} animasi",
                        filtered_anims.len(),
                        cat_total
                    ))
                    .size(10.0)
                    .color(egui::Color32::from_rgb(148, 163, 184)),
                );
            }

            ui.add_space(4.0);
            ui.separator();
            ui.add_space(4.0);

            egui::ScrollArea::vertical()
                .id_salt("play_panel_anims_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if let Some(cat) = selected_cat
                        && !matches!(
                            cat,
                            AnimCategory::Route
                                | AnimCategory::Shoot
                                | AnimCategory::Particle
                                | AnimCategory::SpawnHide
                                | AnimCategory::Group
                                | AnimCategory::Other
                                | AnimCategory::Audio
                                | AnimCategory::Camera
                                | AnimCategory::Environment
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
                                if !search_query.is_empty() {
                                    ui.label(
                                        egui::RichText::new("Tidak ada animasi yang cocok.")
                                            .size(12.0)
                                            .strong()
                                            .color(egui::Color32::from_rgb(226, 232, 240)),
                                    );
                                    ui.add_space(4.0);
                                    let cat_suffix = selected_cat
                                        .map(|c| format!(" pada kategori '{}'", c.label()))
                                        .unwrap_or_default();
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "Kata kunci \"{}\" tidak ditemukan{cat_suffix}.",
                                            playback.play_panel_search.trim(),
                                        ))
                                        .size(11.0)
                                        .color(egui::Color32::from_rgb(148, 163, 184)),
                                    );
                                    ui.add_space(8.0);
                                    ui.horizontal(|ui| {
                                        let clear_btn = egui::Button::new(
                                            egui::RichText::new("Reset Pencarian")
                                                .size(10.5)
                                                .strong()
                                                .color(egui::Color32::WHITE),
                                        )
                                        .fill(egui::Color32::from_rgb(14, 116, 144));
                                        if ui.add(clear_btn).clicked() {
                                            playback.play_panel_search.clear();
                                        }
                                        if selected_cat.is_some()
                                            && ui
                                                .add(
                                                    egui::Button::new(
                                                        egui::RichText::new("Semua Kategori")
                                                            .size(10.5)
                                                            .color(egui::Color32::from_rgb(
                                                                203, 213, 225,
                                                            )),
                                                    )
                                                    .fill(egui::Color32::from_rgb(30, 41, 59)),
                                                )
                                                .clicked()
                                        {
                                            playback.play_panel_category = None;
                                        }
                                    });
                                } else {
                                    let empty_title = if let Some(cat) = selected_cat {
                                        format!("Belum ada animasi di kategori '{}'.", cat.label())
                                    } else {
                                        "Belum ada animasi di skenario ini.".to_string()
                                    };
                                    ui.label(
                                        egui::RichText::new(empty_title)
                                            .size(12.0)
                                            .strong()
                                            .color(egui::Color32::from_rgb(226, 232, 240)),
                                    );
                                    ui.add_space(4.0);
                                    ui.label(
                                        egui::RichText::new(
                                            "Klik 'Keluar' lalu buka tab 'Animasi' untuk membuat animasi baru.",
                                        )
                                        .size(11.0)
                                        .color(egui::Color32::from_rgb(148, 163, 184)),
                                    );
                                }
                            });
                        return;
                    }

                    for (global_idx, anim) in &filtered_anims {
                        if twg_shared::audio_anim::is_audio_anim(anim) {
                            use super::audio_anim::AudioAction;
                            match super::audio_anim::draw_play_card(ui, state, playback, anim, *global_idx) {
                                Some(AudioAction::Play) => action_play = Some(anim.id),
                                Some(AudioAction::Pause) => action_pause = Some(anim.id),
                                Some(AudioAction::Resume) => action_resume = Some(anim.id),
                                Some(AudioAction::Stop) => action_stop = Some(anim.id),
                                None => {}
                            }
                            continue;
                        }
                        if twg_shared::billboard_anim::is_billboard_anim(anim) {
                            use super::billboard_anim::BillboardPlayAction;
                            match super::billboard_anim::draw_play_card(
                                ui,
                                state,
                                playback,
                                anim,
                                *global_idx,
                                true,
                            ) {
                                Some(BillboardPlayAction::Play) => action_play = Some(anim.id),
                                Some(BillboardPlayAction::Pause) => action_pause = Some(anim.id),
                                Some(BillboardPlayAction::Resume) => action_resume = Some(anim.id),
                                Some(BillboardPlayAction::Stop) => action_stop = Some(anim.id),
                                Some(BillboardPlayAction::Focus) => {
                                    if let Some((lng, lat)) = anim_primary_focus_lnglat(
                                        anim,
                                        &state.anims,
                                        &state.markers,
                                    ) {
                                        fly_to = Some((lng, lat));
                                    }
                                }
                                None => {}
                            }
                            continue;
                        }
                        if twg_shared::camera_anim::is_camera_anim(anim) {
                            use super::camera_anim::CameraPlayAction;
                            match super::camera_anim::draw_play_card(
                                ui,
                                state,
                                playback,
                                anim,
                                *global_idx,
                                true,
                            ) {
                                Some(CameraPlayAction::Play) => action_play = Some(anim.id),
                                Some(CameraPlayAction::Pause) => action_pause = Some(anim.id),
                                Some(CameraPlayAction::Resume) => action_resume = Some(anim.id),
                                Some(CameraPlayAction::Stop) => action_stop = Some(anim.id),
                                Some(CameraPlayAction::Focus) => {
                                    if let Some((lng, lat)) = anim_primary_focus_lnglat(
                                        anim,
                                        &state.anims,
                                        &state.markers,
                                    ) {
                                        fly_to = Some((lng, lat));
                                    }
                                }
                                None => {}
                            }
                            continue;
                        }
                        if twg_shared::model_action_anim::is_model_action_anim(anim) {
                            use super::model_action_anim::ModelActionPlayAction;
                            match super::model_action_anim::draw_play_card(
                                ui,
                                state,
                                playback,
                                anim,
                                *global_idx,
                                true,
                            ) {
                                Some(ModelActionPlayAction::Play) => action_play = Some(anim.id),
                                Some(ModelActionPlayAction::Pause) => action_pause = Some(anim.id),
                                Some(ModelActionPlayAction::Resume) => {
                                    action_resume = Some(anim.id)
                                }
                                Some(ModelActionPlayAction::Stop) => action_stop = Some(anim.id),
                                Some(ModelActionPlayAction::Focus) => {
                                    if let Some((lng, lat)) = anim_primary_focus_lnglat(
                                        anim,
                                        &state.anims,
                                        &state.markers,
                                    ) {
                                        fly_to = Some((lng, lat));
                                    }
                                }
                                None => {}
                            }
                            continue;
                        }
                        if twg_shared::environment_anim::is_environment_anim(anim) {
                            use super::environment_anim::EnvPlayAction;
                            match super::environment_anim::draw_play_card(
                                ui,
                                state,
                                playback,
                                anim,
                                *global_idx,
                                true,
                            ) {
                                Some(EnvPlayAction::Play) => action_play = Some(anim.id),
                                Some(EnvPlayAction::Pause) => action_pause = Some(anim.id),
                                Some(EnvPlayAction::Resume) => action_resume = Some(anim.id),
                                Some(EnvPlayAction::Stop) => action_stop = Some(anim.id),
                                Some(EnvPlayAction::Focus) => {
                                    if let Some((lng, lat)) = anim_primary_focus_lnglat(
                                        anim,
                                        &state.anims,
                                        &state.markers,
                                    ) {
                                        fly_to = Some((lng, lat));
                                    }
                                }
                                None => {}
                            }
                            continue;
                        }
                        let shoot_kind = shoot_anim_kind(anim);
                        let is_shoot = shoot_kind.is_some();
                        let is_particle = is_particle_anim(anim);
                        let is_traffic = is_traffic_anim(anim);
                        let is_traffic_signal = is_traffic_signal_anim(anim);
                        let is_traffic_gridlock = is_traffic_gridlock_anim(anim);
                        let is_traffic_manual = is_traffic_manual_anim(anim);
                        let is_traffic_queue = is_traffic_queue_anim(anim);
                        let is_traffic_diversion = is_traffic_diversion_anim(anim);
                        let is_pointer = is_pointer_arrow_anim(anim);
                        let is_spawn_hide = is_spawn_hide_anim(anim);
                        let is_group = is_group_anim(anim);
                        let is_multi = is_multi_route_anim(anim);
                        let shoot_continuous = if is_shoot {
                            shoot_anim_is_continuous(anim)
                        } else {
                            false
                        };
                        let tr_cfg = if is_traffic || is_traffic_signal {
                            Some(parse_traffic_anim_config(anim))
                        } else {
                            None
                        };
                        let ts_live = if is_traffic_signal {
                            state.traffic_signal_live_status.get(&anim.id).cloned()
                        } else {
                            None
                        };
                        let tq_cfg = if is_traffic_queue {
                            Some(parse_traffic_queue_config(anim))
                        } else {
                            None
                        };
                        let tq_live = if is_traffic_queue {
                            state.traffic_queue_live_status.get(&anim.id).cloned()
                        } else {
                            None
                        };
                        let td_cfg = if is_traffic_diversion {
                            Some(parse_traffic_diversion_config(anim))
                        } else {
                            None
                        };
                        let td_live = if is_traffic_diversion {
                            state.traffic_diversion_live_status.get(&anim.id).cloned()
                        } else {
                            None
                        };
                        let p_cfg = if is_particle {
                            Some(parse_particle_anim_config(anim))
                        } else {
                            None
                        };
                        let ptr_cfg = if is_pointer {
                            Some(parse_pointer_arrow_config(anim))
                        } else {
                            None
                        };
                        let sh_cfg = if is_spawn_hide {
                            Some(parse_spawn_hide_config(anim))
                        } else {
                            None
                        };
                        let grp_cfg = if is_group {
                            Some(parse_group_anim_config(anim))
                        } else {
                            None
                        };
                        let status = playback.track_status(anim.id);
                        let track = playback.tracks.get(&anim.id);
                        let (dist_m, total_m, progress) = if is_shoot {
                            let elapsed = track.map(|t| t.distance_m).unwrap_or(0.0);
                            if shoot_continuous {
                                let p = if status == AnimRunStatus::Playing { 1.0 } else { 0.0 };
                                (elapsed, 0.0, p)
                            } else {
                                let dur = anim.duration_sec.max(0.5);
                                (elapsed.clamp(0.0, dur), dur, (elapsed / dur).clamp(0.0, 1.0) as f32)
                            }
                        } else if let Some(ref trc) = tr_cfg {
                            let elapsed = track.map(|t| t.distance_m).unwrap_or(0.0);
                            if trc.continuous {
                                let p = if status == AnimRunStatus::Playing { 1.0 } else { 0.0 };
                                (elapsed, 0.0, p)
                            } else {
                                let dur = anim.duration_sec.max(10.0);
                                (elapsed.clamp(0.0, dur), dur, (elapsed / dur).clamp(0.0, 1.0) as f32)
                            }
                        } else if tq_cfg.is_some() || td_cfg.is_some() {
                            let elapsed = track.map(|t| t.distance_m).unwrap_or(0.0);
                            let p = if status == AnimRunStatus::Finished {
                                1.0
                            } else if status == AnimRunStatus::Playing {
                                1.0
                            } else {
                                0.0
                            };
                            (elapsed, 0.0, p)
                        } else if let Some(ref pcfg) = p_cfg {
                            let elapsed = track.map(|t| t.distance_m).unwrap_or(0.0);
                            if pcfg.continuous {
                                let p = if status == AnimRunStatus::Playing { 1.0 } else { 0.0 };
                                (elapsed, 0.0, p)
                            } else {
                                let dur = anim.duration_sec.max(0.5);
                                (elapsed.clamp(0.0, dur), dur, (elapsed / dur).clamp(0.0, 1.0) as f32)
                            }
                        } else if let Some(ref ptrc) = ptr_cfg {
                            let elapsed = track.map(|t| t.distance_m).unwrap_or(0.0);
                            if ptrc.continuous {
                                let p = if status == AnimRunStatus::Playing { 1.0 } else { 0.0 };
                                (elapsed, 0.0, p)
                            } else {
                                let dur = anim.duration_sec.max(0.5);
                                (elapsed.clamp(0.0, dur), dur, (elapsed / dur).clamp(0.0, 1.0) as f32)
                            }
                        } else if let Some(ref sh) = sh_cfg {
                            let elapsed = track.map(|t| t.distance_m).unwrap_or(0.0);
                            let dur = sh.total_duration_sec();
                            let prog = if status == AnimRunStatus::Finished {
                                1.0
                            } else if dur > 1e-4 {
                                (elapsed / dur).clamp(0.0, 1.0) as f32
                            } else if status == AnimRunStatus::Playing {
                                1.0
                            } else {
                                0.0
                            };
                            (elapsed.clamp(0.0, dur), dur, prog)
                        } else if let Some(ref grp) = grp_cfg {
                            let total_steps = grp.items.len();
                            let launched = track.map(|t| t.group_launched_count).unwrap_or(0);
                            let prog = if status == AnimRunStatus::Finished {
                                1.0
                            } else if total_steps > 0 {
                                (launched as f32 / total_steps as f32).clamp(0.0, 1.0)
                            } else {
                                0.0
                            };
                            (launched as f64, total_steps as f64, prog)
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
                            AnimRunStatus::Stopped => {
                                if let Some(ref sh) = sh_cfg {
                                    match sh.action {
                                        SpawnHideAction::Spawn => (
                                            "TERSEMBUNYI",
                                            egui::Color32::from_rgb(38, 28, 62),
                                            egui::Color32::from_rgb(196, 181, 253),
                                            egui::Color32::from_rgb(64, 52, 98),
                                        ),
                                        SpawnHideAction::Hide => (
                                            "TERLIHAT",
                                            egui::Color32::from_rgb(30, 41, 59),
                                            egui::Color32::from_rgb(148, 163, 184),
                                            egui::Color32::from_rgb(44, 58, 82),
                                        ),
                                    }
                                } else if is_group {
                                    (
                                        "SIAP",
                                        egui::Color32::from_rgb(30, 27, 75),
                                        egui::Color32::from_rgb(165, 180, 252),
                                        egui::Color32::from_rgb(55, 48, 163),
                                    )
                                } else {
                                    (
                                        if is_shoot || is_particle || is_pointer {
                                            "STANDBY"
                                        } else {
                                            "IDLE"
                                        },
                                        egui::Color32::from_rgb(30, 41, 59),
                                        egui::Color32::from_rgb(148, 163, 184),
                                        egui::Color32::from_rgb(44, 58, 82),
                                    )
                                }
                            }
                            AnimRunStatus::Playing => {
                                if let Some(ref grp) = grp_cfg {
                                    match grp.mode {
                                        GroupAnimMode::Combine => (
                                            "BARENG",
                                            egui::Color32::from_rgb(23, 37, 84),
                                            egui::Color32::from_rgb(147, 197, 253),
                                            egui::Color32::from_rgb(59, 130, 246),
                                        ),
                                        GroupAnimMode::Sequence => (
                                            "BERURUTAN",
                                            egui::Color32::from_rgb(59, 7, 100),
                                            egui::Color32::from_rgb(216, 180, 254),
                                            egui::Color32::from_rgb(168, 85, 247),
                                        ),
                                    }
                                } else if let Some(ref sh) = sh_cfg {
                                    match sh.action {
                                        SpawnHideAction::Spawn => (
                                            "MUNCUL...",
                                            egui::Color32::from_rgb(6, 78, 59),
                                            egui::Color32::from_rgb(110, 231, 183),
                                            egui::Color32::from_rgb(16, 185, 129),
                                        ),
                                        SpawnHideAction::Hide => (
                                            "MENGHILANG...",
                                            egui::Color32::from_rgb(136, 19, 55),
                                            egui::Color32::from_rgb(253, 164, 175),
                                            egui::Color32::from_rgb(244, 63, 94),
                                        ),
                                    }
                                } else if let Some(ref pcfg) = p_cfg {
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
                                } else if is_traffic_signal {
                                    if let Some(ref live) = ts_live {
                                        if live.is_all_red {
                                            (
                                                "ALL-RED",
                                                egui::Color32::from_rgb(136, 19, 55),
                                                egui::Color32::from_rgb(253, 164, 175),
                                                egui::Color32::from_rgb(225, 29, 72),
                                            )
                                        } else {
                                            match live.active_color {
                                                TrafficLightColor::Green => (
                                                    "LAMPU HIJAU",
                                                    egui::Color32::from_rgb(6, 78, 59),
                                                    egui::Color32::from_rgb(110, 231, 183),
                                                    egui::Color32::from_rgb(16, 185, 129),
                                                ),
                                                TrafficLightColor::Yellow => (
                                                    "LAMPU KUNING",
                                                    egui::Color32::from_rgb(113, 63, 18),
                                                    egui::Color32::from_rgb(254, 240, 138),
                                                    egui::Color32::from_rgb(234, 179, 8),
                                                ),
                                                TrafficLightColor::Red => (
                                                    "LAMPU MERAH",
                                                    egui::Color32::from_rgb(136, 19, 55),
                                                    egui::Color32::from_rgb(253, 164, 175),
                                                    egui::Color32::from_rgb(225, 29, 72),
                                                ),
                                            }
                                        }
                                    } else {
                                        (
                                            "LAMPU MERAH",
                                            egui::Color32::from_rgb(136, 19, 55),
                                            egui::Color32::from_rgb(253, 164, 175),
                                            egui::Color32::from_rgb(225, 29, 72),
                                        )
                                    }
                                } else if is_traffic {
                                    (
                                        "LALU LINTAS",
                                        egui::Color32::from_rgb(120, 53, 15),
                                        egui::Color32::from_rgb(253, 230, 138),
                                        egui::Color32::from_rgb(245, 158, 11),
                                    )
                                } else if is_traffic_queue {
                                    let is_clearing =
                                        track.is_some_and(|t| t.is_queue_clearing());
                                    if is_clearing {
                                        (
                                            "MODE LANCAR",
                                            egui::Color32::from_rgb(6, 78, 59),
                                            egui::Color32::from_rgb(110, 231, 183),
                                            egui::Color32::from_rgb(16, 185, 129),
                                        )
                                    } else if is_traffic_gridlock {
                                        (
                                            "MACET TOTAL",
                                            egui::Color32::from_rgb(136, 19, 55),
                                            egui::Color32::from_rgb(253, 164, 175),
                                            egui::Color32::from_rgb(225, 29, 72),
                                        )
                                    } else if is_traffic_manual {
                                        if tq_live.as_ref().is_some_and(|s| s.is_all_stopped) {
                                            (
                                                "TAHAN SEMUA",
                                                egui::Color32::from_rgb(136, 19, 55),
                                                egui::Color32::from_rgb(253, 164, 175),
                                                egui::Color32::from_rgb(225, 29, 72),
                                            )
                                        } else {
                                            (
                                                "MANUAL AKTIF",
                                                egui::Color32::from_rgb(49, 46, 129),
                                                egui::Color32::from_rgb(199, 210, 254),
                                                egui::Color32::from_rgb(99, 102, 241),
                                            )
                                        }
                                    } else {
                                        (
                                            "URAI SIMPANG",
                                            egui::Color32::from_rgb(17, 94, 89),
                                            egui::Color32::from_rgb(153, 246, 228),
                                            egui::Color32::from_rgb(20, 184, 166),
                                        )
                                    }
                                } else if is_traffic_diversion {
                                    let is_div = td_live
                                        .as_ref()
                                        .map(|s| s.is_diverted)
                                        .unwrap_or_else(|| {
                                            td_cfg.as_ref().is_some_and(|c| c.start_diverted)
                                        });
                                    if is_div {
                                        (
                                            "DIALIHKAN",
                                            egui::Color32::from_rgb(120, 53, 15),
                                            egui::Color32::from_rgb(253, 230, 138),
                                            egui::Color32::from_rgb(245, 158, 11),
                                        )
                                    } else {
                                        (
                                            "ARUS NORMAL",
                                            egui::Color32::from_rgb(12, 74, 110),
                                            egui::Color32::from_rgb(125, 211, 252),
                                            egui::Color32::from_rgb(14, 165, 233),
                                        )
                                    }
                                } else if is_pointer {
                                    (
                                        "MENUNJUK",
                                        egui::Color32::from_rgb(113, 63, 18),
                                        egui::Color32::from_rgb(254, 240, 138),
                                        egui::Color32::from_rgb(250, 204, 21),
                                    )
                                } else if let Some(sk) = shoot_kind {
                                    match sk {
                                        ShootAnimKind::WaterCannon => (
                                            sk.playing_status_label(),
                                            egui::Color32::from_rgb(12, 74, 110),
                                            egui::Color32::from_rgb(125, 211, 252),
                                            egui::Color32::from_rgb(56, 189, 248),
                                        ),
                                        ShootAnimKind::StoneThrow => (
                                            sk.playing_status_label(),
                                            egui::Color32::from_rgb(120, 53, 15),
                                            egui::Color32::from_rgb(253, 230, 138),
                                            egui::Color32::from_rgb(245, 158, 11),
                                        ),
                                        ShootAnimKind::TearGas => (
                                            sk.playing_status_label(),
                                            egui::Color32::from_rgb(51, 65, 85),
                                            egui::Color32::from_rgb(241, 245, 249),
                                            egui::Color32::from_rgb(203, 213, 225),
                                        ),
                                        ShootAnimKind::Bullet => (
                                            sk.playing_status_label(),
                                            egui::Color32::from_rgb(127, 29, 29),
                                            egui::Color32::from_rgb(254, 202, 202),
                                            egui::Color32::from_rgb(239, 68, 68),
                                        ),
                                        ShootAnimKind::Apar => (
                                            sk.playing_status_label(),
                                            egui::Color32::from_rgb(17, 94, 89),
                                            egui::Color32::from_rgb(153, 246, 228),
                                            egui::Color32::from_rgb(20, 184, 166),
                                        ),
                                    }
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
                                if is_shoot
                                    || is_particle
                                    || is_traffic
                                    || is_pointer
                                    || is_spawn_hide
                                    || is_group
                                {
                                    "DIJEDA"
                                } else {
                                    "DIJEDA (IDLE)"
                                },
                                egui::Color32::from_rgb(120, 53, 15),
                                egui::Color32::from_rgb(253, 230, 138),
                                egui::Color32::from_rgb(245, 158, 11),
                            ),
                            AnimRunStatus::Finished => {
                                if let Some(ref sh) = sh_cfg {
                                    match sh.action {
                                        SpawnHideAction::Spawn => (
                                            "MUNCUL",
                                            egui::Color32::from_rgb(6, 78, 59),
                                            egui::Color32::from_rgb(167, 243, 208),
                                            egui::Color32::from_rgb(16, 185, 129),
                                        ),
                                        SpawnHideAction::Hide => (
                                            "TERSEMBUNYI",
                                            egui::Color32::from_rgb(127, 29, 29),
                                            egui::Color32::from_rgb(254, 202, 202),
                                            egui::Color32::from_rgb(244, 63, 94),
                                        ),
                                    }
                                } else {
                                    (
                                        if is_shoot || is_particle || is_pointer || is_group {
                                            "SELESAI"
                                        } else {
                                            "SELESAI (IDLE)"
                                        },
                                        egui::Color32::from_rgb(30, 58, 138),
                                        egui::Color32::from_rgb(191, 219, 254),
                                        egui::Color32::from_rgb(56, 189, 248),
                                    )
                                }
                            }
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
                                    } else if let Some(ref grp) = grp_cfg {
                                        match grp.mode {
                                            GroupAnimMode::Combine => (
                                                "[COMBINE]",
                                                egui::Color32::from_rgb(96, 165, 250),
                                            ),
                                            GroupAnimMode::Sequence => (
                                                "[SEQUENCE]",
                                                egui::Color32::from_rgb(192, 132, 252),
                                            ),
                                        }
                                    } else if let Some(ref sh) = sh_cfg {
                                        match sh.action {
                                            SpawnHideAction::Spawn => (
                                                "[SPAWN]",
                                                egui::Color32::from_rgb(52, 211, 153),
                                            ),
                                            SpawnHideAction::Hide => (
                                                "[HIDE]",
                                                egui::Color32::from_rgb(251, 113, 133),
                                            ),
                                        }
                                    } else if is_pointer {
                                        ("[PANAH 3D]", egui::Color32::from_rgb(250, 204, 21))
                                    } else if let Some(sk) = shoot_kind {
                                        match sk {
                                            ShootAnimKind::WaterCannon => (
                                                "[WATER CANNON]",
                                                egui::Color32::from_rgb(56, 189, 248),
                                            ),
                                            ShootAnimKind::StoneThrow => (
                                                "[BATU]",
                                                egui::Color32::from_rgb(251, 191, 36),
                                            ),
                                            ShootAnimKind::TearGas => (
                                                "[GAS AIR MATA]",
                                                egui::Color32::from_rgb(226, 232, 240),
                                            ),
                                            ShootAnimKind::Bullet => (
                                                "[PELURU]",
                                                egui::Color32::from_rgb(248, 113, 113),
                                            ),
                                            ShootAnimKind::Apar => (
                                                "[APAR]",
                                                egui::Color32::from_rgb(45, 212, 191),
                                            ),
                                        }
                                    } else if is_traffic_signal {
                                        ("[LAMPU MERAH]", egui::Color32::from_rgb(251, 113, 133))
                                    } else if is_traffic {
                                        ("[TRAFFIC]", egui::Color32::from_rgb(251, 191, 36))
                                    } else if is_traffic_gridlock {
                                        ("[MACET TOTAL]", egui::Color32::from_rgb(251, 113, 133))
                                    } else if is_traffic_manual {
                                        ("[SIMPANG MANUAL]", egui::Color32::from_rgb(165, 180, 252))
                                    } else if is_traffic_queue {
                                        ("[URAI SIMPANG]", egui::Color32::from_rgb(45, 212, 191))
                                    } else if is_traffic_diversion {
                                        ("[ALIH ARUS]", egui::Color32::from_rgb(56, 189, 248))
                                    } else if is_multi {
                                        if parse_multi_route_config(anim).formation
                                            == RouteFormationKind::Patwal
                                        {
                                            ("[PATWAL]", egui::Color32::from_rgb(96, 165, 250))
                                        } else {
                                            ("[FORMASI]", egui::Color32::from_rgb(56, 189, 248))
                                        }
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
                                            let go_target = anim_primary_focus_lnglat(
                                                anim,
                                                &state.anims,
                                                &state.markers,
                                            );
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

                                // Target 3D Marker(s) / Particle Location / Group Members info
                                if let Some(ref grp) = grp_cfg {
                                    let child_names: Vec<String> = grp
                                        .items
                                        .iter()
                                        .map(|it| {
                                            state
                                                .anims
                                                .iter()
                                                .find(|x| x.id == it.anim_id)
                                                .map(|x| x.name.clone())
                                                .unwrap_or_else(|| "(Terhapus)".to_string())
                                        })
                                        .collect();
                                    let grp_line = if child_names.is_empty() {
                                        "Belum ada animasi anggota (tambah di Mode Editor)".to_string()
                                    } else if grp.mode == GroupAnimMode::Sequence
                                        && matches!(status, AnimRunStatus::Playing | AnimRunStatus::Paused)
                                    {
                                        let launched =
                                            track.map(|t| t.group_launched_count).unwrap_or(0);
                                        let cur_idx = launched.saturating_sub(1).min(child_names.len() - 1);
                                        if launched == 0 {
                                            format!(
                                                "Menunggu jeda awal ({} langkah)...",
                                                child_names.len()
                                            )
                                        } else {
                                            format!(
                                                "Langkah {}/{}  •  Aktif: {}",
                                                launched,
                                                child_names.len(),
                                                child_names[cur_idx]
                                            )
                                        }
                                    } else {
                                        let sep = match grp.mode {
                                            GroupAnimMode::Combine => " + ",
                                            GroupAnimMode::Sequence => " → ",
                                        };
                                        let preview = if child_names.len() <= 3 {
                                            child_names.join(sep)
                                        } else {
                                            format!(
                                                "{}{sep}{}{sep}+{} lagi",
                                                child_names[0],
                                                child_names[1],
                                                child_names.len() - 2
                                            )
                                        };
                                        format!("{} Animasi: {preview}", child_names.len())
                                    };
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(grp_line).size(11.0).color(
                                                if child_names.is_empty() {
                                                    egui::Color32::from_rgb(248, 113, 113)
                                                } else {
                                                    egui::Color32::from_rgb(199, 210, 254)
                                                },
                                            ),
                                        )
                                        .truncate(),
                                    );
                                } else if let Some(ref trc) = tr_cfg {
                                    let active_lanes = trc
                                        .lanes
                                        .iter()
                                        .filter(|l| l.enabled && l.geom.len() >= 2)
                                        .count();
                                    let (info_line, info_col) = if is_traffic_signal {
                                        if let Some(ref live) = ts_live
                                            && matches!(
                                                status,
                                                AnimRunStatus::Playing | AnimRunStatus::Paused
                                            )
                                        {
                                            let (c_lbl, c_col) = if live.is_all_red {
                                                ("ALL-RED", egui::Color32::from_rgb(253, 164, 175))
                                            } else {
                                                match live.active_color {
                                                    TrafficLightColor::Green => (
                                                        "HIJAU",
                                                        egui::Color32::from_rgb(110, 231, 183),
                                                    ),
                                                    TrafficLightColor::Yellow => (
                                                        "KUNING",
                                                        egui::Color32::from_rgb(254, 240, 138),
                                                    ),
                                                    TrafficLightColor::Red => (
                                                        "MERAH",
                                                        egui::Color32::from_rgb(253, 164, 175),
                                                    ),
                                                }
                                            };
                                            (
                                                format!(
                                                    "{} ({c_lbl} {:.1}s)  •  {} kendaraan ({} berhenti)",
                                                    if live.active_phase_name.is_empty() {
                                                        "APILL"
                                                    } else {
                                                        &live.active_phase_name
                                                    },
                                                    live.remaining_sec,
                                                    live.total_vehicles,
                                                    live.stopped_vehicles
                                                ),
                                                c_col,
                                            )
                                        } else {
                                            let lights = trc
                                                .traffic_lights
                                                .iter()
                                                .filter(|l| l.enabled)
                                                .count();
                                            let phases = trc.effective_signal_phases().len();
                                            (
                                                format!(
                                                    "{active_lanes} Jalur  •  {lights} Lampu ({phases} Fase)  •  {:.0}–{:.0} km/j{}",
                                                    trc.min_speed_mps * 3.6,
                                                    trc.max_speed_mps * 3.6,
                                                    if trc.allow_overtaking {
                                                        "  •  Salip"
                                                    } else {
                                                        ""
                                                    }
                                                ),
                                                egui::Color32::from_rgb(253, 164, 175),
                                            )
                                        }
                                    } else {
                                        (
                                            format!(
                                                "{active_lanes} Jalur  •  {} Model Pool  •  {:.0}–{:.0} km/j{}",
                                                trc.vehicle_pool.len(),
                                                trc.min_speed_mps * 3.6,
                                                trc.max_speed_mps * 3.6,
                                                if trc.allow_overtaking { "  •  Salip" } else { "" }
                                            ),
                                            egui::Color32::from_rgb(253, 230, 138),
                                        )
                                    };
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(info_line)
                                                .size(11.0)
                                                .color(info_col),
                                        )
                                        .truncate(),
                                    );
                                } else if let Some(ref tqc) = tq_cfg {
                                    let incomplete = tqc.incomplete_corridor_names();
                                    let (info_line, info_col) = if let Some(ref live) = tq_live
                                        && matches!(status, AnimRunStatus::Playing | AnimRunStatus::Paused)
                                    {
                                        if live.clearance_active {
                                            if is_traffic_manual {
                                                (
                                                    format!(
                                                        "LANCAR MANUAL: {}  •  Sisa {} ({} antre, {} keluar)",
                                                        live.active_corridor_name
                                                            .as_deref()
                                                            .unwrap_or("-"),
                                                        live.total_vehicles,
                                                        live.queued_vehicles,
                                                        live.clearing_vehicles
                                                    ),
                                                    egui::Color32::from_rgb(110, 231, 183),
                                                )
                                            } else {
                                                (
                                                    format!(
                                                        "MODE LANCAR  •  Sisa {} kendaraan ({} antre, {} keluar)",
                                                        live.total_vehicles,
                                                        live.queued_vehicles,
                                                        live.clearing_vehicles
                                                    ),
                                                    egui::Color32::from_rgb(110, 231, 183),
                                                )
                                            }
                                        } else if live.is_all_stopped {
                                            if is_traffic_manual {
                                                (
                                                    format!(
                                                        "TAHAN SEMUA  •  {} kendaraan antre — pilih tombol jalur untuk membuka",
                                                        live.total_vehicles
                                                    ),
                                                    egui::Color32::from_rgb(253, 164, 175),
                                                )
                                            } else {
                                                (
                                                    format!(
                                                        "MACET TOTAL  •  {} kendaraan tertahan diam — klik LANCAR untuk urai",
                                                        live.total_vehicles
                                                    ),
                                                    egui::Color32::from_rgb(253, 164, 175),
                                                )
                                            }
                                        } else if live.is_switch_pause {
                                            (
                                                format!(
                                                    "Jeda pindah -> {} ({:.1}s)  •  {} kendaraan",
                                                    live.active_corridor_name.as_deref().unwrap_or("-"),
                                                    live.remaining_turn_sec,
                                                    live.total_vehicles
                                                ),
                                                egui::Color32::from_rgb(253, 230, 138),
                                            )
                                        } else if is_traffic_manual {
                                            (
                                                format!(
                                                    "Dibuka Manual: {}  •  {} antre / {} lancar",
                                                    live.active_corridor_name.as_deref().unwrap_or("-"),
                                                    live.queued_vehicles,
                                                    live.clearing_vehicles
                                                ),
                                                egui::Color32::from_rgb(199, 210, 254),
                                            )
                                        } else {
                                            (
                                                format!(
                                                    "Giliran: {} ({:.0}s)  •  {} antre / {} lancar",
                                                    live.active_corridor_name.as_deref().unwrap_or("-"),
                                                    live.remaining_turn_sec,
                                                    live.queued_vehicles,
                                                    live.clearing_vehicles
                                                ),
                                                egui::Color32::from_rgb(153, 246, 228),
                                            )
                                        }
                                    } else if !incomplete.is_empty() {
                                        (
                                            format!(
                                                "Belum lengkap: {} belum punya Jalur Lancar!",
                                                incomplete.join(", ")
                                            ),
                                            egui::Color32::from_rgb(248, 113, 113),
                                        )
                                    } else {
                                        let active_cor = tqc
                                            .corridors
                                            .iter()
                                            .filter(|c| c.enabled && c.is_complete())
                                            .count();
                                        (
                                            if is_traffic_manual {
                                                format!(
                                                    "{active_cor} Jalur Berpasangan  •  {} Model  •  Kontrol Manual",
                                                    tqc.vehicle_pool.len(),
                                                )
                                            } else {
                                                format!(
                                                    "{active_cor} Jalur Berpasangan  •  {} Model  •  {:.0}s/giliran",
                                                    tqc.vehicle_pool.len(),
                                                    tqc.turn_duration_sec
                                                )
                                            },
                                            egui::Color32::from_rgb(153, 246, 228),
                                        )
                                    };
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(info_line)
                                                .size(11.0)
                                                .color(info_col),
                                        )
                                        .truncate(),
                                    );
                                } else if let Some(ref tdc) = td_cfg {
                                    let incomplete = tdc.incomplete_corridor_labels();
                                    let (info_line, info_col) = if let Some(ref live) = td_live
                                        && matches!(status, AnimRunStatus::Playing | AnimRunStatus::Paused)
                                    {
                                        if live.is_diverted {
                                            (
                                                format!(
                                                    "DIALIHKAN -> Alternatif  •  {} pendekat / {} utama / {} alih",
                                                    live.on_approach, live.on_main, live.on_divert
                                                ),
                                                egui::Color32::from_rgb(253, 230, 138),
                                            )
                                        } else {
                                            (
                                                format!(
                                                    "NORMAL -> Jalur Utama  •  {} pendekat / {} utama / {} alih",
                                                    live.on_approach, live.on_main, live.on_divert
                                                ),
                                                egui::Color32::from_rgb(125, 211, 252),
                                            )
                                        }
                                    } else if !incomplete.is_empty() {
                                        (
                                            format!("Belum lengkap: {}", incomplete.join(", ")),
                                            egui::Color32::from_rgb(248, 113, 113),
                                        )
                                    } else {
                                        let active_cor = tdc
                                            .corridors
                                            .iter()
                                            .filter(|c| c.enabled && c.is_complete())
                                            .count();
                                        (
                                            format!(
                                                "{active_cor} Koridor Y  •  {} Model  •  {}",
                                                tdc.vehicle_pool.len(),
                                                if tdc.start_diverted {
                                                    "Mulai Dialihkan"
                                                } else {
                                                    "Mulai Normal"
                                                }
                                            ),
                                            egui::Color32::from_rgb(125, 211, 252),
                                        )
                                    };
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(info_line)
                                                .size(11.0)
                                                .color(info_col),
                                        )
                                        .truncate(),
                                    );
                                } else if let Some(ref pcfg) = p_cfg {
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
                                    let target_line = if sh_cfg.is_some() {
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
                                        format!("Target ({} Objek): {preview_names}", bound_names.len())
                                    } else if is_multi {
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

                                // Speed / distance OR Water Cannon OR Particle OR Group metadata
                                let meta_line = if let Some(ref grp) = grp_cfg {
                                    let delay_tail = if grp.initial_delay_sec > 0.001 {
                                        format!("  •  Awal {:.2}s", grp.initial_delay_sec)
                                    } else {
                                        String::new()
                                    };
                                    let play_all_tail = if grp.include_in_play_all {
                                        "  •  Play Semua"
                                    } else {
                                        ""
                                    };
                                    match grp.mode {
                                        GroupAnimMode::Combine => format!(
                                            "Mode: Jalan serentak (paralel){delay_tail}{play_all_tail}"
                                        ),
                                        GroupAnimMode::Sequence => {
                                            let trig = match grp.sequence_trigger {
                                                SequenceTriggerMode::WaitComplete => {
                                                    if grp.sequence_gap_sec > 0.001 {
                                                        format!(
                                                            "Tunggu Selesai (+{:.2}s)",
                                                            grp.sequence_gap_sec
                                                        )
                                                    } else {
                                                        "Tunggu Selesai".to_string()
                                                    }
                                                }
                                                SequenceTriggerMode::WaitDelay => {
                                                    format!("Jeda {:.2} dtk", grp.sequence_delay_sec)
                                                }
                                            };
                                            let rep_tail = if anim.replay { "  •  Repeat" } else { "" };
                                            format!("Mode: {trig}{delay_tail}{rep_tail}{play_all_tail}")
                                        }
                                    }
                                } else if let Some(ref trc) = tr_cfg {
                                    if trc.continuous {
                                        format!(
                                            "Pool: max {}/jalur (tot {})  •  Terus ({:.1}s)",
                                            trc.max_vehicles_per_lane,
                                            trc.max_total_vehicles,
                                            dist_m
                                        )
                                    } else {
                                        format!(
                                            "Pool: max {}/jalur  •  {:.1} / {:.1} dtk{}",
                                            trc.max_vehicles_per_lane,
                                            dist_m,
                                            total_m,
                                            if anim.replay { "  •  Repeat" } else { "" }
                                        )
                                    }
                                } else if let Some(ref pcfg) = p_cfg {
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
                                } else if let Some(ref sh) = sh_cfg {
                                    let seq_lbl = if sh.stagger_sec > 0.001 {
                                        format!("Berurutan +{:.2}s", sh.stagger_sec)
                                    } else {
                                        "Serentak".to_string()
                                    };
                                    let delay_lbl = if sh.delay_sec > 0.001 {
                                        format!("  •  Delay {:.1}s", sh.delay_sec)
                                    } else {
                                        String::new()
                                    };
                                    let play_all_lbl = if sh.include_in_play_all {
                                        "  •  Play Semua"
                                    } else {
                                        ""
                                    };
                                    format!(
                                        "Efek: {}  •  {seq_lbl}{delay_lbl}{play_all_lbl}",
                                        sh.transition.short_label()
                                    )
                                } else if let Some(ref ptrc) = ptr_cfg {
                                    let lbl_info = match ptrc.effective_label() {
                                        Some(t) => format!("Teks: \"{t}\""),
                                        None => "Tanpa teks".to_string(),
                                    };
                                    if ptrc.continuous {
                                        format!(
                                            "Skala: {:.1}x  •  {lbl_info}  •  Terus-menerus",
                                            ptrc.arrow_scale
                                        )
                                    } else {
                                        format!(
                                            "Skala: {:.1}x  •  {lbl_info}  •  {:.1} / {:.1} dtk{}",
                                            ptrc.arrow_scale,
                                            dist_m,
                                            total_m,
                                            if anim.replay { " (Repeat)" } else { "" }
                                        )
                                    }
                                } else if let Some(sk) = shoot_kind {
                                    let time_tail = if shoot_continuous {
                                        "Terus-menerus".to_string()
                                    } else {
                                        format!(
                                            "{:.1} / {:.1} dtk{}",
                                            dist_m,
                                            total_m,
                                            if anim.replay { " (Repeat)" } else { "" }
                                        )
                                    };
                                    match sk {
                                        ShootAnimKind::WaterCannon => {
                                            let wcfg = parse_water_cannon_config(anim);
                                            format!(
                                                "Jarak: {:.0} m  •  Tekanan: {:.1} bar  •  {time_tail}",
                                                wcfg.range_m, wcfg.pressure_bar
                                            )
                                        }
                                        ShootAnimKind::StoneThrow => {
                                            let scfg = parse_stone_throw_config(anim);
                                            format!(
                                                "Jarak: {:.0} m  •  Laju: {:.1}/dtk  •  {time_tail}",
                                                scfg.range_m, scfg.throw_rate_hz
                                            )
                                        }
                                        ShootAnimKind::TearGas => {
                                            let tcfg = parse_tear_gas_config(anim);
                                            format!(
                                                "Jarak: {:.0} m  •  Asap: {:.0} dtk  •  {time_tail}",
                                                tcfg.range_m, tcfg.smoke_duration_sec
                                            )
                                        }
                                        ShootAnimKind::Bullet => {
                                            let bcfg = parse_bullet_shoot_config(anim);
                                            format!(
                                                "Hilang: {:.0} m  •  Laju: {:.1}/dtk  •  {time_tail}",
                                                bcfg.max_range_m, bcfg.fire_rate_rps
                                            )
                                        }
                                        ShootAnimKind::Apar => {
                                            let acfg = parse_apar_spray_config(anim);
                                            format!(
                                                "Jarak: {:.1} m  •  Sapuan: {}  •  {time_tail}",
                                                acfg.range_m,
                                                if acfg.sweep_enabled { "Aktif" } else { "Lurus" }
                                            )
                                        }
                                    }
                                } else if let Some(ref tqc) = tq_cfg {
                                    format!(
                                        "Konvoi: {:.1} m/s  •  Lancar: {:.1} m/s  •  t = {:.1} dtk",
                                        tqc.convoy_speed_mps, tqc.clear_speed_mps, dist_m
                                    )
                                } else if let Some(ref tdc) = td_cfg {
                                    match tdc.flow_mode {
                                        twg_shared::traffic_diversion::DiversionFlowMode::Convoy => {
                                            format!(
                                                "Konvoi: {:.1} / Alih {:.1} m/s  •  t = {:.1} dtk",
                                                tdc.normal_speed_mps, tdc.divert_speed_mps, dist_m
                                            )
                                        }
                                        twg_shared::traffic_diversion::DiversionFlowMode::FreeFlow => {
                                            format!(
                                                "Arus Normal: {:.0}–{:.0} m/s{}  •  t = {:.1} dtk",
                                                tdc.min_speed_mps,
                                                tdc.max_speed_mps,
                                                if tdc.allow_overtaking { " • Salip" } else { "" },
                                                dist_m
                                            )
                                        }
                                    }
                                } else {
                                    let move_tag =
                                        twg_shared::route_formation::parse_route_move_anim_config(anim)
                                            .badge_label()
                                            .map(|b| format!("  •  {b}"))
                                            .unwrap_or_default();
                                    format!(
                                        "Speed: {:.1} m/s{move_tag}  •  Jarak: {:.1} / {:.1} m  •  Repeat: {}",
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

                                draw_card_progress_bar(ui, status, progress);

                                ui.add_space(7.0);

                                // EXACT per-route state machine buttons:
                                // - Stopped -> [Play]
                                // - Playing -> [Pause] [Stop]
                                // - Paused  -> [Resume] [Stop]
                                // - Finished -> [Play] [Stop (Reset Awal)]
                                match status {
                                    AnimRunStatus::Stopped => {
                                        let (play_txt, play_fill) = match sh_cfg.as_ref() {
                                            Some(sh) if sh.action == SpawnHideAction::Spawn => (
                                                "Play (Munculkan)",
                                                egui::Color32::from_rgb(5, 150, 105),
                                            ),
                                            Some(_) => (
                                                "Play (Sembunyikan)",
                                                egui::Color32::from_rgb(190, 18, 60),
                                            ),
                                            None => ("Play", egui::Color32::from_rgb(5, 150, 105)),
                                        };
                                        let play_btn = egui::Button::new(
                                            egui::RichText::new(play_txt)
                                                .size(12.0)
                                                .strong()
                                                .color(egui::Color32::WHITE),
                                        )
                                        .fill(if can_play {
                                            play_fill
                                        } else {
                                            egui::Color32::from_rgb(30, 41, 59)
                                        })
                                        .min_size(egui::vec2(ui.available_width(), 28.0));
                                        if ui.add_enabled(can_play, play_btn).clicked() {
                                            action_play = Some(anim.id);
                                        }
                                    }
                                    AnimRunStatus::Playing => {
                                        if is_traffic_queue {
                                            let is_clearing =
                                                track.is_some_and(|t| t.is_queue_clearing());
                                            if is_traffic_manual
                                                && let Some(ref tqc) = tq_cfg
                                            {
                                                let open_src = tq_live
                                                    .as_ref()
                                                    .and_then(|s| s.active_corridor_source_idx);
                                                let all_stopped = tq_live
                                                    .as_ref()
                                                    .is_none_or(|s| s.is_all_stopped);
                                                if !is_clearing {
                                                    ui.horizontal_wrapped(|ui| {
                                                        ui.spacing_mut().item_spacing =
                                                            egui::vec2(4.0, 4.0);
                                                        for (c_idx, cor) in
                                                            tqc.corridors.iter().enumerate()
                                                        {
                                                            if !cor.enabled || !cor.is_complete() {
                                                                continue;
                                                            }
                                                            let is_open = open_src == Some(c_idx);
                                                            let btn = egui::Button::new(
                                                                egui::RichText::new(if is_open {
                                                                    format!("{} [BUKA]", cor.name)
                                                                } else {
                                                                    format!("Buka {}", cor.name)
                                                                })
                                                                .size(10.5)
                                                                .strong()
                                                                .color(egui::Color32::WHITE),
                                                            )
                                                            .fill(if is_open {
                                                                egui::Color32::from_rgb(37, 99, 235)
                                                            } else {
                                                                egui::Color32::from_rgb(51, 65, 85)
                                                            });
                                                            if ui.add(btn).clicked() {
                                                                action_queue_manual = Some((
                                                                    anim.id,
                                                                    QueueManualAction::OpenCorridor {
                                                                        corridor_idx: c_idx,
                                                                    },
                                                                ));
                                                            }
                                                        }
                                                        let hold_btn = egui::Button::new(
                                                            egui::RichText::new("Tahan Semua")
                                                                .size(10.5)
                                                                .strong()
                                                                .color(egui::Color32::WHITE),
                                                        )
                                                        .fill(if all_stopped {
                                                            egui::Color32::from_rgb(190, 18, 60)
                                                        } else {
                                                            egui::Color32::from_rgb(71, 85, 105)
                                                        });
                                                        if ui.add(hold_btn).clicked() {
                                                            action_queue_manual = Some((
                                                                anim.id,
                                                                QueueManualAction::HoldAll,
                                                            ));
                                                        }
                                                    });
                                                    ui.add_space(4.0);
                                                }
                                                ui.horizontal_wrapped(|ui| {
                                                    ui.spacing_mut().item_spacing =
                                                        egui::vec2(4.0, 4.0);
                                                    for (c_idx, cor) in
                                                        tqc.corridors.iter().enumerate()
                                                    {
                                                        if !cor.enabled || !cor.is_complete() {
                                                            continue;
                                                        }
                                                        let (q_cnt, tot_cnt) = tq_live
                                                            .as_ref()
                                                            .map(|s| s.corridor_counts(c_idx))
                                                            .unwrap_or((0, 0));
                                                        let is_draining =
                                                            is_clearing && open_src == Some(c_idx);
                                                        let can_click = !is_clearing || q_cnt > 0;
                                                        let label = if is_clearing && tot_cnt == 0 {
                                                            format!("{} [Habis]", cor.name)
                                                        } else if is_clearing && q_cnt == 0 {
                                                            format!(
                                                                "{} [Keluar {}]",
                                                                cor.name, tot_cnt
                                                            )
                                                        } else if is_draining {
                                                            format!(
                                                                "{} [LANCAR · {}]",
                                                                cor.name, q_cnt
                                                            )
                                                        } else if q_cnt > 0 {
                                                            format!(
                                                                "Lancar {} ({})",
                                                                cor.name, q_cnt
                                                            )
                                                        } else {
                                                            format!("Lancar {}", cor.name)
                                                        };
                                                        let fill = if !can_click {
                                                            egui::Color32::from_rgb(30, 41, 59)
                                                        } else if is_draining {
                                                            egui::Color32::from_rgb(5, 150, 105)
                                                        } else {
                                                            egui::Color32::from_rgb(6, 95, 70)
                                                        };
                                                        let btn = egui::Button::new(
                                                            egui::RichText::new(label)
                                                                .size(10.5)
                                                                .strong()
                                                                .color(if can_click {
                                                                    egui::Color32::WHITE
                                                                } else {
                                                                    egui::Color32::from_rgb(
                                                                        148, 163, 184,
                                                                    )
                                                                }),
                                                        )
                                                        .fill(fill);
                                                        if ui
                                                            .add_enabled(can_click, btn)
                                                            .on_hover_text(
                                                                "Aktifkan Mode Lancar (stop spawn) & urai khusus jalur ini secara manual",
                                                            )
                                                            .clicked()
                                                        {
                                                            action_queue_manual = Some((
                                                                anim.id,
                                                                QueueManualAction::DrainCorridor {
                                                                    corridor_idx: c_idx,
                                                                },
                                                            ));
                                                        }
                                                    }
                                                });
                                                ui.add_space(4.0);
                                            } else {
                                                let rem = tq_live
                                                    .as_ref()
                                                    .map(|s| s.total_vehicles)
                                                    .unwrap_or(0);
                                                let clear_btn = egui::Button::new(
                                                    egui::RichText::new(if is_clearing {
                                                        format!(
                                                            "Mode Lancar Aktif (Sisa {rem} kendaraan...)"
                                                        )
                                                    } else {
                                                        "LANCAR (Urai Sampai Habis)".to_string()
                                                    })
                                                    .size(11.5)
                                                    .strong()
                                                    .color(egui::Color32::WHITE),
                                                )
                                                .fill(if is_clearing {
                                                    egui::Color32::from_rgb(6, 78, 59)
                                                } else {
                                                    egui::Color32::from_rgb(5, 150, 105)
                                                })
                                                .min_size(egui::vec2(ui.available_width(), 28.0));
                                                if ui
                                                    .add_enabled(!is_clearing, clear_btn)
                                                    .on_hover_text(
                                                        "Stop spawn mobil baru & percepat seluruh kendaraan yang tersisa bergantian sampai habis (0 kendaraan)",
                                                    )
                                                    .clicked()
                                                {
                                                    action_queue_clear = Some(anim.id);
                                                }
                                                ui.add_space(4.0);
                                            }
                                        } else if is_traffic_diversion {
                                            let is_div = td_live
                                                .as_ref()
                                                .map(|s| s.is_diverted)
                                                .unwrap_or_else(|| {
                                                    td_cfg
                                                        .as_ref()
                                                        .is_some_and(|c| c.start_diverted)
                                                });
                                            let toggle_btn = egui::Button::new(
                                                egui::RichText::new(if is_div {
                                                    "NORMALKAN ARUS (Kembali ke Jalur Utama)"
                                                } else {
                                                    "ALIHKAN ARUS (Belok ke Jalur Alternatif)"
                                                })
                                                .size(11.5)
                                                .strong()
                                                .color(egui::Color32::WHITE),
                                            )
                                            .fill(if is_div {
                                                egui::Color32::from_rgb(2, 132, 199)
                                            } else {
                                                egui::Color32::from_rgb(217, 119, 6)
                                            })
                                            .min_size(egui::vec2(ui.available_width(), 28.0));
                                            if ui.add(toggle_btn).clicked() {
                                                action_diversion_toggle = Some((anim.id, !is_div));
                                            }
                                            ui.add_space(4.0);
                                        }
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

            // Apply deferred playback commands (broadcast ONLY in Tactical Live!)
            if let Some(aid) = action_play {
                playback.play_anim(aid);
                broadcast_tactical_anim_cmd(
                    state,
                    ws,
                    "play_anim",
                    Some(aid),
                    false,
                    playback.speed_multiplier,
                );
            } else if let Some(aid) = action_pause {
                playback.pause_anim_tree(aid, &state.anims);
                broadcast_tactical_anim_cmd(
                    state,
                    ws,
                    "pause_anim",
                    Some(aid),
                    false,
                    playback.speed_multiplier,
                );
            } else if let Some(aid) = action_resume {
                playback.resume_anim_tree(aid, &state.anims);
                broadcast_tactical_anim_cmd(
                    state,
                    ws,
                    "play_anim",
                    Some(aid),
                    false,
                    playback.speed_multiplier,
                );
            } else if let Some(aid) = action_stop {
                playback.stop_anim_tree(aid, &state.anims);
                broadcast_tactical_anim_cmd(
                    state,
                    ws,
                    "stop_anim",
                    Some(aid),
                    false,
                    playback.speed_multiplier,
                );
            } else if let Some(aid) = action_queue_clear {
                if let Some(trig_sec) = playback.trigger_queue_clearance(aid, None) {
                    broadcast_tactical_queue_clear_cmd(state, ws, aid, trig_sec);
                }
            } else if let Some((aid, act)) = action_queue_manual {
                if let Some(trig_sec) = playback.push_queue_manual_action(aid, act, None) {
                    broadcast_tactical_queue_manual_cmd(state, ws, aid, act, trig_sec);
                }
            } else if let Some((aid, is_div)) = action_diversion_toggle {
                if let Some(trig_sec) = playback.push_diversion_toggle(aid, is_div, None) {
                    broadcast_tactical_diversion_toggle_cmd(state, ws, aid, is_div, trig_sec);
                }
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

pub(super) fn draw_card_progress_bar(ui: &mut egui::Ui, status: AnimRunStatus, progress: f32) {
    let bar_h = 5.0;
    let (bar_rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), bar_h),
        egui::Sense::hover(),
    );
    let painter = ui.painter();
    painter.rect_filled(bar_rect, 2.5, egui::Color32::from_rgb(12, 18, 30));
    if progress > 0.0 {
        let fill_w = (bar_rect.width() * progress.clamp(0.0, 1.0)).max(4.0);
        let fill_rect = egui::Rect::from_min_size(bar_rect.min, egui::vec2(fill_w, bar_h));
        let color = match status {
            AnimRunStatus::Playing => egui::Color32::from_rgb(16, 185, 129),
            AnimRunStatus::Paused => egui::Color32::from_rgb(245, 158, 11),
            _ => egui::Color32::from_rgb(56, 189, 248),
        };
        painter.rect_filled(fill_rect, 2.5, color);
    }
}

fn draw_tactical_skenario_tab(
    ui: &mut egui::Ui,
    state: &mut AppState,
    playback: &mut PlaybackState,
    play_rts: &mut PlayRtsState,
    api: &ApiClientBridge,
    ws: &WsClientBridge,
) {
    let active_sk_opt = state.active_skenario.clone();
    let op_name = if !state.tactical_page.active_operasi_name.is_empty() {
        state.tactical_page.active_operasi_name.clone()
    } else {
        active_sk_opt
            .as_ref()
            .and_then(|s| s.operasi_name.clone())
            .unwrap_or_else(|| "Operasi Aktif".to_string())
    };
    let can_big = state.can_control_active_big_screen();

    egui::Frame::new()
        .fill(egui::Color32::from_rgb(18, 28, 44))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(38, 52, 74)))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                egui::RichText::new(format!("Operasi: {op_name}"))
                    .size(12.0)
                    .strong()
                    .color(egui::Color32::WHITE),
            );
            ui.add_space(4.0);
            if let Some(active_sk) = &active_sk_opt {
                ui.horizontal(|ui| {
                    let reload_btn = egui::Button::new(
                        egui::RichText::new(if can_big {
                            "Reload Skenario (+ Layar Besar)"
                        } else {
                            "Reload Skenario"
                        })
                        .size(11.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                    )
                    .fill(egui::Color32::from_rgb(180, 83, 9))
                    .min_size(egui::vec2(ui.available_width(), 26.0));
                    if ui
                        .add(reload_btn)
                        .on_hover_text(if can_big {
                            "Muat ulang data skenario terbaru dari server dan sinkronkan reload ke Layar Besar & peserta lain"
                        } else {
                            "Muat ulang data skenario terbaru dari server"
                        })
                        .clicked()
                    {
                        play_rts.reset_all();
                        let siblings = state.tactical_page.active_operasi_skenarios.clone();
                        open_tactical_skenario(
                            state,
                            playback,
                            api,
                            active_sk,
                            op_name.clone(),
                            siblings,
                            if can_big {
                                Some("reload_skenario")
                            } else {
                                None
                            },
                        );
                    }
                });
            }
        });

    ui.add_space(6.0);
    ui.separator();
    ui.add_space(4.0);

    let siblings = state.tactical_page.active_operasi_skenarios.clone();
    let active_id = active_sk_opt.as_ref().map(|s| s.id);
    let mut switch_to: Option<twg_shared::models::Skenario> = None;
    let mut send_big_only: Option<twg_shared::models::Skenario> = None;

    egui::ScrollArea::vertical()
        .id_salt("play_panel_skenarios_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if siblings.is_empty() {
                ui.label(
                    egui::RichText::new("Belum ada daftar skenario lain di operasi ini.")
                        .size(11.5)
                        .color(egui::Color32::from_rgb(148, 163, 184)),
                );
                return;
            }

            for sk in &siblings {
                let is_active = active_id == Some(sk.id);
                let sk_can_big = state.can_control_skenario_big_screen(sk);
                egui::Frame::new()
                    .fill(if is_active {
                        egui::Color32::from_rgb(12, 54, 48)
                    } else {
                        egui::Color32::from_rgb(20, 29, 44)
                    })
                    .stroke(egui::Stroke::new(
                        1.2,
                        if is_active {
                            egui::Color32::from_rgb(16, 185, 129)
                        } else {
                            egui::Color32::from_rgb(44, 58, 82)
                        },
                    ))
                    .corner_radius(6.0)
                    .inner_margin(egui::Margin::same(9))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(&sk.name)
                                    .size(12.5)
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            );
                            if is_active {
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            egui::RichText::new("AKTIF")
                                                .size(9.5)
                                                .strong()
                                                .color(egui::Color32::from_rgb(110, 231, 183)),
                                        );
                                    },
                                );
                            }
                        });
                        ui.label(
                            egui::RichText::new(format!(
                                "{} unit  •  {} animasi",
                                sk.marker_count, sk.anim_count
                            ))
                            .size(10.5)
                            .color(egui::Color32::from_rgb(148, 163, 184)),
                        );
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            let btn_label = if is_active {
                                "Reload"
                            } else if sk_can_big {
                                "Buka (+ Layar Besar)"
                            } else {
                                "Buka Skenario"
                            };
                            let open_btn = egui::Button::new(
                                egui::RichText::new(btn_label)
                                    .size(11.0)
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            )
                            .fill(if is_active {
                                egui::Color32::from_rgb(180, 83, 9)
                            } else {
                                egui::Color32::from_rgb(5, 150, 105)
                            });
                            if ui.add(open_btn).clicked() {
                                switch_to = Some(sk.clone());
                            }
                            if is_active && sk_can_big && !state.offline_preview {
                                let push_btn = egui::Button::new(
                                    egui::RichText::new("Kirim Layar Besar")
                                        .size(10.5)
                                        .color(egui::Color32::from_rgb(254, 240, 138)),
                                )
                                .fill(egui::Color32::from_rgb(113, 63, 18));
                                if ui.add(push_btn).clicked() {
                                    send_big_only = Some(sk.clone());
                                }
                            }
                        });
                    });
                ui.add_space(6.0);
            }
        });

    if let Some(sk) = switch_to {
        let is_reload = active_id == Some(sk.id);
        let sk_can_big = state.can_control_skenario_big_screen(&sk);
        play_rts.reset_all();
        open_tactical_skenario(
            state,
            playback,
            api,
            &sk,
            op_name,
            siblings,
            if sk_can_big {
                Some(if is_reload {
                    "reload_skenario"
                } else {
                    "load_skenario"
                })
            } else {
                None
            },
        );
    } else if let Some(sk) = send_big_only {
        ws.send(WsOutCommand::SendCommand(TacticalCommand {
            operasi_id: sk.operasi_id,
            skenario_id: sk.id,
            marker_id: None,
            command: "reload_skenario".to_string(),
            payload: serde_json::json!({ "skenario_name": sk.name }),
            save_history: false,
        }));
        state.status_banner = Some(format!(
            "Skenario '{}' dikirim / di-reload ke Layar Besar.",
            sk.name
        ));
    }
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

fn anim_matches_play_search(
    anim: &twg_shared::models::Anim,
    all_anims: &[twg_shared::models::Anim],
    markers: &[twg_shared::models::Marker],
    query_lower: &str,
) -> bool {
    if query_lower.is_empty() {
        return true;
    }

    let mut haystack = String::with_capacity(128);
    haystack.push_str(&anim.name);
    haystack.push(' ');
    haystack.push_str(&anim.anim_type);
    haystack.push(' ');
    haystack.push_str(AnimCategory::from_anim_type(&anim.anim_type).label());

    if let Some(kind) = shoot_anim_kind(anim) {
        haystack.push(' ');
        haystack.push_str(kind.label());
        haystack.push(' ');
        haystack.push_str(kind.short_tag());
    } else if is_particle_anim(anim) {
        let cfg = parse_particle_anim_config(anim);
        haystack.push(' ');
        haystack.push_str(cfg.kind.label());
        haystack.push(' ');
        haystack.push_str(cfg.kind.short_tag());
    } else if is_group_anim(anim) {
        let cfg = parse_group_anim_config(anim);
        haystack.push_str(" grup combine sequence bareng berurutan ");
        haystack.push_str(cfg.mode.label());
        haystack.push(' ');
        haystack.push_str(cfg.mode.short_tag());
        for it in &cfg.items {
            if let Some(child) = all_anims.iter().find(|x| x.id == it.anim_id) {
                haystack.push(' ');
                haystack.push_str(&child.name);
            }
        }
    } else if is_spawn_hide_anim(anim) {
        let cfg = parse_spawn_hide_config(anim);
        haystack.push_str(" spawn hide muncul sembunyi ");
        haystack.push_str(cfg.action.label());
        haystack.push(' ');
        haystack.push_str(cfg.action.short_tag());
        haystack.push(' ');
        haystack.push_str(cfg.transition.label());
    } else if is_pointer_arrow_anim(anim) {
        let cfg = parse_pointer_arrow_config(anim);
        haystack.push_str(" panah pointer penunjuk");
        if !cfg.label_text.is_empty() {
            haystack.push(' ');
            haystack.push_str(&cfg.label_text);
        }
    } else if twg_shared::model_action_anim::is_model_action_anim(anim) {
        let cfg = twg_shared::model_action_anim::parse_model_action_config(anim);
        haystack.push_str(" aksi model 3d klip animasi action ");
        haystack.push_str(&cfg.action_slug);
        haystack.push(' ');
        haystack.push_str(cfg.play_mode.label());
    } else if twg_shared::billboard_anim::is_billboard_anim(anim) {
        haystack.push_str(" billboard videotron layar video gambar media ");
        let cfg = twg_shared::billboard_anim::parse_billboard_anim_config(anim);
        haystack.push_str(cfg.media_mode.label());
    } else if is_traffic_flow_or_signal(anim) {
        let cfg = parse_traffic_anim_config(anim);
        haystack.push_str(
            " traffic lalu lintas kendaraan jalan salip lampu merah apill simpang signal ",
        );
        for l in &cfg.lanes {
            haystack.push(' ');
            haystack.push_str(&l.name);
        }
        for tl in &cfg.traffic_lights {
            haystack.push(' ');
            haystack.push_str(&tl.name);
        }
        for v in &cfg.vehicle_pool {
            haystack.push(' ');
            haystack.push_str(&v.name);
        }
    } else if is_traffic_queue_anim(anim) {
        let cfg = parse_traffic_queue_config(anim);
        haystack.push_str(
            " urai simpang antrean lancar buka tutup konvoi macet total stuck blokade manual petugas traffic ",
        );
        for c in &cfg.corridors {
            haystack.push(' ');
            haystack.push_str(&c.name);
        }
        for v in &cfg.vehicle_pool {
            haystack.push(' ');
            haystack.push_str(&v.name);
        }
    } else if is_traffic_diversion_anim(anim) {
        let cfg = parse_traffic_diversion_config(anim);
        haystack.push_str(" pengalihan arus alih diversion rekayasa alternatif belok traffic ");
        for c in &cfg.corridors {
            haystack.push(' ');
            haystack.push_str(&c.name);
        }
        for v in &cfg.vehicle_pool {
            haystack.push(' ');
            haystack.push_str(&v.name);
        }
    } else if is_multi_route_anim(anim) {
        let cfg = parse_multi_route_config(anim);
        haystack.push_str(" formasi multi ");
        haystack.push_str(cfg.formation.label());
        haystack.push(' ');
        haystack.push_str(cfg.formation.short_label());
    }

    for mid in anim_bound_marker_ids(anim) {
        if let Some(m) = markers.iter().find(|m| m.id == mid) {
            haystack.push(' ');
            haystack.push_str(&m.name);
        }
    }

    let haystack_lower = haystack.to_lowercase();
    query_lower
        .split_whitespace()
        .all(|token| haystack_lower.contains(token))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;
    use twg_shared::models::{Anim, Marker};

    fn dummy_marker(id: Uuid, name: &str) -> Marker {
        let now = Utc::now();
        Marker {
            id,
            operasi_id: Uuid::nil(),
            skenario_id: Uuid::nil(),
            unit_id: "unit".into(),
            name: name.into(),
            category: "kendaraan".into(),
            model_path: String::new(),
            quantity: 1,
            lng: 106.8,
            lat: -6.2,
            altitude: 0.0,
            rot_x: 0.0,
            rot_y: 0.0,
            rot_z: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            scale_z: 1.0,
            is_spawn: true,
            description: None,
            metadata: json!({}),
            created_at: now,
            updated_at: now,
        }
    }

    fn dummy_anim(
        name: &str,
        anim_type: &str,
        marker_id: Option<Uuid>,
        data: serde_json::Value,
    ) -> Anim {
        let now = Utc::now();
        Anim {
            id: Uuid::new_v4(),
            operasi_id: Uuid::nil(),
            skenario_id: Uuid::nil(),
            marker_id,
            name: name.into(),
            anim_type: anim_type.into(),
            sort_order: 0,
            speed: 5.0,
            duration_sec: 10.0,
            replay: false,
            autoplay: false,
            geom: vec![],
            data,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn play_panel_search_matches_name_marker_type_and_tokens() {
        let m_awc = Uuid::new_v4();
        let m_vip = Uuid::new_v4();
        let markers = vec![
            dummy_marker(m_awc, "Mobil AWC 01"),
            dummy_marker(m_vip, "Sedan VIP"),
        ];

        let shoot_anim = dummy_anim(
            "Semprot Gerbang",
            "shoot_water_cannon",
            Some(m_awc),
            json!({}),
        );
        let pointer_anim = dummy_anim(
            "Penanda Posko",
            "pointer_arrow",
            Some(m_vip),
            json!({ "pointer_arrow": { "label_text": "TITIK KUMPUL" } }),
        );

        let all_anims = vec![shoot_anim.clone(), pointer_anim.clone()];
        assert!(anim_matches_play_search(
            &shoot_anim,
            &all_anims,
            &markers,
            ""
        ));
        assert!(anim_matches_play_search(
            &shoot_anim,
            &all_anims,
            &markers,
            "gerbang"
        ));
        assert!(anim_matches_play_search(
            &shoot_anim,
            &all_anims,
            &markers,
            "awc 01"
        ));
        assert!(anim_matches_play_search(
            &shoot_anim,
            &all_anims,
            &markers,
            "water gerbang"
        ));
        assert!(!anim_matches_play_search(
            &shoot_anim,
            &all_anims,
            &markers,
            "patwal"
        ));

        assert!(anim_matches_play_search(
            &pointer_anim,
            &all_anims,
            &markers,
            "titik kumpul"
        ));
        assert!(anim_matches_play_search(
            &pointer_anim,
            &all_anims,
            &markers,
            "panah sedan"
        ));
        assert!(!anim_matches_play_search(
            &pointer_anim,
            &all_anims,
            &markers,
            "gerbang"
        ));
    }

    #[test]
    fn playback_state_clears_search_on_enter_and_exit() {
        let mut pb = PlaybackState {
            play_panel_search: "patwal".into(),
            ..Default::default()
        };
        pb.enter_play_mode();
        assert!(pb.play_panel_search.is_empty());

        pb.play_panel_search = "water".into();
        pb.stop_play_test();
        assert!(pb.play_panel_search.is_empty());
    }
}
