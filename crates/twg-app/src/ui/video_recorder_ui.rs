use bevy_egui::egui;

use crate::scene::video_recorder::{RecordCaptureArea, VideoRecorderState, open_path_in_os};

/// Feature toggle to temporarily hide the MP4 recorder UI while keeping the full implementation intact.
pub const VIDEO_RECORDER_UI_ENABLED: bool = false;

#[derive(Clone, Copy)]
enum RecorderBtnIcon {
    RecordDot,
    StopSquare,
}

fn draw_recorder_icon_button(
    ui: &mut egui::Ui,
    icon: RecorderBtnIcon,
    label: &str,
    fill: egui::Color32,
    enabled: bool,
) -> egui::Response {
    let font = egui::FontId::proportional(10.5);
    let text_col = if enabled {
        egui::Color32::WHITE
    } else {
        egui::Color32::from_rgb(148, 163, 184)
    };
    let galley = ui.painter().layout_no_wrap(label.to_string(), font, text_col);
    let pad_x = 8.0_f32;
    let icon_d = 8.0_f32;
    let gap = 5.0_f32;
    let w = (pad_x * 2.0 + icon_d + gap + galley.size().x).ceil();
    let h = (galley.size().y + 8.0).max(22.0);
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, h), sense);
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let bg = if !enabled {
            egui::Color32::from_rgb(45, 55, 72)
        } else if resp.hovered() {
            fill.gamma_multiply(1.12)
        } else {
            fill
        };
        p.rect_filled(rect, 4.0, bg);
        let ic = egui::pos2(rect.left() + pad_x + icon_d * 0.5, rect.center().y);
        match icon {
            RecorderBtnIcon::RecordDot => {
                p.circle_filled(ic, 3.6, text_col);
            }
            RecorderBtnIcon::StopSquare => {
                let sq = egui::Rect::from_center_size(ic, egui::vec2(6.6, 6.6));
                p.rect_filled(sq, 1.0, text_col);
            }
        }
        let tx = rect.left() + pad_x + icon_d + gap;
        let ty = rect.center().y - galley.size().y * 0.5;
        p.galley(egui::pos2(tx, ty), galley, text_col);
    }
    if enabled && resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Compact MP4 presentation recorder control card rendered inside Play Panel (Preview Lokal & Tactical Live).
