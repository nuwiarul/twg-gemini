use std::cmp::Ordering;
use std::collections::BinaryHeap;

use uuid::Uuid;

/// 2D horizontal XZ obstacle footprint in local metric coordinates (meters).
#[derive(Debug, Clone)]
pub enum ObstacleShape {
    Circle { x: f64, z: f64, radius: f64 },
    Polygon { verts: Vec<[f64; 2]> },
}

impl ObstacleShape {
    fn aabb(&self, pad: f64) -> (f64, f64, f64, f64) {
        match self {
            Self::Circle { x, z, radius } => {
                let r = radius.max(0.1) + pad;
                (x - r, x + r, z - r, z + r)
            }
            Self::Polygon { verts } => {
                let mut min_x = f64::INFINITY;
                let mut max_x = f64::NEG_INFINITY;
                let mut min_z = f64::INFINITY;
                let mut max_z = f64::NEG_INFINITY;
                for v in verts {
                    min_x = min_x.min(v[0]);
                    max_x = max_x.max(v[0]);
                    min_z = min_z.min(v[1]);
                    max_z = max_z.max(v[1]);
                }
                (min_x - pad, max_x + pad, min_z - pad, max_z + pad)
            }
        }
    }
}

#[inline]
fn dist_sq(a: [f64; 2], b: [f64; 2]) -> f64 {
    let dx = a[0] - b[0];
    let dz = a[1] - b[1];
    dx * dx + dz * dz
}

fn point_to_segment_dist_sq(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let abx = b[0] - a[0];
    let abz = b[1] - a[1];
    let len_sq = abx * abx + abz * abz;
    if len_sq <= 1e-10 {
        return dist_sq(p, a);
    }
    let t = (((p[0] - a[0]) * abx + (p[1] - a[1]) * abz) / len_sq).clamp(0.0, 1.0);
    let cx = a[0] + abx * t;
    let cz = a[1] + abz * t;
    dist_sq(p, [cx, cz])
}

