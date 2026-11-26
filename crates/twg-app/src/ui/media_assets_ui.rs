use std::path::PathBuf;

use bevy_egui::egui;
use twg_shared::models::{MediaAssetDto, MediaAssetType};

use crate::{
    net::{
        asset_sync::{AssetSyncBridge, AssetSyncCommand},
        client::{ApiClientBridge, ApiCommand, MediaAssetUploadPayload},
    },
    scene::{
        markers::current_auth,
        media_preview::{AudioControlCmd, MediaPreviewPlayer, open_in_system_player},
    },
    state::{AppState, MediaFormMode},
    ui::{draw_search_input_with_reset, spinner::numeric_spinner_u32_w},
};

fn format_bytes(bytes: i64) -> String {
    let b = bytes.max(0) as f64;
    if b >= 1024.0 * 1024.0 {
        format!("{:.2} MB", b / (1024.0 * 1024.0))
    } else if b >= 1024.0 {
        format!("{:.0} KB", b / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

fn format_duration(sec: f32) -> String {
    let total = sec.max(0.0).floor() as u32;
    let m = total / 60;
    let s = total % 60;
    format!("{m:02}:{s:02}")
}

fn pick_media_file(media_type: MediaAssetType) -> Option<PathBuf> {
    let dialog = match media_type {
        MediaAssetType::Texture => rfd::FileDialog::new()
            .set_title("Pilih Image Texture (PNG / JPG / WebP)")
            .add_filter("Image Texture", &["png", "jpg", "jpeg", "webp"]),
        MediaAssetType::Sound => rfd::FileDialog::new()
            .set_title("Pilih File Sound / Audio")
            .add_filter("Audio File", &["mp3", "wav", "ogg", "flac", "aac", "m4a"]),
        MediaAssetType::Video => rfd::FileDialog::new()
            .set_title("Pilih File Video")
            .add_filter("Video File", &["mp4", "webm", "mov", "mkv", "avi"]),
    };
    dialog.pick_file()
}

pub fn draw_media_left_panel_body(
    ui: &mut egui::Ui,
    state: &mut AppState,
    media_type: MediaAssetType,
    player: &mut MediaPreviewPlayer,
) {
    let add_label = match media_type {
        MediaAssetType::Texture => "+ Texture",
        MediaAssetType::Sound => "+ Sound",
        MediaAssetType::Video => "+ Video",
    };

    ui.horizontal(|ui| {
        ui.heading(media_type.label());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let add_btn = egui::Button::new(
                egui::RichText::new(add_label)
                    .strong()
                    .color(egui::Color32::WHITE),
            )
            .fill(egui::Color32::from_rgb(14, 116, 144));
            if ui.add(add_btn).clicked() {
                state.media_form_mode = MediaFormMode::Create;
                state.media_form.reset_for_create();
            }
        });
    });

    ui.add_space(6.0);
    draw_search_input_with_reset(
        ui,
        &mut state.media_search,
        &format!("Cari {}...", media_type.label()),
        false,
        &format!("Clear / reset pencarian {}", media_type.label()),
    );
    ui.separator();

    let needle = state.media_search.trim().to_lowercase();
    let filtered: Vec<MediaAssetDto> = state
        .media_assets
        .iter()
        .filter(|m| {
            if m.media_type != media_type {
                return false;
            }
            if !needle.is_empty()
                && !m.name.to_lowercase().contains(&needle)
                && !m.slug.to_lowercase().contains(&needle)
                && !m.category.to_lowercase().contains(&needle)
            {
                return false;
            }
            true
        })
        .cloned()
        .collect();

    ui.label(
        egui::RichText::new(format!("Menampilkan {} item", filtered.len()))
            .size(11.0)
            .color(egui::Color32::from_rgb(148, 163, 184)),
    );
    ui.add_space(4.0);

    let mut clicked_id = None;
    egui::ScrollArea::vertical()
        .id_salt("media_assets_list_scroll")
        .show(ui, |ui| {
            if filtered.is_empty() {
                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(format!(
                        "Belum ada {}. Klik '{add_label}' untuk mengunggah.",
                        media_type.label()
                    ))
                    .color(egui::Color32::from_rgb(148, 163, 184)),
                );
                return;
            }

            for item in &filtered {
                let selected = state.selected_media_id == Some(item.id);
                let ready = state.available_model_paths.contains(&item.active_file.rel_path)
                    || state.assets_root.join(&item.active_file.rel_path).exists();
                let bg = if selected {
                    egui::Color32::from_rgb(14, 116, 144)
                } else {
                    egui::Color32::from_rgb(24, 33, 50)
                };

                let resp = egui::Frame::new()
                    .fill(bg)
                    .corner_radius(5.0)
                    .inner_margin(egui::Margin::symmetric(8, 6))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(&item.name)
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let col = if ready {
                                        egui::Color32::from_rgb(74, 222, 128)
                                    } else {
                                        egui::Color32::from_rgb(250, 204, 21)
                                    };
                                    ui.colored_label(
                                        col,
                                        egui::RichText::new(if ready { "OK" } else { "DL" })
                                            .size(10.0)
                                            .strong(),
                                    );
                                },
                            );
                        });

                        let detail = match item.media_type {
                            MediaAssetType::Texture => {
                                let aw = item.active_width.unwrap_or(0);
                                let ah = item.active_height.unwrap_or(0);
                                let ow = item.original_width.unwrap_or(aw);
                                if item.is_resized_texture() {
                                    format!(
                                        "{aw}x{ah} (orig {ow}x{ow}) • {} • v{}",
                                        format_bytes(item.active_file.size_bytes),
                                        item.version
                                    )
                                } else {
                                    format!(
                                        "{aw}x{ah} (Original) • {} • v{}",
                                        format_bytes(item.active_file.size_bytes),
                                        item.version
                                    )
                                }
                            }
                            MediaAssetType::Sound => {
                                let dur = item
                                    .duration_sec
                                    .map(|d| format_duration(d as f32))
                                    .unwrap_or_else(|| "--:--".to_string());
                                format!(
                                    "{dur} • {} • {}",
                                    format_bytes(item.active_file.size_bytes),
                                    item.category
                                )
                            }
                            MediaAssetType::Video => {
                                let dims = match (item.active_width, item.active_height) {
                                    (Some(w), Some(h)) => format!("{w}x{h} • "),
                                    _ => String::new(),
                                };
                                let dur = item
                                    .duration_sec
                                    .map(|d| format_duration(d as f32))
                                    .unwrap_or_else(|| "--:--".to_string());
                                format!(
                                    "{dims}{dur} • {}",
                                    format_bytes(item.active_file.size_bytes)
                                )
                            }
                        };

                        ui.label(
                            egui::RichText::new(detail)
                                .size(10.5)
                                .color(egui::Color32::from_rgb(148, 163, 184)),
                        );
                    })
                    .response
                    .interact(egui::Sense::click());

                if resp.clicked() {
                    clicked_id = Some(item.id);
                }
                ui.add_space(4.0);
            }
        });

    if let Some(id) = clicked_id {
        if state.selected_media_id != Some(id) {
            player.stop_all_playback();
        }
        state.select_media(id);
    }
}

