use bevy_egui::egui;
use twg_shared::{
    api::CreateAnimRequest,
    camera_anim::{
        ANIM_TYPE_CAMERA_FOLLOW, ANIM_TYPE_CAMERA_STATIC, CameraEasing, CameraFollowConfig,
        CameraFollowPreset, CameraFollowStopMode, CameraStaticConfig, CameraStaticHoldMode,
        is_camera_follow_anim, is_camera_static_anim, parse_camera_follow_config,
        parse_camera_static_config, write_camera_follow_config, write_camera_static_config,
    },
    models::Anim,
};
use uuid::Uuid;

use crate::{
    map::camera::TacticalCamera,
    net::client::{ApiClientBridge, ApiCommand},
    scene::{
        camera_anim::{
            capture_camera_into_static, jump_camera_to_follow_preview, jump_camera_to_static,
            marker_has_route_anim,
        },
        markers::current_auth,
    },
    state::{AnimRunStatus, AppState, PlaybackState, SelectedEntity, ToolMode},
    ui::{
        right_panel::inspector_card,
        spinner::{SpinnerConfig, numeric_spinner_w},
    },
};

pub fn anim_summary(state: &AppState, anim: &Anim) -> String {
    if is_camera_static_anim(anim) {
        let cfg = parse_camera_static_config(anim);
        let orbit = if cfg.orbit_enabled {
            format!(" · Orbit {:.0}°/dtk", cfg.orbit_speed_deg_per_sec)
        } else {
            String::new()
        };
        format!(
            "Fly {:.1} dtk · Jarak {:.0}m · Pitch {:.0}°{orbit}",
            cfg.fly_duration_sec, cfg.distance_m, cfg.pitch_deg
        )
    } else if is_camera_follow_anim(anim) {
        let cfg = parse_camera_follow_config(anim);
        let target = cfg
            .target_marker_id
            .and_then(|id| state.markers.iter().find(|m| m.id == id))
            .map(|m| m.name.as_str())
            .unwrap_or("Belum pilih objek");
        format!(
            "{} · {} · {:.0}m",
            target,
            cfg.preset.short_label(),
            cfg.distance_m
        )
    } else {
        String::new()
    }
}

