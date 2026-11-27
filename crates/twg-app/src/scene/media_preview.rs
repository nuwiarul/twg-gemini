use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use bevy::{
    audio::{PlaybackMode, Volume},
    prelude::*,
};
use bevy_egui::egui;
use crossbeam_channel::{Receiver, bounded};
use uuid::Uuid;

use crate::state::{AppScreen, AppState, AssetsSection};

#[derive(Component)]
pub struct PreviewAudioEntity;

#[derive(Debug, Clone)]
pub enum AudioControlCmd {
    Play {
        rel_path: String,
        looping: bool,
        volume: f32,
        speed: f32,
        seek_sec: f32,
    },
    Pause,
    Resume,
    Stop,
    Seek(f32),
    SetVolume(f32),
    SetSpeed(f32),
}

pub struct DecodedVideoFrame {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
    pub pts_sec: f32,
}

struct ActiveVideoDecoder {
    stop_flag: Arc<AtomicBool>,
    pause_flag: Arc<AtomicBool>,
    frame_rx: Receiver<DecodedVideoFrame>,
    ended_flag: Arc<AtomicBool>,
}

impl ActiveVideoDecoder {
    fn stop(&self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        self.pause_flag.store(false, Ordering::SeqCst);
    }
}

impl Drop for ActiveVideoDecoder {
    fn drop(&mut self) {
        self.stop();
    }
}

#[derive(Resource)]
pub struct MediaPreviewPlayer {
    // Cached 2D Texture / Video thumbnail in egui
    pub tex_cache_key: Option<(String, u64)>,
    pub tex_handle: Option<egui::TextureHandle>,
    pub tex_dims: Option<(u32, u32)>,
    pub tex_error: Option<String>,

    // Audio playback state (drives both Sound tab and Video audio track)
    pub pending_audio_cmds: Vec<AudioControlCmd>,
    pub audio_asset_id: Option<Uuid>,
    pub audio_rel_path: Option<String>,
    pub audio_playing: bool,
    pub audio_paused: bool,
    pub audio_loop: bool,
    pub audio_volume: f32,
    pub audio_speed: f32,
    pub audio_position_sec: f32,
    pub pending_seek_after_spawn: Option<f32>,

    // Inline Video playback state
    pub ffmpeg_available: bool,
    pub video_asset_id: Option<Uuid>,
    pub video_abs_path: Option<PathBuf>,
    pub video_playing: bool,
    pub video_paused: bool,
    pub video_loop: bool,
    pub video_volume: f32,
    pub video_position_sec: f32,
    pub video_duration_sec: f32,
    pub video_fps: f32,
    pub video_dims: Option<(u32, u32)>,
    pub video_frame_tex: Option<egui::TextureHandle>,
    pub video_error: Option<String>,
    pub video_audio_nonce: u64,
    video_decoder: Option<ActiveVideoDecoder>,
}

impl Default for MediaPreviewPlayer {
    fn default() -> Self {
        let ffmpeg_available = check_command_exists("ffmpeg") && check_command_exists("ffprobe");
        Self {
            tex_cache_key: None,
            tex_handle: None,
            tex_dims: None,
            tex_error: None,
            pending_audio_cmds: Vec::new(),
            audio_asset_id: None,
            audio_rel_path: None,
            audio_playing: false,
            audio_paused: false,
            audio_loop: false,
            audio_volume: 0.85,
            audio_speed: 1.0,
            audio_position_sec: 0.0,
            pending_seek_after_spawn: None,
            ffmpeg_available,
            video_asset_id: None,
            video_abs_path: None,
            video_playing: false,
            video_paused: false,
            video_loop: false,
            video_volume: 0.85,
            video_position_sec: 0.0,
            video_duration_sec: 0.0,
            video_fps: 24.0,
            video_dims: None,
            video_frame_tex: None,
            video_error: None,
            video_audio_nonce: 0,
            video_decoder: None,
        }
    }
}

