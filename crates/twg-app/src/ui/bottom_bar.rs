use bevy_egui::egui;
use twg_shared::geo::polyline_length_m;

use crate::{
    map::camera::TacticalCamera,
    net::ws_client::WsClientBridge,
    state::{AnimRunStatus, AppState, PlaybackState, ToolMode},
};

pub fn draw_bottom_bar(
    viewport_ui: &mut egui::Ui,
    state: &mut AppState,
    tcam: &TacticalCamera,
    playback: &mut PlaybackState,
    _ws: &WsClientBridge,
) -> f32 {
    egui::Panel::bottom("editor_bottom_timeline")
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(13, 19, 31))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(32, 44, 66)))
                .inner_margin(egui::Margin::symmetric(10, 5)),
        )
        .show(viewport_ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                if playback.play_test_mode {
                    let running = playback
                        .tracks
                        .values()
                        .filter(|t| t.status == AnimRunStatus::Playing)
                        .count();
                    let paused = playback
                        .tracks
                        .values()
                        .filter(|t| t.status == AnimRunStatus::Paused)
                        .count();
                    ui.label(
                        egui::RichText::new(format!(
                            "Mode Play Skenario  •  {} animasi ({} berjalan, {} dijeda)",
                            state.anims.len(),
                            running,
                            paused
                        ))
                        .size(11.0)
                        .strong()
                        .color(egui::Color32::from_rgb(110, 231, 183)),
                    );
                } else {
                    ui.label(
                        egui::RichText::new(format!(
                            "Tool: {}  •  Unit: {}  •  Animasi: {}",
                            state.tool_mode.label(),
                            state.markers.len(),
                            state.anims.len()
                        ))
                        .size(11.0)
                        .color(egui::Color32::from_rgb(148, 163, 184)),
                    );
                }

                if let Some(banner) = &state.status_banner {
                    ui.separator();
                    ui.colored_label(egui::Color32::from_rgb(250, 204, 21), banner);
                }

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
                });
            });
        })
        .response
        .rect
        .height()
}
