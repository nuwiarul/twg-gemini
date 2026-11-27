use std::{
    collections::VecDeque,
    path::PathBuf,
    time::Instant,
};

use bevy::prelude::*;
use bevy_egui::egui;
use twg_shared::{
    models::{Anim, Marker, ModelAssetType},
    particle_anim::{ParticleAnimConfig, ParticleAnimKind, write_particle_anim_config},
    route_formation::{MultiRouteConfig, RouteFormationKind, write_multi_route_config},
};
use uuid::Uuid;

use crate::{
    map::camera::TacticalCamera,
    state::{AppState, PlaybackState, SelectedEntity},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BenchmarkScenarioKind {
    #[default]
    StaticOnly,
    AnimatedOnly,
    MixedStaticAnimated,
    FullCombined,
}

impl BenchmarkScenarioKind {
    pub const ALL: [Self; 4] = [
        Self::StaticOnly,
        Self::AnimatedOnly,
        Self::MixedStaticAnimated,
        Self::FullCombined,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::StaticOnly => "1. Static 3D Only",
            Self::AnimatedOnly => "2. Animated GLB (Play)",
            Self::MixedStaticAnimated => "3. Gabungan Static + Animated",
            Self::FullCombined => "4. Full (Static + Animated + Partikel + Rute)",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            Self::StaticOnly => "Static",
            Self::AnimatedOnly => "Animated",
            Self::MixedStaticAnimated => "Static+Anim",
            Self::FullCombined => "Full+Partikel+Rute",
        }
    }
}

#[derive(Debug, Clone)]
pub struct BenchmarkRecord {
    pub timestamp: String,
    pub scenario: String,
    pub count: usize,
    pub avg_fps: f32,
    pub low_1pct_fps: f32,
    pub avg_frame_ms: f32,
    pub ecs_entities: u32,
    pub anim_culling: bool,
    pub anim_cull_dist_m: f32,
    pub cam_dist_m: f32,
    pub passed_30fps: bool,
}

struct ScenarioBackup {
    markers: Vec<Marker>,
    anims: Vec<Anim>,
    offline_preview: bool,
    play_test_mode: bool,
}

#[derive(Resource)]
pub struct PerformanceBenchmarkState {
    pub panel_open: bool,
    pub show_corner_fps: bool,
    pub selected_scenario: BenchmarkScenarioKind,
    pub selected_count: usize,
    pub prefer_lod2: bool,
    /// Uniform scale multiplier applied on top of each model's auto-calibrated catalog scale.
    pub scale_multiplier: f32,
    last_applied_scale_mult: f32,
    /// Grid spacing between spawned benchmark markers in meters.
    pub spacing_m: f32,
    /// When `true` during Play/Benchmark, every animated GLB plays its clip immediately (even single-clip models).
    pub force_play_all_anims: bool,
    /// Live runtime optimization toggles (can be flipped mid-benchmark to compare before/after!)
    pub auto_lod_enabled: bool,
    pub lod_switch_distance_m: f32,
    pub strip_lod2_skinning: bool,
    pub lod_far_cull_enabled: bool,
    pub lod_far_cull_distance_m: f32,
    pub anim_culling_enabled: bool,
    pub anim_cull_distance_m: f32,
    pub lazy_walk_spawn: bool,
    /// Live LOD tier counters populated by `sync_markers_system`
    pub lod0_active_count: usize,
    pub lod2_active_count: usize,
    pub far_culled_count: usize,
    /// Rolling frame time window (seconds) for smooth FPS + 1% Low calculation.
    frame_dts: VecDeque<f32>,
    pub current_fps: f32,
    pub avg_fps: f32,
    pub low_1pct_fps: f32,
    pub avg_frame_ms: f32,
    pub ecs_entity_count: u32,
    pub active_bench_label: Option<String>,
    backup: Option<ScenarioBackup>,
    /// Timed 10-second measurement run
    measuring: bool,
    measure_started_at: Option<Instant>,
    measure_warmup_sec: f32,
    measure_duration_sec: f32,
    measure_samples: Vec<f32>,
    pub records: Vec<BenchmarkRecord>,
    pub last_export_msg: Option<String>,
    /// Pending UI actions processed inside `performance_benchmark_system`
    pub pending_spawn: Option<(BenchmarkScenarioKind, usize)>,
    pub pending_restore: bool,
    pub pending_start_measure: bool,
    pub pending_export_csv: bool,
    pub pending_camera_preset: Option<f64>,
}