impl MediaPreviewPlayer {
    /// Load or return cached 2D egui texture for Image Texture preview or Video thumbnail.
    pub fn ensure_image_texture(
        &mut self,
        ctx: &egui::Context,
        assets_root: &Path,
        rel_path: &str,
        reload_nonce: u64,
    ) -> Option<&egui::TextureHandle> {
        let key = (rel_path.to_string(), reload_nonce);
        if self.tex_cache_key.as_ref() != Some(&key) {
            self.tex_cache_key = Some(key);
            self.tex_handle = None;
            self.tex_dims = None;
            self.tex_error = None;

            let abs = assets_root.join(rel_path);
            match image::open(&abs) {
                Ok(img) => {
                    let rgba = img.to_rgba8();
                    let (w, h) = rgba.dimensions();
                    let color_img = egui::ColorImage::from_rgba_unmultiplied(
                        [w as usize, h as usize],
                        rgba.as_raw(),
                    );
                    let handle = ctx.load_texture(
                        format!("media_tex_{rel_path}_{reload_nonce}"),
                        color_img,
                        egui::TextureOptions::LINEAR,
                    );
                    self.tex_dims = Some((w, h));
                    self.tex_handle = Some(handle);
                }
                Err(err) => {
                    self.tex_error = Some(format!("Gagal memuat preview gambar: {err}"));
                }
            }
        }
        self.tex_handle.as_ref()
    }

    pub fn play_sound(&mut self, asset_id: Uuid, rel_path: String, seek_sec: f32) {
        self.stop_video();
        self.audio_asset_id = Some(asset_id);
        self.audio_rel_path = Some(rel_path.clone());
        self.audio_playing = true;
        self.audio_paused = false;
        self.audio_position_sec = seek_sec;
        self.pending_audio_cmds.push(AudioControlCmd::Play {
            rel_path,
            looping: self.audio_loop,
            volume: self.audio_volume,
            speed: self.audio_speed,
            seek_sec,
        });
    }

    pub fn pause_sound(&mut self) {
        self.audio_paused = true;
        self.pending_audio_cmds.push(AudioControlCmd::Pause);
    }

    pub fn resume_sound(&mut self) {
        self.audio_paused = false;
        self.pending_audio_cmds.push(AudioControlCmd::Resume);
    }

    pub fn stop_sound(&mut self) {
        self.audio_playing = false;
        self.audio_paused = false;
        self.audio_position_sec = 0.0;
        self.pending_audio_cmds.push(AudioControlCmd::Stop);
    }