pub fn draw_create(
    ui: &mut egui::Ui,
    state: &mut AppState,
    tcam: &TacticalCamera,
    api: &ApiClientBridge,
    editable: bool,
) {
    // Card 1: Static Camera (Capture current view)
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(16, 26, 42))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(14, 165, 233)))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                egui::RichText::new("Static Camera (Fly-To / Orbit)")
                    .size(11.5)
                    .strong()
                    .color(egui::Color32::from_rgb(125, 211, 252)),
            );
            ui.label(
                egui::RichText::new(
                    "Arahkan kamera peta ke sudut yang diinginkan lalu klik tombol di bawah — posisi, zoom, pitch & bearing otomatis tertangkap.",
                )
                .size(10.0)
                .color(egui::Color32::from_rgb(148, 163, 184)),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Nama:").size(10.5));
                ui.add_enabled(
                    editable,
                    egui::TextEdit::singleline(&mut state.new_camera_static_name)
                        .desired_width(ui.available_width()),
                );
            });
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new(format!(
                    "Kamera saat ini: Jarak {:.0}m · Pitch {:.0}° · Bearing {:.0}°",
                    tcam.smooth_distance, tcam.smooth_pitch, tcam.smooth_bearing
                ))
                .size(9.5)
                .color(egui::Color32::from_rgb(186, 230, 253)),
            );
            ui.add_space(5.0);
            if ui
                .add_enabled(
                    editable,
                    egui::Button::new(
                        egui::RichText::new("+ Tangkap & Buat Static Camera")
                            .size(11.0)
                            .strong()
                            .color(egui::Color32::WHITE),
                    )
                    .fill(egui::Color32::from_rgb(2, 132, 199))
                    .min_size(egui::vec2(ui.available_width(), 24.0)),
                )
                .clicked()
            {
                create_static_camera_anim(state, tcam, api);
            }
        });

    ui.add_space(6.0);

    // Card 2: Camera Follow (Follow 3D Marker / Route Vehicle)
    let is_picking = state.tool_mode == ToolMode::PickCameraFollowMarker
        && state.picking_camera_follow_for_anim_id.is_none();
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(20, 24, 44))
        .stroke(egui::Stroke::new(
            1.0,
            if is_picking {
                egui::Color32::from_rgb(250, 204, 21)
            } else {
                egui::Color32::from_rgb(16, 185, 129)
            },
        ))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                egui::RichText::new("Camera Follow (Ikuti Objek 3D)")
                    .size(11.5)
                    .strong()
                    .color(egui::Color32::from_rgb(110, 231, 183)),
            );
            ui.label(
                egui::RichText::new(
                    "Ikuti pergerakan 1 Marker / Objek 3D (terutama yang memiliki rute) dari belakang, depan, samping kanan/kiri, atau atas.",
                )
                .size(10.0)
                .color(egui::Color32::from_rgb(148, 163, 184)),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Nama:").size(10.5));
                ui.add_enabled(
                    editable,
                    egui::TextEdit::singleline(&mut state.new_camera_follow_name)
                        .desired_width(ui.available_width()),
                );
            });
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Preset Sudut Awal:")
                    .size(10.0)
                    .color(egui::Color32::from_rgb(203, 213, 225)),
            );
            ui.horizontal_wrapped(|ui| {
                for &p in &[
                    CameraFollowPreset::Rear,
                    CameraFollowPreset::Front,
                    CameraFollowPreset::Right,
                    CameraFollowPreset::Left,
                    CameraFollowPreset::TopDown,
                ] {
                    ui.selectable_value(&mut state.new_camera_follow_preset, p, p.short_label());
                }
            });
            ui.add_space(5.0);
            ui.horizontal(|ui| {
                let pick_btn = egui::Button::new(
                    egui::RichText::new(if is_picking {
                        "Klik Objek di Peta... (Batal)"
                    } else {
                        "+ Pilih Objek di Peta"
                    })
                    .size(10.5)
                    .strong()
                    .color(egui::Color32::WHITE),
                )
                .fill(if is_picking {
                    egui::Color32::from_rgb(180, 83, 9)
                } else {
                    egui::Color32::from_rgb(5, 150, 105)
                });
                if ui.add_enabled(editable, pick_btn).clicked() {
                    if is_picking {
                        state.tool_mode = ToolMode::Select;
                    } else {
                        state.picking_camera_follow_for_anim_id = None;
                        state.tool_mode = ToolMode::PickCameraFollowMarker;
                        state.status_banner = Some(
                            "Mode Camera Follow: Klik 1 Objek / Marker 3D di peta yang ingin diikuti (Esc batal).".into(),
                        );
                    }
                }
                if ui
                    .add_enabled(editable, egui::Button::new("Buat Kosong").small())
                    .on_hover_text("Buat dulu lalu pilih target Marker dari dropdown Inspector")
                    .clicked()
                {
                    create_follow_camera_anim(state, api, None);
                }
            });
        });
}

fn create_static_camera_anim(state: &mut AppState, tcam: &TacticalCamera, api: &ApiClientBridge) {
    let Some(sk) = state.active_skenario.clone() else {
        return;
    };
    let cam_cnt = state
        .anims
        .iter()
        .filter(|a| is_camera_static_anim(a))
        .count()
        + 1;
    let name = if state.new_camera_static_name.trim().is_empty() {
        format!("Static Camera {cam_cnt}")
    } else {
        state.new_camera_static_name.trim().to_string()
    };
    let mut cfg = CameraStaticConfig::default();
    capture_camera_into_static(state, tcam, &mut cfg);
    let sort_order = (state.anims.len() + 1) as i32;
    let now = chrono::Utc::now();
    let mut anim = Anim {
        id: Uuid::new_v4(),
        operasi_id: sk.operasi_id,
        skenario_id: sk.id,
        marker_id: None,
        name: name.clone(),
        anim_type: ANIM_TYPE_CAMERA_STATIC.to_string(),
        sort_order,
        speed: 1.0,
        duration_sec: 0.0,
        replay: false,
        autoplay: false,
        geom: vec![],
        data: serde_json::json!({}),
        created_by: state.current_user_id(),
        allowed_user_ids: Vec::new(),
        created_at: now,
        updated_at: now,
    };
    write_camera_static_config(&mut anim, &cfg);

    if state.offline_preview {
        let aid = anim.id;
        state.selected = SelectedEntity::Anim(aid);
        state.anim_edit = Some(anim.clone());
        state.marker_edit = None;
        state.building_edit = None;
        state.line_edit = None;
        state.text_edit = None;
        state.anims.push(anim);
        state.sort_anims_by_order();
        state.status_banner = Some(format!(
            "Static Camera '{name}' dibuat dari posisi kamera saat ini."
        ));
    } else if let Some(auth) = current_auth(state) {
        api.send(ApiCommand::CreateAnim {
            auth,
            req: CreateAnimRequest {
                operasi_id: sk.operasi_id,
                skenario_id: sk.id,
                marker_id: None,
                name,
                anim_type: ANIM_TYPE_CAMERA_STATIC.to_string(),
                sort_order,
                speed: 1.0,
                duration_sec: anim.duration_sec,
                replay: false,
                autoplay: false,
                geom: anim.geom,
                data: anim.data,
                allowed_user_ids: Vec::new(),
            },
        });
    }
    state.new_camera_static_name = format!("Static Camera {}", cam_cnt + 1);
}