impl Default for PerformanceBenchmarkState {
    fn default() -> Self {
        let saved = crate::settings::load_performance_settings();
        Self {
            panel_open: false,
            show_corner_fps: saved.show_corner_fps,
            selected_scenario: BenchmarkScenarioKind::StaticOnly,
            selected_count: 1000,
            prefer_lod2: true,
            scale_multiplier: 1.0,
            last_applied_scale_mult: 1.0,
            spacing_m: 8.0,
            force_play_all_anims: true,
            auto_lod_enabled: saved.auto_lod_enabled,
            lod_switch_distance_m: saved.lod_switch_distance_m,
            strip_lod2_skinning: saved.strip_lod2_skinning,
            lod_far_cull_enabled: saved.lod_far_cull_enabled,
            lod_far_cull_distance_m: saved.lod_far_cull_distance_m,
            anim_culling_enabled: saved.anim_culling_enabled,
            anim_cull_distance_m: saved.anim_cull_distance_m,
            lazy_walk_spawn: saved.lazy_walk_spawn,
            lod0_active_count: 0,
            lod2_active_count: 0,
            far_culled_count: 0,
            frame_dts: VecDeque::with_capacity(240),
            current_fps: 0.0,
            avg_fps: 0.0,
            low_1pct_fps: 0.0,
            avg_frame_ms: 0.0,
            ecs_entity_count: 0,
            active_bench_label: None,
            backup: None,
            measuring: false,
            measure_started_at: None,
            measure_warmup_sec: 2.0,
            measure_duration_sec: 10.0,
            measure_samples: Vec::new(),
            records: Vec::new(),
            last_export_msg: None,
            pending_spawn: None,
            pending_restore: false,
            pending_start_measure: false,
            pending_export_csv: false,
            pending_camera_preset: None,
        }
    }
}

impl PerformanceBenchmarkState {
    pub fn to_local_settings(&self) -> crate::settings::LocalPerformanceSettings {
        let mut s = crate::settings::LocalPerformanceSettings {
            preset: crate::settings::PerformancePreset::Custom,
            auto_lod_enabled: self.auto_lod_enabled,
            lod_switch_distance_m: self.lod_switch_distance_m,
            strip_lod2_skinning: self.strip_lod2_skinning,
            lod_far_cull_enabled: self.lod_far_cull_enabled,
            lod_far_cull_distance_m: self.lod_far_cull_distance_m,
            anim_culling_enabled: self.anim_culling_enabled,
            anim_cull_distance_m: self.anim_cull_distance_m,
            lazy_walk_spawn: self.lazy_walk_spawn,
            show_corner_fps: self.show_corner_fps,
        };
        s.preset = s.detect_matching_preset();
        s
    }

    pub fn apply_local_settings(&mut self, s: &crate::settings::LocalPerformanceSettings) {
        self.auto_lod_enabled = s.auto_lod_enabled;
        self.lod_switch_distance_m = s.lod_switch_distance_m;
        self.strip_lod2_skinning = s.strip_lod2_skinning;
        self.lod_far_cull_enabled = s.lod_far_cull_enabled;
        self.lod_far_cull_distance_m = s.lod_far_cull_distance_m;
        self.anim_culling_enabled = s.anim_culling_enabled;
        self.anim_cull_distance_m = s.anim_cull_distance_m;
        self.lazy_walk_spawn = s.lazy_walk_spawn;
        self.show_corner_fps = s.show_corner_fps;
    }

    pub fn persist_local_settings(&self) {
        let _ = crate::settings::save_performance_settings(&self.to_local_settings());
    }

    pub fn has_backup(&self) -> bool {
        self.backup.is_some()
    }

    pub fn measure_progress(&self) -> Option<(f32, bool)> {
        if !self.measuring {
            return None;
        }
        let elapsed = self
            .measure_started_at
            .map(|t| t.elapsed().as_secs_f32())
            .unwrap_or(0.0);
        if elapsed < self.measure_warmup_sec {
            Some((elapsed / self.measure_warmup_sec.max(0.1), true))
        } else {
            let rec_t = elapsed - self.measure_warmup_sec;
            Some(((rec_t / self.measure_duration_sec.max(1.0)).clamp(0.0, 1.0), false))
        }
    }
}

fn compute_fps_stats(dts: &[f32]) -> (f32, f32, f32) {
    if dts.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let sum: f32 = dts.iter().copied().sum();
    let avg_dt = (sum / dts.len() as f32).max(1e-5);
    let avg_fps = 1.0 / avg_dt;
    let avg_ms = avg_dt * 1000.0;

    let mut sorted = dts.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    // 1% worst frame times (top 1% slowest frames)
    let tail_count = ((sorted.len() as f32) * 0.01).ceil().max(1.0) as usize;
    let start = sorted.len().saturating_sub(tail_count);
    let worst_avg_dt =
        (sorted[start..].iter().copied().sum::<f32>() / tail_count as f32).max(1e-5);
    let low_1pct = 1.0 / worst_avg_dt;
    (avg_fps, low_1pct, avg_ms)
}