pub fn draw_media_center_preview(
    ctx: &egui::Context,
    center_rect: egui::Rect,
    state: &mut AppState,
    player: &mut MediaPreviewPlayer,
    sync_bridge: &AssetSyncBridge,
    media_type: MediaAssetType,
) {
    let Some(asset) = state.selected_media().cloned().filter(|m| m.media_type == media_type) else {
        egui::Area::new(egui::Id::new("media_center_empty"))
            .fixed_pos(center_rect.min)
            .show(ctx, |ui| {
                ui.set_min_size(center_rect.size());
                ui.set_max_size(center_rect.size());
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(10, 15, 24))
                    .show(ui, |ui| {
                        ui.set_min_size(center_rect.size());
                        ui.centered_and_justified(|ui| {
                            ui.label(
                                egui::RichText::new(format!(
                                    "Pilih {} di panel kiri atau klik tombol '+' untuk mengunggah.",
                                    media_type.label()
                                ))
                                .size(15.0)
                                .color(egui::Color32::from_rgb(148, 163, 184)),
                            );
                        });
                    });
            });
        return;
    };

    // Ensure local file is synced (trigger priority download if missing)
    let active_rel = if media_type == MediaAssetType::Texture && state.texture_preview_original {
        asset.original_file.rel_path.clone()
    } else {
        asset.active_file.rel_path.clone()
    };
    let local_ready = state.available_model_paths.contains(&active_rel)
        || state.assets_root.join(&active_rel).exists();

    if !local_ready {
        sync_bridge.send(AssetSyncCommand::PriorityDownload {
            server_url: state.server_url.clone(),
            local_root: state.assets_root.clone(),
            files: asset.all_files().into_iter().cloned().collect(),
        });
    }

    egui::Area::new(egui::Id::new("media_center_stage"))
        .fixed_pos(center_rect.min)
        .show(ctx, |ui| {
            ui.set_min_size(center_rect.size());
            ui.set_max_size(center_rect.size());
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(10, 15, 24))
                .inner_margin(egui::Margin::same(16))
                .show(ui, |ui| {
                    ui.set_min_size((center_rect.size() - egui::vec2(32.0, 32.0)).max(egui::vec2(64.0, 64.0)));

                    if !local_ready {
                        ui.centered_and_justified(|ui| {
                            ui.colored_label(
                                egui::Color32::from_rgb(250, 204, 21),
                                format!("Mengunduh {} dari server untuk preview offline...", asset.name),
                            );
                        });
                        return;
                    }

                    match media_type {
                        MediaAssetType::Texture => {
                            draw_texture_canvas(ui, ctx, state, player, &asset, &active_rel);
                        }
                        MediaAssetType::Sound => {
                            draw_sound_player_canvas(ui, state, player, &asset);
                        }
                        MediaAssetType::Video => {
                            draw_video_player_canvas(ui, ctx, state, player, &asset);
                        }
                    }
                });
        });
}

