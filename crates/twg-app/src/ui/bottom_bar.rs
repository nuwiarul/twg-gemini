use bevy_egui::egui;
use twg_shared::geo::polyline_length_m;

use crate::{
    map::camera::TacticalCamera,
    net::ws_client::WsClientBridge,
    scene::playback::scenario_total_duration,
    state::{AppState, PlaybackState, ToolMode},
    ui::{
        spinner::{SpinnerConfig, numeric_spinner_f32_w},
        topbar::broadcast_playback_cmd,
    },
};

pub fn draw_bottom_bar(
    viewport_ui: &mut egui::Ui,
    state: &mut AppState,
    tcam: &TacticalCamera,
    playback: &mut PlaybackState,
    ws: &WsClientBridge,
) -> f32 {
    egui::Panel::bottom("editor_bottom_timeline")
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(13, 19, 31))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(32, 44, 66)))
                .inner_margin(egui::Margin::symmetric(10, 6)),
        )
        .show(viewport_ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                // Animation track selector (All Anims vs Individual Track)
                let selected_label = if playback.play_all_anims {
                    format!("Semua Animasi ({})", state.anims.len())
                } else {
                    playback
                        .active_anim_id
                        .and_then(|id| state.anims.iter().find(|a| a.id == id))
                        .map(|a| a.name.clone())
                        .unwrap_or_else(|| "Semua Animasi".to_string())
                };

                egui::ComboBox::from_id_salt("timeline_anim_selector")
                    .selected_text(selected_label)
                    .width(185.0)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(
                                playback.play_all_anims,
                                format!("Semua Animasi ({})", state.anims.len()),
                            )
                            .clicked()
                        {
                            playback.play_all_anims = true;
                            playback.elapsed_sec = 0.0;
                        }
                        ui.separator();
                        for anim in &state.anims {
                            let is_sel = !playback.play_all_anims
                                && playback.active_anim_id == Some(anim.id);
                            if ui
                                .selectable_label(
                                    is_sel,
                                    format!("{} ({:.0}s)", anim.name, anim.duration_sec),
                                )
                                .clicked()
                            {
                                playback.play_all_anims = false;
                                playback.active_anim_id = Some(anim.id);
                                playback.elapsed_sec = 0.0;
                                playback.loop_anim = anim.replay;
                            }
                        }
                    });

                // Quick Play / Pause & Stop
                let play_btn_text = if playback.play_test_mode && playback.playing {
                    "Pause"
                } else {
                    "Play"
                };
                if ui.button(play_btn_text).clicked() {
                    if !playback.play_test_mode {
                        state.draft_points_lnglat.clear();
                        state.tool_mode = ToolMode::Select;
                        playback.start_play_test();
                        broadcast_playback_cmd(state, playback, ws, "play_anim");
                    } else {
                        playback.toggle_pause();
                        broadcast_playback_cmd(
                            state,
                            playback,
                            ws,
                            if playback.playing {
                                "play_anim"
                            } else {
                                "pause_anim"
                            },
                        );
                    }
                }

                if ui.button("Stop").clicked() {
                    playback.stop_play_test();
                    broadcast_playback_cmd(state, playback, ws, "stop_anim");
                }

                let max_dur = scenario_total_duration(state, playback);
                let slider_resp = ui.add(
                    egui::Slider::new(&mut playback.elapsed_sec, 0.0..=max_dur)
                        .suffix("s")
                        .fixed_decimals(1),
                );
                if slider_resp.changed() && playback.elapsed_sec > 0.001 && !playback.play_test_mode {
                    playback.play_test_mode = true;
                    playback.playing = false;
                }

                numeric_spinner_f32_w(
                    ui,
                    "playback_speed_spinner",
                    &mut playback.speed_multiplier,
                    82.0,
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
                ui.checkbox(&mut playback.loop_anim, "Loop");
                ui.checkbox(&mut playback.hide_gizmos_in_play, "Clean Play View")
                    .on_hover_text("Sembunyikan garis rute & gizmo seleksi saat Play Mode aktif");

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let zoom = tcam.current_zoom(820.0, state.geo_anchor.cos_lat);
                    ui.label(
                        egui::RichText::new(format!(
                            "Zoom {:.1} | Pitch {:.0}° | Bearing {:.0}°",
                            zoom, tcam.pitch_deg, tcam.bearing_deg
                        ))
                        .size(11.0)
                        .color(egui::Color32::from_rgb(148, 163, 184)),
                    );

                    if let Some([lng, lat]) = state.cursor_lnglat {
                        ui.separator();
                        ui.label(
                            egui::RichText::new(format!("Lat {:.5}, Lng {:.5}", lat, lng))
                                .size(11.0)
                                .color(egui::Color32::from_rgb(56, 189, 248)),
                        );
                    }

                    if state.tool_mode == ToolMode::Measure
                        && state.measure_points_lnglat.len() >= 2
                    {
                        let dist_m = polyline_length_m(&state.measure_points_lnglat);
                        ui.separator();
                        let text = if dist_m >= 1000.0 {
                            format!("Distance: {:.2} km", dist_m / 1000.0)
                        } else {
                            format!("Distance: {:.1} m", dist_m)
                        };
                        ui.colored_label(egui::Color32::from_rgb(244, 114, 182), text);
                    }

                    if let Some(banner) = &state.status_banner {
                        ui.separator();
                        ui.colored_label(egui::Color32::from_rgb(250, 204, 21), banner);
                    }
                });
            });
        })
        .response
        .rect
        .height()
}