    pub fn start_video(
        &mut self,
        asset_id: Uuid,
        assets_root: &Path,
        rel_path: &str,
        start_sec: f32,
        fallback_duration: f32,
    ) {
        self.stop_video();
        self.stop_sound();

        let abs = assets_root.join(rel_path);
        self.video_asset_id = Some(asset_id);
        self.video_abs_path = Some(abs.clone());
        self.video_error = None;

        if !self.ffmpeg_available {
            self.video_error = Some(
                "ffmpeg/ffprobe tidak ditemukan di PATH. Install ffmpeg untuk memutar video langsung, atau klik 'Buka di Pemutar Sistem'.".into(),
            );
            return;
        }

        let probe = probe_video_stream(&abs);
        let src_w = probe.width.unwrap_or(640).max(64);
        let src_h = probe.height.unwrap_or(360).max(64);
        // Fit decoded stream inside 960px wide while preserving even dimensions
        let (out_w, out_h) = if src_w > 960 {
            let h = ((src_h as f64 * 960.0 / src_w as f64).round() as u32).max(64);
            (960, (h / 2) * 2)
        } else {
            ((src_w / 2) * 2, (src_h / 2) * 2)
        };
        let fps = probe.fps.clamp(5.0, 60.0);
        let dur = probe
            .duration_sec
            .unwrap_or(fallback_duration as f64)
            .max(0.0) as f32;

        self.video_dims = Some((out_w, out_h));
        self.video_fps = fps;
        self.video_duration_sec = dur;
        self.video_position_sec = start_sec.clamp(0.0, dur.max(start_sec));
        self.video_playing = true;
        self.video_paused = false;

        // Spawn synced audio track via Bevy native AudioPlayer if the video has an audio stream
        if probe.has_audio {
            self.video_audio_nonce = self.video_audio_nonce.wrapping_add(1);
            let cache_dir = assets_root.join(".preview_cache");
            let _ = std::fs::create_dir_all(&cache_dir);
            // Clean previous temp wavs
            if let Ok(entries) = std::fs::read_dir(&cache_dir) {
                for e in entries.flatten() {
                    let _ = std::fs::remove_file(e.path());
                }
            }
            let wav_rel = format!(".preview_cache/video_audio_{}.wav", self.video_audio_nonce);
            let wav_abs = assets_root.join(&wav_rel);
            let mut acmd = silent_command("ffmpeg");
            acmd.args(["-y", "-v", "quiet", "-ss", &format!("{start_sec:.3}"), "-i"])
                .arg(&abs)
                .args(["-vn", "-acodec", "pcm_s16le", "-ar", "44100", "-ac", "2"])
                .arg(&wav_abs);
            if acmd.status().is_ok_and(|s| s.success()) && wav_abs.exists() {
                self.pending_audio_cmds.push(AudioControlCmd::Play {
                    rel_path: wav_rel,
                    looping: false,
                    volume: self.video_volume,
                    speed: 1.0,
                    seek_sec: 0.0,
                });
            }
        }

        let stop_flag = Arc::new(AtomicBool::new(false));
        let pause_flag = Arc::new(AtomicBool::new(false));
        let ended_flag = Arc::new(AtomicBool::new(false));
        let (frame_tx, frame_rx) = bounded::<DecodedVideoFrame>(3);

        let stop_c = Arc::clone(&stop_flag);
        let pause_c = Arc::clone(&pause_flag);
        let ended_c = Arc::clone(&ended_flag);

        std::thread::Builder::new()
            .name("twg-video-decoder".into())
            .spawn(move || {
                let mut cmd = silent_command("ffmpeg");
                cmd.args(["-v", "quiet", "-ss", &format!("{start_sec:.3}"), "-i"])
                    .arg(&abs)
                    .args([
                        "-vf",
                        &format!("scale={out_w}:{out_h}"),
                        "-f",
                        "rawvideo",
                        "-pix_fmt",
                        "rgba",
                        "pipe:1",
                    ])
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null());

                let Ok(mut child) = cmd.spawn() else {
                    ended_c.store(true, Ordering::SeqCst);
                    return;
                };
                let Some(mut stdout) = child.stdout.take() else {
                    let _ = child.kill();
                    let _ = child.wait();
                    ended_c.store(true, Ordering::SeqCst);
                    return;
                };

                let frame_bytes = (out_w as usize) * (out_h as usize) * 4;
                let frame_dur = Duration::from_secs_f64(1.0 / (fps as f64));
                let mut frame_idx = 0u64;
                let mut stream_start = Instant::now();
                let mut total_paused = Duration::ZERO;

                loop {
                    if stop_c.load(Ordering::Relaxed) {
                        break;
                    }
                    if pause_c.load(Ordering::Relaxed) {
                        let p_start = Instant::now();
                        while pause_c.load(Ordering::Relaxed) && !stop_c.load(Ordering::Relaxed) {
                            std::thread::sleep(Duration::from_millis(20));
                        }
                        total_paused += p_start.elapsed();
                        continue;
                    }

                    let mut buf = vec![0u8; frame_bytes];
                    if stdout.read_exact(&mut buf).is_err() {
                        break;
                    }

                    // Pace playback to native FPS
                    let target_time = frame_dur.mul_f64(frame_idx as f64);
                    let elapsed = stream_start.elapsed().saturating_sub(total_paused);
                    if target_time > elapsed {
                        let wait = target_time - elapsed;
                        if wait > Duration::from_millis(2) {
                            std::thread::sleep(wait.min(Duration::from_millis(120)));
                        }
                    } else if elapsed.saturating_sub(target_time) > Duration::from_secs(2) {
                        stream_start = Instant::now();
                        total_paused = Duration::ZERO;
                        frame_idx = 0;
                    }

                    let pts_sec = start_sec + (frame_idx as f32 / fps);
                    frame_idx += 1;

                    let frame = DecodedVideoFrame {
                        width: out_w as usize,
                        height: out_h as usize,
                        rgba: buf,
                        pts_sec,
                    };
                    if frame_tx.send(frame).is_err() {
                        break;
                    }
                }

                let _ = child.kill();
                let _ = child.wait();
                ended_c.store(true, Ordering::SeqCst);
            })
            .ok();

