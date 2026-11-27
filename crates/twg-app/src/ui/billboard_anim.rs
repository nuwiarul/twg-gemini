//! Media -> Billboard / Videotron 3D creation card, Inspector UI, and Play Panel card.

use bevy_egui::egui;
use twg_shared::{
    billboard_anim::{
        BillboardIdleMode, BillboardMediaMode, BillboardRepeat, BillboardStructureKind,
        is_billboard_anim, parse_billboard_anim_config, write_billboard_anim_config,
    },
    models::{Anim, MediaAssetType},
};

use crate::{
    map::camera::TacticalCamera,
    state::{AnimRunStatus, AppState, PlaybackState, ToolMode},
    ui::{
        right_panel::inspector_card,
        spinner::{SpinnerConfig, numeric_spinner_u32_w, numeric_spinner_w},
    },
};

pub fn draw_create(ui: &mut egui::Ui, state: &mut AppState, editable: bool) {
    let is_placing = state.tool_mode == ToolMode::PlaceBillboard;
    egui::Frame::new()
        .fill(if is_placing {
            egui::Color32::from_rgb(12, 46, 62)
        } else {
            egui::Color32::from_rgb(20, 29, 44)
        })
        .stroke(egui::Stroke::new(
            if is_placing { 1.4 } else { 1.0 },
            if is_placing {
                egui::Color32::from_rgb(56, 189, 248)
            } else {
                egui::Color32::from_rgb(44, 62, 92)
            },
        ))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.strong("Visual · Billboard / Videotron 3D");
            ui.label(
                egui::RichText::new(
                    "Pasang layar 3D prosedural di peta untuk menampilkan Gambar statis atau Video (Videotron) dari Assets.",
                )
                .size(10.5)
                .color(egui::Color32::from_rgb(148, 163, 184)),
            );
            ui.add_space(4.0);
            let btn = egui::Button::new(
                egui::RichText::new(if is_placing {
                    "Klik Lokasi di Peta (Aktif · Esc Batal)"
                } else {
                    "+ Billboard / Videotron 3D"
                })
                .size(11.0)
                .strong()
                .color(egui::Color32::WHITE),
            )
            .fill(if is_placing {
                egui::Color32::from_rgb(5, 150, 105)
            } else {
                egui::Color32::from_rgb(2, 132, 199)
            })
            .min_size(egui::vec2(ui.available_width(), 28.0));

            if ui.add_enabled(editable, btn).clicked() {
                if is_placing {
                    state.tool_mode = ToolMode::Select;
                } else {
                    let cnt = state
                        .anims
                        .iter()
                        .filter(|a| is_billboard_anim(a))
                        .count()
                        + 1;
                    state.new_billboard_name = format!("Billboard {cnt}");
                    state.tool_mode = ToolMode::PlaceBillboard;
                    state.draft_points_lnglat.clear();
                    state.picking_marker_for_anim_id = None;
                    state.picking_shoot_for_anim_id = None;
                    state.picking_pointer_for_anim_id = None;
                    state.picking_spawn_hide_for_anim_id = None;
                    state.placing_traffic_light_for_anim_id = None;
                    state.status_banner = Some(
                        "Billboard / Videotron 3D: Klik lokasi di peta untuk meletakkan (Esc batal)."
                            .into(),
                    );
                }
            }
        });
    ui.add_space(6.0);
}

pub fn asset_summary(state: &AppState, anim: &Anim) -> String {
    let cfg = parse_billboard_anim_config(anim);
    match cfg.media_mode {
        BillboardMediaMode::Image => {
            let name = state
                .media_assets
                .iter()
                .find(|m| Some(m.id) == cfg.image_asset_id && m.media_type == MediaAssetType::Texture)
                .map(|m| m.name.as_str())
                .unwrap_or("Belum pilih Gambar");
            format!("Gambar Statis · {name}")
        }
        BillboardMediaMode::Video => {
            let name = state
                .media_assets
                .iter()
                .find(|m| Some(m.id) == cfg.video_asset_id && m.media_type == MediaAssetType::Video)
                .map(|m| m.name.as_str())
                .unwrap_or("Belum pilih Video");
            format!("Video · {name} · {}", cfg.repeat.label())
        }
    }
}

