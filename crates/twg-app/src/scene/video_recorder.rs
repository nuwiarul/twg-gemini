use std::{
    io::Write,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use bevy::{
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured},
};
use crossbeam_channel::{Receiver, Sender, bounded};

use crate::{
    scene::media_preview::{check_command_exists, silent_command},
    state::{AppState, UiPointerState},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RecordCaptureArea {
    /// Captures only the clean 3D map viewport (excludes operator side panels & toolbars).
    #[default]
    CleanViewport,
    /// Captures the entire application window.
    FullWindow,
}

impl RecordCaptureArea {
    pub fn label(self) -> &'static str {
        match self {
            Self::CleanViewport => "Viewport 3D Bersih",
            Self::FullWindow => "Seluruh Jendela",
        }
    }
}

struct CapturedFrame {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    captured_at: Instant,
}

#[derive(Resource)]
pub struct VideoRecorderState {
    pub recording: bool,
    pub fps: u32,
    pub capture_area: RecordCaptureArea,
    pub output_path: Option<PathBuf>,
    pub last_saved_path: Option<PathBuf>,
    pub status_message: Option<String>,
    pub error_message: Option<String>,
    pub ffmpeg_available: bool,
    pub started_at: Option<Instant>,
    pub frames_written: Arc<AtomicU64>,
    pub encoder_error: Arc<Mutex<Option<String>>>,
    capture_in_flight: Arc<AtomicBool>,
    last_capture_request: Option<Instant>,
    frame_tx: Option<Sender<CapturedFrame>>,
    worker_handle: Option<std::thread::JoinHandle<()>>,
    /// UI intent flags processed cleanly inside `video_recorder_system`.
    pub pending_start_with_dialog: bool,
    pub pending_quick_start: bool,
    pub pending_stop: bool,
}

impl Default for VideoRecorderState {
    fn default() -> Self {
        Self {
            recording: false,
            fps: 24,
            capture_area: RecordCaptureArea::CleanViewport,
            output_path: None,
            last_saved_path: None,
            status_message: None,
            error_message: None,
            ffmpeg_available: check_command_exists("ffmpeg"),
            started_at: None,
            frames_written: Arc::new(AtomicU64::new(0)),
            encoder_error: Arc::new(Mutex::new(None)),
            capture_in_flight: Arc::new(AtomicBool::new(false)),
            last_capture_request: None,
            frame_tx: None,
            worker_handle: None,
            pending_start_with_dialog: false,
            pending_quick_start: false,
            pending_stop: false,
        }
    }
}

impl VideoRecorderState {
    pub fn elapsed_sec(&self) -> f64 {
        self.started_at
            .map(|t| t.elapsed().as_secs_f64())
            .unwrap_or(0.0)
    }

    pub fn formatted_elapsed(&self) -> String {
        let secs = self.elapsed_sec().floor() as u64;
        let mins = secs / 60;
        let rem = secs % 60;
        format!("{mins:02}:{rem:02}")
    }

    pub fn written_frame_count(&self) -> u64 {
        self.frames_written.load(Ordering::Relaxed)
    }

    fn default_recording_path(scenario_name: &str) -> PathBuf {
        let sanitized: String = scenario_name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let clean = sanitized.trim_matches('_');
        let prefix = if clean.is_empty() { "twg_presentasi" } else { clean };
        let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let dir = std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("recordings");
        let _ = std::fs::create_dir_all(&dir);
        dir.join(format!("{prefix}_{ts}.mp4"))
    }

    pub fn start_recording(&mut self, output_path: PathBuf) -> Result<(), String> {
        if self.recording {
            return Err("Perekaman sedang berjalan.".into());
        }
        if !self.ffmpeg_available {
            self.ffmpeg_available = check_command_exists("ffmpeg");
            if !self.ffmpeg_available {
                return Err(
                    "FFmpeg belum ditemukan. Instal FFmpeg atau taruh ffmpeg.exe di folder bin/."
                        .into(),
                );
            }
        }
        if let Some(parent) = output_path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Gagal membuat folder output: {e}"))?;
        }

