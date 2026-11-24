use std::f64::consts::TAU;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    geo::GeoAnchor,
    models::{Anim, Marker},
};

pub const ANIM_TYPE_ROUTE_SINGLE: &str = "route";
pub const ANIM_TYPE_ROUTE_MULTI: &str = "route_multi";

/// Tactical formation presets supported by `Rute Unit Multiple`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteFormationKind {
    /// Rows × Columns grid centered on the route axis (e.g., 5 units with 4 cols = 4 front + 1 back).
    #[default]
    Grid,
    /// Free / Custom formation where each unit keeps its own local `(lateral_m, longitudinal_m)` offset.
    Custom,
    /// Tactical V / Arrowhead formation with Unit #1 at the point.
    Wedge,
    /// Two parallel columns staggered front-to-back (street patrol formation).
    Staggered,
    /// Unit #1 (VIP / Commander) centered with remaining units forming a protective perimeter ring.
    VipEscort,
}

impl RouteFormationKind {
    pub const ALL: &'static [Self] = &[
        Self::Grid,
        Self::Custom,
        Self::Wedge,
        Self::Staggered,
        Self::VipEscort,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Grid => "Barisan Grid (Kolom × Baris)",
            Self::Custom => "Bebas / Kustom",
            Self::Wedge => "Panah / V (Wedge)",
            Self::Staggered => "Zig-Zag (Staggered)",
            Self::VipEscort => "Kawal VIP (Lingkaran)",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            Self::Grid => "Barisan Grid",
            Self::Custom => "Bebas / Kustom",
            Self::Wedge => "Panah / V",
            Self::Staggered => "Zig-Zag",
            Self::VipEscort => "Kawal VIP",
        }
    }
}

/// How the formation handles bends and corners along the route polyline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormationCornerMode {
    /// Each unit samples the route at `s + longitudinal_m` and offsets laterally along that curve,
    /// keeping rear rows neatly inside road turns instead of swinging wide into buildings.
    #[default]
    FollowPath,
    /// All units rotate rigidly around the formation center `(s)` as a single solid frame.
    Rigid,
}

impl FormationCornerMode {
    pub const ALL: &'static [Self] = &[Self::FollowPath, Self::Rigid];

    pub fn label(self) -> &'static str {
        match self {
            Self::FollowPath => "Ikuti Jalan (Follow-Path)",
            Self::Rigid => "Kerangka Kaku (Rigid)",
        }
    }
}

/// Relative local offset of a single unit in `RouteFormationKind::Custom`.
/// Coordinates are always expressed relative to the route's travel direction:
/// - `lateral_m > 0`: right side of travel direction, `< 0`: left side
/// - `longitudinal_m > 0`: ahead of center, `< 0`: behind center
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomUnitOffset {
    pub marker_id: Uuid,
    pub lateral_m: f64,
    pub longitudinal_m: f64,
}