        self.video_decoder = Some(ActiveVideoDecoder {
            stop_flag,
            pause_flag,
            frame_rx,
            ended_flag,
        });
    }

    pub fn pause_video(&mut self) {
        self.video_paused = true;
        if let Some(dec) = &self.video_decoder {
            dec.pause_flag.store(true, Ordering::SeqCst);
        }
        self.pending_audio_cmds.push(AudioControlCmd::Pause);
    }

    pub fn resume_video(&mut self) {
        self.video_paused = false;
        if let Some(dec) = &self.video_decoder {
            dec.pause_flag.store(false, Ordering::SeqCst);
        }
        self.pending_audio_cmds.push(AudioControlCmd::Resume);
    }

    pub fn stop_video(&mut self) {
        if let Some(dec) = self.video_decoder.take() {
            dec.stop();
        }
        self.video_playing = false;
        self.video_paused = false;
        self.video_position_sec = 0.0;
        self.pending_audio_cmds.push(AudioControlCmd::Stop);
    }

    pub fn poll_video_frame(&mut self, ctx: &egui::Context, assets_root: &Path, active_rel: Option<&str>) {
        let mut latest_frame = None;
        let mut ended = false;
        if let Some(dec) = &self.video_decoder {
            while let Ok(frame) = dec.frame_rx.try_recv() {
                latest_frame = Some(frame);
            }
            ended = dec.ended_flag.load(Ordering::Relaxed);
        }

        if let Some(frame) = latest_frame {
            self.video_position_sec = frame.pts_sec;
            let color_img =
                egui::ColorImage::from_rgba_unmultiplied([frame.width, frame.height], &frame.rgba);
            if let Some(tex) = &mut self.video_frame_tex {
                tex.set(color_img, egui::TextureOptions::LINEAR);
            } else {
                self.video_frame_tex = Some(ctx.load_texture(
                    "twg_video_live_frame",
                    color_img,
                    egui::TextureOptions::LINEAR,
                ));
            }
            ctx.request_repaint();
        } else if self.video_playing && !self.video_paused {
            ctx.request_repaint();
        }

        if ended && self.video_playing {
            let should_loop = self.video_loop;
            let vid = self.video_asset_id;
            let dur = self.video_duration_sec;
            self.stop_video();
            if should_loop
                && let (Some(id), Some(rel)) = (vid, active_rel)
            {
                self.start_video(id, assets_root, rel, 0.0, dur);
            }
        }
    }

    pub fn stop_all_playback(&mut self) {
        if self.video_decoder.is_some() || self.video_playing {
            self.stop_video();
        }
        if self.audio_playing || self.audio_paused {
            self.stop_sound();
        }
    }
}

pub fn open_in_system_player(abs_path: &Path) {
    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("cmd")
            .args(["/C", "start", ""])
            .arg(abs_path)
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("open").arg(abs_path).spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = Command::new("xdg-open").arg(abs_path).spawn();
    }
}