pub fn create_follow_camera_anim(
    state: &mut AppState,
    api: &ApiClientBridge,
    target_marker_id: Option<Uuid>,
) {
    let Some(sk) = state.active_skenario.clone() else {
        return;
    };
    let cam_cnt = state
        .anims
        .iter()
        .filter(|a| is_camera_follow_anim(a))
        .count()
        + 1;
    let name = if state.new_camera_follow_name.trim().is_empty() {
        format!("Camera Follow {cam_cnt}")
    } else {
        state.new_camera_follow_name.trim().to_string()
    };
    let mut cfg = CameraFollowConfig {
        target_marker_id,
        ..Default::default()
    };
    cfg.apply_preset(state.new_camera_follow_preset);
    let sort_order = (state.anims.len() + 1) as i32;
    let now = chrono::Utc::now();
    let mut anim = Anim {
        id: Uuid::new_v4(),
        operasi_id: sk.operasi_id,
        skenario_id: sk.id,
        marker_id: target_marker_id,
        name: name.clone(),
        anim_type: ANIM_TYPE_CAMERA_FOLLOW.to_string(),
        sort_order,
        speed: 1.0,
        duration_sec: 0.0,
        replay: false,
        autoplay: false,
        geom: vec![],
        data: serde_json::json!({}),
        created_by: state.current_user_id(),
        allowed_user_ids: Vec::new(),
        created_at: now,
        updated_at: now,
    };
    write_camera_follow_config(&mut anim, &cfg);

    if state.offline_preview {
        let aid = anim.id;
        state.selected = SelectedEntity::Anim(aid);
        state.anim_edit = Some(anim.clone());
        state.marker_edit = None;
        state.building_edit = None;
        state.line_edit = None;
        state.text_edit = None;
        state.anims.push(anim);
        state.sort_anims_by_order();
        state.status_banner = Some(format!(
            "Camera Follow '{name}' dibuat — atur sudut & kehalusan di Inspector!"
        ));
    } else if let Some(auth) = current_auth(state) {
        api.send(ApiCommand::CreateAnim {
            auth,
            req: CreateAnimRequest {
                operasi_id: sk.operasi_id,
                skenario_id: sk.id,
                marker_id: target_marker_id,
                name,
                anim_type: ANIM_TYPE_CAMERA_FOLLOW.to_string(),
                sort_order,
                speed: 1.0,
                duration_sec: anim.duration_sec,
                replay: false,
                autoplay: false,
                geom: vec![],
                data: anim.data,
                allowed_user_ids: Vec::new(),
            },
        });
    }
    state.new_camera_follow_name = format!("Camera Follow {}", cam_cnt + 1);
}

pub fn draw_inspector(
    ui: &mut egui::Ui,
    state: &mut AppState,
    tcam: &mut TacticalCamera,
    anim: &mut Anim,
    editable: bool,
) {
    if is_camera_static_anim(anim) {
        draw_static_inspector(ui, state, tcam, anim, editable);
    } else if is_camera_follow_anim(anim) {
        draw_follow_inspector(ui, state, tcam, anim, editable);
    }
}

