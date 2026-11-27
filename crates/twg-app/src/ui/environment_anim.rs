use bevy_egui::egui;
use twg_shared::{
    api::CreateAnimRequest,
    environment_anim::{
        ANIM_TYPE_RAIN_SIM, RainAnimConfig, RainCoverage, RainPreset, WaterAnimConfig,
        WaterSimKind, is_rain_anim, is_water_anim, parse_rain_anim_config, parse_water_anim_config,
        write_rain_anim_config, write_water_anim_config,
    },
    models::Anim,
};
use uuid::Uuid;

use crate::{
    net::client::{ApiClientBridge, ApiCommand},
    scene::markers::current_auth,
    state::{AnimRunStatus, AppState, PlaybackState, SelectedEntity, ToolMode},
    ui::{
        right_panel::inspector_card,
        spinner::{SpinnerConfig, numeric_spinner_w},
    },
};

pub fn anim_summary(_state: &AppState, anim: &Anim) -> String {
    if is_water_anim(anim) {
        let cfg = parse_water_anim_config(anim);
        let pts = anim.geom.len();
        match cfg.kind {
            WaterSimKind::Flood => format!(
                "{}  •  Naik {:.1}m -> {:.1}m ({:.0} dtk)  •  {} titik",
                cfg.kind.label(),
                cfg.start_level_m,
                cfg.target_level_m,
                cfg.rise_duration_sec,
                pts
            ),
            WaterSimKind::Ocean | WaterSimKind::River => format!(
                "{}  •  Ombak {:.2}m  •  Arus {:.1} m/s  •  {} titik",
                cfg.kind.label(),
                cfg.wave_height_m,
                cfg.flow_speed_mps,
                pts
            ),
        }
    } else if is_rain_anim(anim) {
        let cfg = parse_rain_anim_config(anim);
        let cov = match cfg.coverage {
            RainCoverage::Global => "Seluruh Area".to_string(),
            RainCoverage::LocalArea => format!("Lokal r={:.0}m", cfg.radius_m),
        };
        let bolt = if cfg.lightning_enabled {
            "  •  Petir"
        } else {
            ""
        };
        format!(
            "{}  •  {}  •  Angin {:.0} m/s{bolt}",
            cfg.preset.label(),
            cov,
            cfg.wind_speed_mps
        )
    } else {
        String::new()
    }
}