pub fn draw_play_panel_recorder_card(ui: &mut egui::Ui, recorder: &mut VideoRecorderState) {
    if !VIDEO_RECORDER_UI_ENABLED {
        return;
    }
    let is_rec = recorder.recording;
    let border_col = if is_rec {
        egui::Color32::from_rgb(239, 68, 68)
    } else {
        egui::Color32::from_rgb(45, 62, 88)
    };
    let bg_col = if is_rec {
        egui::Color32::from_rgb(38, 16, 22)
    } else {
        egui::Color32::from_rgb(16, 25, 39)
    };

    egui::Frame::new()
        .fill(bg_col)
        .stroke(egui::Stroke::new(if is_rec { 1.4 } else { 1.0 }, border_col))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                if is_rec {
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(220, 38, 38))
                        .corner_radius(3.0)
                        .inner_margin(egui::Margin::symmetric(5, 1))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
                                let (dot_rect, _) = ui.allocate_exact_size(
                                    egui::vec2(7.0, 10.0),
                                    egui::Sense::hover(),
                                );
                                if ui.is_rect_visible(dot_rect) {
                                    ui.painter().circle_filled(
                                        dot_rect.center(),
                                        3.0,
                                        egui::Color32::WHITE,
                                    );
                                }
                                ui.label(
                                    egui::RichText::new("REC")
                                        .size(9.5)
                                        .strong()
                                        .color(egui::Color32::WHITE),
                                );
                            });
                        });
                    ui.label(
                        egui::RichText::new(format!(
                            "{}  •  {} frm  •  {} FPS",
                            recorder.formatted_elapsed(),
                            recorder.written_frame_count(),
                            recorder.fps
                        ))
                        .size(10.5)
                        .strong()
                        .color(egui::Color32::from_rgb(254, 202, 202)),
                    );
                } else {
                    ui.label(
                        egui::RichText::new("Rekam Presentasi (MP4)")
                            .size(11.0)
                            .strong()
                            .color(egui::Color32::from_rgb(226, 232, 240)),
                    );
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if is_rec {
                        if draw_recorder_icon_button(
                            ui,
                            RecorderBtnIcon::StopSquare,
                            "Stop MP4",
                            egui::Color32::from_rgb(220, 38, 38),
                            true,
                        )
                        .clicked()
                        {
                            recorder.pending_stop = true;
                        }
                    } else {
                        let save_as_btn = egui::Button::new(
                            egui::RichText::new("Simpan...")
                                .size(10.0)
                                .color(egui::Color32::from_rgb(203, 213, 225)),
                        )
                        .fill(egui::Color32::from_rgb(30, 41, 59));
                        if ui
                            .add_enabled(recorder.ffmpeg_available, save_as_btn)
                            .on_hover_text("Pilih lokasi file MP4 lalu mulai rekam")
                            .clicked()
                        {
                            recorder.pending_start_with_dialog = true;
                        }

                        if draw_recorder_icon_button(
                            ui,
                            RecorderBtnIcon::RecordDot,
                            "Rekam",
                            egui::Color32::from_rgb(185, 28, 28),
                            recorder.ffmpeg_available,
                        )
                        .on_hover_text("Rekam langsung ke folder recordings/ dalam format MP4")
                        .clicked()
                        {
                            recorder.pending_quick_start = true;
                        }
                    }
                });
            });

            if !is_rec {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 5.0;
                    egui::ComboBox::from_id_salt("play_rec_area_cb")
                        .width(136.0)
                        .selected_text(recorder.capture_area.label())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut recorder.capture_area,
                                RecordCaptureArea::CleanViewport,
                                RecordCaptureArea::CleanViewport.label(),
                            );
                            ui.selectable_value(
                                &mut recorder.capture_area,
                                RecordCaptureArea::FullWindow,
                                RecordCaptureArea::FullWindow.label(),
                            );
                        });
                    egui::ComboBox::from_id_salt("play_rec_fps_cb")
                        .width(68.0)
                        .selected_text(format!("{} FPS", recorder.fps))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut recorder.fps, 24, "24 FPS");
                            ui.selectable_value(&mut recorder.fps, 30, "30 FPS");
                        });

                    if let Some(last_path) = recorder.last_saved_path.clone() {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .small_button("Folder")
                                .on_hover_text("Buka folder tempat file MP4 disimpan")
                                .clicked()
                                && let Some(parent) = last_path.parent()
                            {
                                open_path_in_os(parent);
                            }
                            if ui
                                .small_button("Putar")
                                .on_hover_text(format!("Buka {}", last_path.display()))
                                .clicked()
                            {
                                open_path_in_os(&last_path);
                            }
                        });
                    }
                });
            }

            if !recorder.ffmpeg_available {
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new(
                        "FFmpeg belum ditemukan (letakkan ffmpeg.exe di folder bin/ atau instal FFmpeg).",
                    )
                    .size(9.5)
                    .color(egui::Color32::from_rgb(252, 165, 165)),
                );
            } else if let Some(err) = &recorder.error_message {
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new(err)
                        .size(9.5)
                        .color(egui::Color32::from_rgb(252, 165, 165)),
                );
            } else if !is_rec
                && let Some(msg) = &recorder.status_message
            {
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new(msg)
                        .size(9.5)
                        .color(egui::Color32::from_rgb(134, 239, 172)),
                );
            }
        });
}

/// Compact inline record button for Layar Besar top bar (subtle when idle; clean indicator when recording).
pub fn draw_compact_recorder_button(ui: &mut egui::Ui, recorder: &mut VideoRecorderState) {
    if !VIDEO_RECORDER_UI_ENABLED {
        return;
    }
    if recorder.recording {
        let stop_label = format!("Stop REC ({})", recorder.formatted_elapsed());
        if draw_recorder_icon_button(
            ui,
            RecorderBtnIcon::StopSquare,
            &stop_label,
            egui::Color32::from_rgb(220, 38, 38),
            true,
        )
        .clicked()
        {
            recorder.pending_stop = true;
        }
    } else if draw_recorder_icon_button(
        ui,
        RecorderBtnIcon::RecordDot,
        "Rekam MP4",
        egui::Color32::from_rgb(153, 27, 27),
        recorder.ffmpeg_available,
    )
    .on_hover_text("Rekam tampilan ini ke video MP4 (H.264)")
    .clicked()
    {
        recorder.pending_quick_start = true;
    }
}