fn draw_static_inspector(
    ui: &mut egui::Ui,
    state: &mut AppState,
    tcam: &mut TacticalCamera,
    anim: &mut Anim,
    editable: bool,
) {
    let mut cfg = parse_camera_static_config(anim);
    let original = cfg.clone();

    if let Some(st) = state.camera_status.get(&anim.id) {
        egui::Frame::new()
            .fill(egui::Color32::from_rgb(14, 34, 52))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(56, 189, 248)))
            .corner_radius(4.0)
            .inner_margin(egui::Margin::symmetric(8, 4))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(
                    egui::RichText::new(format!("Status: {st}"))
                        .size(10.5)
                        .strong()
                        .color(egui::Color32::from_rgb(186, 230, 253)),
                );
            });
        ui.add_space(4.0);
    }

    // Card 1: Identity & Target Camera Pose
    inspector_card(ui, "Static Camera — Posisi & Sudut", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            ui.label("Nama Animasi:");
            ui.add(egui::TextEdit::singleline(&mut anim.name).desired_width(f32::INFINITY));
            ui.add_space(5.0);
        });

        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    editable,
                    egui::Button::new(
                        egui::RichText::new("Tangkap Kamera Saat Ini")
                            .size(10.5)
                            .strong()
                            .color(egui::Color32::WHITE),
                    )
                    .fill(egui::Color32::from_rgb(2, 132, 199)),
                )
                .on_hover_text("Simpan posisi, zoom, pitch & bearing kamera peta saat ini")
                .clicked()
            {
                capture_camera_into_static(state, tcam, &mut cfg);
                state.status_banner = Some("Posisi Static Camera diperbarui dari tampilan saat ini.".into());
            }
            if ui
                .button(egui::RichText::new("Lihat Sudut Ini").size(10.5))
                .on_hover_text("Lompat instan ke sudut kamera yang tersimpan")
                .clicked()
            {
                jump_camera_to_static(state, tcam, &cfg);
            }
        });

        ui.add_space(6.0);
        ui.add_enabled_ui(editable, |ui| {
            ui.horizontal(|ui| {
                ui.label("Zoom (Jarak):");
                numeric_spinner_w(
                    ui,
                    ("cam_st_dist", anim.id),
                    &mut cfg.distance_m,
                    96.0,
                    SpinnerConfig {
                        min: 3.0,
                        max: 80_000.0,
                        step: 10.0,
                        drag_speed: 2.0,
                        decimals: 1,
                        suffix: " m",
                        wrap_around: false,
                    },
                );
            });
            ui.horizontal(|ui| {
                ui.label("Pitch (Kemiringan):");
                numeric_spinner_w(
                    ui,
                    ("cam_st_pitch", anim.id),
                    &mut cfg.pitch_deg,
                    86.0,
                    SpinnerConfig {
                        min: 5.0,
                        max: 82.0,
                        step: 1.0,
                        drag_speed: 0.3,
                        decimals: 1,
                        suffix: "°",
                        wrap_around: false,
                    },
                );
            });
            ui.horizontal(|ui| {
                ui.label("Bearing (Arah):");
                numeric_spinner_w(
                    ui,
                    ("cam_st_bearing", anim.id),
                    &mut cfg.bearing_deg,
                    86.0,
                    SpinnerConfig {
                        min: 0.0,
                        max: 360.0,
                        step: 5.0,
                        drag_speed: 0.6,
                        decimals: 1,
                        suffix: "°",
                        wrap_around: true,
                    },
                );
            });
            ui.horizontal(|ui| {
                ui.label("Tinggi Fokus (Alt):");
                numeric_spinner_w(
                    ui,
                    ("cam_st_alt", anim.id),
                    &mut cfg.target_altitude_m,
                    86.0,
                    SpinnerConfig {
                        min: -100.0,
                        max: 5000.0,
                        step: 1.0,
                        drag_speed: 0.3,
                        decimals: 1,
                        suffix: " m",
                        wrap_around: false,
                    },
                );
            });
            ui.add_space(3.0);
            ui.label(
                egui::RichText::new(format!("Target: {:.6}, {:.6}", cfg.lat, cfg.lng))
                    .size(9.5)
                    .color(egui::Color32::from_rgb(148, 163, 184)),
            );
        });
    });

    // Card 2: Fly-To Transition
    inspector_card(ui, "Transisi Fly-To", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            ui.horizontal(|ui| {
                ui.label("Durasi Fly:");
                numeric_spinner_w(
                    ui,
                    ("cam_st_fly", anim.id),
                    &mut cfg.fly_duration_sec,
                    88.0,
                    SpinnerConfig {
                        min: 0.0,
                        max: 60.0,
                        step: 0.5,
                        drag_speed: 0.1,
                        decimals: 1,
                        suffix: " dtk",
                        wrap_around: false,
                    },
                );
            });
            ui.add_space(3.0);
            ui.label("Kurva Gerak (Easing):");
            egui::ComboBox::from_id_salt(("cam_st_ease", anim.id))
                .width(ui.available_width())
                .selected_text(cfg.easing.label())
                .show_ui(ui, |ui| {
                    for &e in CameraEasing::ALL {
                        ui.selectable_value(&mut cfg.easing, e, e.label());
                    }
                });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Lengkung Terbang:");
                numeric_spinner_w(
                    ui,
                    ("cam_st_arc", anim.id),
                    &mut cfg.fly_arc_m,
                    88.0,
                    SpinnerConfig {
                        min: 0.0,
                        max: 5000.0,
                        step: 10.0,
                        drag_speed: 1.5,
                        decimals: 0,
                        suffix: " m",
                        wrap_around: false,
                    },
                );
            });
            ui.horizontal(|ui| {
                ui.label("Jeda Awal:");
                numeric_spinner_w(
                    ui,
                    ("cam_st_delay", anim.id),
                    &mut cfg.delay_sec,
                    88.0,
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
        });
    });

    // Card 3: Orbit & Hold
    inspector_card(ui, "Orbit & Durasi Tahan", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            ui.checkbox(&mut cfg.orbit_enabled, "Aktifkan Orbit Setelah Sampai");
            if cfg.orbit_enabled {
                ui.add_space(3.0);
                ui.horizontal(|ui| {
                    ui.label("Kecepatan Orbit:");
                    numeric_spinner_w(
                        ui,
                        ("cam_st_ospd", anim.id),
                        &mut cfg.orbit_speed_deg_per_sec,
                        92.0,
                        SpinnerConfig {
                            min: -120.0,
                            max: 120.0,
                            step: 2.0,
                            drag_speed: 0.4,
                            decimals: 1,
                            suffix: " °/dtk",
                            wrap_around: false,
                        },
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Arah:").size(9.5).color(egui::Color32::from_rgb(148, 163, 184)));
                    if ui.small_button("Searah (+12°)").clicked() {
                        cfg.orbit_speed_deg_per_sec = 12.0;
                    }
                    if ui.small_button("Berlawanan (-12°)").clicked() {
                        cfg.orbit_speed_deg_per_sec = -12.0;
                    }
                });
            }

            ui.add_space(5.0);
            ui.label("Mode Setelah Sampai:");
            egui::ComboBox::from_id_salt(("cam_st_holdm", anim.id))
                .width(ui.available_width())
                .selected_text(cfg.hold_mode.label())
                .show_ui(ui, |ui| {
                    for &m in CameraStaticHoldMode::ALL {
                        ui.selectable_value(&mut cfg.hold_mode, m, m.label());
                    }
                });
            if cfg.hold_mode == CameraStaticHoldMode::HoldDuration {
                ui.add_space(3.0);
                ui.horizontal(|ui| {
                    ui.label("Durasi Tahan:");
                    numeric_spinner_w(
                        ui,
                        ("cam_st_holdd", anim.id),
                        &mut cfg.hold_duration_sec,
                        88.0,
                        SpinnerConfig {
                            min: 0.0,
                            max: 3600.0,
                            step: 1.0,
                            drag_speed: 0.2,
                            decimals: 1,
                            suffix: " dtk",
                            wrap_around: false,
                        },
                    );
                });
            }
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(
                    "Tips: Geser/zoom mouse manual kapan saja akan otomatis melepas animasi kamera.",
                )
                .size(9.5)
                .color(egui::Color32::from_rgb(148, 163, 184)),
            );
        });
    });

    if cfg != original {
        write_camera_static_config(anim, &cfg);
    }
}