pub fn draw_inspector(
    ui: &mut egui::Ui,
    state: &mut AppState,
    tcam: &mut TacticalCamera,
    anim: &mut Anim,
    editable: bool,
) {
    let mut cfg = parse_billboard_anim_config(anim);
    let original = cfg.clone();

    // Card 1: Media Source (Image vs Video)
    inspector_card(ui, "Billboard / Videotron 3D · Media", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            ui.label("Nama Billboard:");
            ui.add(egui::TextEdit::singleline(&mut anim.name).desired_width(f32::INFINITY));
            ui.add_space(5.0);

            ui.label("Tipe Konten:");
            ui.horizontal(|ui| {
                ui.selectable_value(
                    &mut cfg.media_mode,
                    BillboardMediaMode::Image,
                    "Gambar (Image)",
                );
                ui.selectable_value(
                    &mut cfg.media_mode,
                    BillboardMediaMode::Video,
                    "Video (Videotron)",
                );
            });
            ui.add_space(5.0);

            match cfg.media_mode {
                BillboardMediaMode::Image => {
                    ui.label("Gambar dari Assets:");
                    let selected = state.media_assets.iter().find(|m| {
                        Some(m.id) == cfg.image_asset_id && m.media_type == MediaAssetType::Texture
                    });
                    let selected_text = selected
                        .map(|m| m.name.as_str())
                        .unwrap_or("Pilih Gambar...");
                    egui::ComboBox::from_id_salt(("bb_img_asset", anim.id))
                        .width(ui.available_width())
                        .selected_text(selected_text)
                        .show_ui(ui, |ui| {
                            let mut found = false;
                            for asset in &state.media_assets {
                                if asset.media_type != MediaAssetType::Texture {
                                    continue;
                                }
                                found = true;
                                let dims = match (asset.active_width, asset.active_height) {
                                    (Some(w), Some(h)) => format!(" · {w}×{h}"),
                                    _ => String::new(),
                                };
                                if ui
                                    .selectable_label(
                                        cfg.image_asset_id == Some(asset.id),
                                        format!("{}{dims}", asset.name),
                                    )
                                    .clicked()
                                {
                                    cfg.image_asset_id = Some(asset.id);
                                }
                            }
                            if !found {
                                ui.label("Belum ada Gambar / Texture di halaman Assets.");
                            }
                        });
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(
                            "Mode Gambar tampil statis di Billboard 3D pada peta (tidak memutar animasi saat Play).",
                        )
                        .size(10.0)
                        .color(egui::Color32::from_rgb(148, 163, 184)),
                    );
                }
                BillboardMediaMode::Video => {
                    ui.label("Video dari Assets:");
                    let selected = state.media_assets.iter().find(|m| {
                        Some(m.id) == cfg.video_asset_id && m.media_type == MediaAssetType::Video
                    });
                    let selected_text = selected
                        .map(|m| m.name.as_str())
                        .unwrap_or("Pilih Video...");
                    egui::ComboBox::from_id_salt(("bb_vid_asset", anim.id))
                        .width(ui.available_width())
                        .selected_text(selected_text)
                        .show_ui(ui, |ui| {
                            let mut found = false;
                            for asset in &state.media_assets {
                                if asset.media_type != MediaAssetType::Video {
                                    continue;
                                }
                                found = true;
                                let dur = asset
                                    .duration_sec
                                    .map(|d| format!(" · {d:.1} dtk"))
                                    .unwrap_or_default();
                                if ui
                                    .selectable_label(
                                        cfg.video_asset_id == Some(asset.id),
                                        format!("{}{dur}", asset.name),
                                    )
                                    .clicked()
                                {
                                    cfg.video_asset_id = Some(asset.id);
                                }
                            }
                            if !found {
                                ui.label("Belum ada Video di halaman Assets.");
                            }
                        });
                    ui.add_space(4.0);
                    ui.checkbox(&mut anim.autoplay, "Autoplay saat Play Skenario");
                }
            }
        });

        if let Some(status) = state.billboard_status.get(&anim.id) {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(format!("Status: {status}"))
                    .size(10.5)
                    .color(egui::Color32::from_rgb(125, 211, 252)),
            );
        }
    });

    // Card 2: Video Playback Settings (only shown in Video mode)
    if cfg.media_mode == BillboardMediaMode::Video {
        inspector_card(ui, "Pengaturan Putar Video", true, |ui| {
            ui.add_enabled_ui(editable, |ui| {
                ui.label("Pengulangan (Repeat):");
                ui.horizontal_wrapped(|ui| {
                    ui.selectable_value(&mut cfg.repeat, BillboardRepeat::Once, "Sekali");
                    ui.selectable_value(&mut cfg.repeat, BillboardRepeat::Count, "Ulangi N×");
                    ui.selectable_value(&mut cfg.repeat, BillboardRepeat::Forever, "Loop Terus");
                });
                if cfg.repeat == BillboardRepeat::Count {
                    ui.add_space(3.0);
                    ui.horizontal(|ui| {
                        ui.label("Jumlah putaran:");
                        numeric_spinner_u32_w(
                            ui,
                            ("bb_rep_cnt", anim.id),
                            &mut cfg.repeat_count,
                            84.0,
                            1,
                            10_000,
                            1,
                            0.2,
                        );
                    });
                }

                ui.add_space(5.0);
                ui.label("Tampilan Saat Standby / Selesai:");
                egui::ComboBox::from_id_salt(("bb_idle_mode", anim.id))
                    .width(ui.available_width())
                    .selected_text(cfg.idle_mode.label())
                    .show_ui(ui, |ui| {
                        for &m in BillboardIdleMode::ALL {
                            ui.selectable_value(&mut cfg.idle_mode, m, m.label());
                        }
                    });

                ui.add_space(5.0);
                ui.horizontal(|ui| {
                    ui.label("Jeda awal:");
                    numeric_spinner_w(
                        ui,
                        ("bb_delay", anim.id),
                        &mut cfg.delay_sec,
                        92.0,
                        SpinnerConfig {
                            min: 0.0,
                            max: 3600.0,
                            step: 0.5,
                            drag_speed: 0.1,
                            decimals: 1,
                            suffix: " dtk",
                            wrap_around: false,
                        },
                    );
                });

                ui.add_space(5.0);
                ui.checkbox(&mut cfg.play_audio, "Putar Suara (Audio Video)");
                if cfg.play_audio {
                    ui.horizontal(|ui| {
                        ui.label("Volume:");
                        let mut vol_pct = cfg.volume * 100.0;
                        if ui
                            .add(egui::Slider::new(&mut vol_pct, 0.0..=100.0).suffix("%"))
                            .changed()
                        {
                            cfg.volume = (vol_pct / 100.0).clamp(0.0, 1.0);
                        }
                    });
                }
            });
        });
    }

    // Card 3: 3D Procedural Structure & Screen Dimensions
    inspector_card(ui, "Bentuk & Layar 3D Prosedural", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            ui.label("Model Struktur:");
            egui::ComboBox::from_id_salt(("bb_struct", anim.id))
                .width(ui.available_width())
                .selected_text(cfg.structure.label())
                .show_ui(ui, |ui| {
                    for &st in BillboardStructureKind::ALL {
                        ui.selectable_value(&mut cfg.structure, st, st.label());
                    }
                });
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.label("Lebar:");
                numeric_spinner_w(
                    ui,
                    ("bb_w", anim.id),
                    &mut cfg.screen_width_m,
                    76.0,
                    SpinnerConfig {
                        min: 0.5,
                        max: 120.0,
                        step: 0.5,
                        drag_speed: 0.1,
                        decimals: 2,
                        suffix: " m",
                        wrap_around: false,
                    },
                );
                ui.label("Tinggi:");
                numeric_spinner_w(
                    ui,
                    ("bb_h", anim.id),
                    &mut cfg.screen_height_m,
                    76.0,
                    SpinnerConfig {
                        min: 0.5,
                        max: 80.0,
                        step: 0.5,
                        drag_speed: 0.1,
                        decimals: 2,
                        suffix: " m",
                        wrap_around: false,
                    },
                );
            });

            // Quick aspect ratio presets
            ui.add_space(3.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new("Rasio:")
                        .size(10.0)
                        .color(egui::Color32::from_rgb(148, 163, 184)),
                );
                if ui.small_button("16:9").clicked() {
                    cfg.screen_height_m = ((cfg.screen_width_m * 9.0 / 16.0) * 100.0).round() / 100.0;
                }
                if ui.small_button("4:3").clicked() {
                    cfg.screen_height_m = ((cfg.screen_width_m * 3.0 / 4.0) * 100.0).round() / 100.0;
                }
                if ui.small_button("9:16").clicked() {
                    cfg.screen_height_m = ((cfg.screen_width_m * 16.0 / 9.0) * 100.0).round() / 100.0;
                }
                if ui.small_button("Sesuai Aset").clicked() {
                    let target_id = match cfg.media_mode {
                        BillboardMediaMode::Image => cfg.image_asset_id,
                        BillboardMediaMode::Video => cfg.video_asset_id,
                    };
                    if let Some(asset) = target_id
                        .and_then(|id| state.media_assets.iter().find(|m| m.id == id))
                        && let (Some(aw), Some(ah)) = (
                            asset.active_width.or(asset.original_width),
                            asset.active_height.or(asset.original_height),
                        )
                        && aw > 0
                        && ah > 0
                    {
                        let ratio = ah as f64 / aw as f64;
                        cfg.screen_height_m =
                            ((cfg.screen_width_m * ratio).clamp(0.5, 80.0) * 100.0).round() / 100.0;
                    }
                }
            });

            if cfg.structure != BillboardStructureKind::WallMount {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Tinggi Tiang:");
                    numeric_spinner_w(
                        ui,
                        ("bb_pole_h", anim.id),
                        &mut cfg.pole_height_m,
                        92.0,
                        SpinnerConfig {
                            min: 0.0,
                            max: 60.0,
                            step: 0.5,
                            drag_speed: 0.1,
                            decimals: 1,
                            suffix: " m",
                            wrap_around: false,
                        },
                    );
                });
            }

            ui.add_space(4.0);
            ui.checkbox(&mut cfg.double_sided, "Layar 2 Sisi (Depan & Belakang)");

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Kecerahan LED:");
                ui.add(egui::Slider::new(&mut cfg.brightness, 0.2..=3.5).suffix("x"));
            });

            ui.add_space(3.0);
            ui.horizontal(|ui| {
                ui.label("Frame:");
                for (lbl, hex) in [
                    ("Gelap", "#1e293b"),
                    ("Hitam", "#0f172a"),
                    ("Abu", "#475569"),
                    ("Putih", "#cbd5e1"),
                ] {
                    if ui
                        .selectable_label(cfg.frame_color_hex.eq_ignore_ascii_case(hex), lbl)
                        .clicked()
                    {
                        cfg.frame_color_hex = hex.to_string();
                    }
                }
            });
        });
    });

    // Card 4: Full 3D Transform (Move / Rotate / Scale synced with 3D Gizmo)
    inspector_card(ui, "Transform 3D (Move / Rotate / Scale)", true, |ui| {
        if editable {
            ui.horizontal(|ui| {
                let is_mv = matches!(state.tool_mode, ToolMode::Select | ToolMode::MoveMarker);
                let is_rot = state.tool_mode == ToolMode::RotateMarker;
                let is_scl = state.tool_mode == ToolMode::ScaleMarker;
                if ui.selectable_label(is_mv, "Move (2)").clicked() {
                    state.tool_mode = ToolMode::MoveMarker;
                }
                if ui.selectable_label(is_rot, "Rotate (3)").clicked() {
                    state.tool_mode = ToolMode::RotateMarker;
                }
                if ui.selectable_label(is_scl, "Scale (4)").clicked() {
                    state.tool_mode = ToolMode::ScaleMarker;
                }
                if ui.small_button("Focus").clicked() {
                    let l = state.geo_anchor.lng_lat_alt_to_local(
                        cfg.pos_lnglat[0],
                        cfg.pos_lnglat[1],
                        cfg.pos_lnglat[2],
                    );
                    tcam.target_local = bevy::math::Vec2::new(l[0], l[2]);
                }
            });
            ui.add_space(4.0);
        }

        ui.add_enabled_ui(editable, |ui| {
            ui.horizontal(|ui| {
                ui.label("Alt (Y):");
                numeric_spinner_w(
                    ui,
                    ("bb_alt", anim.id),
                    &mut cfg.pos_lnglat[2],
                    88.0,
                    SpinnerConfig {
                        min: -50.0,
                        max: 2000.0,
                        step: 0.5,
                        drag_speed: 0.1,
                        decimals: 2,
                        suffix: " m",
                        wrap_around: false,
                    },
                );
            });
            ui.add_space(3.0);
            ui.horizontal(|ui| {
                ui.label("Rotasi:");
                for (i, lbl) in [(0, "X"), (1, "Y"), (2, "Z")] {
                    ui.label(lbl);
                    numeric_spinner_w(
                        ui,
                        ("bb_rot", anim.id, i),
                        &mut cfg.rot_deg[i],
                        54.0,
                        SpinnerConfig {
                            min: -360.0,
                            max: 360.0,
                            step: 5.0,
                            drag_speed: 0.8,
                            decimals: 1,
                            suffix: "°",
                            wrap_around: true,
                        },
                    );
                }
            });
            ui.add_space(3.0);
            ui.horizontal(|ui| {
                ui.label("Skala:");
                let mut uni = cfg.scale[0];
                let prev_u = uni;
                numeric_spinner_w(
                    ui,
                    ("bb_scl_uni", anim.id),
                    &mut uni,
                    72.0,
                    SpinnerConfig {
                        min: 0.05,
                        max: 25.0,
                        step: 0.1,
                        drag_speed: 0.03,
                        decimals: 2,
                        suffix: "x",
                        wrap_around: false,
                    },
                );
                if (uni - prev_u).abs() > 1e-6 {
                    cfg.scale = [uni, uni, uni];
                }
                if ui.small_button("Reset 1x").clicked() {
                    cfg.scale = [1.0, 1.0, 1.0];
                }
            });
            ui.horizontal(|ui| {
                for (i, lbl) in [(0, "SX"), (1, "SY"), (2, "SZ")] {
                    ui.label(lbl);
                    numeric_spinner_w(
                        ui,
                        ("bb_scl_ax", anim.id, i),
                        &mut cfg.scale[i],
                        54.0,
                        SpinnerConfig {
                            min: 0.05,
                            max: 25.0,
                            step: 0.1,
                            drag_speed: 0.03,
                            decimals: 2,
                            suffix: "",
                            wrap_around: false,
                        },
                    );
                }
            });
        });
    });

    if cfg != original {
        write_billboard_anim_config(anim, &cfg);
    }
}