fn draw_texture_canvas(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    state: &mut AppState,
    player: &mut MediaPreviewPlayer,
    asset: &MediaAssetDto,
    shown_rel: &str,
) {
    let orig_w = asset.original_width.unwrap_or(0);
    let orig_h = asset.original_height.unwrap_or(0);
    let act_w = asset.active_width.unwrap_or(orig_w);
    let act_h = asset.active_height.unwrap_or(orig_h);

    // Top studio controls
    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new(&asset.name)
                .size(16.0)
                .strong()
                .color(egui::Color32::WHITE),
        );
        ui.separator();

        if ui
            .selectable_label(
                !state.texture_preview_original,
                format!("Aktif ({act_w}x{act_h})"),
            )
            .clicked()
        {
            state.texture_preview_original = false;
        }
        if ui
            .selectable_label(
                state.texture_preview_original,
                format!("Original ({orig_w}x{orig_h})"),
            )
            .clicked()
        {
            state.texture_preview_original = true;
        }

        ui.separator();
        if ui
            .selectable_label(!state.texture_preview_tiled, "1x1")
            .clicked()
        {
            state.texture_preview_tiled = false;
        }
        if ui
            .selectable_label(state.texture_preview_tiled, "2x2 Tile")
            .clicked()
        {
            state.texture_preview_tiled = true;
        }

        ui.separator();
        ui.label("Zoom:");
        ui.add(
            egui::Slider::new(&mut state.texture_preview_zoom, 0.25..=3.0)
                .fixed_decimals(2)
                .suffix("x"),
        );
    });
    ui.separator();

    let tex_id_opt = player
        .ensure_image_texture(ctx, &state.assets_root, shown_rel, state.media_reload_nonce)
        .map(|h| (h.id(), h.size_vec2()));

    if let Some(err) = &player.tex_error {
        ui.colored_label(egui::Color32::from_rgb(248, 113, 113), err);
        return;
    }

    let Some((tex_id, raw_sz)) = tex_id_opt else {
        ui.label("Memuat texture...");
        return;
    };

    let avail = ui.available_size();
    let max_fit = (avail.x.min(avail.y) - 36.0).max(128.0);
    let base_side = max_fit.min(480.0);
    let display_side = (base_side * state.texture_preview_zoom).clamp(64.0, max_fit);

    egui::ScrollArea::both()
        .id_salt("texture_preview_scroll")
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(((avail.y - display_side - 30.0) * 0.4).max(8.0));
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(display_side, display_side),
                    egui::Sense::hover(),
                );

                // Checkerboard backdrop for alpha transparency inspection
                draw_checkerboard(ui.painter(), rect, 16.0);

                if state.texture_preview_tiled {
                    let half = display_side * 0.5;
                    for gy in 0..2 {
                        for gx in 0..2 {
                            let tile_rect = egui::Rect::from_min_size(
                                rect.min + egui::vec2(gx as f32 * half, gy as f32 * half),
                                egui::vec2(half, half),
                            );
                            ui.painter().image(
                                tex_id,
                                tile_rect,
                                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                                egui::Color32::WHITE,
                            );
                        }
                    }
                } else {
                    ui.painter().image(
                        tex_id,
                        rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                }

                ui.painter().rect_stroke(
                    rect,
                    0.0,
                    egui::Stroke::new(1.5, egui::Color32::from_rgb(56, 189, 248)),
                    egui::StrokeKind::Outside,
                );

                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(format!(
                        "{} • Dimensi terload: {:.0}x{:.0} px • {}",
                        shown_rel,
                        raw_sz.x,
                        raw_sz.y,
                        if state.texture_preview_original {
                            "ORIGINAL (tersimpan permanen)"
                        } else {
                            "ACTIVE"
                        }
                    ))
                    .size(11.0)
                    .color(egui::Color32::from_rgb(148, 163, 184)),
                );
            });
        });
}

fn draw_checkerboard(painter: &egui::Painter, rect: egui::Rect, cell: f32) {
    let c1 = egui::Color32::from_rgb(38, 46, 62);
    let c2 = egui::Color32::from_rgb(56, 66, 86);
    let cols = (rect.width() / cell).ceil() as i32;
    let rows = (rect.height() / cell).ceil() as i32;
    for r in 0..rows {
        for c in 0..cols {
            let min = rect.min + egui::vec2(c as f32 * cell, r as f32 * cell);
            let max = (min + egui::vec2(cell, cell)).min(rect.max);
            let col = if (r + c) % 2 == 0 { c1 } else { c2 };
            painter.rect_filled(egui::Rect::from_min_max(min, max), 0.0, col);
        }
    }
}