        let (tx, rx) = bounded::<CapturedFrame>(4);
        let frames_written = Arc::new(AtomicU64::new(0));
        let encoder_error = Arc::new(Mutex::new(None));
        let fps = self.fps.clamp(12, 60);
        let path_clone = output_path.clone();
        let fw_clone = Arc::clone(&frames_written);
        let err_clone = Arc::clone(&encoder_error);

        let handle = std::thread::Builder::new()
            .name("twg-mp4-recorder".into())
            .spawn(move || {
                run_ffmpeg_encoder_worker(rx, path_clone, fps, fw_clone, err_clone);
            })
            .map_err(|e| format!("Gagal menjalankan thread encoder MP4: {e}"))?;

        self.recording = true;
        self.output_path = Some(output_path);
        self.error_message = None;
        self.status_message = Some(format!("Merekam MP4 ({fps} FPS)..."));
        self.started_at = Some(Instant::now());
        self.frames_written = frames_written;
        self.encoder_error = encoder_error;
        self.capture_in_flight.store(false, Ordering::Release);
        self.last_capture_request = None;
        self.frame_tx = Some(tx);
        self.worker_handle = Some(handle);
        Ok(())
    }

    pub fn stop_recording(&mut self) -> Option<PathBuf> {
        if !self.recording {
            return None;
        }
        self.recording = false;
        self.frame_tx = None; // Drop sender so worker flushes and closes ffmpeg stdin cleanly
        if let Some(handle) = self.worker_handle.take() {
            let _ = handle.join();
        }
        self.started_at = None;
        self.last_capture_request = None;
        self.capture_in_flight.store(false, Ordering::Release);

        let enc_err = self
            .encoder_error
            .lock()
            .ok()
            .and_then(|g| g.clone());
        let out_path = self.output_path.take();

        if let Some(err) = enc_err {
            self.error_message = Some(err);
            self.status_message = None;
            return None;
        }

        if let Some(path) = out_path {
            let written = self.frames_written.load(Ordering::Relaxed);
            if written == 0 {
                self.error_message = Some("Rekaman dihentikan sebelum ada frame yang terekam.".into());
                self.status_message = None;
                return None;
            }
            self.last_saved_path = Some(path.clone());
            self.status_message = Some(format!(
                "Video MP4 tersimpan ({written} frame): {}",
                path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("output.mp4")
            ));
            return Some(path);
        }
        None
    }
}

impl Drop for VideoRecorderState {
    fn drop(&mut self) {
        if self.recording {
            let _ = self.stop_recording();
        }
    }
}