pub fn media_preview_audio_system(
    mut commands: Commands,
    state: Res<AppState>,
    mut player: ResMut<MediaPreviewPlayer>,
    mut audio_sources: ResMut<Assets<AudioSource>>,
    mut sink_q: Query<(Entity, &mut AudioSink), With<PreviewAudioEntity>>,
) {
    // Stop all playback if user leaves Assets screen or returns to 3D Models/Texture tabs
    if state.screen != AppScreen::Assets
        || matches!(
            state.assets_section,
            AssetsSection::Models3d | AssetsSection::Texture
        )
    {
        player.stop_all_playback();
    }

    let cmds: Vec<AudioControlCmd> = player.pending_audio_cmds.drain(..).collect();
    for cmd in cmds {
        match cmd {
            AudioControlCmd::Play {
                rel_path,
                looping,
                volume,
                speed,
                seek_sec,
            } => {
                for (ent, sink) in &mut sink_q {
                    sink.stop();
                    commands.entity(ent).despawn();
                }
                let abs = state.assets_root.join(&rel_path);
                let ext = abs
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|s| s.to_lowercase())
                    .unwrap_or_default();

                // Native pure-Rust decode via rodio (mp3 via minimp3, wav via hound, ogg via lewton, flac via claxon).
                // For aac/m4a fallback to ffmpeg -> WAV PCM bytes.
                let audio_bytes_opt: Option<Vec<u8>> = if matches!(
                    ext.as_str(),
                    "mp3" | "wav" | "ogg" | "oga" | "flac"
                ) {
                    std::fs::read(&abs).ok()
                } else if player.ffmpeg_available {
                    silent_command("ffmpeg")
                        .args(["-v", "quiet", "-nostdin", "-i"])
                        .arg(&abs)
                        .args(["-vn", "-f", "s16le", "-acodec", "pcm_s16le", "-ar", "44100", "-ac", "2", "pipe:1"])
                        .output()
                        .ok()
                        .filter(|o| o.status.success() && !o.stdout.is_empty())
                        .map(|o| wrap_pcm_s16le_as_wav(&o.stdout, 44_100, 2))
                } else {
                    None
                };

                if let Some(bytes) = audio_bytes_opt {
                    let handle = audio_sources.add(AudioSource {
                        bytes: bytes.into(),
                    });
                    let settings = PlaybackSettings {
                        mode: if looping {
                            PlaybackMode::Loop
                        } else {
                            PlaybackMode::Once
                        },
                        volume: Volume::Linear(volume.clamp(0.0, 2.0)),
                        speed: speed.clamp(0.25, 3.0),
                        ..default()
                    };
                    commands.spawn((AudioPlayer::new(handle), settings, PreviewAudioEntity));
                    player.pending_seek_after_spawn = if seek_sec > 0.05 {
                        Some(seek_sec)
                    } else {
                        None
                    };
                } else {
                    player.audio_playing = false;
                    player.audio_paused = false;
                }
            }
            AudioControlCmd::Pause => {
                for (_, sink) in &mut sink_q {
                    sink.pause();
                }
            }
            AudioControlCmd::Resume => {
                for (_, sink) in &mut sink_q {
                    sink.play();
                }
            }
            AudioControlCmd::Stop => {
                for (ent, sink) in &mut sink_q {
                    sink.stop();
                    commands.entity(ent).despawn();
                }
                player.pending_seek_after_spawn = None;
            }
            AudioControlCmd::Seek(sec) => {
                for (_, sink) in &mut sink_q {
                    let _ = sink.try_seek(Duration::from_secs_f32(sec.max(0.0)));
                }
            }
            AudioControlCmd::SetVolume(vol) => {
                for (_, mut sink) in &mut sink_q {
                    sink.set_volume(Volume::Linear(vol.clamp(0.0, 2.0)));
                }
            }
            AudioControlCmd::SetSpeed(spd) => {
                for (_, sink) in &mut sink_q {
                    sink.set_speed(spd.clamp(0.25, 3.0));
                }
            }
        }
    }

    // Apply deferred seek once AudioSink finishes spawning, and mirror playback clock into UI
    if let Ok((ent, sink)) = sink_q.single_mut() {
        if let Some(seek_sec) = player.pending_seek_after_spawn.take() {
            let _ = sink.try_seek(Duration::from_secs_f32(seek_sec.max(0.0)));
        }
        if player.audio_playing {
            player.audio_position_sec = sink.position().as_secs_f32();
            player.audio_paused = sink.is_paused();
            if sink.empty() {
                player.audio_playing = false;
                player.audio_paused = false;
                player.audio_position_sec = 0.0;
                commands.entity(ent).despawn();
            }
        }
    }
}

pub struct VideoStreamProbe {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: f32,
    pub duration_sec: Option<f64>,
    pub has_audio: bool,
}

pub fn probe_video_stream(path: &Path) -> VideoStreamProbe {
    let mut out = VideoStreamProbe {
        width: None,
        height: None,
        fps: 24.0,
        duration_sec: None,
        has_audio: false,
    };

    let Ok(res) = silent_command("ffprobe")
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(path)
        .output()
    else {
        return out;
    };
    if !res.status.success() {
        return out;
    }
    let Ok(val) = serde_json::from_slice::<serde_json::Value>(&res.stdout) else {
        return out;
    };

    out.duration_sec = val
        .pointer("/format/duration")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|d| d.is_finite() && *d > 0.0);

    if let Some(streams) = val.get("streams").and_then(|v| v.as_array()) {
        for st in streams {
            match st.get("codec_type").and_then(|v| v.as_str()) {
                Some("video") if out.width.is_none() => {
                    out.width = st.get("width").and_then(|v| v.as_u64()).map(|n| n as u32);
                    out.height = st.get("height").and_then(|v| v.as_u64()).map(|n| n as u32);
                    if let Some(rate) = st
                        .get("avg_frame_rate")
                        .or_else(|| st.get("r_frame_rate"))
                        .and_then(|v| v.as_str())
                    {
                        if let Some((a, b)) = rate.split_once('/') {
                            if let (Ok(num), Ok(den)) = (a.parse::<f32>(), b.parse::<f32>())
                                && den > 0.0
                                && num > 0.0
                            {
                                out.fps = num / den;
                            }
                        } else if let Ok(v) = rate.parse::<f32>()
                            && v > 0.0
                        {
                            out.fps = v;
                        }
                    }
                }
                Some("audio") => {
                    out.has_audio = true;
                }
                _ => {}
            }
        }
    }
    out
}