fn draw_follow_inspector(
    ui: &mut egui::Ui,
    state: &mut AppState,
    tcam: &mut TacticalCamera,
    anim: &mut Anim,
    editable: bool,
) {
    let mut cfg = parse_camera_follow_config(anim);
    let original = cfg.clone();

    if let Some(st) = state.camera_status.get(&anim.id) {
        egui::Frame::new()
            .fill(egui::Color32::from_rgb(10, 42, 34))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(16, 185, 129)))
            .corner_radius(4.0)
            .inner_margin(egui::Margin::symmetric(8, 4))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(
                    egui::RichText::new(format!("Status: {st}"))
                        .size(10.5)
                        .strong()
                        .color(egui::Color32::from_rgb(167, 243, 208)),
                );
            });
        ui.add_space(4.0);
    }

    // Card 1: Identity & Target 3D Marker
    inspector_card(ui, "Camera Follow — Objek Target 3D", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            ui.label("Nama Animasi:");
            ui.add(egui::TextEdit::singleline(&mut anim.name).desired_width(f32::INFINITY));
            ui.add_space(5.0);

            let selected_label = cfg
                .target_marker_id
                .and_then(|id| state.markers.iter().find(|m| m.id == id))
                .map(|m| {
                    if marker_has_route_anim(m.id, &state.anims) {
                        format!("[RUTE] {}", m.name)
                    } else {
                        m.name.clone()
                    }
                })
                .unwrap_or_else(|| "— Pilih Objek / Marker 3D —".to_string());

            ui.label("Objek 3D / Marker yang Diikuti:");
            egui::ComboBox::from_id_salt(("cam_fl_marker", anim.id))
                .width(ui.available_width())
                .selected_text(selected_label)
                .show_ui(ui, |ui| {
                    // Show markers with routes first so they're super easy to pick!
                    let mut ordered: Vec<(Uuid, String, bool)> = state
                        .markers
                        .iter()
                        .filter(|m| state.can_edit_marker(m))
                        .map(|m| (m.id, m.name.clone(), marker_has_route_anim(m.id, &state.anims)))
                        .collect();
                    ordered.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.1.cmp(&b.1)));
                    for (mid, mname, has_route) in ordered {
                        let lbl = if has_route {
                            format!("[RUTE] {mname}")
                        } else {
                            mname
                        };
                        ui.selectable_value(&mut cfg.target_marker_id, Some(mid), lbl);
                    }
                });
        });

        ui.add_space(5.0);
        let picking_this = state.tool_mode == ToolMode::PickCameraFollowMarker
            && state.picking_camera_follow_for_anim_id == Some(anim.id);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    editable,
                    egui::Button::new(
                        egui::RichText::new(if picking_this {
                            "Klik Objek di Peta... (Batal)"
                        } else {
                            "Pilih Objek di Peta"
                        })
                        .size(10.5)
                        .strong()
                        .color(egui::Color32::WHITE),
                    )
                    .fill(if picking_this {
                        egui::Color32::from_rgb(180, 83, 9)
                    } else {
                        egui::Color32::from_rgb(5, 150, 105)
                    }),
                )
                .clicked()
            {
                if picking_this {
                    state.picking_camera_follow_for_anim_id = None;
                    state.tool_mode = ToolMode::Select;
                } else {
                    state.picking_camera_follow_for_anim_id = Some(anim.id);
                    state.tool_mode = ToolMode::PickCameraFollowMarker;
                    state.status_banner = Some(
                        "Klik 1 Objek / Marker 3D di peta untuk dijadikan target Camera Follow (Esc batal).".into(),
                    );
                }
            }
            if ui
                .add_enabled(
                    cfg.target_marker_id.is_some(),
                    egui::Button::new(egui::RichText::new("Lihat Sudut Ikuti").size(10.5)),
                )
                .on_hover_text("Pratinjau langsung sudut kamera terhadap objek target saat ini")
                .clicked()
            {
                jump_camera_to_follow_preview(state, tcam, &cfg);
            }
        });

        ui.add_space(4.0);
        ui.add_enabled_ui(editable, |ui| {
            ui.horizontal(|ui| {
                ui.label("Offset Tinggi Fokus:");
                numeric_spinner_w(
                    ui,
                    ("cam_fl_hoff", anim.id),
                    &mut cfg.height_offset_m,
                    86.0,
                    SpinnerConfig {
                        min: -20.0,
                        max: 200.0,
                        step: 0.5,
                        drag_speed: 0.15,
                        decimals: 1,
                        suffix: " m",
                        wrap_around: false,
                    },
                );
            });
        });
    });

    // Card 2: View Preset, Zoom, Pitch, Bearing
    inspector_card(ui, "Sudut Pandang & Zoom", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            ui.label("Preset Posisi Kamera:");
            ui.horizontal_wrapped(|ui| {
                for &p in CameraFollowPreset::ALL {
                    let active = cfg.preset == p;
                    if ui.selectable_label(active, p.short_label()).clicked() && !active {
                        cfg.apply_preset(p);
                    }
                }
            });
            ui.add_space(4.0);
            let prev_fh = cfg.follow_heading;
            ui.checkbox(&mut cfg.follow_heading, "Ikuti Arah Hadap Objek (Heading)");
            if cfg.follow_heading != prev_fh {
                cfg.preset = CameraFollowPreset::Custom;
            }
            ui.add_space(4.0);

            let prev_dist = cfg.distance_m;
            let prev_pitch = cfg.pitch_deg;
            let prev_off = cfg.bearing_offset_deg;

            ui.horizontal(|ui| {
                ui.label("Zoom (Jarak):");
                numeric_spinner_w(
                    ui,
                    ("cam_fl_dist", anim.id),
                    &mut cfg.distance_m,
                    92.0,
                    SpinnerConfig {
                        min: 3.0,
                        max: 80_000.0,
                        step: 5.0,
                        drag_speed: 1.0,
                        decimals: 1,
                        suffix: " m",
                        wrap_around: false,
                    },
                );
            });
            ui.horizontal(|ui| {
                ui.label("Pitch (Kemiringan):");
                numeric_spinner_w(
                    ui,
                    ("cam_fl_pitch", anim.id),
                    &mut cfg.pitch_deg,
                    86.0,
                    SpinnerConfig {
                        min: 5.0,
                        max: 82.0,
                        step: 1.0,
                        drag_speed: 0.3,
                        decimals: 1,
                        suffix: "°",
                        wrap_around: false,
                    },
                );
            });
            ui.horizontal(|ui| {
                ui.label(if cfg.follow_heading {
                    "Offset Bearing:"
                } else {
                    "Bearing Kompas:"
                });
                numeric_spinner_w(
                    ui,
                    ("cam_fl_boff", anim.id),
                    &mut cfg.bearing_offset_deg,
                    86.0,
                    SpinnerConfig {
                        min: if cfg.follow_heading { -180.0 } else { 0.0 },
                        max: if cfg.follow_heading { 180.0 } else { 360.0 },
                        step: 5.0,
                        drag_speed: 0.5,
                        decimals: 1,
                        suffix: "°",
                        wrap_around: true,
                    },
                );
            });
            if (cfg.distance_m - prev_dist).abs() > 1e-4
                || (cfg.pitch_deg - prev_pitch).abs() > 1e-4
                || (cfg.bearing_offset_deg - prev_off).abs() > 1e-4
            {
                // Keep preset label unless bearing offset moved away from preset direction
                if (cfg.bearing_offset_deg - prev_off).abs() > 1e-4 {
                    cfg.preset = CameraFollowPreset::Custom;
                }
            }
            if cfg.follow_heading {
                ui.label(
                    egui::RichText::new("0° = Belakang · 180° = Depan · -90° = Kanan · +90° = Kiri")
                        .size(9.5)
                        .color(egui::Color32::from_rgb(148, 163, 184)),
                );
            }
        });
    });

    // Card 3: Smoothing, Fly-In & Stop Condition
    inspector_card(ui, "Kehalusan & Kondisi Berhenti", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            ui.horizontal(|ui| {
                ui.label("Durasi Fly-In:");
                numeric_spinner_w(
                    ui,
                    ("cam_fl_flyin", anim.id),
                    &mut cfg.fly_in_sec,
                    86.0,
                    SpinnerConfig {
                        min: 0.0,
                        max: 30.0,
                        step: 0.2,
                        drag_speed: 0.05,
                        decimals: 1,
                        suffix: " dtk",
                        wrap_around: false,
                    },
                );
            });
            ui.horizontal(|ui| {
                ui.label("Halus Posisi:");
                ui.add(egui::Slider::new(&mut cfg.position_smoothness, 1.0..=25.0).fixed_decimals(1));
            });
            ui.horizontal(|ui| {
                ui.label("Halus Rotasi:");
                ui.add(egui::Slider::new(&mut cfg.rotation_smoothness, 0.5..=20.0).fixed_decimals(1));
            });

            ui.add_space(5.0);
            ui.label("Kapan Berhenti Mengikuti:");
            egui::ComboBox::from_id_salt(("cam_fl_stop", anim.id))
                .width(ui.available_width())
                .selected_text(cfg.stop_mode.label())
                .show_ui(ui, |ui| {
                    for &m in CameraFollowStopMode::ALL {
                        ui.selectable_value(&mut cfg.stop_mode, m, m.label());
                    }
                });
            if cfg.stop_mode == CameraFollowStopMode::FixedDuration {
                ui.add_space(3.0);
                ui.horizontal(|ui| {
                    ui.label("Durasi Ikuti:");
                    numeric_spinner_w(
                        ui,
                        ("cam_fl_dur", anim.id),
                        &mut cfg.duration_sec,
                        86.0,
                        SpinnerConfig {
                            min: 0.5,
                            max: 3600.0,
                            step: 1.0,
                            drag_speed: 0.2,
                            decimals: 1,
                            suffix: " dtk",
                            wrap_around: false,
                        },
                    );
                });
            }
            ui.add_space(3.0);
            ui.horizontal(|ui| {
                ui.label("Jeda Awal:");
                numeric_spinner_w(
                    ui,
                    ("cam_fl_delay", anim.id),
                    &mut cfg.delay_sec,
                    86.0,
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
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(
                    "Saat dihentikan atau digeser manual, kamera tetap bertahan di posisi terakhir.",
                )
                .size(9.5)
                .color(egui::Color32::from_rgb(148, 163, 184)),
            );
        });
    });

    if cfg != original {
        write_camera_follow_config(anim, &cfg);
    }
}