fn run_ffmpeg_encoder_worker(
    rx: Receiver<CapturedFrame>,
    output_path: PathBuf,
    fps: u32,
    frames_written: Arc<AtomicU64>,
    encoder_error: Arc<Mutex<Option<String>>>,
) {
    let set_err = |msg: String| {
        if let Ok(mut g) = encoder_error.lock() {
            *g = Some(msg);
        }
    };

    let Ok(first) = rx.recv() else {
        return;
    };

    let locked_w = (first.width & !1).max(16);
    let locked_h = (first.height & !1).max(16);
    let first_at = first.captured_at;

    let mut cmd = silent_command("ffmpeg");
    cmd.args([
        "-y",
        "-loglevel",
        "error",
        "-f",
        "rawvideo",
        "-pixel_format",
        "rgba",
        "-video_size",
        &format!("{locked_w}x{locked_h}"),
        "-framerate",
        &fps.to_string(),
        "-i",
        "pipe:0",
        "-c:v",
        "libx264",
        "-preset",
        "veryfast",
        "-crf",
        "20",
        "-pix_fmt",
        "yuv420p",
        "-movflags",
        "+faststart",
    ])
    .arg(&output_path)
    .stdin(Stdio::piped())
    .stdout(Stdio::null())
    .stderr(Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            set_err(format!("Gagal menjalankan FFmpeg: {e}"));
            return;
        }
    };

    let Some(mut stdin) = child.stdin.take() else {
        set_err("FFmpeg stdin pipe tidak tersedia.".into());
        let _ = child.kill();
        return;
    };

    let normalize_frame = |frame: CapturedFrame| -> Option<Vec<u8>> {
        if frame.width == locked_w && frame.height == locked_h {
            return Some(frame.rgba);
        }
        let img = image::RgbaImage::from_raw(frame.width, frame.height, frame.rgba)?;
        let resized = image::imageops::resize(
            &img,
            locked_w,
            locked_h,
            image::imageops::FilterType::Triangle,
        );
        Some(resized.into_raw())
    };

    let Some(mut last_buf) = normalize_frame(first) else {
        set_err("Frame awal tidak valid.".into());
        drop(stdin);
        let _ = child.wait();
        return;
    };

    if stdin.write_all(&last_buf).is_err() {
        drop(stdin);
        let output = child.wait_with_output();
        let detail = output
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stderr).trim().to_string())
            .unwrap_or_default();
        set_err(format!("FFmpeg berhenti menulis frame: {detail}"));
        return;
    }
    let mut total_emitted: u64 = 1;
    frames_written.store(total_emitted, Ordering::Relaxed);

    while let Ok(frame) = rx.recv() {
        let elapsed_s = frame.captured_at.saturating_duration_since(first_at).as_secs_f64();
        let target_idx = (elapsed_s * fps as f64).round() as u64;
        let Some(norm_buf) = normalize_frame(frame) else {
            continue;
        };

        // Pad any skipped real-time frame slots with the previous frame so playback duration matches wall clock
        let max_catchup = total_emitted.saturating_add((fps as u64) * 2);
        let desired_total = target_idx.max(total_emitted).min(max_catchup);
        while total_emitted < desired_total {
            if stdin.write_all(&last_buf).is_err() {
                break;
            }
            total_emitted += 1;
            frames_written.store(total_emitted, Ordering::Relaxed);
        }

        if stdin.write_all(&norm_buf).is_err() {
            break;
        }
        total_emitted += 1;
        frames_written.store(total_emitted, Ordering::Relaxed);
        last_buf = norm_buf;
    }

    let _ = stdin.flush();
    drop(stdin);

    match child.wait_with_output() {
        Ok(out) if out.status.success() => {}
        Ok(out) => {
            let err_txt = String::from_utf8_lossy(&out.stderr).trim().to_string();
            set_err(if err_txt.is_empty() {
                format!("FFmpeg keluar dengan kode {:?}", out.status.code())
            } else {
                format!("FFmpeg error: {err_txt}")
            });
        }
        Err(e) => {
            set_err(format!("Gagal menunggu proses FFmpeg selesai: {e}"));
        }
    }
}

/// Opens a file or its parent folder in the native OS shell.
pub fn open_path_in_os(path: &Path) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer").arg(path).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(path).spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = std::process::Command::new("xdg-open").arg(path).spawn();
    }
}

