use std::{
    collections::HashMap,
    io::Cursor,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use bevy::{
    audio::{PlaybackMode, Volume},
    prelude::*,
};
use crossbeam_channel::{Receiver, bounded};
use rodio::Source;
use twg_shared::{
    audio_anim::{AudioAnimConfig, is_audio_anim, parse_audio_anim_config},
    models::MediaAssetType,
};
use uuid::Uuid;

use crate::{
    net::asset_sync::{AssetSyncBridge, AssetSyncCommand},
    state::{AnimRunStatus, AppState, PlaybackState},
};

type LoadedSound = Result<(Arc<[u8]>, f64), String>;

enum CachedSound {
    Loading(Receiver<LoadedSound>),
    Ready(Handle<AudioSource>, f64),
    Failed(String),
}

struct AudioRun {
    trigger: u64,
    config: AudioAnimConfig,
    entity: Option<Entity>,
    delay_elapsed: f64,
    plays_done: u32,
    waiting_since: Instant,
    download_requested: bool,
}

#[derive(Resource, Default)]
pub struct ScenarioAudio {
    session_generation: Option<u64>,
    autoplay_started: bool,
    runs: HashMap<Uuid, AudioRun>,
    cache: HashMap<String, CachedSound>,
}

fn load_sound(path: PathBuf) -> LoadedSound {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let bytes = if matches!(ext.as_str(), "mp3" | "wav" | "ogg" | "oga" | "flac") {
        std::fs::read(&path).map_err(|e| format!("Tidak dapat membaca suara: {e}"))?
    } else {
        let mut cmd = std::process::Command::new("ffmpeg");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        let output = cmd
            .args(["-v", "error", "-nostdin", "-i"])
            .arg(&path)
            .args([
                "-vn",
                "-f",
                "wav",
                "-acodec",
                "pcm_s16le",
                "-ar",
                "44100",
                "-ac",
                "2",
                "pipe:1",
            ])
            .output()
            .map_err(|e| format!("Format suara ini memerlukan FFmpeg: {e}"))?;
        if !output.status.success() {
            return Err("FFmpeg tidak dapat membaca file suara.".into());
        }
        output.stdout
    };
    let bytes: Arc<[u8]> = bytes.into();
    // Validate before passing bytes to Bevy (its decoder assumes a valid source).
    let decoder = rodio::Decoder::try_from(Cursor::new(bytes.clone()))
        .map_err(|e| format!("File suara tidak dapat diputar: {e}"))?;
    // FFmpeg writes an unspecified WAV length to stdout; count decoded samples
    // instead of trusting that streaming header as a multi-hour duration.
    let known_duration = if matches!(ext.as_str(), "mp3" | "wav" | "ogg" | "oga" | "flac") {
        decoder.total_duration()
    } else {
        None
    };
    let duration = if let Some(duration) = known_duration {
        duration.as_secs_f64()
    } else {
        let samples_per_sec = decoder.sample_rate().get() as f64 * decoder.channels().get() as f64;
        decoder.count() as f64 / samples_per_sec
    };
    if duration <= 0.001 || !duration.is_finite() {
        return Err("File suara kosong.".into());
    }
    Ok((bytes, duration))
}

fn stop_entity(commands: &mut Commands, sinks: &Query<&mut AudioSink>, entity: Option<Entity>) {
    if let Some(entity) = entity {
        if let Ok(sink) = sinks.get(entity) {
            sink.stop();
        }
        commands.entity(entity).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{AppScreen, SelectedEntity};
    use twg_shared::{
        audio_anim::{ANIM_TYPE_PLAY_SOUND, AudioRepeat, write_audio_anim_config},
        models::{Anim, MediaAssetDto, ModelFileMeta},
    };

    #[test]
    fn audio_loader_validates_wav_duration_and_rejects_corrupt_files() {
        let path = std::env::temp_dir().join(format!("twg-audio-{}.wav", Uuid::new_v4()));
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&16036u32.to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&8000u32.to_le_bytes());
        bytes.extend_from_slice(&16000u32.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&16000u32.to_le_bytes());
        bytes.resize(16044, 0);
        std::fs::write(&path, bytes).unwrap();
        let valid = load_sound(path.clone());
        std::fs::write(&path, b"invalid sound").unwrap();
        let invalid = load_sound(path.clone());
        std::fs::remove_file(path).unwrap();
        assert!((valid.unwrap().1 - 1.0).abs() < 0.001);
        assert!(invalid.is_err());
    }

    fn fixture() -> (App, Uuid) {
        let mut app = App::new();
        let mut state = AppState::new(std::env::temp_dir().join("twg-audio-test-unused"));
        state.screen = AppScreen::PlotSkenario;
        state.scenario_audio_enabled = true;
        let now = chrono::Utc::now();
        let file = ModelFileMeta {
            rel_path: "sound/test.wav".into(),
            sha256: "test".into(),
            size_bytes: 0,
            tris: 0,
            download_url: String::new(),
        };
        let asset_id = Uuid::new_v4();
        state.media_assets = vec![MediaAssetDto {
            id: asset_id,
            slug: "test".into(),
            name: "Test sound".into(),
            media_type: MediaAssetType::Sound,
            category: "Sound".into(),
            mime_type: "audio/wav".into(),
            original_file: file.clone(),
            active_file: file,
            thumbnail_file: None,
            original_width: None,
            original_height: None,
            active_width: None,
            active_height: None,
            duration_sec: Some(2.0),
            crop_mode: None,
            version: 1,
            created_at: now,
            updated_at: now,
        }];
        let mut anim = Anim {
            id: Uuid::new_v4(),
            operasi_id: Uuid::new_v4(),
            skenario_id: Uuid::new_v4(),
            marker_id: None,
            name: "Play Suara".into(),
            anim_type: ANIM_TYPE_PLAY_SOUND.into(),
            sort_order: 1,
            speed: 1.0,
            duration_sec: 2.0,
            replay: false,
            autoplay: false,
            geom: vec![],
            data: serde_json::json!({}),
            created_at: now,
            updated_at: now,
        };
        write_audio_anim_config(
            &mut anim,
            &AudioAnimConfig {
                asset_id: Some(asset_id),
                ..Default::default()
            },
        );
        let id = anim.id;
        state.selected = SelectedEntity::Anim(id);
        state.anims = vec![anim];
        let mut sources = Assets::<AudioSource>::default();
        let handle = sources.add(AudioSource {
            bytes: Arc::from([]),
        });
        let mut runtime = ScenarioAudio::default();
        runtime.cache.insert(
            "sound/test.wav:test".into(),
            CachedSound::Ready(handle, 2.0),
        );
        app.insert_resource(state)
            .insert_resource(runtime)
            .insert_resource(sources)
            .insert_resource(AssetSyncBridge::start())
            .init_resource::<PlaybackState>()
            .init_resource::<Time<Real>>()
            .add_systems(Update, scenario_audio_system);
        (app, id)
    }

    #[test]
    fn audio_retrigger_preview_transition_and_exit_remove_old_players() {
        let (mut app, id) = fixture();
        app.world_mut()
            .resource_mut::<PlaybackState>()
            .start_editor_preview(id);
        app.update();
        let first = app.world().resource::<ScenarioAudio>().runs[&id]
            .entity
            .unwrap();
        app.update();
        assert_eq!(
            app.world().resource::<ScenarioAudio>().runs[&id].entity,
            Some(first)
        );
        // Restarting the same preview resets trigger_seq to 1, so session identity matters.
        app.world_mut()
            .resource_mut::<PlaybackState>()
            .start_editor_preview(id);
        app.update();
        let second = app.world().resource::<ScenarioAudio>().runs[&id]
            .entity
            .unwrap();
        assert_ne!(first, second);
        assert!(app.world().get_entity(first).is_err());
        app.world_mut()
            .resource_mut::<PlaybackState>()
            .play_single_immediately(id);
        app.update();
        assert!(app.world().get_entity(second).is_err());
        let third = app.world().resource::<ScenarioAudio>().runs[&id]
            .entity
            .unwrap();
        app.world_mut().resource_mut::<AppState>().screen = AppScreen::Assets;
        app.update();
        assert!(app.world().resource::<ScenarioAudio>().runs.is_empty());
        assert!(app.world().get_entity(third).is_err());
    }

    #[test]
    fn audio_waits_for_asset_manifest_before_starting() {
        let (mut app, id) = fixture();
        let assets = {
            let mut state = app.world_mut().resource_mut::<AppState>();
            state.sync_status.checking = true;
            std::mem::take(&mut state.media_assets)
        };
        app.world_mut()
            .resource_mut::<PlaybackState>()
            .play_single_immediately(id);
        app.update();
        assert_eq!(
            app.world().resource::<PlaybackState>().track_status(id),
            AnimRunStatus::Playing
        );
        assert!(
            app.world().resource::<ScenarioAudio>().runs[&id]
                .entity
                .is_none()
        );
        {
            let mut state = app.world_mut().resource_mut::<AppState>();
            state.media_assets = assets;
            state.sync_status.checking = false;
        }
        app.update();
        assert!(
            app.world().resource::<ScenarioAudio>().runs[&id]
                .entity
                .is_some()
        );
    }

    #[test]
    fn audio_pause_resume_mute_and_stop_control_existing_sink() {
        let (mut app, id) = fixture();
        app.world_mut()
            .resource_mut::<PlaybackState>()
            .play_single_immediately(id);
        app.update();
        let entity = app.world().resource::<ScenarioAudio>().runs[&id]
            .entity
            .unwrap();
        let (player, _output) = rodio::Player::new();
        player.append(rodio::buffer::SamplesBuffer::new(
            1.try_into().unwrap(),
            8000.try_into().unwrap(),
            vec![0.0f32; 16000],
        ));
        app.world_mut()
            .entity_mut(entity)
            .insert(AudioSink::new(player));
        app.update();
        assert!(!app.world().get::<AudioSink>(entity).unwrap().is_paused());
        app.world_mut()
            .resource_mut::<PlaybackState>()
            .pause_anim(id);
        app.update();
        assert!(app.world().get::<AudioSink>(entity).unwrap().is_paused());
        app.world_mut()
            .resource_mut::<PlaybackState>()
            .resume_anim(id);
        app.world_mut()
            .resource_mut::<AppState>()
            .scenario_audio_enabled = false;
        app.update();
        let sink = app.world().get::<AudioSink>(entity).unwrap();
        assert!(!sink.is_paused());
        assert_eq!(sink.volume(), Volume::Linear(0.0));
        assert_eq!(sink.speed(), 1.0);
        app.world_mut()
            .resource_mut::<PlaybackState>()
            .stop_anim(id);
        app.update();
        assert!(app.world().get_entity(entity).is_err());
        assert!(
            !app.world()
                .resource::<AppState>()
                .audio_status
                .contains_key(&id)
        );
    }

    #[test]
    fn audio_finite_repeat_completes_only_after_last_sink_and_autoplay_runs_once() {
        let (mut app, id) = fixture();
        {
            let mut state = app.world_mut().resource_mut::<AppState>();
            let anim = &mut state.anims[0];
            anim.autoplay = true;
            let mut cfg = parse_audio_anim_config(anim);
            cfg.repeat = AudioRepeat::Count;
            cfg.repeat_count = 2;
            write_audio_anim_config(anim, &cfg);
        }
        app.world_mut()
            .resource_mut::<PlaybackState>()
            .enter_play_mode();
        app.update();
        for repetition in 0..2 {
            let entity = app.world().resource::<ScenarioAudio>().runs[&id]
                .entity
                .unwrap();
            let (player, _output) = rodio::Player::new();
            app.world_mut()
                .entity_mut(entity)
                .insert(AudioSink::new(player)); // drained clip
            app.update();
            assert!(app.world().get_entity(entity).is_err());
            let track = &app.world().resource::<PlaybackState>().tracks[&id];
            assert_eq!(track.completed_cycles, if repetition == 0 { 0 } else { 1 });
            assert_eq!(
                track.status,
                if repetition == 0 {
                    AnimRunStatus::Playing
                } else {
                    AnimRunStatus::Finished
                }
            );
        }
        app.update();
        assert!(app.world().resource::<ScenarioAudio>().runs.is_empty());
        assert_eq!(
            app.world().resource::<PlaybackState>().track_status(id),
            AnimRunStatus::Finished
        );
    }
}

/// One entity per running animation. Preview and scenario playback share the normal tracks,
/// including Group ownership; audio output is local and never changes other clients' volume.
pub fn scenario_audio_system(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut state: ResMut<AppState>,
    mut playback: ResMut<PlaybackState>,
    sync: Res<AssetSyncBridge>,
    mut runtime: ResMut<ScenarioAudio>,
    mut sources: ResMut<Assets<AudioSource>>,
    mut sinks: Query<&mut AudioSink>,
) {
    let active = state.is_3d_map_screen()
        && (playback.play_test_mode || playback.editor_preview_anim_id.is_some());
    let new_session = runtime.session_generation != Some(playback.session_generation);
    if new_session || !active {
        for (_, run) in runtime.runs.drain() {
            stop_entity(&mut commands, &sinks, run.entity);
        }
        runtime.session_generation = Some(playback.session_generation);
        runtime.autoplay_started = false;
        state.audio_status.clear();
    }
    if active && playback.play_test_mode && !runtime.autoplay_started {
        runtime.autoplay_started = true;
        for anim in &state.anims {
            if is_audio_anim(anim) && anim.autoplay && !playback.tracks.contains_key(&anim.id) {
                playback.play_anim(anim.id);
            }
        }
    }
    state.audio_status.retain(|id, _| {
        active
            && playback
                .tracks
                .get(id)
                .is_some_and(|t| t.status != AnimRunStatus::Stopped)
    });
    let obsolete: Vec<_> = runtime
        .runs
        .iter()
        .filter_map(|(&id, run)| {
            let valid = active
                && state.anims.iter().any(|a| a.id == id && is_audio_anim(a))
                && playback.tracks.get(&id).is_some_and(|t| {
                    t.trigger_seq == run.trigger
                        && matches!(t.status, AnimRunStatus::Playing | AnimRunStatus::Paused)
                });
            (!valid).then_some(id)
        })
        .collect();
    for id in obsolete {
        if let Some(run) = runtime.runs.remove(&id) {
            stop_entity(&mut commands, &sinks, run.entity);
        }
        state.audio_status.remove(&id);
    }
    if !active {
        runtime.cache.clear();
        return;
    }

    for cached in runtime.cache.values_mut() {
        if let CachedSound::Loading(rx) = cached {
            match rx.try_recv() {
                Ok(Ok((bytes, duration))) => {
                    *cached = CachedSound::Ready(sources.add(AudioSource { bytes }), duration)
                }
                Ok(Err(error)) => *cached = CachedSound::Failed(error),
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    *cached = CachedSound::Failed("Tidak dapat memuat suara.".into())
                }
                Err(crossbeam_channel::TryRecvError::Empty) => {}
            }
        }
    }
    let anims: Vec<_> = state
        .anims
        .iter()
        .filter(|a| is_audio_anim(a))
        .cloned()
        .collect();
    let audible = !playback.play_test_mode || state.scenario_audio_enabled;
    let dt = time.delta_secs_f64();
    for anim in anims {
        let Some(track) = playback.tracks.get_mut(&anim.id) else {
            continue;
        };
        if !matches!(track.status, AnimRunStatus::Playing | AnimRunStatus::Paused) {
            continue;
        }
        let mut run = runtime.runs.remove(&anim.id).unwrap_or_else(|| AudioRun {
            trigger: track.trigger_seq,
            config: parse_audio_anim_config(&anim),
            entity: None,
            delay_elapsed: 0.0,
            plays_done: 0,
            waiting_since: Instant::now(),
            download_requested: false,
        });
        // Freeze configuration for this run; edits take effect on the next Play.
        let cfg = &run.config;
        let result = (|| -> Result<bool, String> {
            let asset = state
                .media_assets
                .iter()
                .find(|m| Some(m.id) == cfg.asset_id && m.media_type == MediaAssetType::Sound);
            let Some(asset) = asset else {
                if cfg.asset_id.is_some()
                    && state.sync_status.checking
                    && run.waiting_since.elapsed().as_secs() < 90
                {
                    state
                        .audio_status
                        .insert(anim.id, "Menunggu daftar Sound dari server...".into());
                    return Ok(false);
                }
                return Err("Pilih Sound yang tersedia di halaman Assets.".into());
            };
            let file = &asset.active_file;
            let key = format!("{}:{}", file.rel_path, file.sha256);
            let path = state.assets_root.join(&file.rel_path);
            if !runtime.cache.contains_key(&key) {
                if !path.is_file() {
                    if !run.download_requested {
                        sync.send(AssetSyncCommand::PriorityDownload {
                            server_url: state.server_url.clone(),
                            local_root: state.assets_root.clone(),
                            files: vec![file.clone()],
                        });
                        run.download_requested = true;
                    }
                    if run.waiting_since.elapsed().as_secs() > 90 {
                        return Err(
                            "Unduhan suara belum selesai. Periksa koneksi lalu Play ulang.".into(),
                        );
                    }
                    state
                        .audio_status
                        .insert(anim.id, "Mengunduh suara dari server...".into());
                    return Ok(false);
                }
                let (tx, rx) = bounded(1);
                std::thread::spawn(move || {
                    let _ = tx.send(load_sound(path));
                });
                runtime.cache.insert(key.clone(), CachedSound::Loading(rx));
            }
            let (handle, source_duration) = match runtime.cache.get(&key).unwrap() {
                CachedSound::Ready(handle, duration) => (handle.clone(), *duration),
                CachedSound::Failed(error) => return Err(error.clone()),
                CachedSound::Loading(_) => {
                    state
                        .audio_status
                        .insert(anim.id, "Menyiapkan suara...".into());
                    return Ok(false);
                }
            };
            let clip_duration = cfg
                .clip_duration(source_duration)
                .ok_or("Rentang potongan suara tidak valid.")?;
            track.total_length_m = cfg.total_duration(source_duration).unwrap_or(0.0);
            if track.status == AnimRunStatus::Paused {
                if let Some(entity) = run.entity {
                    if let Ok(sink) = sinks.get(entity) {
                        sink.pause();
                    }
                }
                state.audio_status.insert(anim.id, "Dijeda".into());
                return Ok(false);
            }
            if run.delay_elapsed < cfg.delay_sec {
                run.delay_elapsed = (run.delay_elapsed + dt).min(cfg.delay_sec);
                track.distance_m = run.delay_elapsed;
                state.audio_status.insert(
                    anim.id,
                    format!("Jeda awal: {:.1} dtk", cfg.delay_sec - run.delay_elapsed),
                );
                return Ok(false);
            }
            if let Some(entity) = run.entity {
                if let Ok(mut sink) = sinks.get_mut(entity) {
                    if sink.empty() {
                        stop_entity(&mut commands, &sinks, run.entity.take());
                        run.plays_done += 1;
                        track.distance_m = cfg.delay_sec + clip_duration * run.plays_done as f64;
                        if cfg.plays().is_some_and(|n| run.plays_done >= n) {
                            return Ok(true);
                        }
                    } else {
                        sink.play();
                        let position = sink.position().as_secs_f64().min(clip_duration);
                        sink.set_volume(Volume::Linear(if audible {
                            cfg.gain(position, clip_duration)
                        } else {
                            0.0
                        }));
                        track.distance_m =
                            cfg.delay_sec + clip_duration * run.plays_done as f64 + position;
                        state.audio_status.insert(
                            anim.id,
                            format!(
                                "{} · {:.1} / {:.1} dtk · putaran {}",
                                if audible {
                                    "Memutar"
                                } else {
                                    "Output perangkat dibisukan"
                                },
                                position,
                                clip_duration,
                                run.plays_done + 1
                            ),
                        );
                    }
                } else if run.waiting_since.elapsed().as_secs() > 10 {
                    return Err(
                        "Perangkat audio tidak tersedia. Periksa output suara lalu Play ulang."
                            .into(),
                    );
                }
            }
            if run.entity.is_none() {
                let settings = PlaybackSettings {
                    paused: true,
                    mode: PlaybackMode::Once,
                    volume: Volume::Linear(if audible {
                        cfg.gain(0.0, clip_duration)
                    } else {
                        0.0
                    }),
                    start_position: Some(Duration::from_secs_f64(cfg.clip_start_sec)),
                    duration: Some(Duration::from_secs_f64(clip_duration)),
                    ..default()
                };
                run.entity = Some(commands.spawn((AudioPlayer::new(handle), settings)).id());
                run.waiting_since = Instant::now();
            }
            Ok(false)
        })();
        match result {
            Ok(false) => {
                runtime.runs.insert(anim.id, run);
            }
            Ok(true) => {
                stop_entity(&mut commands, &sinks, run.entity);
                track.status = AnimRunStatus::Finished;
                track.completed_cycles = track.completed_cycles.saturating_add(1);
                state.audio_status.insert(anim.id, "Selesai".into());
            }
            Err(error) => {
                stop_entity(&mut commands, &sinks, run.entity);
                // A failed cue must not leave a Sequence waiting forever.
                track.status = AnimRunStatus::Finished;
                track.completed_cycles = track.completed_cycles.saturating_add(1);
                state
                    .audio_status
                    .insert(anim.id, format!("Gagal: {error}"));
                state.status_banner = Some(format!("Play Suara '{}': {error}", anim.name));
            }
        }
    }
}