fn draw_sound_player_canvas(
    ui: &mut egui::Ui,
    state: &AppState,
    player: &mut MediaPreviewPlayer,
    asset: &MediaAssetDto,
) {
    let is_this = player.audio_asset_id == Some(asset.id);
    let is_playing = is_this && player.audio_playing && !player.audio_paused;
    let is_paused = is_this && player.audio_playing && player.audio_paused;
    let duration = asset.duration_sec.unwrap_or(0.0) as f32;
    let pos = if is_this {
        player.audio_position_sec
    } else {
        0.0
    };

    ui.vertical_centered(|ui| {
        ui.add_space((ui.available_height() * 0.16).max(24.0));
        egui::Frame::new()
            .fill(egui::Color32::from_rgb(17, 24, 39))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(51, 65, 85)))
            .corner_radius(12.0)
            .inner_margin(egui::Margin::same(24))
            .show(ui, |ui| {
                ui.set_max_width(520.0);
                ui.set_min_width(420.0);

                ui.label(
                    egui::RichText::new("SOUND PREVIEW PLAYER")
                        .size(11.0)
                        .strong()
                        .color(egui::Color32::from_rgb(56, 189, 248)),
                );
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(&asset.name)
                        .size(20.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                );
                ui.label(
                    egui::RichText::new(format!(
                        "{} • {} • {}",
                        asset.mime_type,
                        format_bytes(asset.active_file.size_bytes),
                        asset.active_file.rel_path
                    ))
                    .size(11.5)
                    .color(egui::Color32::from_rgb(148, 163, 184)),
                );
                ui.add_space(18.0);

                // Animated level bars visual indicator
                let (bar_rect, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), 56.0),
                    egui::Sense::hover(),
                );
                ui.painter().rect_filled(
                    bar_rect,
                    6.0,
                    egui::Color32::from_rgb(11, 17, 29),
                );
                let num_bars = 32;
                let gap = 4.0;
                let bw = ((bar_rect.width() - gap * (num_bars as f32 + 1.0)) / num_bars as f32)
                    .max(3.0);
                for i in 0..num_bars {
                    let phase = pos * 8.0 + i as f32 * 0.45;
                    let amp = if is_playing {
                        (0.22 + 0.72 * (phase.sin() * 0.5 + 0.5) * ((phase * 0.37).cos().abs()))
                            .clamp(0.12, 0.95)
                    } else {
                        0.12 + 0.08 * ((i as f32 * 0.5).sin().abs())
                    };
                    let bh = (bar_rect.height() - 12.0) * amp;
                    let x = bar_rect.min.x + gap + i as f32 * (bw + gap);
                    let y = bar_rect.center().y - bh * 0.5;
                    let col = if is_playing {
                        egui::Color32::from_rgb(56, 189, 248)
                    } else {
                        egui::Color32::from_rgb(71, 85, 105)
                    };
                    ui.painter().rect_filled(
                        egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(bw, bh)),
                        2.0,
                        col,
                    );
                }

                ui.add_space(14.0);

                // Progress / Seek scrubber
                let max_t = duration.max(pos).max(1.0);
                let mut seek_val = pos.clamp(0.0, max_t);
                let slider_resp = ui.add(
                    egui::Slider::new(&mut seek_val, 0.0..=max_t)
                        .show_value(false)
                        .trailing_fill(true),
                );
                if slider_resp.drag_stopped() && is_this && player.audio_playing {
                    player.audio_position_sec = seek_val;
                    player
                        .pending_audio_cmds
                        .push(AudioControlCmd::Seek(seek_val));
                }

                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format_duration(pos))
                            .size(12.0)
                            .color(egui::Color32::from_rgb(203, 213, 225)),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let total_str = if duration > 0.0 {
                            format_duration(duration)
                        } else {
                            "--:--".to_string()
                        };
                        ui.label(
                            egui::RichText::new(total_str)
                                .size(12.0)
                                .color(egui::Color32::from_rgb(148, 163, 184)),
                        );
                    });
                });

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if is_playing {
                        if ui
                            .add_sized([110.0, 34.0], egui::Button::new("Pause"))
                            .clicked()
                        {
                            player.pause_sound();
                        }
                    } else if is_paused {
                        if ui
                            .add_sized(
                                [110.0, 34.0],
                                egui::Button::new("Resume")
                                    .fill(egui::Color32::from_rgb(14, 116, 144)),
                            )
                            .clicked()
                        {
                            player.resume_sound();
                        }
                    } else if ui
                        .add_sized(
                            [110.0, 34.0],
                            egui::Button::new(egui::RichText::new("Play Sound").strong())
                                .fill(egui::Color32::from_rgb(14, 116, 144)),
                        )
                        .clicked()
                    {
                        player.play_sound(asset.id, asset.active_file.rel_path.clone(), 0.0);
                    }

                    if ui
                        .add_sized([80.0, 34.0], egui::Button::new("Stop"))
                        .clicked()
                    {
                        player.stop_sound();
                    }

                    ui.checkbox(&mut player.audio_loop, "Loop");
                });

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.label("Volume:");
                    if ui
                        .add(
                            egui::Slider::new(&mut player.audio_volume, 0.0..=1.5)
                                .fixed_decimals(2),
                        )
                        .changed()
                        && is_this
                    {
                        player
                            .pending_audio_cmds
                            .push(AudioControlCmd::SetVolume(player.audio_volume));
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Speed:");
                    if ui
                        .add(
                            egui::Slider::new(&mut player.audio_speed, 0.5..=2.0)
                                .fixed_decimals(2)
                                .suffix("x"),
                        )
                        .changed()
                        && is_this
                    {
                        player
                            .pending_audio_cmds
                            .push(AudioControlCmd::SetSpeed(player.audio_speed));
                    }
                });

                if is_playing {
                    ui.ctx().request_repaint();
                }

                ui.add_space(6.0);
                if ui.small_button("Buka di Pemutar Sistem").clicked() {
                    open_in_system_player(&state.assets_root.join(&asset.active_file.rel_path));
                }
            });
    });
}