/// Resolves `ffmpeg` or `ffprobe` by checking:
/// 1. Explicit env var override (`TWG_FFMPEG_PATH` / `TWG_FFPROBE_PATH`)
/// 2. Sidecar binary bundled beside `twg-app` (`<exe_dir>/bin/`, `<exe_dir>/`,
///    macOS `<exe_dir>/../Resources/bin/`, Linux `<exe_dir>/../lib/twg-app/bin/`)
/// 3. Fallback to system `PATH` (for local dev and Linux `.deb` apt dependency)
pub fn resolve_media_binary(program: &str) -> PathBuf {
    let env_key = if program.eq_ignore_ascii_case("ffprobe") {
        "TWG_FFPROBE_PATH"
    } else {
        "TWG_FFMPEG_PATH"
    };
    if let Ok(custom) = std::env::var(env_key) {
        let trimmed = custom.trim();
        if !trimmed.is_empty() {
            let p = PathBuf::from(trimmed);
            if p.is_file() {
                return p;
            }
        }
    }

    #[cfg(target_os = "windows")]
    let file_name = if program.to_ascii_lowercase().ends_with(".exe") {
        program.to_string()
    } else {
        format!("{program}.exe")
    };
    #[cfg(not(target_os = "windows"))]
    let file_name = program.to_string();

    if let Ok(exe) = std::env::current_exe()
        && let Some(exe_dir) = exe.parent()
    {
        let candidates = [
            exe_dir.join("bin").join(&file_name),
            exe_dir.join(&file_name),
            exe_dir.join("..").join("Resources").join("bin").join(&file_name),
            exe_dir
                .join("..")
                .join("lib")
                .join("twg-app")
                .join("bin")
                .join(&file_name),
        ];
        for cand in candidates {
            if cand.is_file() {
                return cand;
            }
        }
    }

    PathBuf::from(program)
}

pub fn silent_command(program: &str) -> Command {
    let resolved = resolve_media_binary(program);
    #[allow(unused_mut)]
    let mut cmd = Command::new(resolved);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

pub fn check_command_exists(program: &str) -> bool {
    silent_command(program)
        .arg("-version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Wraps raw signed 16-bit little-endian PCM bytes (`ffmpeg -f s16le`) in a standard 44-byte
/// RIFF/WAVE header with exact chunk sizes.
///
/// Piping `-f wav` directly from FFmpeg writes `0xFFFFFFFF` (odd!) into the `data` chunk size,
/// which `hound` / `rodio` rejects as "data chunk length is not a multiple of sample size".
pub fn wrap_pcm_s16le_as_wav(pcm: &[u8], sample_rate: u32, channels: u16) -> Vec<u8> {
    let ch = channels.max(1);
    let sr = sample_rate.max(1);
    let bits_per_sample: u16 = 16;
    let block_align = ch * (bits_per_sample / 8);
    let byte_rate = sr.saturating_mul(block_align as u32);

    let max_pcm = ((u32::MAX as usize).saturating_sub(36) / block_align as usize)
        * block_align as usize;
    let aligned_len = (pcm.len().min(max_pcm) / block_align as usize) * block_align as usize;
    let data_size = aligned_len as u32;
    let riff_size = 36u32.saturating_add(data_size);

    let mut out = Vec::with_capacity(44 + aligned_len);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&riff_size.to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&ch.to_le_bytes());
    out.extend_from_slice(&sr.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&bits_per_sample.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_size.to_le_bytes());
    out.extend_from_slice(&pcm[..aligned_len]);
    out
}