pub fn draw_create(
    ui: &mut egui::Ui,
    state: &mut AppState,
    api: &ApiClientBridge,
    editable: bool,
) {
    // Card 1: Water Simulation (Ocean / River / Flood)
    let is_drawing_water = state.tool_mode == ToolMode::DrawWaterPolygon;
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(13, 28, 44))
        .stroke(egui::Stroke::new(
            1.0,
            if is_drawing_water {
                egui::Color32::from_rgb(250, 204, 21)
            } else {
                egui::Color32::from_rgb(14, 165, 233)
            },
        ))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                egui::RichText::new("Air — Laut / Sungai / Banjir")
                    .size(11.5)
                    .strong()
                    .color(egui::Color32::from_rgb(56, 189, 248)),
            );
            ui.label(
                egui::RichText::new(
                    "Pilih jenis perairan lalu gambar area poligon (>= 3 titik) di peta. Klik titik #1 atau tekan Enter untuk selesai.",
                )
                .size(10.0)
                .color(egui::Color32::from_rgb(148, 163, 184)),
            );
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                for &k in WaterSimKind::ALL {
                    let sel = state.new_water_kind == k;
                    if ui.selectable_label(sel, k.short_tag()).clicked() && !sel {
                        state.new_water_kind = k;
                        let cnt = state.anims.iter().filter(|a| is_water_anim(a)).count() + 1;
                        state.new_water_name = k.default_name(cnt);
                    }
                }
            });
            ui.add_space(3.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Nama:").size(10.5));
                ui.add_enabled(
                    editable,
                    egui::TextEdit::singleline(&mut state.new_water_name)
                        .desired_width(ui.available_width()),
                );
            });
            ui.add_space(5.0);
            let btn = egui::Button::new(
                egui::RichText::new(if is_drawing_water {
                    "Menggambar Poligon Air... (Batal)"
                } else {
                    "+ Gambar Area Air di Peta"
                })
                .size(11.0)
                .strong()
                .color(egui::Color32::WHITE),
            )
            .fill(if is_drawing_water {
                egui::Color32::from_rgb(180, 83, 9)
            } else {
                egui::Color32::from_rgb(2, 132, 199)
            })
            .min_size(egui::vec2(ui.available_width(), 24.0));
            if ui.add_enabled(editable, btn).clicked() {
                if is_drawing_water {
                    state.draft_points_lnglat.clear();
                    state.redrawing_water_anim_id = None;
                    state.tool_mode = ToolMode::Select;
                } else {
                    state.draft_points_lnglat.clear();
                    state.redrawing_water_anim_id = None;
                    state.tool_mode = ToolMode::DrawWaterPolygon;
                    state.status_banner = Some(format!(
                        "Mode Gambar Area {} — klik minimal 3 titik di peta lalu klik titik #1 atau tekan Enter (Esc batal).",
                        state.new_water_kind.label()
                    ));
                }
            }
        });

    ui.add_space(6.0);

    // Card 2: Rain / Storm Simulation
    let is_placing_rain = state.tool_mode == ToolMode::PlaceRainZone
        && state.relocating_rain_anim_id.is_none();
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(18, 25, 42))
        .stroke(egui::Stroke::new(
            1.0,
            if is_placing_rain {
                egui::Color32::from_rgb(250, 204, 21)
            } else {
                egui::Color32::from_rgb(99, 102, 241)
            },
        ))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                egui::RichText::new("Simulasi Hujan & Badai")
                    .size(11.5)
                    .strong()
                    .color(egui::Color32::from_rgb(165, 180, 252)),
            );
            ui.label(
                egui::RichText::new(
                    "Simulasi hujan 3D dengan percikan tanah, angin, mendung & petir. Bisa global (ikut kamera) atau area lokal.",
                )
                .size(10.0)
                .color(egui::Color32::from_rgb(148, 163, 184)),
            );
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                for &p in RainPreset::ALL {
                    let sel = state.new_rain_preset == p;
                    if ui.selectable_label(sel, p.label()).clicked() && !sel {
                        state.new_rain_preset = p;
                        let cnt = state.anims.iter().filter(|a| is_rain_anim(a)).count() + 1;
                        state.new_rain_name = format!("{} {cnt}", p.label());
                    }
                }
            });
            ui.add_space(3.0);
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(
                    &mut state.new_rain_coverage,
                    RainCoverage::Global,
                    "Seluruh Area",
                );
                ui.selectable_value(
                    &mut state.new_rain_coverage,
                    RainCoverage::LocalArea,
                    "Area Lokal",
                );
            });
            ui.add_space(3.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Nama:").size(10.5));
                ui.add_enabled(
                    editable,
                    egui::TextEdit::singleline(&mut state.new_rain_name)
                        .desired_width(ui.available_width()),
                );
            });
            ui.add_space(5.0);

            if state.new_rain_coverage == RainCoverage::Global {
                let btn = egui::Button::new(
                    egui::RichText::new("+ Buat Simulasi Hujan")
                        .size(11.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .fill(egui::Color32::from_rgb(79, 70, 229))
                .min_size(egui::vec2(ui.available_width(), 24.0));
                if ui.add_enabled(editable, btn).clicked() {
                    create_rain_anim(state, api, None);
                }
            } else {
                let btn = egui::Button::new(
                    egui::RichText::new(if is_placing_rain {
                        "Klik Lokasi Hujan di Peta... (Batal)"
                    } else {
                        "+ Letakkan Zona Hujan di Peta"
                    })
                    .size(11.0)
                    .strong()
                    .color(egui::Color32::WHITE),
                )
                .fill(if is_placing_rain {
                    egui::Color32::from_rgb(180, 83, 9)
                } else {
                    egui::Color32::from_rgb(79, 70, 229)
                })
                .min_size(egui::vec2(ui.available_width(), 24.0));
                if ui.add_enabled(editable, btn).clicked() {
                    if is_placing_rain {
                        state.tool_mode = ToolMode::Select;
                    } else {
                        state.relocating_rain_anim_id = None;
                        state.tool_mode = ToolMode::PlaceRainZone;
                        state.status_banner = Some(
                            "Mode Zona Hujan Lokal: Klik lokasi pusat hujan di peta (Esc batal)."
                                .into(),
                        );
                    }
                }
            }
        });
}