pub enum BillboardPlayAction {
    Play,
    Pause,
    Resume,
    Stop,
    Focus,
}

pub fn draw_play_card(
    ui: &mut egui::Ui,
    state: &AppState,
    playback: &PlaybackState,
    anim: &Anim,
    global_idx: usize,
    can_control: bool,
) -> Option<BillboardPlayAction> {
    let cfg = parse_billboard_anim_config(anim);
    let track = playback.tracks.get(&anim.id);
    let status = track.map(|t| t.status).unwrap_or(AnimRunStatus::Stopped);
    let is_video = cfg.media_mode == BillboardMediaMode::Video;
    let playable = cfg.is_playable_video();

    let (status_txt, status_bg, status_fg, card_stroke) = if !is_video {
        (
            "GAMBAR STATIS",
            egui::Color32::from_rgb(30, 41, 59),
            egui::Color32::from_rgb(186, 230, 253),
            egui::Color32::from_rgb(44, 58, 82),
        )
    } else {
        match status {
            AnimRunStatus::Stopped => (
                "SIAP",
                egui::Color32::from_rgb(30, 41, 59),
                egui::Color32::from_rgb(148, 163, 184),
                egui::Color32::from_rgb(44, 58, 82),
            ),
            AnimRunStatus::Playing => (
                "MEMUTAR",
                egui::Color32::from_rgb(12, 74, 110),
                egui::Color32::from_rgb(125, 211, 252),
                egui::Color32::from_rgb(14, 165, 233),
            ),
            AnimRunStatus::Paused => (
                "DIJEDA",
                egui::Color32::from_rgb(120, 53, 15),
                egui::Color32::from_rgb(253, 230, 138),
                egui::Color32::from_rgb(245, 158, 11),
            ),
            AnimRunStatus::Finished => (
                "SELESAI",
                egui::Color32::from_rgb(30, 58, 138),
                egui::Color32::from_rgb(191, 219, 254),
                egui::Color32::from_rgb(56, 189, 248),
            ),
        }
    };

    let mut action = None;
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(20, 29, 44))
        .stroke(egui::Stroke::new(1.2, card_stroke))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(9))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());

            // Row 1: #Order + [TAG] + Name + Status + Go
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("#{}", global_idx + 1))
                        .size(10.0)
                        .strong()
                        .color(egui::Color32::from_rgb(125, 211, 252)),
                );
                ui.label(
                    egui::RichText::new(if is_video { "[VIDEOTRON]" } else { "[BILLBOARD]" })
                        .size(9.5)
                        .strong()
                        .color(if is_video {
                            egui::Color32::from_rgb(56, 189, 248)
                        } else {
                            egui::Color32::from_rgb(45, 212, 191)
                        }),
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
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("Go")
                                    .size(10.0)
                                    .color(egui::Color32::from_rgb(125, 211, 252)),
                            )
                            .small()
                            .frame(false),
                        )
                        .on_hover_text("Fokus kamera ke lokasi billboard")
                        .clicked()
                    {
                        action = Some(BillboardPlayAction::Focus);
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
                });
            });

            ui.add_space(3.0);

            // Info line 1: Media asset summary
            ui.add(
                egui::Label::new(
                    egui::RichText::new(asset_summary(state, anim))
                        .size(11.0)
                        .color(egui::Color32::from_rgb(203, 213, 225)),
                )
                .truncate(),
            );

            // Info line 2: Mode / live status
            let detail_line = if is_video {
                if status != AnimRunStatus::Stopped
                    && let Some(live) = state.billboard_status.get(&anim.id)
                {
                    live.clone()
                } else {
                    let rep_lbl = match cfg.repeat {
                        BillboardRepeat::Once => "Sekali",
                        BillboardRepeat::Count => "Ulangi N×",
                        BillboardRepeat::Forever => "Loop Terus",
                    };
                    let audio_lbl = if cfg.play_audio {
                        format!("Suara {:.0}%", cfg.volume * 100.0)
                    } else {
                        "Tanpa suara".to_string()
                    };
                    format!("Mode: {rep_lbl}  •  {audio_lbl}")
                }
            } else {
                "Tampil statis di peta (tidak diputar)".to_string()
            };
            ui.add(
                egui::Label::new(
                    egui::RichText::new(detail_line)
                        .size(10.0)
                        .color(egui::Color32::from_rgb(148, 163, 184)),
                )
                .truncate(),
            );

            if is_video {
                let progress = if status == AnimRunStatus::Finished {
                    1.0
                } else if let Some(tr) = track {
                    if tr.total_length_m > 0.0 {
                        (tr.distance_m / tr.total_length_m).clamp(0.0, 1.0) as f32
                    } else if status == AnimRunStatus::Playing {
                        1.0
                    } else {
                        0.0
                    }
                } else {
                    0.0
                };
                ui.add_space(4.0);
                super::play_panel::draw_card_progress_bar(ui, status, progress);
                ui.add_space(7.0);

                match status {
                    AnimRunStatus::Stopped => {
                        let play_btn = egui::Button::new(
                            egui::RichText::new("Play")
                                .size(12.0)
                                .strong()
                                .color(egui::Color32::WHITE),
                        )
                        .fill(if can_control && playable {
                            egui::Color32::from_rgb(5, 150, 105)
                        } else {
                            egui::Color32::from_rgb(30, 41, 59)
                        })
                        .min_size(egui::vec2(ui.available_width(), 28.0));
                        if ui.add_enabled(can_control && playable, play_btn).clicked() {
                            action = Some(BillboardPlayAction::Play);
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
                            if cols[0].add_enabled(can_control, pause_btn).clicked() {
                                action = Some(BillboardPlayAction::Pause);
                            }

                            let stop_btn = egui::Button::new(
                                egui::RichText::new("Stop")
                                    .size(11.5)
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            )
                            .fill(egui::Color32::from_rgb(185, 28, 28))
                            .min_size(egui::vec2(cols[1].available_width(), 28.0));
                            if cols[1].add_enabled(can_control, stop_btn).clicked() {
                                action = Some(BillboardPlayAction::Stop);
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
                            if cols[0].add_enabled(can_control, resume_btn).clicked() {
                                action = Some(BillboardPlayAction::Resume);
                            }

                            let stop_btn = egui::Button::new(
                                egui::RichText::new("Stop")
                                    .size(11.5)
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            )
                            .fill(egui::Color32::from_rgb(185, 28, 28))
                            .min_size(egui::vec2(cols[1].available_width(), 28.0));
                            if cols[1].add_enabled(can_control, stop_btn).clicked() {
                                action = Some(BillboardPlayAction::Stop);
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
                            if cols[0].add_enabled(can_control && playable, replay_btn).clicked() {
                                action = Some(BillboardPlayAction::Play);
                            }

                            let stop_btn = egui::Button::new(
                                egui::RichText::new("Stop")
                                    .size(11.5)
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            )
                            .fill(egui::Color32::from_rgb(185, 28, 28))
                            .min_size(egui::vec2(cols[1].available_width(), 28.0));
                            if cols[1].add_enabled(can_control, stop_btn).clicked() {
                                action = Some(BillboardPlayAction::Stop);
                            }
                        });
                    }
                }
            }
        });
    ui.add_space(6.0);
    action
}