fn pick_benchmark_model_pools(
    state: &AppState,
    prefer_lod2: bool,
) -> (Vec<(String, String, f64)>, Vec<(String, String, f64)>) {
    let mut static_pool: Vec<(String, String, f64)> = Vec::new();
    let mut anim_pool: Vec<(String, String, f64)> = Vec::new();

    for entry in &state.catalog {
        let chosen_path = if prefer_lod2 {
            entry
                .lod2_path
                .as_deref()
                .filter(|p| !p.trim().is_empty())
                .unwrap_or(&entry.model_path)
        } else {
            &entry.model_path
        };
        let norm = chosen_path.trim().replace('\\', "/");
        if norm.is_empty() {
            continue;
        }
        let exists = state.available_model_paths.contains(&norm)
            || state.assets_root.join(&norm).exists();
        if !exists {
            continue;
        }
        // Use the exact same auto-calibrated Blender/GLB drop scale as `spawn_marker_from_catalog`!
        let scale = crate::scene::markers::compute_initial_spawn_scale(&state.assets_root, &norm);
        if entry.model_type == ModelAssetType::Animated || entry.walk_model_path.is_some() {
            anim_pool.push((norm, entry.category.clone(), scale));
        } else {
            static_pool.push((norm, entry.category.clone(), scale));
        }
    }

    // Fallback scan of available_model_paths if catalog hasn't been loaded
    if static_pool.is_empty() || anim_pool.is_empty() {
        for p in &state.available_model_paths {
            let lower = p.to_ascii_lowercase();
            if !lower.ends_with(".glb") || lower.contains("_act_") || lower.contains("_walk") {
                continue;
            }
            let scale = crate::scene::markers::compute_initial_spawn_scale(&state.assets_root, p);
            if lower.contains("_idle") || lower.contains("_anim_") {
                if anim_pool.len() < 8 {
                    anim_pool.push((p.clone(), "units".into(), scale));
                }
            } else if static_pool.len() < 8 {
                static_pool.push((p.clone(), "kendaraan".into(), scale));
            }
        }
    }

    // Limit variety to top 6 distinct models per pool so GPU instancing reflects realistic battalions
    static_pool.truncate(6);
    anim_pool.truncate(6);
    (static_pool, anim_pool)
}