pub fn create_rain_anim(
    state: &mut AppState,
    api: &ApiClientBridge,
    local_lnglat: Option<[f64; 2]>,
) {
    let Some(sk) = state.active_skenario.clone() else {
        return;
    };
    let cnt = state.anims.iter().filter(|a| is_rain_anim(a)).count() + 1;
    let name = if state.new_rain_name.trim().is_empty() {
        format!("{} {cnt}", state.new_rain_preset.label())
    } else {
        state.new_rain_name.trim().to_string()
    };
    let mut cfg = RainAnimConfig::default_for(state.new_rain_preset);
    if let Some([lng, lat]) = local_lnglat {
        cfg.coverage = RainCoverage::LocalArea;
        cfg.lng = lng;
        cfg.lat = lat;
    } else {
        cfg.coverage = RainCoverage::Global;
    }
    let sort_order = (state.anims.len() + 1) as i32;
    let now = chrono::Utc::now();
    let mut anim = Anim {
        id: Uuid::new_v4(),
        operasi_id: sk.operasi_id,
        skenario_id: sk.id,
        marker_id: None,
        name: name.clone(),
        anim_type: ANIM_TYPE_RAIN_SIM.to_string(),
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
    write_rain_anim_config(&mut anim, &cfg);

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
            "Simulasi Hujan '{name}' dibuat — klik Preview atau atur intensitas & petir di Inspector!"
        ));
    } else if let Some(auth) = current_auth(state) {
        api.send(ApiCommand::CreateAnim {
            auth,
            req: CreateAnimRequest {
                operasi_id: sk.operasi_id,
                skenario_id: sk.id,
                marker_id: None,
                name,
                anim_type: ANIM_TYPE_RAIN_SIM.to_string(),
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
    state.new_rain_name = format!("{} {}", state.new_rain_preset.label(), cnt + 1);
}

pub fn draw_inspector(
    ui: &mut egui::Ui,
    state: &mut AppState,
    anim: &mut Anim,
    editable: bool,
) {
    if is_water_anim(anim) {
        draw_water_inspector(ui, state, anim, editable);
    } else if is_rain_anim(anim) {
        draw_rain_inspector(ui, state, anim, editable);
    }
}

fn draw_water_inspector(
    ui: &mut egui::Ui,
    state: &mut AppState,
    anim: &mut Anim,
    editable: bool,
) {
    let mut cfg = parse_water_anim_config(anim);
    let original = cfg.clone();

    if let Some(st) = state.environment_status.get(&anim.id) {
        egui::Frame::new()
            .fill(egui::Color32::from_rgb(12, 34, 54))
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

    // Card 1: Identity & Water Type
    inspector_card(ui, "Simulasi Air — Jenis & Area", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            ui.label("Nama Animasi:");
            ui.add(egui::TextEdit::singleline(&mut anim.name).desired_width(f32::INFINITY));
            ui.add_space(5.0);

            ui.label("Jenis Perairan:");
            let prev_kind = cfg.kind;
            egui::ComboBox::from_id_salt(("water_kind", anim.id))
                .width(ui.available_width())
                .selected_text(cfg.kind.label())
                .show_ui(ui, |ui| {
                    for &k in WaterSimKind::ALL {
                        ui.selectable_value(&mut cfg.kind, k, k.label());
                    }
                });
            if cfg.kind != prev_kind {
                let keep_preview = cfg.always_preview_in_editor;
                cfg = WaterAnimConfig::default_for(cfg.kind);
                cfg.always_preview_in_editor = keep_preview;
            }
            ui.add_space(4.0);
            ui.checkbox(
                &mut cfg.always_preview_in_editor,
                "Tampilkan Preview Air di Editor",
            );
            ui.add_space(4.0);
            let redrawing = state.tool_mode == ToolMode::DrawWaterPolygon
                && state.redrawing_water_anim_id == Some(anim.id);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("Poligon: {} titik", anim.geom.len()))
                        .size(10.0)
                        .color(egui::Color32::from_rgb(148, 163, 184)),
                );
                if ui
                    .button(if redrawing {
                        "Batal Gambar Ulang"
                    } else {
                        "Gambar Ulang Area"
                    })
                    .clicked()
                {
                    if redrawing {
                        state.draft_points_lnglat.clear();
                        state.redrawing_water_anim_id = None;
                        state.tool_mode = ToolMode::Select;
                    } else {
                        state.draft_points_lnglat.clear();
                        state.redrawing_water_anim_id = Some(anim.id);
                        state.tool_mode = ToolMode::DrawWaterPolygon;
                        state.status_banner = Some(
                            "Klik minimal 3 titik baru di peta untuk mengganti area poligon air (Enter selesai, Esc batal).".into(),
                        );
                    }
                }
            });
        });
    });

    // Card 2: Water Elevation & Flood Rise
    inspector_card(ui, "Ketinggian Muka Air", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            ui.horizontal(|ui| {
                ui.label(if cfg.kind == WaterSimKind::Flood {
                    "Tinggi Awal:"
                } else {
                    "Tinggi Permukaan:"
                });
                numeric_spinner_w(
                    ui,
                    ("wat_lvl0", anim.id),
                    &mut cfg.start_level_m,
                    88.0,
                    SpinnerConfig {
                        min: -2.0,
                        max: 40.0,
                        step: 0.1,
                        drag_speed: 0.05,
                        decimals: 2,
                        suffix: " m",
                        wrap_around: false,
                    },
                );
            });
            if cfg.kind == WaterSimKind::Flood {
                ui.horizontal(|ui| {
                    ui.label("Tinggi Puncak Banjir:");
                    numeric_spinner_w(
                        ui,
                        ("wat_lvl1", anim.id),
                        &mut cfg.target_level_m,
                        88.0,
                        SpinnerConfig {
                            min: 0.0,
                            max: 40.0,
                            step: 0.25,
                            drag_speed: 0.08,
                            decimals: 2,
                            suffix: " m",
                            wrap_around: false,
                        },
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("Durasi Naik:");
                    numeric_spinner_w(
                        ui,
                        ("wat_rise", anim.id),
                        &mut cfg.rise_duration_sec,
                        88.0,
                        SpinnerConfig {
                            min: 0.5,
                            max: 600.0,
                            step: 1.0,
                            drag_speed: 0.2,
                            decimals: 1,
                            suffix: " dtk",
                            wrap_around: false,
                        },
                    );
                });
                ui.checkbox(&mut cfg.recede_after_peak, "Surut Kembali Setelah Puncak");
                if cfg.recede_after_peak {
                    ui.horizontal(|ui| {
                        ui.label("Tahan di Puncak:");
                        numeric_spinner_w(
                            ui,
                            ("wat_phold", anim.id),
                            &mut cfg.peak_hold_sec,
                            88.0,
                            SpinnerConfig {
                                min: 0.0,
                                max: 600.0,
                                step: 1.0,
                                drag_speed: 0.2,
                                decimals: 1,
                                suffix: " dtk",
                                wrap_around: false,
                            },
                        );
                    });
                }
            } else {
                cfg.target_level_m = cfg.start_level_m;
            }
        });
    });

    // Card 3: Waves, Current & Visual Appearance
    inspector_card(ui, "Ombak, Arus & Warna Air", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            ui.horizontal(|ui| {
                ui.label("Tinggi Ombak:");
                numeric_spinner_w(
                    ui,
                    ("wat_wh", anim.id),
                    &mut cfg.wave_height_m,
                    86.0,
                    SpinnerConfig {
                        min: 0.0,
                        max: 4.0,
                        step: 0.05,
                        drag_speed: 0.02,
                        decimals: 2,
                        suffix: " m",
                        wrap_around: false,
                    },
                );
            });
            ui.horizontal(|ui| {
                ui.label("Kerapatan Riak:");
                ui.add(egui::Slider::new(&mut cfg.wave_frequency, 0.2..=4.0).fixed_decimals(2));
            });
            ui.horizontal(|ui| {
                ui.label("Kecepatan Arus:");
                numeric_spinner_w(
                    ui,
                    ("wat_spd", anim.id),
                    &mut cfg.flow_speed_mps,
                    86.0,
                    SpinnerConfig {
                        min: 0.0,
                        max: 18.0,
                        step: 0.5,
                        drag_speed: 0.1,
                        decimals: 1,
                        suffix: " m/s",
                        wrap_around: false,
                    },
                );
            });
            ui.horizontal(|ui| {
                ui.label("Arah Arus / Ombak:");
                numeric_spinner_w(
                    ui,
                    ("wat_dir", anim.id),
                    &mut cfg.flow_dir_deg,
                    86.0,
                    SpinnerConfig {
                        min: 0.0,
                        max: 360.0,
                        step: 15.0,
                        drag_speed: 1.0,
                        decimals: 0,
                        suffix: "°",
                        wrap_around: true,
                    },
                );
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Warna:");
                for (lbl, hex) in [
                    ("Laut", "#0c5d8a"),
                    ("Sungai", "#197278"),
                    ("Toska", "#0d9488"),
                    ("Banjir", "#6e5c3f"),
                    ("Lumpur", "#57442c"),
                ] {
                    if ui
                        .selectable_label(cfg.color_hex.eq_ignore_ascii_case(hex), lbl)
                        .clicked()
                    {
                        cfg.color_hex = hex.to_string();
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label("Opasitas:");
                ui.add(egui::Slider::new(&mut cfg.opacity, 0.25..=0.98).fixed_decimals(2));
            });
            ui.horizontal(|ui| {
                ui.label("Buih / Kilau:");
                ui.add(egui::Slider::new(&mut cfg.foam_intensity, 0.0..=1.5).fixed_decimals(2));
            });
        });
    });

    // Card 4: Timing
    inspector_card(ui, "Durasi & Jeda", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            ui.checkbox(&mut cfg.continuous, "Aktif Terus (Sampai Stop)");
            if !cfg.continuous {
                ui.horizontal(|ui| {
                    ui.label("Durasi:");
                    numeric_spinner_w(
                        ui,
                        ("wat_dur", anim.id),
                        &mut cfg.duration_sec,
                        88.0,
                        SpinnerConfig {
                            min: 1.0,
                            max: 3600.0,
                            step: 5.0,
                            drag_speed: 0.5,
                            decimals: 1,
                            suffix: " dtk",
                            wrap_around: false,
                        },
                    );
                });
            }
            ui.horizontal(|ui| {
                ui.label("Jeda Awal:");
                numeric_spinner_w(
                    ui,
                    ("wat_delay", anim.id),
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

    if cfg != original {
        write_water_anim_config(anim, &cfg);
    }
}

fn draw_rain_inspector(
    ui: &mut egui::Ui,
    state: &mut AppState,
    anim: &mut Anim,
    editable: bool,
) {
    let mut cfg = parse_rain_anim_config(anim);
    let original = cfg.clone();

    if let Some(st) = state.environment_status.get(&anim.id) {
        egui::Frame::new()
            .fill(egui::Color32::from_rgb(20, 26, 52))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(129, 140, 248)))
            .corner_radius(4.0)
            .inner_margin(egui::Margin::symmetric(8, 4))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(
                    egui::RichText::new(format!("Status: {st}"))
                        .size(10.5)
                        .strong()
                        .color(egui::Color32::from_rgb(199, 210, 254)),
                );
            });
        ui.add_space(4.0);
    }

    // Card 1: Identity, Preset & Coverage
    inspector_card(ui, "Simulasi Hujan — Preset & Cakupan", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            ui.label("Nama Animasi:");
            ui.add(egui::TextEdit::singleline(&mut anim.name).desired_width(f32::INFINITY));
            ui.add_space(5.0);

            ui.label("Preset Cuaca:");
            ui.horizontal_wrapped(|ui| {
                for &p in RainPreset::ALL {
                    let sel = cfg.preset == p;
                    if ui.selectable_label(sel, p.label()).clicked() && !sel {
                        cfg.apply_preset(p);
                    }
                }
            });
            ui.add_space(5.0);
            ui.label("Cakupan Hujan:");
            egui::ComboBox::from_id_salt(("rain_cov", anim.id))
                .width(ui.available_width())
                .selected_text(cfg.coverage.label())
                .show_ui(ui, |ui| {
                    for &c in RainCoverage::ALL {
                        ui.selectable_value(&mut cfg.coverage, c, c.label());
                    }
                });

            if cfg.coverage == RainCoverage::LocalArea {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Radius Zona:");
                    numeric_spinner_w(
                        ui,
                        ("rain_rad", anim.id),
                        &mut cfg.radius_m,
                        92.0,
                        SpinnerConfig {
                            min: 15.0,
                            max: 4000.0,
                            step: 20.0,
                            drag_speed: 2.0,
                            decimals: 0,
                            suffix: " m",
                            wrap_around: false,
                        },
                    );
                });
                let relocating = state.tool_mode == ToolMode::PlaceRainZone
                    && state.relocating_rain_anim_id == Some(anim.id);
                if ui
                    .button(if relocating {
                        "Klik Lokasi Baru di Peta... (Batal)"
                    } else {
                        "Pindah Lokasi Zona di Peta"
                    })
                    .clicked()
                {
                    if relocating {
                        state.relocating_rain_anim_id = None;
                        state.tool_mode = ToolMode::Select;
                    } else {
                        state.relocating_rain_anim_id = Some(anim.id);
                        state.tool_mode = ToolMode::PlaceRainZone;
                        state.status_banner = Some(
                            "Klik lokasi pusat baru di peta untuk zona hujan lokal (Esc batal)."
                                .into(),
                        );
                    }
                }
            }

            ui.add_space(4.0);
            ui.checkbox(
                &mut cfg.always_preview_in_editor,
                "Selalu Tampilkan Hujan di Editor",
            );
        });
    });

    // Card 2: Rain Physics & Wind
    inspector_card(ui, "Intensitas, Angin & Percikan", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            ui.horizontal(|ui| {
                ui.label("Intensitas:");
                ui.add(egui::Slider::new(&mut cfg.intensity, 0.15..=2.0).fixed_decimals(2));
            });
            ui.horizontal(|ui| {
                ui.label("Kecepatan Jatuh:");
                numeric_spinner_w(
                    ui,
                    ("rain_fspd", anim.id),
                    &mut cfg.fall_speed_mps,
                    86.0,
                    SpinnerConfig {
                        min: 10.0,
                        max: 60.0,
                        step: 2.0,
                        drag_speed: 0.4,
                        decimals: 0,
                        suffix: " m/s",
                        wrap_around: false,
                    },
                );
            });
            ui.horizontal(|ui| {
                ui.label("Panjang Tetesan:");
                numeric_spinner_w(
                    ui,
                    ("rain_len", anim.id),
                    &mut cfg.drop_length_m,
                    86.0,
                    SpinnerConfig {
                        min: 0.4,
                        max: 7.0,
                        step: 0.2,
                        drag_speed: 0.05,
                        decimals: 1,
                        suffix: " m",
                        wrap_around: false,
                    },
                );
            });
            ui.horizontal(|ui| {
                ui.label("Kecepatan Angin:");
                numeric_spinner_w(
                    ui,
                    ("rain_wspd", anim.id),
                    &mut cfg.wind_speed_mps,
                    86.0,
                    SpinnerConfig {
                        min: 0.0,
                        max: 28.0,
                        step: 1.0,
                        drag_speed: 0.2,
                        decimals: 1,
                        suffix: " m/s",
                        wrap_around: false,
                    },
                );
            });
            ui.horizontal(|ui| {
                ui.label("Arah Angin:");
                numeric_spinner_w(
                    ui,
                    ("rain_wdir", anim.id),
                    &mut cfg.wind_dir_deg,
                    86.0,
                    SpinnerConfig {
                        min: 0.0,
                        max: 360.0,
                        step: 15.0,
                        drag_speed: 1.0,
                        decimals: 0,
                        suffix: "°",
                        wrap_around: true,
                    },
                );
            });
            ui.checkbox(&mut cfg.splash_enabled, "Percikan Air di Permukaan Tanah");
        });
    });

    // Card 3: Storm Atmosphere & Lightning
    inspector_card(ui, "Mendung, Petir & Durasi", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            ui.horizontal(|ui| {
                ui.label("Efek Mendung:");
                ui.add(egui::Slider::new(&mut cfg.overcast_strength, 0.0..=1.0).fixed_decimals(2));
            });
            ui.checkbox(&mut cfg.lightning_enabled, "Aktifkan Petir & Kilat");
            if cfg.lightning_enabled {
                ui.horizontal(|ui| {
                    ui.label("Interval Petir:");
                    numeric_spinner_w(
                        ui,
                        ("rain_ltg", anim.id),
                        &mut cfg.lightning_interval_sec,
                        86.0,
                        SpinnerConfig {
                            min: 1.5,
                            max: 30.0,
                            step: 0.5,
                            drag_speed: 0.1,
                            decimals: 1,
                            suffix: " dtk",
                            wrap_around: false,
                        },
                    );
                });
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Transisi Mulai (Ramp):");
                numeric_spinner_w(
                    ui,
                    ("rain_ramp", anim.id),
                    &mut cfg.ramp_up_sec,
                    86.0,
                    SpinnerConfig {
                        min: 0.0,
                        max: 20.0,
                        step: 0.5,
                        drag_speed: 0.1,
                        decimals: 1,
                        suffix: " dtk",
                        wrap_around: false,
                    },
                );
            });
            ui.checkbox(&mut cfg.continuous, "Hujan Terus (Sampai Stop)");
            if !cfg.continuous {
                ui.horizontal(|ui| {
                    ui.label("Durasi:");
                    numeric_spinner_w(
                        ui,
                        ("rain_dur", anim.id),
                        &mut cfg.duration_sec,
                        86.0,
                        SpinnerConfig {
                            min: 1.0,
                            max: 3600.0,
                            step: 5.0,
                            drag_speed: 0.5,
                            decimals: 1,
                            suffix: " dtk",
                            wrap_around: false,
                        },
                    );
                });
            }
            ui.horizontal(|ui| {
                ui.label("Jeda Awal:");
                numeric_spinner_w(
                    ui,
                    ("rain_delay", anim.id),
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
        });
    });

    if cfg != original {
        write_rain_anim_config(anim, &cfg);
    }
}