fn default_columns() -> u32 {
    3
}
fn default_spacing_m() -> f64 {
    3.0
}
fn default_vip_radius_m() -> f64 {
    4.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MultiRouteConfig {
    #[serde(default)]
    pub marker_ids: Vec<Uuid>,
    #[serde(default)]
    pub formation: RouteFormationKind,
    #[serde(default)]
    pub corner_mode: FormationCornerMode,
    #[serde(default = "default_columns")]
    pub columns: u32,
    #[serde(default = "default_spacing_m")]
    pub spacing_x_m: f64,
    #[serde(default = "default_spacing_m")]
    pub spacing_z_m: f64,
    #[serde(default = "default_vip_radius_m")]
    pub vip_radius_m: f64,
    #[serde(default)]
    pub custom_offsets: Vec<CustomUnitOffset>,
}

impl Default for MultiRouteConfig {
    fn default() -> Self {
        Self {
            marker_ids: Vec::new(),
            formation: RouteFormationKind::Grid,
            corner_mode: FormationCornerMode::FollowPath,
            columns: default_columns(),
            spacing_x_m: default_spacing_m(),
            spacing_z_m: default_spacing_m(),
            vip_radius_m: default_vip_radius_m(),
            custom_offsets: Vec::new(),
        }
    }
}

impl MultiRouteConfig {
    pub fn sanitize(&mut self) {
        self.columns = self.columns.clamp(1, 64);
        self.spacing_x_m = if self.spacing_x_m.is_finite() {
            self.spacing_x_m.clamp(0.2, 200.0)
        } else {
            default_spacing_m()
        };
        self.spacing_z_m = if self.spacing_z_m.is_finite() {
            self.spacing_z_m.clamp(0.2, 200.0)
        } else {
            default_spacing_m()
        };
        self.vip_radius_m = if self.vip_radius_m.is_finite() {
            self.vip_radius_m.clamp(0.5, 200.0)
        } else {
            default_vip_radius_m()
        };
        // Deduplicate marker_ids while preserving exact order
        let mut seen = std::collections::HashSet::new();
        self.marker_ids.retain(|id| seen.insert(*id));
        // Keep custom_offsets clean and valid
        self.custom_offsets
            .retain(|o| self.marker_ids.contains(&o.marker_id));
        for o in &mut self.custom_offsets {
            if !o.lateral_m.is_finite() {
                o.lateral_m = 0.0;
            }
            if !o.longitudinal_m.is_finite() {
                o.longitudinal_m = 0.0;
            }
            o.lateral_m = o.lateral_m.clamp(-250.0, 250.0);
            o.longitudinal_m = o.longitudinal_m.clamp(-250.0, 250.0);
        }
    }

    /// Computes local `(lateral_m, longitudinal_m)` offset for unit at `index` (`marker_id`)
    /// among `total` units using the specified formation kind.
    pub fn offset_for_kind(&self, kind: RouteFormationKind, index: usize, total: usize) -> (f64, f64) {
        let sx = self.spacing_x_m.clamp(0.2, 200.0);
        let sz = self.spacing_z_m.clamp(0.2, 200.0);
        match kind {
            RouteFormationKind::Grid => {
                let cols = self.columns.max(1) as usize;
                let row = index / cols;
                let col = index % cols;
                let row_start = row * cols;
                let items_in_row = total.saturating_sub(row_start).min(cols).max(1);
                let lat = (col as f64 - (items_in_row as f64 - 1.0) * 0.5) * sx;
                let lon = -(row as f64) * sz;
                (lat, lon)
            }
            RouteFormationKind::Wedge => {
                if index == 0 {
                    (0.0, 0.0)
                } else {
                    let rank = ((index - 1) / 2 + 1) as f64;
                    let side = if index % 2 == 1 { -1.0 } else { 1.0 };
                    (side * rank * sx, -rank * sz)
                }
            }
            RouteFormationKind::Staggered => {
                let side = if index.is_multiple_of(2) { -0.5 } else { 0.5 };
                let lat = side * sx;
                let lon = -(index as f64) * (sz * 0.5);
                (lat, lon)
            }
            RouteFormationKind::VipEscort => {
                if index == 0 {
                    (0.0, 0.0)
                } else {
                    let guards = total.saturating_sub(1).max(1);
                    let r = self.vip_radius_m.clamp(0.5, 200.0);
                    let theta = TAU * ((index - 1) as f64) / (guards as f64);
                    // theta=0 -> directly ahead (+longitudinal); clockwise -> +lateral (right)
                    (r * theta.sin(), r * theta.cos())
                }
            }
            RouteFormationKind::Custom => {
                if let Some(mid) = self.marker_ids.get(index)
                    && let Some(entry) = self.custom_offsets.iter().find(|o| o.marker_id == *mid)
                {
                    return (entry.lateral_m, entry.longitudinal_m);
                }
                self.offset_for_kind(RouteFormationKind::Grid, index, total)
            }
        }
    }

    /// Computes local `(lateral_m, longitudinal_m)` offset for every unit in `self.marker_ids`.
    pub fn compute_all_offsets(&self) -> Vec<(Uuid, f64, f64)> {
        let total = self.marker_ids.len();
        self.marker_ids
            .iter()
            .enumerate()
            .map(|(i, &mid)| {
                let (lat, lon) = self.offset_for_kind(self.formation, i, total);
                (mid, lat, lon)
            })
            .collect()
    }

    /// Ensures every unit in `self.marker_ids` has an explicit entry in `self.custom_offsets`,
    /// initializing missing units from `fallback_kind`.
    pub fn ensure_custom_offsets_seeded(&mut self, fallback_kind: RouteFormationKind) {
        let total = self.marker_ids.len();
        let ids = self.marker_ids.clone();
        for (i, mid) in ids.into_iter().enumerate() {
            if !self.custom_offsets.iter().any(|o| o.marker_id == mid) {
                let (lat, lon) = self.offset_for_kind(fallback_kind, i, total);
                self.custom_offsets.push(CustomUnitOffset {
                    marker_id: mid,
                    lateral_m: lat,
                    longitudinal_m: lon,
                });
            }
        }
        self.sanitize();
    }

    /// Overwrites all `custom_offsets` from a preset formation layout (useful as a starting base).
    pub fn copy_preset_into_custom(&mut self, preset: RouteFormationKind) {
        let kind = if preset == RouteFormationKind::Custom {
            RouteFormationKind::Grid
        } else {
            preset
        };
        let total = self.marker_ids.len();
        self.custom_offsets = self
            .marker_ids
            .iter()
            .enumerate()
            .map(|(i, &mid)| {
                let (lat, lon) = self.offset_for_kind(kind, i, total);
                CustomUnitOffset {
                    marker_id: mid,
                    lateral_m: lat,
                    longitudinal_m: lon,
                }
            })
            .collect();
    }

    /// Sets or inserts a custom local offset for a specific unit.
    pub fn set_custom_offset(&mut self, marker_id: Uuid, lateral_m: f64, longitudinal_m: f64) {
        let lat = lateral_m.clamp(-250.0, 250.0);
        let lon = longitudinal_m.clamp(-250.0, 250.0);
        if let Some(entry) = self
            .custom_offsets
            .iter_mut()
            .find(|o| o.marker_id == marker_id)
        {
            entry.lateral_m = lat;
            entry.longitudinal_m = lon;
        } else {
            self.custom_offsets.push(CustomUnitOffset {
                marker_id,
                lateral_m: lat,
                longitudinal_m: lon,
            });
        }
    }

    /// Adds a marker to `marker_ids` if not present, or removes it if already present.
    /// Returns `true` if added, `false` if removed.
    pub fn toggle_marker(&mut self, marker_id: Uuid) -> bool {
        if let Some(pos) = self.marker_ids.iter().position(|id| *id == marker_id) {
            self.marker_ids.remove(pos);
            self.custom_offsets.retain(|o| o.marker_id != marker_id);
            false
        } else {
            self.marker_ids.push(marker_id);
            if self.formation == RouteFormationKind::Custom {
                let idx = self.marker_ids.len() - 1;
                let total = self.marker_ids.len();
                let (lat, lon) = self.offset_for_kind(RouteFormationKind::Grid, idx, total);
                self.set_custom_offset(marker_id, lat, lon);
            }
            true
        }
    }

    /// Moves a bound marker one step up or down in formation order (`#1..#N`).
    pub fn move_marker_order(&mut self, marker_id: Uuid, move_up: bool) -> bool {
        let Some(pos) = self.marker_ids.iter().position(|id| *id == marker_id) else {
            return false;
        };
        if move_up && pos > 0 {
            self.marker_ids.swap(pos, pos - 1);
            true
        } else if !move_up && pos + 1 < self.marker_ids.len() {
            self.marker_ids.swap(pos, pos + 1);
            true
        } else {
            false
        }
    }

    /// Captures relative positions of all currently bound markers on the 3D map and projects them
    /// into the route's initial travel frame `(right, forward)` around the group centroid,
    /// switching the formation to `RouteFormationKind::Custom`.
    ///
    /// Returns the number of markers captured.
    pub fn capture_custom_from_map(&mut self, markers: &[Marker], geom: &[[f64; 3]]) -> usize {
        if geom.len() < 2 || self.marker_ids.is_empty() {
            return 0;
        }
        let anchor = GeoAnchor::new(geom[0][0], geom[0][1]);
        let p0 = anchor.lng_lat_alt_to_local(geom[0][0], geom[0][1], 0.0);
        let p1 = anchor.lng_lat_alt_to_local(geom[1][0], geom[1][1], 0.0);
        let dx = (p1[0] - p0[0]) as f64;
        let dz = (p1[2] - p0[2]) as f64;
        let len = dx.hypot(dz);
        let (fx, fz) = if len > 1e-6 {
            (dx / len, dz / len)
        } else {
            (0.0, -1.0)
        };
        // Right vector in Bevy XZ plane (+X East, +Z South): forward × +Y = (-fz, fx)
        let (rx, rz) = (-fz, fx);

        let mut found: Vec<(Uuid, f64, f64)> = Vec::new();
        for &mid in &self.marker_ids {
            if let Some(m) = markers.iter().find(|x| x.id == mid) {
                let loc = anchor.lng_lat_alt_to_local(m.lng, m.lat, 0.0);
                found.push((mid, loc[0] as f64, loc[2] as f64));
            }
        }
        if found.is_empty() {
            return 0;
        }

        let n = found.len() as f64;
        let cx: f64 = found.iter().map(|(_, x, _)| *x).sum::<f64>() / n;
        let cz: f64 = found.iter().map(|(_, _, z)| *z).sum::<f64>() / n;

        self.formation = RouteFormationKind::Custom;
        self.custom_offsets.clear();
        for (mid, x, z) in &found {
            let rx_m = x - cx;
            let rz_m = z - cz;
            let lateral = (rx_m * rx + rz_m * rz).clamp(-250.0, 250.0);
            let longitudinal = (rx_m * fx + rz_m * fz).clamp(-250.0, 250.0);
            // Round to 0.05m for clean inspector display
            let lat_r = (lateral * 20.0).round() / 20.0;
            let lon_r = (longitudinal * 20.0).round() / 20.0;
            self.custom_offsets.push(CustomUnitOffset {
                marker_id: *mid,
                lateral_m: lat_r,
                longitudinal_m: lon_r,
            });
        }
        found.len()
    }

    /// Generates a tidy, non-overlapping organic scatter in `Custom` formation using a deterministic
    /// PRNG seed (e.g. a click counter / timestamp) so units never stack on top of each other.
    pub fn randomize_custom_offsets(&mut self, mut seed: u64) {
        self.formation = RouteFormationKind::Custom;
        let n = self.marker_ids.len();
        if n == 0 {
            self.custom_offsets.clear();
            return;
        }
        let sx = self.spacing_x_m.clamp(1.0, 80.0);
        let sz = self.spacing_z_m.clamp(1.0, 80.0);
        let min_sep = sx.min(sz) * 0.72;
        let cols_est = (n as f64).sqrt().ceil().max(2.0);
        let half_w = (cols_est * sx * 0.65).max(sx);
        let half_d = (cols_est * sz * 0.65).max(sz);

        let mut next_u01 = || -> f64 {
            // SplitMix64 step
            seed = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = seed;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^= z >> 31;
            ((z >> 11) as f64) / ((1u64 << 53) as f64)
        };

        let mut placed: Vec<(f64, f64)> = Vec::with_capacity(n);
        self.custom_offsets.clear();
        for (i, &mid) in self.marker_ids.iter().enumerate() {
            let mut best = (0.0_f64, 0.0_f64);
            let mut found_clear = false;
            for _attempt in 0..48 {
                let lat = (next_u01() * 2.0 - 1.0) * half_w;
                let lon = (next_u01() * 2.0 - 1.0) * half_d;
                if placed
                    .iter()
                    .all(|(px, pz)| (lat - px).hypot(lon - pz) >= min_sep)
                {
                    best = (lat, lon);
                    found_clear = true;
                    break;
                }
            }
            if !found_clear {
                best = self.offset_for_kind(RouteFormationKind::Grid, i, n);
            }
            let lat_r = (best.0 * 10.0).round() / 10.0;
            let lon_r = (best.1 * 10.0).round() / 10.0;
            placed.push((lat_r, lon_r));
            self.custom_offsets.push(CustomUnitOffset {
                marker_id: mid,
                lateral_m: lat_r,
                longitudinal_m: lon_r,
            });
        }
    }
}

/// Returns `true` if `anim` is a Multi-Unit Formation Route (`route_multi`).
#[inline]
pub fn is_multi_route_anim(anim: &Anim) -> bool {
    anim.anim_type == ANIM_TYPE_ROUTE_MULTI
}

/// Extracts `MultiRouteConfig` from `anim.data["multi_route"]` (with graceful defaults).
pub fn parse_multi_route_config(anim: &Anim) -> MultiRouteConfig {
    let mut cfg: MultiRouteConfig = anim
        .data
        .get("multi_route")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    if cfg.marker_ids.is_empty()
        && let Some(mid) = anim.marker_id
    {
        cfg.marker_ids.push(mid);
    }
    cfg.sanitize();
    cfg
}

/// Stores `MultiRouteConfig` cleanly inside `anim.data["multi_route"]` and keeps `anim.marker_id`
/// synced to the lead unit (`marker_ids[0]`) for backward compatibility.
pub fn write_multi_route_config(anim: &mut Anim, cfg: &MultiRouteConfig) {
    let mut clean = cfg.clone();
    clean.sanitize();
    if !anim.data.is_object() {
        anim.data = serde_json::json!({});
    }
    if let Some(obj) = anim.data.as_object_mut()
        && let Ok(val) = serde_json::to_value(&clean)
    {
        obj.insert("multi_route".into(), val);
    }
    anim.marker_id = clean.marker_ids.first().copied();
}

/// Unified helper returning all bound marker UUIDs for either a Single Route or Multi-Unit Route.
pub fn anim_bound_marker_ids(anim: &Anim) -> Vec<Uuid> {
    if is_multi_route_anim(anim) {
        parse_multi_route_config(anim).marker_ids
    } else {
        anim.marker_id.into_iter().collect()
    }
}

/// Returns `true` if the animation has a valid polyline (>= 2 waypoints) and at least one
/// bound marker that currently exists in `markers`.
pub fn anim_is_playable(anim: &Anim, markers: &[Marker]) -> bool {
    if anim.geom.len() < 2 {
        return false;
    }
    let bound = anim_bound_marker_ids(anim);
    bound
        .iter()
        .any(|id| markers.iter().any(|m| m.id == *id))
}

/// Computed geographic pose for a single unit in a route animation.
#[derive(Debug, Clone, Copy)]
pub struct RouteUnitPose {
    pub marker_id: Uuid,
    pub lng: f64,
    pub lat: f64,
    pub altitude: f64,
    /// glTF `+Z`-forward yaw around Bevy `+Y` in degrees.
    pub rot_y: f64,
}

#[derive(Debug, Clone, Copy)]
struct LocalPolylineSample {
    x: f64,
    y: f64,
    z: f64,
    fx: f64,
    fz: f64,
}

/// Precomputed local-meter polyline allowing exact, direction-invariant formation sampling.
struct LocalRoutePolyline {
    anchor: GeoAnchor,
    pts: Vec<[f64; 3]>,
    seg_lens: Vec<f64>,
    seg_dirs: Vec<(f64, f64)>,
    cum_lens: Vec<f64>,
    total_len: f64,
}

impl LocalRoutePolyline {
    fn from_geom(geom: &[[f64; 3]]) -> Option<Self> {
        if geom.len() < 2 {
            return None;
        }
        let anchor = GeoAnchor::new(geom[0][0], geom[0][1]);
        let pts: Vec<[f64; 3]> = geom
            .iter()
            .map(|p| {
                let loc = anchor.lng_lat_alt_to_local(p[0], p[1], p[2]);
                [loc[0] as f64, loc[1] as f64, loc[2] as f64]
            })
            .collect();

        let mut seg_lens = Vec::with_capacity(pts.len() - 1);
        let mut seg_dirs = Vec::with_capacity(pts.len() - 1);
        let mut cum_lens = Vec::with_capacity(pts.len());
        cum_lens.push(0.0);

        let mut total_len = 0.0_f64;
        let mut last_dir = (0.0_f64, -1.0_f64);
        for w in pts.windows(2) {
            let dx = w[1][0] - w[0][0];
            let dz = w[1][2] - w[0][2];
            let horiz = dx.hypot(dz);
            let dir = if horiz > 1e-6 {
                let d = (dx / horiz, dz / horiz);
                last_dir = d;
                d
            } else {
                last_dir
            };
            let len = horiz.max(0.01);
            seg_lens.push(len);
            seg_dirs.push(dir);
            total_len += len;
            cum_lens.push(total_len);
        }

        if total_len <= 0.0 {
            return None;
        }

        Some(Self {
            anchor,
            pts,
            seg_lens,
            seg_dirs,
            cum_lens,
            total_len,
        })
    }

    /// Samples position and smoothly blended forward direction at distance `dist_m`.
    /// Distances `< 0` extrapolate smoothly backward along segment 0; distances `> total_len`
    /// extrapolate smoothly forward along the final segment so multi-row formations remain
    /// intact at start & finish without bunching.
    fn sample(&self, dist_m: f64, corner_blend_m: f64) -> LocalPolylineSample {
        let seg_count = self.seg_lens.len();
        if dist_m <= 0.0 {
            let (fx, fz) = self.seg_dirs[0];
            let p0 = self.pts[0];
            return LocalPolylineSample {
                x: p0[0] + fx * dist_m,
                y: p0[1],
                z: p0[2] + fz * dist_m,
                fx,
                fz,
            };
        }
        if dist_m >= self.total_len {
            let last = seg_count - 1;
            let (fx, fz) = self.seg_dirs[last];
            let p_end = self.pts[seg_count];
            let extra = dist_m - self.total_len;
            return LocalPolylineSample {
                x: p_end[0] + fx * extra,
                y: p_end[1],
                z: p_end[2] + fz * extra,
                fx,
                fz,
            };
        }

        // Find active segment
        let mut idx = seg_count - 1;
        for i in 0..seg_count {
            if dist_m <= self.cum_lens[i + 1] {
                idx = i;
                break;
            }
        }

        let seg_start = self.cum_lens[idx];
        let seg_len = self.seg_lens[idx];
        let local_d = (dist_m - seg_start).clamp(0.0, seg_len);
        let t = (local_d / seg_len).clamp(0.0, 1.0);
        let p0 = self.pts[idx];
        let p1 = self.pts[idx + 1];

        let x = p0[0] + (p1[0] - p0[0]) * t;
        let y = p0[1] + (p1[1] - p0[1]) * t;
        let z = p0[2] + (p1[2] - p0[2]) * t;

        // Smooth tangent transition across interior corners so lateral-offset units glide around bends
        let mut dir = self.seg_dirs[idx];
        if corner_blend_m > 0.05 {
            if idx > 0 {
                let half_w = corner_blend_m.min(seg_len * 0.45).min(self.seg_lens[idx - 1] * 0.45);
                if half_w > 1e-3 && local_d < half_w {
                    let prev = self.seg_dirs[idx - 1];
                    let alpha = 0.5 + 0.5 * (local_d / half_w);
                    dir = blend_unit_dir(prev, dir, alpha);
                }
            }
            if idx + 1 < seg_count {
                let dist_to_end = seg_len - local_d;
                let half_w = corner_blend_m.min(seg_len * 0.45).min(self.seg_lens[idx + 1] * 0.45);
                if half_w > 1e-3 && dist_to_end < half_w {
                    let next = self.seg_dirs[idx + 1];
                    let alpha = 0.5 * (1.0 - dist_to_end / half_w);
                    dir = blend_unit_dir(dir, next, alpha);
                }
            }
        }

        LocalPolylineSample {
            x,
            y,
            z,
            fx: dir.0,
            fz: dir.1,
        }
    }

    fn local_to_geo(&self, x: f64, y: f64, z: f64) -> (f64, f64, f64) {
        self.anchor
            .local_to_lng_lat_alt([x as f32, y as f32, z as f32])
    }
}

fn blend_unit_dir(a: (f64, f64), b: (f64, f64), t: f64) -> (f64, f64) {
    let x = a.0 * (1.0 - t) + b.0 * t;
    let z = a.1 * (1.0 - t) + b.1 * t;
    let len = x.hypot(z);
    if len > 1e-6 {
        (x / len, z / len)
    } else {
        b
    }
}

/// Samples exact geographic positions and glTF `+Z`-forward headings for EVERY bound unit of a
/// Multi-Unit Formation Route at distance `distance_m` along `geom`.
///
/// Guarantees identical relative formation geometry regardless of travel heading (East, West,
/// North, South, or any diagonal angle).
pub fn sample_multi_route_poses(
    geom: &[[f64; 3]],
    cfg: &MultiRouteConfig,
    distance_m: f64,
) -> Vec<RouteUnitPose> {
    if cfg.marker_ids.is_empty() {
        return Vec::new();
    }
    let Some(poly) = LocalRoutePolyline::from_geom(geom) else {
        return Vec::new();
    };

    let offsets = cfg.compute_all_offsets();
    let max_lat = offsets
        .iter()
        .map(|(_, lat, _)| lat.abs())
        .fold(0.0_f64, f64::max);
    let corner_blend_m = (max_lat * 0.85).clamp(0.5, 6.0);

    let mut poses = Vec::with_capacity(offsets.len());
    match cfg.corner_mode {
        FormationCornerMode::FollowPath => {
            for (marker_id, lateral_m, longitudinal_m) in offsets {
                let s_unit = distance_m + longitudinal_m;
                let sample = poly.sample(s_unit, corner_blend_m);
                // Right vector in Bevy XZ plane (+X East, +Z South): forward × +Y = (-fz, fx)
                let rx = -sample.fz;
                let rz = sample.fx;
                let ux = sample.x + rx * lateral_m;
                let uz = sample.z + rz * lateral_m;
                let (lng, lat, altitude) = poly.local_to_geo(ux, sample.y, uz);
                let rot_y = sample.fx.atan2(sample.fz).to_degrees();
                poses.push(RouteUnitPose {
                    marker_id,
                    lng,
                    lat,
                    altitude,
                    rot_y,
                });
            }
        }
        FormationCornerMode::Rigid => {
            let center = poly.sample(distance_m, corner_blend_m);
            let rx = -center.fz;
            let rz = center.fx;
            let rot_y = center.fx.atan2(center.fz).to_degrees();
            for (marker_id, lateral_m, longitudinal_m) in offsets {
                let ux = center.x + rx * lateral_m + center.fx * longitudinal_m;
                let uz = center.z + rz * lateral_m + center.fz * longitudinal_m;
                let (lng, lat, altitude) = poly.local_to_geo(ux, center.y, uz);
                poses.push(RouteUnitPose {
                    marker_id,
                    lng,
                    lat,
                    altitude,
                    rot_y,
                });
            }
        }
    }

    poses
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_ids(n: usize) -> Vec<Uuid> {
        (0..n).map(|_| Uuid::new_v4()).collect()
    }

    #[test]
    fn grid_five_units_four_columns_places_fifth_behind_center() {
        let cfg = MultiRouteConfig {
            marker_ids: mk_ids(5),
            formation: RouteFormationKind::Grid,
            columns: 4,
            spacing_x_m: 3.0,
            spacing_z_m: 4.0,
            ..Default::default()
        };
        let offsets = cfg.compute_all_offsets();
        assert_eq!(offsets.len(), 5);
        // Front row (4 units centered around 0): -4.5, -1.5, +1.5, +4.5 at long = 0
        assert!((offsets[0].1 - (-4.5)).abs() < 1e-6 && offsets[0].2.abs() < 1e-6);
        assert!((offsets[1].1 - (-1.5)).abs() < 1e-6 && offsets[1].2.abs() < 1e-6);
        assert!((offsets[2].1 - 1.5).abs() < 1e-6 && offsets[2].2.abs() < 1e-6);
        assert!((offsets[3].1 - 4.5).abs() < 1e-6 && offsets[3].2.abs() < 1e-6);
        // 5th unit centered directly behind at (0.0, -4.0)
        assert!(offsets[4].1.abs() < 1e-6 && (offsets[4].2 - (-4.0)).abs() < 1e-6);
    }

    #[test]
    fn formation_geometry_is_rotation_invariant_across_all_headings() {
        // Custom asymmetric formation: Unit 0 at (0,0), Unit 1 at (+3m right, -2m behind), Unit 2 at (-4m left, -5m behind)
        let ids = mk_ids(3);
        let mut cfg = MultiRouteConfig {
            marker_ids: ids.clone(),
            formation: RouteFormationKind::Custom,
            corner_mode: FormationCornerMode::FollowPath,
            ..Default::default()
        };
        cfg.set_custom_offset(ids[0], 0.0, 0.0);
        cfg.set_custom_offset(ids[1], 3.0, -2.0);
        cfg.set_custom_offset(ids[2], -4.0, -5.0);

        // Test 8 compass headings (N, NE, E, SE, S, SW, W, NW)
        let origin = GeoAnchor::new(115.22, -8.65);
        let headings_deg = [0.0_f64, 45.0, 90.0, 135.0, 180.0, 225.0, 270.0, 315.0];

        for deg in headings_deg {
            let rad = deg.to_radians();
            let fx = rad.sin();
            let fz = -rad.cos();
            let (lng1, lat1, _) = origin.local_to_lng_lat_alt([(fx * 100.0) as f32, 0.0, (fz * 100.0) as f32]);
            let geom = vec![[115.22, -8.65, 0.0], [lng1, lat1, 0.0]];

            for mode in FormationCornerMode::ALL {
                cfg.corner_mode = *mode;
                let poses = sample_multi_route_poses(&geom, &cfg, 40.0);
                assert_eq!(poses.len(), 3);

                let p0 = origin.lng_lat_alt_to_local(poses[0].lng, poses[0].lat, 0.0);
                let p1 = origin.lng_lat_alt_to_local(poses[1].lng, poses[1].lat, 0.0);
                let p2 = origin.lng_lat_alt_to_local(poses[2].lng, poses[2].lat, 0.0);

                // Project (p1 - p0) onto (right, forward) of this heading
                let rx = -fz;
                let rz = fx;
                let d1x = (p1[0] - p0[0]) as f64;
                let d1z = (p1[2] - p0[2]) as f64;
                let lat1_m = d1x * rx + d1z * rz;
                let lon1_m = d1x * fx + d1z * fz;
                assert!(
                    (lat1_m - 3.0).abs() < 0.05 && (lon1_m - (-2.0)).abs() < 0.05,
                    "heading {deg} mode {mode:?}: expected (+3,-2), got ({lat1_m:.3},{lon1_m:.3})"
                );

                let d2x = (p2[0] - p0[0]) as f64;
                let d2z = (p2[2] - p0[2]) as f64;
                let lat2_m = d2x * rx + d2z * rz;
                let lon2_m = d2x * fx + d2z * fz;
                assert!(
                    (lat2_m - (-4.0)).abs() < 0.05 && (lon2_m - (-5.0)).abs() < 0.05,
                    "heading {deg} mode {mode:?}: expected (-4,-5), got ({lat2_m:.3},{lon2_m:.3})"
                );
            }
        }
    }

    #[test]
    fn wedge_staggered_and_vip_formations_generate_expected_shapes() {
        let ids = mk_ids(5);
        // Wedge: point at (0,0), wings symmetric
        let wedge = MultiRouteConfig {
            marker_ids: ids.clone(),
            formation: RouteFormationKind::Wedge,
            spacing_x_m: 2.0,
            spacing_z_m: 2.5,
            ..Default::default()
        }
        .compute_all_offsets();
        assert_eq!((wedge[0].1, wedge[0].2), (0.0, 0.0));
        assert_eq!((wedge[1].1, wedge[1].2), (-2.0, -2.5));
        assert_eq!((wedge[2].1, wedge[2].2), (2.0, -2.5));
        assert_eq!((wedge[3].1, wedge[3].2), (-4.0, -5.0));
        assert_eq!((wedge[4].1, wedge[4].2), (4.0, -5.0));

        // Staggered: alternating left/right with progressive depth
        let stag = MultiRouteConfig {
            marker_ids: ids.clone(),
            formation: RouteFormationKind::Staggered,
            spacing_x_m: 4.0,
            spacing_z_m: 4.0,
            ..Default::default()
        }
        .compute_all_offsets();
        assert_eq!((stag[0].1, stag[0].2), (-2.0, 0.0));
        assert_eq!((stag[1].1, stag[1].2), (2.0, -2.0));
        assert_eq!((stag[2].1, stag[2].2), (-2.0, -4.0));

        // VIP Escort: unit 0 at center, 4 guards at radius 5.0m
        let vip = MultiRouteConfig {
            marker_ids: ids,
            formation: RouteFormationKind::VipEscort,
            vip_radius_m: 5.0,
            ..Default::default()
        }
        .compute_all_offsets();
        assert_eq!((vip[0].1, vip[0].2), (0.0, 0.0));
        for entry in &vip[1..] {
            let r = entry.1.hypot(entry.2);
            assert!((r - 5.0).abs() < 1e-6);
        }
    }

    #[test]
    fn custom_randomize_produces_non_overlapping_offsets() {
        let mut cfg = MultiRouteConfig {
            marker_ids: mk_ids(8),
            spacing_x_m: 3.0,
            spacing_z_m: 3.0,
            ..Default::default()
        };
        cfg.randomize_custom_offsets(42);
        assert_eq!(cfg.formation, RouteFormationKind::Custom);
        assert_eq!(cfg.custom_offsets.len(), 8);
        for i in 0..cfg.custom_offsets.len() {
            for j in (i + 1)..cfg.custom_offsets.len() {
                let a = &cfg.custom_offsets[i];
                let b = &cfg.custom_offsets[j];
                let d = (a.lateral_m - b.lateral_m).hypot(a.longitudinal_m - b.longitudinal_m);
                assert!(d >= 1.5, "units {i} & {j} overlapped: d={d}");
            }
        }
    }
}