fn spawn_benchmark_scenario(
    bench: &mut PerformanceBenchmarkState,
    state: &mut AppState,
    playback: &mut PlaybackState,
    tcam: &mut TacticalCamera,
    kind: BenchmarkScenarioKind,
    count: usize,
) {
    // Save original scenario once before overwriting locally
    if bench.backup.is_none() {
        bench.backup = Some(ScenarioBackup {
            markers: state.markers.clone(),
            anims: state.anims.clone(),
            offline_preview: state.offline_preview,
            play_test_mode: playback.play_test_mode,
        });
    }

    // When Auto Distance LOD is enabled, always spawn markers with their real LOD0 `model_path`
    // so Dynamic LOD automatically renders LOD0 (< 120m) and zero-skinning `*_idle_lod2.glb` (>= 120m)!
    let force_static_lod2_path = !bench.auto_lod_enabled && bench.prefer_lod2;
    let (static_pool, anim_pool) = pick_benchmark_model_pools(state, force_static_lod2_path);
    let fallback_static = static_pool
        .first()
        .cloned()
        .or_else(|| anim_pool.first().cloned())
        .unwrap_or_else(|| ("models/missing.glb".into(), "units".into(), 1.0));
    let fallback_anim = anim_pool
        .first()
        .cloned()
        .or_else(|| static_pool.first().cloned())
        .unwrap_or_else(|| ("models/missing.glb".into(), "units".into(), 1.0));

    let sk_id = state
        .active_skenario
        .as_ref()
        .map(|s| s.id)
        .unwrap_or_else(Uuid::new_v4);
    let op_id = state
        .active_skenario
        .as_ref()
        .map(|s| s.operasi_id)
        .unwrap_or_else(Uuid::new_v4);

    let cols = (count as f64).sqrt().ceil().max(1.0) as usize;
    let rows = count.div_ceil(cols);
    let spacing_m = (bench.spacing_m as f64).clamp(2.0, 50.0);
    let user_scale_mult = (bench.scale_multiplier as f64).clamp(0.05, 5.0);
    bench.last_applied_scale_mult = bench.scale_multiplier;

    let now = chrono::Utc::now();
    let mut new_markers: Vec<Marker> = Vec::with_capacity(count);
    let mut new_anims: Vec<Anim> = Vec::new();
    let mut animated_ids: Vec<Uuid> = Vec::new();

    for i in 0..count {
        let r = i / cols;
        let c = i % cols;
        let dx = (c as f64 - (cols as f64 - 1.0) * 0.5) * spacing_m;
        let dz = (r as f64 - (rows as f64 - 1.0) * 0.5) * spacing_m;
        let (lng, lat, _) = state.geo_anchor.local_to_lng_lat_alt([
            tcam.target_local.x + dx as f32,
            0.0,
            tcam.target_local.y + dz as f32,
        ]);

        let is_anim = match kind {
            BenchmarkScenarioKind::StaticOnly => false,
            BenchmarkScenarioKind::AnimatedOnly => true,
            BenchmarkScenarioKind::MixedStaticAnimated | BenchmarkScenarioKind::FullCombined => {
                i % 2 == 1
            }
        };

        let (model_path, category, def_scale) = if is_anim {
            anim_pool
                .get(i % anim_pool.len().max(1))
                .cloned()
                .unwrap_or_else(|| fallback_anim.clone())
        } else {
            static_pool
                .get(i % static_pool.len().max(1))
                .cloned()
                .unwrap_or_else(|| fallback_static.clone())
        };
        let final_scale = (def_scale * user_scale_mult).max(0.002);

        let id = Uuid::new_v4();
        if is_anim {
            animated_ids.push(id);
        }

        new_markers.push(Marker {
            id,
            operasi_id: op_id,
            skenario_id: sk_id,
            unit_id: format!("bench_{:05}", i + 1),
            name: format!("Bench #{:05}", i + 1),
            category,
            model_path,
            quantity: 1,
            lng,
            lat,
            altitude: 0.0,
            rot_x: 0.0,
            rot_y: ((i * 37) % 360) as f64,
            rot_z: 0.0,
            scale_x: final_scale,
            scale_y: final_scale,
            scale_z: final_scale,
            is_spawn: true,
            description: None,
            metadata: serde_json::json!({}),
            created_by: None,
            allowed_user_ids: Vec::new(),
            created_at: now,
            updated_at: now,
        });
    }

    // For FullCombined: add active multi-route formations + batched particle effects!
    if kind == BenchmarkScenarioKind::FullCombined {
        // 1. Multi-route formations covering up to 400 animated units moving continuously
        let route_unit_budget = animated_ids.len().min(400);
        let units_per_route = 40usize;
        let num_routes = route_unit_budget.div_ceil(units_per_route);
        for r_idx in 0..num_routes {
            let start_i = r_idx * units_per_route;
            let end_i = (start_i + units_per_route).min(route_unit_budget);
            let slice = animated_ids[start_i..end_i].to_vec();
            if slice.is_empty() {
                continue;
            }
            let y_off = ((r_idx as f32) - (num_routes as f32) * 0.5) * 28.0;
            let (p0_lng, p0_lat, _) = state.geo_anchor.local_to_lng_lat_alt([
                tcam.target_local.x - 120.0,
                0.0,
                tcam.target_local.y + y_off,
            ]);
            let (p1_lng, p1_lat, _) = state.geo_anchor.local_to_lng_lat_alt([
                tcam.target_local.x + 120.0,
                0.0,
                tcam.target_local.y + y_off,
            ]);
            let mut anim = Anim {
                id: Uuid::new_v4(),
                operasi_id: op_id,
                skenario_id: sk_id,
                marker_id: slice.first().copied(),
                name: format!("Bench Rute #{}", r_idx + 1),
                anim_type: "route_multi".into(),
                sort_order: (new_anims.len() + 1) as i32,
                speed: 5.0,
                duration_sec: 48.0,
                replay: true,
                autoplay: true,
                geom: vec![[p0_lng, p0_lat, 0.0], [p1_lng, p1_lat, 0.0]],
                data: serde_json::json!({}),
                created_by: None,
                allowed_user_ids: Vec::new(),
                created_at: now,
                updated_at: now,
            };
            let mcfg = MultiRouteConfig {
                marker_ids: slice,
                formation: RouteFormationKind::Grid,
                spacing_x_m: 3.5,
                spacing_z_m: 3.5,
                columns: 8,
                ..Default::default()
            };
            write_multi_route_config(&mut anim, &mcfg);
            new_anims.push(anim);
        }

        // 2. Active Fire, Smoke, and Explosion particle effects scaled with test tier
        let particle_count = match count {
            0..=1500 => 30,
            1501..=6000 => 80,
            _ => 150,
        };
        let p_cols = (particle_count as f32).sqrt().ceil().max(1.0) as usize;
        for p_idx in 0..particle_count {
            let pr = p_idx / p_cols;
            let pc = p_idx % p_cols;
            let px = (pc as f32 - p_cols as f32 * 0.5) * 26.0;
            let pz = (pr as f32 - p_cols as f32 * 0.5) * 26.0;
            let (plng, plat, _) = state.geo_anchor.local_to_lng_lat_alt([
                tcam.target_local.x + px,
                0.0,
                tcam.target_local.y + pz,
            ]);
            let pkind = match p_idx % 3 {
                0 => ParticleAnimKind::Fire,
                1 => ParticleAnimKind::Smoke,
                _ => ParticleAnimKind::Explosion,
            };
            let mut pcfg = ParticleAnimConfig::default_for(pkind);
            pcfg.continuous = true;
            let mut anim = Anim {
                id: Uuid::new_v4(),
                operasi_id: op_id,
                skenario_id: sk_id,
                marker_id: None,
                name: format!("Bench Partikel #{}", p_idx + 1),
                anim_type: pkind.anim_type_str().into(),
                sort_order: (new_anims.len() + 1) as i32,
                speed: 1.0,
                duration_sec: 4.5,
                replay: true,
                autoplay: true,
                geom: vec![[plng, plat, 0.0]],
                data: serde_json::json!({}),
                created_by: None,
                allowed_user_ids: Vec::new(),
                created_at: now,
                updated_at: now,
            };
            write_particle_anim_config(&mut anim, &pcfg);
            new_anims.push(anim);
        }
    }

    // Apply locally in offline-safe mode so server DB is never touched
    state.offline_preview = true;
    state.selected = SelectedEntity::None;
    state.marker_edit = None;
    state.anim_edit = None;
    state.markers = new_markers;
    state.anims = new_anims;
    state.dirty_marker_ids.clear();
    state.dirty_anim_ids.clear();

    // Auto-enter Play mode when testing animated / combined scenarios
    playback.stop_play_test();
    if kind != BenchmarkScenarioKind::StaticOnly {
        playback.play_test_mode = true;
        playback.playing = true;
        if !state.anims.is_empty() {
            playback.play_all_routes(&state.anims);
        }
    }

    // Frame camera nicely to cover the spawned battalion
    let span_m = (cols as f64 * spacing_m).max(80.0);
    tcam.distance_m = (span_m * 0.85).clamp(90.0, 900.0) as f32;
    tcam.pitch_deg = 44.0;

    bench.active_bench_label = Some(format!("{} × {}", kind.short_label(), count));
    bench.frame_dts.clear();
    state.status_banner = Some(format!(
        "Benchmark aktif: {} ({} objek 3D, {} animasi) — mode lokal aman (DB tidak berubah).",
        kind.label(),
        count,
        state.anims.len()
    ));
}