fn point_in_polygon(p: [f64; 2], verts: &[[f64; 2]]) -> bool {
    if verts.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = verts.len() - 1;
    for i in 0..verts.len() {
        let vi = verts[i];
        let vj = verts[j];
        let intersect = ((vi[1] > p[1]) != (vj[1] > p[1]))
            && (p[0] < (vj[0] - vi[0]) * (p[1] - vi[1]) / (vj[1] - vi[1] + 1e-15) + vi[0]);
        if intersect {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn segments_intersect(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    let orient = |p: [f64; 2], q: [f64; 2], r: [f64; 2]| -> f64 {
        (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])
    };
    let o1 = orient(a, b, c);
    let o2 = orient(a, b, d);
    let o3 = orient(c, d, a);
    let o4 = orient(c, d, b);
    (o1 * o2 < 0.0) && (o3 * o4 < 0.0)
}

fn segment_to_segment_dist_sq(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> f64 {
    if segments_intersect(a, b, c, d) {
        return 0.0;
    }
    point_to_segment_dist_sq(a, c, d)
        .min(point_to_segment_dist_sq(b, c, d))
        .min(point_to_segment_dist_sq(c, a, b))
        .min(point_to_segment_dist_sq(d, a, b))
}

pub fn point_blocked(p: [f64; 2], clearance: f64, obstacles: &[ObstacleShape]) -> bool {
    let clr = clearance.max(0.0);
    let clr_sq = clr * clr;
    for obs in obstacles {
        let (min_x, max_x, min_z, max_z) = obs.aabb(clr);
        if p[0] < min_x || p[0] > max_x || p[1] < min_z || p[1] > max_z {
            continue;
        }
        match obs {
            ObstacleShape::Circle { x, z, radius } => {
                let r = radius.max(0.1) + clr;
                if dist_sq(p, [*x, *z]) < r * r {
                    return true;
                }
            }
            ObstacleShape::Polygon { verts } => {
                if verts.len() < 3 {
                    continue;
                }
                if point_in_polygon(p, verts) {
                    return true;
                }
                for i in 0..verts.len() {
                    let a = verts[i];
                    let b = verts[(i + 1) % verts.len()];
                    if point_to_segment_dist_sq(p, a, b) < clr_sq {
                        return true;
                    }
                }
            }
        }
    }
    false
}

pub fn segment_is_clear(
    a: [f64; 2],
    b: [f64; 2],
    clearance: f64,
    obstacles: &[ObstacleShape],
) -> bool {
    let clr = clearance.max(0.0);
    let clr_sq = clr * clr;
    let seg_min_x = a[0].min(b[0]);
    let seg_max_x = a[0].max(b[0]);
    let seg_min_z = a[1].min(b[1]);
    let seg_max_z = a[1].max(b[1]);

    for obs in obstacles {
        let (min_x, max_x, min_z, max_z) = obs.aabb(clr);
        if seg_max_x < min_x || seg_min_x > max_x || seg_max_z < min_z || seg_min_z > max_z {
            continue;
        }
        match obs {
            ObstacleShape::Circle { x, z, radius } => {
                let r = radius.max(0.1) + clr;
                if point_to_segment_dist_sq([*x, *z], a, b) < r * r {
                    return false;
                }
            }
            ObstacleShape::Polygon { verts } => {
                if verts.len() < 3 {
                    continue;
                }
                let mid = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
                if point_in_polygon(a, verts)
                    || point_in_polygon(b, verts)
                    || point_in_polygon(mid, verts)
                {
                    return false;
                }
                for i in 0..verts.len() {
                    let va = verts[i];
                    let vb = verts[(i + 1) % verts.len()];
                    if segment_to_segment_dist_sq(a, b, va, vb) < clr_sq {
                        return false;
                    }
                }
            }
        }
    }
    true
}

/// Nudges `p` outward in concentric rings until it is clear of all `obstacles` and `claimed` slots.
pub fn nudge_point_to_free_space(
    p: [f64; 2],
    clearance: f64,
    obstacles: &[ObstacleShape],
    claimed: &[[f64; 2]],
    min_slot_sep: f64,
) -> [f64; 2] {
    let min_sep_sq = min_slot_sep * min_slot_sep;
    let is_ok = |cand: [f64; 2]| -> bool {
        !point_blocked(cand, clearance, obstacles)
            && claimed.iter().all(|c| dist_sq(cand, *c) >= min_sep_sq)
    };
    if is_ok(p) {
        return p;
    }

    let step = (clearance.max(1.0) * 0.9).clamp(1.0, 4.5);
    for ring in 1..=14 {
        let r = step * ring as f64;
        let samples = 12 + ring * 4;
        for k in 0..samples {
            let ang = (k as f64 / samples as f64) * std::f64::consts::TAU;
            let cand = [p[0] + ang.cos() * r, p[1] + ang.sin() * r];
            if is_ok(cand) {
                return cand;
            }
        }
    }
    p
}

/// Computes non-overlapping grid formation target positions around `goal_xz` for 1 or many
/// selected units, aligned to the travel heading from group centroid -> `goal_xz`, and assigns
/// each unit to the nearest slot (with 2-opt uncrossing) so units never collide or cross paths.
///
/// Input tuples: `(unit_id, current_xz, agent_radius_m)`.
/// Output tuples: `(unit_id, target_xz, facing_rot_y_deg)`.
pub fn compute_formation_targets(
    units: &[(Uuid, [f64; 2], f64)],
    goal_xz: [f64; 2],
    obstacles: &[ObstacleShape],
) -> Vec<(Uuid, [f64; 2], f64)> {
    if units.is_empty() {
        return Vec::new();
    }

    let n = units.len();
    let mut cx = 0.0_f64;
    let mut cz = 0.0_f64;
    let mut max_r = 0.8_f64;
    for &(_, pos, r) in units {
        cx += pos[0];
        cz += pos[1];
        max_r = max_r.max(r);
    }
    cx /= n as f64;
    cz /= n as f64;

    let dx = goal_xz[0] - cx;
    let dz = goal_xz[1] - cz;
    let dlen = dx.hypot(dz);
    let (fx, fz) = if dlen > 1e-4 {
        (dx / dlen, dz / dlen)
    } else {
        (0.0, 1.0)
    };
    // Match glTF `+Z`-forward yaw convention: forward = (sin(yaw), cos(yaw))
    let heading_deg = fx.atan2(fz).to_degrees().rem_euclid(360.0);
    let (rx, rz) = (-fz, fx);

    if n == 1 {
        let (id, _, r) = units[0];
        let free = nudge_point_to_free_space(goal_xz, r + 0.35, obstacles, &[], 0.0);
        return vec![(id, free, heading_deg)];
    }

    let spacing = (max_r * 2.55).clamp(2.4, 15.0);
    let cols = (n as f64).sqrt().ceil() as usize;
    let rows = n.div_ceil(cols);

    let mut raw_slots: Vec<[f64; 2]> = Vec::with_capacity(n);
    for idx in 0..n {
        let r_i = idx / cols;
        let c_i = idx % cols;
        let row_count = if r_i + 1 == rows {
            n - r_i * cols
        } else {
            cols
        };
        let lat = (c_i as f64 - (row_count as f64 - 1.0) * 0.5) * spacing;
        let lon = -((r_i as f64 - (rows as f64 - 1.0) * 0.5) * spacing);
        let sx = goal_xz[0] + rx * lat + fx * lon;
        let sz = goal_xz[1] + rz * lat + fz * lon;
        raw_slots.push([sx, sz]);
    }

    // Nudge any slot landing inside a building/obstacle to nearest free spot
    let mut slots: Vec<[f64; 2]> = Vec::with_capacity(n);
    for s in raw_slots {
        let free = nudge_point_to_free_space(s, max_r + 0.35, obstacles, &slots, spacing * 0.72);
        slots.push(free);
    }

    // Initial greedy nearest assignment + 2-opt uncrossing so unit paths don't intersect
    let mut assign: Vec<usize> = (0..n).collect();
    let mut used = vec![false; n];
    for (ui, &(_, upos, _)) in units.iter().enumerate() {
        let mut best_si = 0usize;
        let mut best_d = f64::INFINITY;
        for (si, &spos) in slots.iter().enumerate() {
            if used[si] {
                continue;
            }
            let d = dist_sq(upos, spos);
            if d < best_d {
                best_d = d;
                best_si = si;
            }
        }
        used[best_si] = true;
        assign[ui] = best_si;
    }

    for _iter in 0..3 {
        let mut improved = false;
        for i in 0..n {
            for j in (i + 1)..n {
                let si = assign[i];
                let sj = assign[j];
                let cur = dist_sq(units[i].1, slots[si]).sqrt()
                    + dist_sq(units[j].1, slots[sj]).sqrt();
                let swapped = dist_sq(units[i].1, slots[sj]).sqrt()
                    + dist_sq(units[j].1, slots[si]).sqrt();
                if swapped + 1e-4 < cur {
                    assign.swap(i, j);
                    improved = true;
                }
            }
        }
        if !improved {
            break;
        }
    }

    units
        .iter()
        .enumerate()
        .map(|(i, &(id, _, _))| (id, slots[assign[i]], heading_deg))
        .collect()
}

#[derive(Copy, Clone)]
struct AStarNode {
    idx: usize,
    f_cost: f64,
    g_cost: f64,
}

impl Eq for AStarNode {}

impl PartialEq for AStarNode {
    fn eq(&self, other: &Self) -> bool {
        self.idx == other.idx && (self.f_cost - other.f_cost).abs() < 1e-9
    }
}

impl Ord for AStarNode {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .f_cost
            .total_cmp(&self.f_cost)
            .then_with(|| other.g_cost.total_cmp(&self.g_cost))
            .then_with(|| self.idx.cmp(&other.idx))
    }
}

impl PartialOrd for AStarNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Finds an obstacle-avoiding 2D XZ path from `start_xz` to `goal_xz` for an agent with
/// `agent_radius` meters:
/// 1. Fast direct line-of-sight check if no obstacles block the straight segment.
/// 2. Adaptive 8-connected local A* grid search around obstacles when blocked.
/// 3. Greedy line-of-sight string-pulling smoothing for crisp, natural detours.
pub fn find_obstacle_avoiding_path(
    start_xz: [f64; 2],
    goal_xz: [f64; 2],
    agent_radius: f64,
    obstacles: &[ObstacleShape],
) -> Vec<[f64; 2]> {
    let clearance = (agent_radius + 0.35).clamp(0.6, 8.0);
    let goal = nudge_point_to_free_space(goal_xz, clearance, obstacles, &[], 0.0);

    if dist_sq(start_xz, goal) < 0.04 {
        return vec![start_xz, goal];
    }

    if obstacles.is_empty() || segment_is_clear(start_xz, goal, clearance, obstacles) {
        return vec![start_xz, goal];
    }

    let dist = dist_sq(start_xz, goal).sqrt();
    let margin = (dist * 0.45).clamp(20.0, 90.0);
    let min_x = start_xz[0].min(goal[0]) - margin;
    let max_x = start_xz[0].max(goal[0]) + margin;
    let min_z = start_xz[1].min(goal[1]) - margin;
    let max_z = start_xz[1].max(goal[1]) + margin;

    let span_x = (max_x - min_x).max(4.0);
    let span_z = (max_z - min_z).max(4.0);
    let max_dim = 88usize;
    let base_cell = (clearance * 0.9).clamp(1.25, 4.0);
    let cell = base_cell
        .max(span_x / (max_dim as f64 - 2.0))
        .max(span_z / (max_dim as f64 - 2.0));

    let cols = ((span_x / cell).ceil() as usize + 1).clamp(4, max_dim);
    let rows = ((span_z / cell).ceil() as usize + 1).clamp(4, max_dim);

    let to_cell = |p: [f64; 2]| -> (usize, usize) {
        let c = (((p[0] - min_x) / cell).round() as isize).clamp(0, cols as isize - 1) as usize;
        let r = (((p[1] - min_z) / cell).round() as isize).clamp(0, rows as isize - 1) as usize;
        (c, r)
    };
    let cell_center = |c: usize, r: usize| -> [f64; 2] {
        [min_x + c as f64 * cell, min_z + r as f64 * cell]
    };

    // Filter obstacles overlapping the local grid bounds for fast bitmap rasterization
    let local_obs: Vec<&ObstacleShape> = obstacles
        .iter()
        .filter(|o| {
            let (ox0, ox1, oz0, oz1) = o.aabb(clearance);
            ox1 >= min_x && ox0 <= max_x && oz1 >= min_z && oz0 <= max_z
        })
        .collect();

    let total = cols * rows;
    let mut blocked = vec![false; total];
    for r in 0..rows {
        for c in 0..cols {
            let pt = cell_center(c, r);
            let mut hit = false;
            for obs in &local_obs {
                if point_blocked(pt, clearance, std::slice::from_ref(*obs)) {
                    hit = true;
                    break;
                }
            }
            blocked[r * cols + c] = hit;
        }
    }

    let (sc, sr) = to_cell(start_xz);
    let (gc, gr) = to_cell(goal);
    let start_idx = sr * cols + sc;
    let goal_idx = gr * cols + gc;

    // Ensure start & goal cells are traversable so tightly parked units can escape cleanly
    blocked[start_idx] = false;
    blocked[goal_idx] = false;

    let octile = |c: usize, r: usize| -> f64 {
        let dx = (c as f64 - gc as f64).abs();
        let dz = (r as f64 - gr as f64).abs();
        let diag = dx.min(dz);
        let straight = (dx - dz).abs();
        (diag * std::f64::consts::SQRT_2 + straight) * cell
    };

    let mut g_score = vec![f64::INFINITY; total];
    let mut came_from = vec![usize::MAX; total];
    let mut open = BinaryHeap::new();

    g_score[start_idx] = 0.0;
    open.push(AStarNode {
        idx: start_idx,
        f_cost: octile(sc, sr),
        g_cost: 0.0,
    });

    let mut best_reachable = start_idx;
    let mut best_h = octile(sc, sr);

    const DIRS: [(isize, isize, f64); 8] = [
        (1, 0, 1.0),
        (-1, 0, 1.0),
        (0, 1, 1.0),
        (0, -1, 1.0),
        (1, 1, std::f64::consts::SQRT_2),
        (1, -1, std::f64::consts::SQRT_2),
        (-1, 1, std::f64::consts::SQRT_2),
        (-1, -1, std::f64::consts::SQRT_2),
    ];

    let mut found_goal = false;
    while let Some(curr) = open.pop() {
        if curr.idx == goal_idx {
            found_goal = true;
            break;
        }
        if curr.g_cost > g_score[curr.idx] + 1e-6 {
            continue;
        }

        let cc = curr.idx % cols;
        let cr = curr.idx / cols;
        let h_curr = octile(cc, cr);
        if h_curr < best_h {
            best_h = h_curr;
            best_reachable = curr.idx;
        }

        for &(dc, dr, step_w) in &DIRS {
            let nc_i = cc as isize + dc;
            let nr_i = cr as isize + dr;
            if nc_i < 0 || nr_i < 0 || nc_i >= cols as isize || nr_i >= rows as isize {
                continue;
            }
            let nc = nc_i as usize;
            let nr = nr_i as usize;
            let nidx = nr * cols + nc;
            if blocked[nidx] {
                continue;
            }
            // Prevent diagonal corner-cutting through blocked orthogonal neighbors
            if dc != 0 && dr != 0 {
                let adj1 = cr * cols + nc;
                let adj2 = nr * cols + cc;
                if blocked[adj1] || blocked[adj2] {
                    continue;
                }
            }

            let tentative_g = curr.g_cost + step_w * cell;
            if tentative_g + 1e-6 < g_score[nidx] {
                g_score[nidx] = tentative_g;
                came_from[nidx] = curr.idx;
                open.push(AStarNode {
                    idx: nidx,
                    f_cost: tentative_g + octile(nc, nr),
                    g_cost: tentative_g,
                });
            }
        }
    }

    let end_idx = if found_goal { goal_idx } else { best_reachable };
    let mut rev_cells = Vec::new();
    let mut walk = end_idx;
    while walk != usize::MAX {
        rev_cells.push(walk);
        if walk == start_idx {
            break;
        }
        walk = came_from[walk];
    }
    rev_cells.reverse();

    let mut raw_pts: Vec<[f64; 2]> = Vec::with_capacity(rev_cells.len() + 2);
    raw_pts.push(start_xz);
    for &idx in rev_cells.iter().skip(1) {
        if idx == goal_idx && found_goal {
            continue;
        }
        raw_pts.push(cell_center(idx % cols, idx / cols));
    }
    if found_goal {
        raw_pts.push(goal);
    }

    if raw_pts.len() <= 2 {
        return raw_pts;
    }

    // String-pulling line-of-sight smoothing for sleek, natural detour paths
    let mut smooth = Vec::with_capacity(raw_pts.len());
    let mut anchor = 0usize;
    smooth.push(raw_pts[0]);
    while anchor + 1 < raw_pts.len() {
        let mut furthest = anchor + 1;
        for cand in (anchor + 2..raw_pts.len()).rev() {
            if segment_is_clear(raw_pts[anchor], raw_pts[cand], clearance, obstacles) {
                furthest = cand;
                break;
            }
        }
        smooth.push(raw_pts[furthest]);
        anchor = furthest;
    }

    smooth
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn straight_path_has_two_points_when_clear() {
        let path = find_obstacle_avoiding_path([0.0, 0.0], [20.0, 0.0], 0.8, &[]);
        assert_eq!(path.len(), 2);
        assert_eq!(path[0], [0.0, 0.0]);
        assert_eq!(path[1], [20.0, 0.0]);
    }

    #[test]
    fn path_detours_cleanly_around_building_wall() {
        // Wide rectangular building blocking x in [8, 12], z in [-10, 10]
        let wall = ObstacleShape::Polygon {
            verts: vec![[8.0, -10.0], [12.0, -10.0], [12.0, 10.0], [8.0, 10.0]],
        };
        let obstacles = vec![wall];
        let path = find_obstacle_avoiding_path([0.0, 0.0], [20.0, 0.0], 0.8, &obstacles);
        assert!(
            path.len() >= 3,
            "expected detour waypoints around wall, got {:?}",
            path
        );
        assert!((path.last().unwrap()[0] - 20.0).abs() < 1e-6);
        assert!((path.last().unwrap()[1] - 0.0).abs() < 1e-6);
        for seg in path.windows(2) {
            assert!(
                segment_is_clear(seg[0], seg[1], 0.9, &obstacles),
                "detour segment {:?} -> {:?} intersected wall",
                seg[0],
                seg[1]
            );
        }
    }

    #[test]
    fn formation_targets_spread_units_without_overlap_or_obstacle_collision() {
        let pillar = ObstacleShape::Circle {
            x: 20.0,
            z: 0.0,
            radius: 2.5,
        };
        let obstacles = vec![pillar];
        let units = vec![
            (Uuid::new_v4(), [0.0, -2.0], 1.0),
            (Uuid::new_v4(), [0.0, 0.0], 1.0),
            (Uuid::new_v4(), [0.0, 2.0], 1.0),
            (Uuid::new_v4(), [-2.0, 0.0], 1.0),
        ];
        let targets = compute_formation_targets(&units, [20.0, 0.0], &obstacles);
        assert_eq!(targets.len(), 4);
        for (i, (_, pa, _)) in targets.iter().enumerate() {
            assert!(
                !point_blocked(*pa, 1.1, &obstacles),
                "formation slot {} landed inside obstacle: {:?}",
                i,
                pa
            );
            for (j, (_, pb, _)) in targets.iter().enumerate() {
                if i != j {
                    assert!(
                        dist_sq(*pa, *pb).sqrt() >= 1.7,
                        "formation slots {} and {} too close: {:?} vs {:?}",
                        i,
                        j,
                        pa,
                        pb
                    );
                }
            }
        }
    }
}