fn draw_video_player_canvas(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    state: &AppState,
    player: &mut MediaPreviewPlayer,
    asset: &MediaAssetDto,
) {
    let is_this = player.video_asset_id == Some(asset.id);
    if is_this {
        player.poll_video_frame(
            ctx,
            &state.assets_root,
            Some(&asset.active_file.rel_path),
        );
    }

    let is_playing = is_this && player.video_playing && !player.video_paused;
    let is_paused = is_this && player.video_playing && player.video_paused;
    let duration = if is_this && player.video_duration_sec > 0.0 {
        player.video_duration_sec
    } else {
        asset.duration_sec.unwrap_or(0.0) as f32
    };
    let pos = if is_this {
        player.video_position_sec
    } else {
        0.0
    };

    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new(&asset.name)
                .size(16.0)
                .strong()
                .color(egui::Color32::WHITE),
        );
        ui.separator();
        if let (Some(w), Some(h)) = (asset.active_width, asset.active_height) {
            ui.label(
                egui::RichText::new(format!("{w}x{h}"))
                    .color(egui::Color32::from_rgb(56, 189, 248)),
            );
        }
        ui.label(
            egui::RichText::new(format!(
                "{} • {}",
                format_bytes(asset.active_file.size_bytes),
                asset.active_file.rel_path
            ))
            .size(11.5)
            .color(egui::Color32::from_rgb(148, 163, 184)),
        );
    });
    ui.separator();

    // Reserve space for video frame + control bar below
    let avail = ui.available_size();
    let frame_max_h = (avail.y - 125.0).max(160.0);
    let frame_max_w = (avail.x - 12.0).max(240.0);

    let active_tex: Option<(egui::TextureId, egui::Vec2)> = if is_this
        && player.video_frame_tex.is_some()
        && (player.video_playing || player.video_position_sec > 0.0)
    {
        player
            .video_frame_tex
            .as_ref()
            .map(|h| (h.id(), h.size_vec2()))
    } else if let Some(thumb) = &asset.thumbnail_file {
        let thumb_exists = state.available_model_paths.contains(&thumb.rel_path)
            || state.assets_root.join(&thumb.rel_path).exists();
        if thumb_exists {
            player
                .ensure_image_texture(
                    ctx,
                    &state.assets_root,
                    &thumb.rel_path,
                    state.media_reload_nonce,
                )
                .map(|h| (h.id(), h.size_vec2()))
        } else {
            None
        }
    } else {
        None
    };

    ui.vertical_centered(|ui| {
        let (disp_w, disp_h) = if let Some((_, sz)) = active_tex {
            let aspect = (sz.x / sz.y.max(1.0)).max(0.2);
            let mut w = frame_max_w;
            let mut h = w / aspect;
            if h > frame_max_h {
                h = frame_max_h;
                w = h * aspect;
            }
            (w.max(160.0), h.max(90.0))
        } else {
            (frame_max_w.min(640.0), frame_max_h.min(360.0))
        };

        let (rect, _) = ui.allocate_exact_size(egui::vec2(disp_w, disp_h), egui::Sense::click());
        ui.painter()
            .rect_filled(rect, 6.0, egui::Color32::from_rgb(5, 8, 14));

        if let Some((tid, _)) = active_tex {
            ui.painter().image(
                tid,
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        } else {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Klik 'Play Video' untuk memutar",
                egui::FontId::proportional(15.0),
                egui::Color32::from_rgb(148, 163, 184),
            );
        }
        ui.painter().rect_stroke(
            rect,
            6.0,
            egui::Stroke::new(1.0, egui::Color32::from_rgb(51, 65, 85)),
            egui::StrokeKind::Outside,
        );
    });

    if let Some(err) = &player.video_error {
        ui.add_space(4.0);
        ui.colored_label(egui::Color32::from_rgb(250, 204, 21), err);
    }

    ui.add_space(8.0);
    // Video Timeline Seek Scrubber
    let max_t = duration.max(pos).max(1.0);
    let mut seek_val = pos.clamp(0.0, max_t);
    let scrub_resp = ui.add(
        egui::Slider::new(&mut seek_val, 0.0..=max_t)
            .show_value(false)
            .trailing_fill(true),
    );
    if scrub_resp.drag_stopped() {
        player.start_video(
            asset.id,
            &state.assets_root,
            &asset.active_file.rel_path,
            seek_val,
            duration,
        );
    }

    ui.horizontal_wrapped(|ui| {
        if is_playing {
            if ui
                .add_sized([105.0, 30.0], egui::Button::new("Pause"))
                .clicked()
            {
                player.pause_video();
            }
        } else if is_paused {
            if ui
                .add_sized(
                    [105.0, 30.0],
                    egui::Button::new("Resume").fill(egui::Color32::from_rgb(14, 116, 144)),
                )
                .clicked()
            {
                player.resume_video();
            }
        } else if ui
            .add_sized(
                [105.0, 30.0],
                egui::Button::new(egui::RichText::new("Play Video").strong())
                    .fill(egui::Color32::from_rgb(14, 116, 144)),
            )
            .clicked()
        {
            player.start_video(
                asset.id,
                &state.assets_root,
                &asset.active_file.rel_path,
                0.0,
                duration,
            );
        }

        if ui
            .add_sized([72.0, 30.0], egui::Button::new("Stop"))
            .clicked()
        {
            player.stop_video();
        }

        ui.label(
            egui::RichText::new(format!(
                "{} / {}",
                format_duration(pos),
                if duration > 0.0 {
                    format_duration(duration)
                } else {
                    "--:--".to_string()
                }
            ))
            .size(12.0)
            .color(egui::Color32::from_rgb(203, 213, 225)),
        );

        ui.checkbox(&mut player.video_loop, "Loop");

        ui.label("Vol:");
        if ui
            .add(
                egui::Slider::new(&mut player.video_volume, 0.0..=1.5)
                    .fixed_decimals(2)
                    .show_value(false),
            )
            .changed()
            && is_this
        {
            player
                .pending_audio_cmds
                .push(AudioControlCmd::SetVolume(player.video_volume));
        }

        if ui.button("Pemutar Sistem").clicked() {
            open_in_system_player(&state.assets_root.join(&asset.active_file.rel_path));
        }
    });
}