fn export_benchmark_csv(records: &[BenchmarkRecord]) -> Result<PathBuf, String> {
    let dir = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("benchmarks");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Gagal membuat folder benchmarks/: {e}"))?;
    let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let path = dir.join(format!("twg_benchmark_{ts}.csv"));
    let mut csv = String::from(
        "timestamp,scenario,object_count,avg_fps,low_1pct_fps,avg_frame_ms,ecs_entities,anim_culling,cull_dist_m,cam_dist_m,pass_30fps\n",
    );
    for r in records {
        csv.push_str(&format!(
            "{},{},{},{:.2},{:.2},{:.2},{},{},{:.0},{:.1},{}\n",
            r.timestamp,
            r.scenario,
            r.count,
            r.avg_fps,
            r.low_1pct_fps,
            r.avg_frame_ms,
            r.ecs_entities,
            r.anim_culling,
            r.anim_cull_dist_m,
            r.cam_dist_m,
            if r.passed_30fps { "PASS" } else { "BELOW_30" }
        ));
    }
    std::fs::write(&path, csv).map_err(|e| format!("Gagal menulis CSV: {e}"))?;
    Ok(path)
}

#[allow(clippy::too_many_arguments)]
pub fn performance_benchmark_system(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    entities: &bevy::ecs::entity::Entities,
    mut bench: ResMut<PerformanceBenchmarkState>,
    mut state: ResMut<AppState>,
    mut playback: ResMut<PlaybackState>,
    mut tcam_q: Query<&mut TacticalCamera>,
) {
    // Hotkey F9 toggles Benchmark & Performance HUD anytime on 3D map screens
    if state.is_3d_map_screen() && keys.just_pressed(KeyCode::F9) {
        bench.panel_open = !bench.panel_open;
    }

    let dt = time.delta_secs().clamp(0.0001, 0.5);
    bench.current_fps = 1.0 / dt;
    if bench.frame_dts.len() >= 180 {
        bench.frame_dts.pop_front();
    }
    bench.frame_dts.push_back(dt);
    bench.ecs_entity_count = entities.len();

    let slice: Vec<f32> = bench.frame_dts.iter().copied().collect();
    let (avg_fps, low_1pct, avg_ms) = compute_fps_stats(&slice);
    bench.avg_fps = avg_fps;
    bench.low_1pct_fps = low_1pct;
    bench.avg_frame_ms = avg_ms;

    // Handle timed 10-second benchmark capture
    if bench.measuring {
        let elapsed = bench
            .measure_started_at
            .map(|t| t.elapsed().as_secs_f32())
            .unwrap_or(0.0);
        if elapsed >= bench.measure_warmup_sec {
            bench.measure_samples.push(dt);
        }
        if elapsed >= bench.measure_warmup_sec + bench.measure_duration_sec {
            bench.measuring = false;
            let (m_avg, m_low, m_ms) = compute_fps_stats(&bench.measure_samples);
            let cam_dist = tcam_q.single().map(|c| c.distance_m).unwrap_or(0.0);
            let label = bench
                .active_bench_label
                .clone()
                .unwrap_or_else(|| "Skenario Aktif".into());
            let rec = BenchmarkRecord {
                timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                scenario: label,
                count: state.markers.len(),
                avg_fps: m_avg,
                low_1pct_fps: m_low,
                avg_frame_ms: m_ms,
                ecs_entities: bench.ecs_entity_count,
                anim_culling: bench.anim_culling_enabled,
                anim_cull_dist_m: bench.anim_cull_distance_m,
                cam_dist_m: cam_dist,
                passed_30fps: m_avg >= 30.0,
            };
            state.status_banner = Some(format!(
                "Hasil Benchmark ({} objek): Avg {:.1} FPS | 1% Low {:.1} FPS | {:.1} ms ({})",
                rec.count,
                rec.avg_fps,
                rec.low_1pct_fps,
                rec.avg_frame_ms,
                if rec.passed_30fps {
                    "PASS >= 30 FPS"
                } else {
                    "DI BAWAH 30 FPS"
                }
            ));
            bench.records.push(rec);
            bench.measure_samples.clear();
        }
    }

    if bench.pending_start_measure {
        bench.pending_start_measure = false;
        bench.measuring = true;
        bench.measure_started_at = Some(Instant::now());
        bench.measure_samples.clear();
    }

    if bench.pending_export_csv {
        bench.pending_export_csv = false;
        match export_benchmark_csv(&bench.records) {
            Ok(p) => {
                bench.last_export_msg = Some(format!("Tersimpan: {}", p.display()));
                state.status_banner = Some(format!("Laporan benchmark disimpan: {}", p.display()));
            }
            Err(e) => {
                bench.last_export_msg = Some(e.clone());
                state.status_banner = Some(e);
            }
        }
    }

    if let Some(dist) = bench.pending_camera_preset.take()
        && let Ok(mut tcam) = tcam_q.single_mut()
    {
        tcam.distance_m = dist as f32;
        tcam.pitch_deg = if dist > 350.0 { 54.0 } else { 36.0 };
    }

    if let Some((kind, count)) = bench.pending_spawn.take()
        && let Ok(mut tcam) = tcam_q.single_mut()
    {
        spawn_benchmark_scenario(&mut bench, &mut state, &mut playback, &mut tcam, kind, count);
    }

    // Live-rescale already-spawned benchmark markers when the user adjusts the Scale slider
    if bench.active_bench_label.is_some()
        && (bench.scale_multiplier - bench.last_applied_scale_mult).abs() > 1e-3
    {
        let ratio = (bench.scale_multiplier / bench.last_applied_scale_mult.max(0.05)) as f64;
        for m in &mut state.markers {
            m.scale_x = (m.scale_x * ratio).max(0.002);
            m.scale_y = (m.scale_y * ratio).max(0.002);
            m.scale_z = (m.scale_z * ratio).max(0.002);
        }
        bench.last_applied_scale_mult = bench.scale_multiplier;
    }

    if bench.pending_restore {
        bench.pending_restore = false;
        if let Some(bak) = bench.backup.take() {
            playback.stop_play_test();
            state.markers = bak.markers;
            state.anims = bak.anims;
            state.offline_preview = bak.offline_preview;
            playback.play_test_mode = bak.play_test_mode;
            state.selected = SelectedEntity::None;
            state.marker_edit = None;
            state.anim_edit = None;
            bench.active_bench_label = None;
            bench.measuring = false;
            state.status_banner = Some("Skenario asli dipulihkan dari backup lokal.".into());
        }
    }
}