pub enum CameraPlayAction {
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
) -> Option<CameraPlayAction> {
    let is_static = is_camera_static_anim(anim);
    let track = playback.tracks.get(&anim.id);
    let status = track.map(|t| t.status).unwrap_or(AnimRunStatus::Stopped);
    let playable = twg_shared::route_formation::anim_is_playable(anim, &state.markers);

    let (status_txt, status_bg, status_fg, card_stroke) = match status {
        AnimRunStatus::Stopped => (
            "SIAP",
            egui::Color32::from_rgb(30, 41, 59),
            egui::Color32::from_rgb(148, 163, 184),
            egui::Color32::from_rgb(44, 58, 82),
        ),
        AnimRunStatus::Playing => (
            if is_static { "KAMERA AKTIF" } else { "MENGIKUTI" },
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
                    egui::RichText::new(if is_static {
                        "[STATIC CAM]"
                    } else {
                        "[FOLLOW CAM]"
                    })
                    .size(9.5)
                    .strong()
                    .color(if is_static {
                        egui::Color32::from_rgb(56, 189, 248)
                    } else {
                        egui::Color32::from_rgb(52, 211, 153)
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
                        .add_enabled(
                            playable,
                            egui::Button::new(
                                egui::RichText::new("Go")
                                    .size(10.0)
                                    .color(egui::Color32::from_rgb(125, 211, 252)),
                            )
                            .small()
                            .frame(false),
                        )
                        .on_hover_text("Fokus kamera ke lokasi animasi")
                        .clicked()
                    {
                        action = Some(CameraPlayAction::Focus);
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

            // Info line 1: Primary target / camera pose summary
            ui.add(
                egui::Label::new(
                    egui::RichText::new(anim_summary(state, anim))
                        .size(11.0)
                        .color(egui::Color32::from_rgb(203, 213, 225)),
                )
                .truncate(),
            );

            // Info line 2: Live status or mode details
            let detail_line = if status != AnimRunStatus::Stopped
                && let Some(live) = state.camera_status.get(&anim.id)
            {
                live.clone()
            } else if is_static {
                let cfg = parse_camera_static_config(anim);
                let hold_lbl = match cfg.hold_mode {
                    CameraStaticHoldMode::HoldDuration => {
                        if cfg.hold_duration_sec > 0.0 {
                            format!("Tahan {:.1} dtk", cfg.hold_duration_sec)
                        } else {
                            "Selesai saat sampai".to_string()
                        }
                    }
                    CameraStaticHoldMode::Continuous => "Terus aktif (sampai Stop)".to_string(),
                };
                format!("Mode: {hold_lbl}")
            } else {
                let cfg = parse_camera_follow_config(anim);
                let stop_lbl = match cfg.stop_mode {
                    CameraFollowStopMode::UntilStopped => "Ikuti sampai Stop".to_string(),
                    CameraFollowStopMode::UntilTargetRouteFinishes => {
                        "Berhenti saat rute target selesai".to_string()
                    }
                    CameraFollowStopMode::FixedDuration => format!("Durasi {:.1} dtk", cfg.duration_sec),
                };
                format!("Mode: {stop_lbl}")
            };
            ui.add(
                egui::Label::new(
                    egui::RichText::new(detail_line)
                        .size(10.0)
                        .color(egui::Color32::from_rgb(148, 163, 184)),
                )
                .truncate(),
            );

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
                        action = Some(CameraPlayAction::Play);
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
                            action = Some(CameraPlayAction::Pause);
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
                            action = Some(CameraPlayAction::Stop);
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
                            action = Some(CameraPlayAction::Resume);
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
                            action = Some(CameraPlayAction::Stop);
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
                            action = Some(CameraPlayAction::Play);
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
                            action = Some(CameraPlayAction::Stop);
                        }
                    });
                }
            }
        });
    ui.add_space(6.0);
    action
}