pub fn draw_media_right_panel_body(
    ui: &mut egui::Ui,
    state: &mut AppState,
    api: &ApiClientBridge,
    media_type: MediaAssetType,
) {
    match state.media_form_mode {
        MediaFormMode::Create => draw_media_create_form(ui, state, api, media_type),
        MediaFormMode::Inspect => draw_media_inspect_and_edit(ui, state, api, media_type),
    }
}

fn draw_media_create_form(
    ui: &mut egui::Ui,
    state: &mut AppState,
    api: &ApiClientBridge,
    media_type: MediaAssetType,
) {
    ui.horizontal(|ui| {
        ui.heading(format!("Upload {} Baru", media_type.label()));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Batal").clicked() {
                state.media_form_mode = MediaFormMode::Inspect;
                if let Some(m) = state.selected_media().cloned() {
                    state.media_form.load_from_media(&m);
                }
            }
        });
    });
    ui.separator();

    ui.label("Nama Asset:");
    ui.add(
        egui::TextEdit::singleline(&mut state.media_form.name)
            .hint_text(match media_type {
                MediaAssetType::Texture => "Contoh: Aspal Jalan Raya",
                MediaAssetType::Sound => "Contoh: Sirine Polisi",
                MediaAssetType::Video => "Contoh: Briefing Sektor A",
            })
            .desired_width(f32::INFINITY),
    );
    ui.add_space(6.0);

    ui.label("Kategori / Grup:");
    ui.add(
        egui::TextEdit::singleline(&mut state.media_form.category)
            .hint_text("general")
            .desired_width(f32::INFINITY),
    );
    ui.add_space(6.0);

    if media_type == MediaAssetType::Texture {
        egui::Frame::new()
            .fill(egui::Color32::from_rgb(30, 41, 59))
            .corner_radius(5.0)
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(
                        "Aturan Image Texture Simetri:\n\
                         • Texture wajib simetri (persegi, mis. 256x256, 512x512).\n\
                         • Jika gambar tidak persegi (mis. 128x256), otomatis diubah ke sisi terendah (128x128).\n\
                         • Setelah upload, ukuran original selalu disimpan permanen dan dapat di-resize ke bawah saja.",
                    )
                    .size(11.0)
                    .color(egui::Color32::from_rgb(203, 213, 225)),
                );
            });
        ui.add_space(6.0);

        ui.label("Jika gambar tidak persegi (W != H):");
        ui.radio_value(
            &mut state.media_form.crop_mode,
            "crop".to_string(),
            "Center-Crop ke min(W, H) (Direkomendasikan — tanpa distorsi)",
        );
        ui.radio_value(
            &mut state.media_form.crop_mode,
            "resize".to_string(),
            "Resize / Fit ke min(W, H) (Seluruh gambar muat)",
        );
        ui.add_space(6.0);
    }

    ui.label(
        egui::RichText::new(format!("Pilih File {}:", media_type.label()))
            .strong()
            .color(egui::Color32::from_rgb(56, 189, 248)),
    );
    ui.horizontal(|ui| {
        if ui.button("Pilih File...").clicked()
            && let Some(p) = pick_media_file(media_type)
        {
            if state.media_form.name.trim().is_empty()
                && let Some(stem) = p.file_stem().and_then(|s| s.to_str())
            {
                state.media_form.name = stem.replace('_', " ");
            }
            state.media_form.file_path = Some(p);
        }
        if state.media_form.file_path.is_some() && ui.small_button("Clear").clicked() {
            state.media_form.file_path = None;
        }
    });
    if let Some(p) = &state.media_form.file_path {
        let fname = p
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("selected");
        ui.label(
            egui::RichText::new(format!("Dipilih: {fname}"))
                .size(11.0)
                .color(egui::Color32::from_rgb(74, 222, 128)),
        );
    }

    ui.add_space(8.0);
    if let Some(err) = &state.media_form.error {
        ui.colored_label(egui::Color32::from_rgb(248, 113, 113), err);
        ui.add_space(4.0);
    }
    if let Some(ok) = &state.media_form.success {
        ui.colored_label(egui::Color32::from_rgb(74, 222, 128), ok);
        ui.add_space(4.0);
    }

    let btn_text = if state.media_form.submitting {
        "Mengunggah ke Server..."
    } else {
        "Upload & Simpan ke Server"
    };
    let submit_btn = egui::Button::new(egui::RichText::new(btn_text).strong())
        .fill(egui::Color32::from_rgb(14, 116, 144));
    if ui
        .add_enabled(!state.media_form.submitting, submit_btn)
        .clicked()
    {
        if state.media_form.name.trim().is_empty() {
            state.media_form.error = Some("Nama asset wajib diisi.".into());
            return;
        }
        if state.media_form.file_path.is_none() {
            state.media_form.error = Some("Pilih file terlebih dahulu.".into());
            return;
        }
        if let Some(auth) = current_auth(state) {
            state.media_form.submitting = true;
            state.media_form.error = None;
            state.media_form.success = None;
            api.send(ApiCommand::CreateMediaAsset {
                auth,
                payload: MediaAssetUploadPayload {
                    name: state.media_form.name.trim().to_string(),
                    media_type,
                    category: state.media_form.category.trim().to_string(),
                    crop_mode: state.media_form.crop_mode.clone(),
                    file_path: state.media_form.file_path.clone(),
                },
            });
        }
    }
}