/// Renders the toggleable `F9` Stress Test & Performance Benchmark window (plus optional compact FPS pill).
pub fn draw_benchmark_overlay(
    ctx: &egui::Context,
    clip_rect: egui::Rect,
    bench: &mut PerformanceBenchmarkState,
    state: &AppState,
    tcam: &TacticalCamera,
) {
    if !state.is_3d_map_screen() {
        return;
    }

    let fps_col = if bench.avg_fps >= 30.0 {
        egui::Color32::from_rgb(74, 222, 128)
    } else if bench.avg_fps >= 20.0 {
        egui::Color32::from_rgb(251, 191, 36)
    } else {
        egui::Color32::from_rgb(248, 113, 113)
    };

    if bench.show_corner_fps && !bench.panel_open {
        egui::Area::new(egui::Id::new("twg_mini_fps_pill"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(clip_rect.right() - 186.0, clip_rect.top() + 12.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(egui::Color32::from_rgba_unmultiplied(10, 16, 28, 215))
                    .stroke(egui::Stroke::new(1.0, fps_col))
                    .corner_radius(6.0)
                    .inner_margin(egui::Margin::symmetric(8, 4))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{:.1} FPS ({:.1} ms)",
                                    bench.avg_fps, bench.avg_frame_ms
                                ))
                                .size(11.0)
                                .strong()
                                .color(fps_col),
                            );
                            if ui.small_button("F9").clicked() {
                                bench.panel_open = true;
                            }
                        });
                    });
            });
    }

    if !bench.panel_open {
        return;
    }

    let mut open = bench.panel_open;
    egui::Window::new("Benchmark & Uji Performa 3D (F9)")
        .open(&mut open)
        .default_width(430.0)
        .default_pos(egui::pos2(clip_rect.right() - 450.0, clip_rect.top() + 16.0))
        .resizable(true)
        .collapsible(true)
        .show(ctx, |ui| {
            // 1. Live FPS & Telemetry Card
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(13, 21, 34))
                .stroke(egui::Stroke::new(1.2, fps_col))
                .corner_radius(6.0)
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!("{:.1} FPS", bench.avg_fps))
                                .size(22.0)
                                .strong()
                                .color(fps_col),
                        );
                        ui.add_space(6.0);
                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new(format!(
                                    "1% Low: {:.1} FPS  •  Frame: {:.1} ms (Target <= 33.3 ms)",
                                    bench.low_1pct_fps, bench.avg_frame_ms
                                ))
                                .size(10.5)
                                .color(egui::Color32::from_rgb(203, 213, 225)),
                            );
                            ui.label(
                                egui::RichText::new(format!(
                                    "Objek: {} (LOD0: {} | LOD2: {}{})  •  ECS: {}  •  Cam: {:.0}m",
                                    state.markers.len(),
                                    bench.lod0_active_count,
                                    bench.lod2_active_count,
                                    if bench.far_culled_count > 0 {
                                        format!(" | Cull: {}", bench.far_culled_count)
                                    } else {
                                        String::new()
                                    },
                                    bench.ecs_entity_count,
                                    tcam.distance_m
                                ))
                                .size(9.8)
                                .color(egui::Color32::from_rgb(148, 163, 184)),
                            );
                        });
                    });
                    #[cfg(debug_assertions)]
                    {
                        ui.add_space(3.0);
                        ui.label(
                            egui::RichText::new(
                                "Perhatian: sedang jalan di build DEBUG. Untuk angka FPS asli jalankan `cargo run --release`!",
                            )
                            .size(9.5)
                            .color(egui::Color32::from_rgb(251, 191, 36)),
                        );
                    }
                });

            ui.add_space(6.0);

            // 2. Stress Test Spawner (1.000 / 5.000 / 10.000)
            ui.label(
                egui::RichText::new("Skenario Stress Test (Offline Aman — Tidak Ubah DB)")
                    .size(11.5)
                    .strong()
                    .color(egui::Color32::from_rgb(186, 230, 253)),
            );
            ui.add_space(3.0);

            egui::ComboBox::from_id_salt("bench_scenario_cb")
                .width(ui.available_width())
                .selected_text(bench.selected_scenario.label())
                .show_ui(ui, |ui| {
                    for s in BenchmarkScenarioKind::ALL {
                        ui.selectable_value(&mut bench.selected_scenario, s, s.label());
                    }
                });

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Jumlah:").size(10.5));
                for cnt in [500usize, 1000, 5000, 10000] {
                    let sel = bench.selected_count == cnt;
                    if ui
                        .selectable_label(sel, format!("{cnt}"))
                        .clicked()
                    {
                        bench.selected_count = cnt;
                    }
                }
                ui.checkbox(&mut bench.prefer_lod2, "LOD2");
            });

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Skala Model:").size(10.5));
                ui.add(
                    egui::Slider::new(&mut bench.scale_multiplier, 0.1..=4.0)
                        .suffix("×")
                        .fixed_decimals(2),
                );
                if ui.small_button("1×").clicked() {
                    bench.scale_multiplier = 1.0;
                }
            });
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Jarak Marker:").size(10.5));
                ui.add(
                    egui::Slider::new(&mut bench.spacing_m, 2.0..=30.0)
                        .suffix(" m")
                        .fixed_decimals(1),
                );
            });
            ui.checkbox(
                &mut bench.force_play_all_anims,
                "Langsung putar animasi semua model Animated GLB saat Spawn",
            );

            ui.add_space(5.0);
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new(format!(
                                "Spawn & Jalankan ({})",
                                bench.selected_count
                            ))
                            .size(11.0)
                            .strong()
                            .color(egui::Color32::WHITE),
                        )
                        .fill(egui::Color32::from_rgb(14, 116, 144)),
                    )
                    .clicked()
                {
                    bench.pending_spawn = Some((bench.selected_scenario, bench.selected_count));
                }

                if bench.has_backup()
                    && ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("Pulihkan Skenario Asli")
                                    .size(10.5)
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            )
                            .fill(egui::Color32::from_rgb(185, 28, 28)),
                        )
                        .clicked()
                {
                    bench.pending_restore = true;
                }
            });

            ui.add_space(5.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Kamera:").size(10.0));
                if ui.small_button("Close-Up (60m)").clicked() {
                    bench.pending_camera_preset = Some(60.0);
                }
                if ui.small_button("Taktis (220m)").clicked() {
                    bench.pending_camera_preset = Some(220.0);
                }
                if ui.small_button("Overview (550m)").clicked() {
                    bench.pending_camera_preset = Some(550.0);
                }
            });

            ui.add_space(6.0);
            ui.separator();
            ui.add_space(4.0);

            // 3. Live Runtime Optimization Switches (auto-saved to local performance-settings.json)
            ui.label(
                egui::RichText::new("Optimasi Runtime Aktif (LOD0 / LOD2 / Zero-Skinning)")
                    .size(11.0)
                    .strong()
                    .color(egui::Color32::from_rgb(110, 231, 183)),
            );
            let mut perf_changed = false;
            perf_changed |= ui
                .checkbox(
                    &mut bench.auto_lod_enabled,
                    "Auto Distance LOD (Dekat = LOD0 penuh, Jauh = *_idle_lod2.glb)",
                )
                .changed();
            if bench.auto_lod_enabled {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Batas LOD0 -> LOD2:").size(10.0));
                    perf_changed |= ui
                        .add(
                            egui::Slider::new(&mut bench.lod_switch_distance_m, 40.0..=400.0)
                                .suffix(" m")
                                .fixed_decimals(0),
                        )
                        .changed();
                });
                perf_changed |= ui
                    .checkbox(
                        &mut bench.strip_lod2_skinning,
                        "Tanpa GPU Skinning di LOD2 (Instancing mesh statis)",
                    )
                    .changed();
            }
            perf_changed |= ui
                .checkbox(
                    &mut bench.lod_far_cull_enabled,
                    "Sembunyikan unit sangat jauh (Far Cull)",
                )
                .changed();
            if bench.lod_far_cull_enabled {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Batas Far Cull:").size(10.0));
                    perf_changed |= ui
                        .add(
                            egui::Slider::new(&mut bench.lod_far_cull_distance_m, 200.0..=1500.0)
                                .suffix(" m")
                                .fixed_decimals(0),
                        )
                        .changed();
                });
            }
            perf_changed |= ui
                .checkbox(
                    &mut bench.anim_culling_enabled,
                    "Distance Animation Culling (Pause animasi LOD0 jauh)",
                )
                .changed();
            if bench.anim_culling_enabled {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Batas Jarak Animasi:").size(10.0));
                    perf_changed |= ui
                        .add(
                            egui::Slider::new(&mut bench.anim_cull_distance_m, 40.0..=1500.0)
                                .suffix(" m")
                                .fixed_decimals(0),
                        )
                        .changed();
                });
            }
            perf_changed |= ui
                .checkbox(
                    &mut bench.lazy_walk_spawn,
                    "Lazy-Spawn Rig Walk (Hemat 50% tulang saat unit diam)",
                )
                .changed();
            perf_changed |= ui
                .checkbox(
                    &mut bench.show_corner_fps,
                    "Tampilkan Mini FPS di Pojok Kanan Atas saat panel ditutup",
                )
                .changed();
            if perf_changed {
                bench.persist_local_settings();
            }

            ui.add_space(6.0);
            ui.separator();
            ui.add_space(4.0);

            // 4. Timed 10s Measurement & CSV Export
            ui.horizontal(|ui| {
                if let Some((prog, warmup)) = bench.measure_progress() {
                    let lbl = if warmup {
                        format!("Warm-up... {:.0}%", prog * 100.0)
                    } else {
                        format!("Mengukur 10 dtk... {:.0}%", prog * 100.0)
                    };
                    ui.add(egui::ProgressBar::new(prog).text(lbl));
                } else {
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("Mulai Ukur (10 dtk)")
                                    .size(10.5)
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            )
                            .fill(egui::Color32::from_rgb(5, 150, 105)),
                        )
                        .clicked()
                    {
                        bench.pending_start_measure = true;
                    }
                    if !bench.records.is_empty() && ui.small_button("Export CSV").clicked() {
                        bench.pending_export_csv = true;
                    }
                    if !bench.records.is_empty() && ui.small_button("Bersihkan").clicked() {
                        bench.records.clear();
                    }
                }
            });

            if let Some(msg) = &bench.last_export_msg {
                ui.label(
                    egui::RichText::new(msg)
                        .size(9.5)
                        .color(egui::Color32::from_rgb(134, 239, 172)),
                );
            }

            if !bench.records.is_empty() {
                ui.add_space(4.0);
                egui::ScrollArea::vertical()
                    .id_salt("bench_records_scroll")
                    .max_height(130.0)
                    .show(ui, |ui| {
                        for r in bench.records.iter().rev() {
                            let badge_col = if r.passed_30fps {
                                egui::Color32::from_rgb(74, 222, 128)
                            } else {
                                egui::Color32::from_rgb(248, 113, 113)
                            };
                            ui.label(
                                egui::RichText::new(format!(
                                    "[{}] {} ({}) -> Avg {:.1} FPS | 1% Low {:.1} | {:.1}ms | {} ent",
                                    r.timestamp,
                                    r.scenario,
                                    r.count,
                                    r.avg_fps,
                                    r.low_1pct_fps,
                                    r.avg_frame_ms,
                                    r.ecs_entities
                                ))
                                .size(9.8)
                                .color(badge_col),
                            );
                        }
                    });
            }
        });
    bench.panel_open = open;
}