#[allow(clippy::too_many_arguments)]
pub fn video_recorder_system(
    mut commands: Commands,
    mut recorder: ResMut<VideoRecorderState>,
    mut state: ResMut<AppState>,
    ui_state: Res<UiPointerState>,
    windows: Query<&Window>,
) {
    // 1. Handle UI Stop trigger
    if recorder.pending_stop {
        recorder.pending_stop = false;
        if let Some(saved) = recorder.stop_recording() {
            state.status_banner = Some(format!(
                "Rekaman MP4 selesai disimpan ke: {}",
                saved.display()
            ));
        } else if let Some(err) = recorder.error_message.clone() {
            state.status_banner = Some(format!("Rekam MP4 gagal: {err}"));
        }
        return;
    }

    // 2. Handle UI Start triggers (Quick Start or Save-As FileDialog)
    if recorder.pending_start_with_dialog || recorder.pending_quick_start {
        let ask_dialog = recorder.pending_start_with_dialog;
        recorder.pending_start_with_dialog = false;
        recorder.pending_quick_start = false;

        let scenario_name = state
            .active_skenario
            .as_ref()
            .map(|s| s.name.clone())
            .unwrap_or_else(|| "twg_presentasi".into());
        let default_path = VideoRecorderState::default_recording_path(&scenario_name);

        let chosen_path = if ask_dialog {
            let file_name = default_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("presentasi.mp4");
            let mut dlg = rfd::FileDialog::new()
                .set_title("Simpan Rekaman Video Presentasi (MP4)")
                .add_filter("Video MP4", &["mp4"])
                .set_file_name(file_name);
            if let Some(parent) = default_path.parent() {
                dlg = dlg.set_directory(parent);
            }
            dlg.save_file()
        } else {
            Some(default_path)
        };

        if let Some(mut path) = chosen_path {
            if path.extension().is_none() {
                path.set_extension("mp4");
            }
            match recorder.start_recording(path.clone()) {
                Ok(()) => {
                    state.status_banner = Some(format!(
                        "Mulai merekam video MP4 ({} FPS, {})...",
                        recorder.fps,
                        recorder.capture_area.label()
                    ));
                }
                Err(err) => {
                    recorder.error_message = Some(err.clone());
                    state.status_banner = Some(err);
                }
            }
        }
        return;
    }

    // 3. Check if background encoder reported an error while recording
    if recorder.recording {
        let bg_err = recorder
            .encoder_error
            .lock()
            .ok()
            .and_then(|g| g.clone());
        if let Some(err) = bg_err {
            let _ = recorder.stop_recording();
            recorder.error_message = Some(err.clone());
            state.status_banner = Some(format!("Rekam MP4 berhenti karena error: {err}"));
            return;
        }
    } else {
        return;
    }

    // 4. Pace GPU screenshot capture according to target FPS (never overlapping in-flight captures)
    if recorder.capture_in_flight.load(Ordering::Acquire) {
        return;
    }
    let interval = Duration::from_secs_f64(1.0 / (recorder.fps.clamp(12, 60) as f64));
    let now = Instant::now();
    if let Some(last) = recorder.last_capture_request
        && now.saturating_duration_since(last) < interval
    {
        return;
    }

    let Some(tx) = recorder.frame_tx.clone() else {
        return;
    };

    let Ok(window) = windows.single() else {
        return;
    };
    let scale = window.scale_factor();
    let crop_rect_px: Option<(u32, u32, u32, u32)> =
        if recorder.capture_area == RecordCaptureArea::CleanViewport
            && let Some(vr) = ui_state.viewport_rect_logical
        {
            let x = (vr.min.x.max(0.0) * scale).round() as u32;
            let y = (vr.min.y.max(0.0) * scale).round() as u32;
            let w = (vr.width().max(16.0) * scale).floor() as u32;
            let h = (vr.height().max(16.0) * scale).floor() as u32;
            Some((x, y, w, h))
        } else {
            None
        };

    recorder.last_capture_request = Some(now);
    recorder.capture_in_flight.store(true, Ordering::Release);
    let in_flight = Arc::clone(&recorder.capture_in_flight);

    commands
        .spawn(Screenshot::primary_window())
        .observe(move |ev: On<ScreenshotCaptured>| {
            in_flight.store(false, Ordering::Release);
            let captured_at = Instant::now();
            let Ok(dyn_img) = ev.image.clone().try_into_dynamic() else {
                return;
            };
            let rgba_img = dyn_img.to_rgba8();
            let img_w = rgba_img.width();
            let img_h = rgba_img.height();

            let (cx, cy, cw, ch) = if let Some((x, y, w, h)) = crop_rect_px {
                let x = x.min(img_w.saturating_sub(16));
                let y = y.min(img_h.saturating_sub(16));
                let w = w.min(img_w.saturating_sub(x)) & !1;
                let h = h.min(img_h.saturating_sub(y)) & !1;
                (x, y, w, h)
            } else {
                (0, 0, img_w & !1, img_h & !1)
            };

            if cw < 16 || ch < 16 {
                return;
            }

            let out_rgba = if cx == 0 && cy == 0 && cw == img_w && ch == img_h {
                rgba_img.into_raw()
            } else {
                let mut buf = Vec::with_capacity((cw as usize) * (ch as usize) * 4);
                let raw = rgba_img.as_raw();
                let stride = (img_w as usize) * 4;
                let row_bytes = (cw as usize) * 4;
                for row in cy..(cy + ch) {
                    let start = (row as usize) * stride + (cx as usize) * 4;
                    buf.extend_from_slice(&raw[start..start + row_bytes]);
                }
                buf
            };

            let _ = tx.try_send(CapturedFrame {
                width: cw,
                height: ch,
                rgba: out_rgba,
                captured_at,
            });
        });
}