fn draw_media_inspect_and_edit(
    ui: &mut egui::Ui,
    state: &mut AppState,
    api: &ApiClientBridge,
    media_type: MediaAssetType,
) {
    let Some(asset) = state
        .selected_media()
        .cloned()
        .filter(|m| m.media_type == media_type)
    else {
        ui.heading(format!("Detail {}", media_type.label()));
        ui.separator();
        ui.label("Pilih item di daftar kiri atau unggah asset baru.");
        return;
    };

    if state.media_form.editing_id != Some(asset.id) {
        state.media_form.load_from_media(&asset);
    }

    // --- Section 1: Texture Symmetric Downscale-Only Resize ---
    if media_type == MediaAssetType::Texture {
        let orig_size = asset
            .original_width
            .unwrap_or(0)
            .min(asset.original_height.unwrap_or(0)) as u32;
        let act_size = asset
            .active_width
            .unwrap_or(orig_size as i32)
            .min(asset.active_height.unwrap_or(orig_size as i32)) as u32;

        ui.heading("Resize Simetri (Ke Bawah Saja)");
        egui::Frame::new()
            .fill(egui::Color32::from_rgb(24, 33, 50))
            .corner_radius(6.0)
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "Original (permanen): {orig_size}x{orig_size} px ({})",
                        format_bytes(asset.original_file.size_bytes)
                    ))
                    .size(11.5)
                    .color(egui::Color32::from_rgb(74, 222, 128)),
                );
                ui.label(
                    egui::RichText::new(format!(
                        "Aktif sekarang: {act_size}x{act_size} px ({})",
                        format_bytes(asset.active_file.size_bytes)
                    ))
                    .size(11.5)
                    .color(egui::Color32::from_rgb(56, 189, 248)),
                );
                ui.add_space(6.0);

                ui.label(
                    egui::RichText::new(format!(
                        "Preset Simetri (maks <= {orig_size}x{orig_size}, tidak bisa ke atas):"
                    ))
                    .size(11.0)
                    .color(egui::Color32::from_rgb(148, 163, 184)),
                );
                ui.horizontal_wrapped(|ui| {
                    for &preset in &[2048u32, 1024, 512, 256, 128, 64, 32] {
                        if preset > orig_size {
                            continue;
                        }
                        let is_current = act_size == preset;
                        let lbl = if preset == orig_size {
                            format!("{preset}x{preset} (Orig)")
                        } else {
                            format!("{preset}x{preset}")
                        };
                        if ui
                            .add_enabled(
                                !state.media_form.submitting && !is_current,
                                egui::Button::new(lbl).small(),
                            )
                            .clicked()
                            && let Some(auth) = current_auth(state)
                        {
                            state.media_form.submitting = true;
                            state.media_form.error = None;
                            state.media_form.success = None;
                            api.send(ApiCommand::ResizeMediaTexture {
                                auth,
                                id: asset.id,
                                target_size: preset,
                            });
                        }
                    }
                });

                ui.add_space(6.0);
                let max_allowed = orig_size.max(16);
                if state.media_form.custom_resize_size > max_allowed {
                    state.media_form.custom_resize_size = max_allowed;
                }
                ui.horizontal(|ui| {
                    ui.label("Custom (px):");
                    numeric_spinner_u32_w(
                        ui,
                        ("media_custom_resize", asset.id),
                        &mut state.media_form.custom_resize_size,
                        92.0,
                        16,
                        max_allowed,
                        16,
                        4.0,
                    );
                    ui.label(format!("x {}", state.media_form.custom_resize_size));
                    if ui
                        .add_enabled(
                            !state.media_form.submitting,
                            egui::Button::new("Resize").small(),
                        )
                        .clicked()
                        && let Some(auth) = current_auth(state)
                    {
                        state.media_form.submitting = true;
                        state.media_form.error = None;
                        state.media_form.success = None;
                        api.send(ApiCommand::ResizeMediaTexture {
                            auth,
                            id: asset.id,
                            target_size: state.media_form.custom_resize_size,
                        });
                    }
                });

                if asset.is_resized_texture() {
                    ui.add_space(6.0);
                    let restore_btn = egui::Button::new(
                        egui::RichText::new(format!(
                            "Kembalikan ke Original ({orig_size}x{orig_size})"
                        ))
                        .strong(),
                    )
                    .fill(egui::Color32::from_rgb(21, 128, 61));
                    if ui
                        .add_enabled(!state.media_form.submitting, restore_btn)
                        .clicked()
                        && let Some(auth) = current_auth(state)
                    {
                        state.media_form.submitting = true;
                        state.media_form.error = None;
                        state.media_form.success = None;
                        api.send(ApiCommand::RestoreMediaTextureOriginal {
                            auth,
                            id: asset.id,
                        });
                    }
                }
            });

        ui.add_space(8.0);
        ui.separator();
    }

    // --- Section 2: Metadata & Sync Info ---
    ui.label(egui::RichText::new("Informasi File & Offline Sync").strong());
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(20, 29, 44))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(format!(
                    "Slug: {} | Versi: v{}",
                    asset.slug, asset.version
                ))
                .size(11.0)
                .color(egui::Color32::from_rgb(203, 213, 225)),
            );
            ui.label(
                egui::RichText::new(format!(
                    "File Aktif: {} ({})",
                    asset.active_file.rel_path,
                    format_bytes(asset.active_file.size_bytes)
                ))
                .size(10.5)
                .color(egui::Color32::from_rgb(148, 163, 184)),
            );
            ui.label(
                egui::RichText::new(format!(
                    "SHA-256: {}",
                    &asset.active_file.sha256[..asset.active_file.sha256.len().min(16)]
                ))
                .size(10.5)
                .color(egui::Color32::from_rgb(148, 163, 184)),
            );
            if let Some(dur) = asset.duration_sec {
                ui.label(
                    egui::RichText::new(format!("Durasi: {}", format_duration(dur as f32)))
                        .size(11.0)
                        .color(egui::Color32::from_rgb(125, 211, 252)),
                );
            }
        });

    ui.add_space(8.0);
    ui.separator();

    // --- Section 3: Edit Metadata / Replace File ---
    ui.heading(format!("Edit {}", media_type.label()));
    ui.label("Nama Asset:");
    ui.add(
        egui::TextEdit::singleline(&mut state.media_form.name).desired_width(f32::INFINITY),
    );
    ui.add_space(4.0);
    ui.label("Kategori / Grup:");
    ui.add(
        egui::TextEdit::singleline(&mut state.media_form.category).desired_width(f32::INFINITY),
    );
    ui.add_space(6.0);

    ui.label(
        egui::RichText::new("Ganti File (opsional):")
            .size(11.5)
            .strong()
            .color(egui::Color32::from_rgb(203, 213, 225)),
    );
    if media_type == MediaAssetType::Texture {
        ui.horizontal(|ui| {
            ui.radio_value(&mut state.media_form.crop_mode, "crop".to_string(), "Center-Crop");
            ui.radio_value(&mut state.media_form.crop_mode, "resize".to_string(), "Fit/Resize");
        });
    }
    ui.horizontal(|ui| {
        if ui.button("Pilih File Pengganti...").clicked()
            && let Some(p) = pick_media_file(media_type)
        {
            state.media_form.file_path = Some(p);
        }
        if state.media_form.file_path.is_some() && ui.small_button("Clear").clicked() {
            state.media_form.file_path = None;
        }
    });
    if let Some(p) = &state.media_form.file_path {
        let fname = p.file_name().and_then(|s| s.to_str()).unwrap_or("file");
        ui.label(
            egui::RichText::new(format!("File baru: {fname}"))
                .size(11.0)
                .color(egui::Color32::from_rgb(74, 222, 128)),
        );
    }

    ui.add_space(8.0);
    if let Some(err) = &state.media_form.error {
        ui.colored_label(egui::Color32::from_rgb(248, 113, 113), err);
        ui.add_space(4.0);
    }
    if let Some(ok) = &state.media_form.success {
        ui.colored_label(egui::Color32::from_rgb(74, 222, 128), ok);
        ui.add_space(4.0);
    }

    let save_btn = egui::Button::new(
        egui::RichText::new(if state.media_form.submitting {
            "Menyimpan..."
        } else {
            "Simpan Perubahan"
        })
        .strong(),
    )
    .fill(egui::Color32::from_rgb(14, 116, 144));
    if ui
        .add_enabled(!state.media_form.submitting, save_btn)
        .clicked()
    {
        if state.media_form.name.trim().is_empty() {
            state.media_form.error = Some("Nama tidak boleh kosong.".into());
            return;
        }
        if let Some(auth) = current_auth(state) {
            state.media_form.submitting = true;
            state.media_form.error = None;
            state.media_form.success = None;
            api.send(ApiCommand::UpdateMediaAsset {
                auth,
                id: asset.id,
                payload: MediaAssetUploadPayload {
                    name: state.media_form.name.trim().to_string(),
                    media_type,
                    category: state.media_form.category.trim().to_string(),
                    crop_mode: state.media_form.crop_mode.clone(),
                    file_path: state.media_form.file_path.clone(),
                },
            });
        }
    }

    ui.add_space(12.0);
    ui.separator();

    // --- Section 4: Delete Media Asset ---
    if state.media_form.confirm_delete_id == Some(asset.id) {
        ui.colored_label(
            egui::Color32::from_rgb(248, 113, 113),
            format!("Hapus permanen '{}' dari server?", asset.name),
        );
        ui.horizontal(|ui| {
            let del_confirm = egui::Button::new(
                egui::RichText::new("Ya, Hapus Permanen")
                    .strong()
                    .color(egui::Color32::WHITE),
            )
            .fill(egui::Color32::from_rgb(185, 28, 28));
            if ui
                .add_enabled(!state.media_form.submitting, del_confirm)
                .clicked()
                && let Some(auth) = current_auth(state)
            {
                state.media_form.submitting = true;
                state.media_form.error = None;
                api.send(ApiCommand::DeleteMediaAsset {
                    auth,
                    id: asset.id,
                });
            }
            if ui.button("Batal").clicked() {
                state.media_form.confirm_delete_id = None;
            }
        });
    } else {
        let del_btn = egui::Button::new(
            egui::RichText::new(format!("Hapus {}", media_type.label()))
                .color(egui::Color32::from_rgb(252, 165, 165)),
        )
        .fill(egui::Color32::from_rgb(69, 20, 26));
        if ui
            .add_enabled(!state.media_form.submitting, del_btn)
            .clicked()
        {
            state.media_form.confirm_delete_id = Some(asset.id);
        }
    }
}