pub enum EnvPlayAction {
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
) -> Option<EnvPlayAction> {
    let is_water = is_water_anim(anim);
    let track = playback.tracks.get(&anim.id);
    let status = track.map(|t| t.status).unwrap_or(AnimRunStatus::Stopped);
    let playable = twg_shared::route_formation::anim_is_playable(anim, &state.markers);

    let (tag_txt, tag_col) = if is_water {
        let cfg = parse_water_anim_config(anim);
        match cfg.kind {
            WaterSimKind::Ocean => ("[LAUT]", egui::Color32::from_rgb(56, 189, 248)),
            WaterSimKind::River => ("[SUNGAI]", egui::Color32::from_rgb(45, 212, 191)),
            WaterSimKind::Flood => ("[BANJIR]", egui::Color32::from_rgb(251, 191, 36)),
        }
    } else {
        let cfg = parse_rain_anim_config(anim);
        match cfg.preset {
            RainPreset::Storm => ("[BADAI]", egui::Color32::from_rgb(192, 132, 252)),
            _ => ("[HUJAN]", egui::Color32::from_rgb(129, 140, 248)),
        }
    };

    let (status_txt, status_bg, status_fg, card_stroke) = match status {
        AnimRunStatus::Stopped => (
            "SIAP",
            egui::Color32::from_rgb(30, 41, 59),
            egui::Color32::from_rgb(148, 163, 184),
            egui::Color32::from_rgb(44, 58, 82),
        ),
        AnimRunStatus::Playing => (
            "AKTIF",
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

    let can_focus = if is_water {
        !anim.geom.is_empty()
    } else {
        parse_rain_anim_config(anim).coverage == RainCoverage::LocalArea
            && !anim.geom.is_empty()
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
                    egui::RichText::new(tag_txt)
                        .size(9.5)
                        .strong()
                        .color(tag_col),
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
                            can_focus,
                            egui::Button::new(
                                egui::RichText::new("Go")
                                    .size(10.0)
                                    .color(egui::Color32::from_rgb(125, 211, 252)),
                            )
                            .small()
                            .frame(false),
                        )
                        .on_hover_text("Fokus kamera ke area animasi")
                        .clicked()
                    {
                        action = Some(EnvPlayAction::Focus);
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

            ui.add(
                egui::Label::new(
                    egui::RichText::new(anim_summary(state, anim))
                        .size(11.0)
                        .color(egui::Color32::from_rgb(203, 213, 225)),
                )
                .truncate(),
            );

            let detail_line = if status != AnimRunStatus::Stopped
                && let Some(live) = state.environment_status.get(&anim.id)
            {
                live.clone()
            } else if is_water {
                let cfg = parse_water_anim_config(anim);
                if cfg.continuous {
                    "Mode: Aktif terus (sampai Stop)".to_string()
                } else {
                    format!("Mode: Durasi {:.0} dtk", cfg.duration_sec)
                }
            } else {
                let cfg = parse_rain_anim_config(anim);
                if cfg.continuous {
                    "Mode: Hujan terus (sampai Stop)".to_string()
                } else {
                    format!("Mode: Durasi {:.0} dtk", cfg.duration_sec)
                }
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
                        action = Some(EnvPlayAction::Play);
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
                            action = Some(EnvPlayAction::Pause);
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
                            action = Some(EnvPlayAction::Stop);
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
                            action = Some(EnvPlayAction::Resume);
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
                            action = Some(EnvPlayAction::Stop);
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
                            action = Some(EnvPlayAction::Play);
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
                            action = Some(EnvPlayAction::Stop);
                        }
                    });
                }
            }
        });
    ui.add_space(6.0);
    action
}
