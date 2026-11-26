pub mod asset_preview;
pub mod basic_mesh;
pub mod buildings;
pub mod gizmo;
pub mod lines;
pub mod markers;
pub mod media_preview;
pub mod particle_fx;
pub mod play_rts;
pub mod playback;
pub mod plot_line_edit;
pub mod pointer_arrow;
pub mod polygon_edit;
pub mod route_edit;
pub mod scenario_audio;
pub mod shoot_fx;
pub mod skybox;
pub mod traffic;
pub mod water_cannon;

pub fn parse_hex_color(hex: &str, alpha: f32) -> bevy::prelude::Color {
    let clean = hex.trim().trim_start_matches('#');
    if clean.len() == 6
        && let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&clean[0..2], 16),
            u8::from_str_radix(&clean[2..4], 16),
            u8::from_str_radix(&clean[4..6], 16),
        )
    {
        return bevy::prelude::Color::srgba(
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            alpha,
        );
    }
    bevy::prelude::Color::srgba(0.22, 0.74, 0.97, alpha)
}

#[derive(Debug, Clone)]
pub struct GroupHighlightEntry {
    pub anim_id: uuid::Uuid,
    /// 1-based step index inside the selected Group, or `None` for a hovered candidate not yet added.
    pub step_number: Option<usize>,
    /// `true` when this animation (or its parent nested group) is currently hovered in the Group UI.
    pub is_focused_candidate: bool,
}

fn expand_group_leaves(
    root_id: uuid::Uuid,
    all_anims: &[twg_shared::models::Anim],
    out: &mut Vec<uuid::Uuid>,
    visited: &mut std::collections::HashSet<uuid::Uuid>,
) {
    if !visited.insert(root_id) {
        return;
    }
    let Some(anim) = all_anims.iter().find(|a| a.id == root_id) else {
        return;
    };
    if twg_shared::group_anim::is_group_anim(anim) {
        let cfg = twg_shared::group_anim::parse_group_anim_config(anim);
        for item in cfg.items {
            expand_group_leaves(item.anim_id, all_anims, out, visited);
        }
    } else {
        out.push(root_id);
    }
}

/// Resolves all member animations (and any hovered candidate from `+ Tambah Animasi ke Grup...`)
/// that should be highlighted on the 3D map whenever a `Grup` (`Combine` / `Sequence`) animation
/// is selected in Plot Editor.
pub fn resolve_group_highlight_entries(
    state: &crate::state::AppState,
) -> Option<(
    twg_shared::group_anim::GroupAnimMode,
    Vec<GroupHighlightEntry>,
)> {
    let crate::state::SelectedEntity::Anim(sel_id) = state.selected else {
        return None;
    };
    let sel_anim = state.anims.iter().find(|a| a.id == sel_id)?;
    let mode = twg_shared::group_anim::group_anim_mode(sel_anim)?;
    let cfg = twg_shared::group_anim::parse_group_anim_config(sel_anim);

    // Collect focused candidate leaf IDs (hovered in dropdown OR hovered in member list)
    let mut focused_leaves = Vec::new();
    if let Some(hid) = state.group_hovered_child_anim_id {
        let mut vis = std::collections::HashSet::new();
        vis.insert(sel_id);
        expand_group_leaves(hid, &state.anims, &mut focused_leaves, &mut vis);
    }
    let focused_set: std::collections::HashSet<uuid::Uuid> =
        focused_leaves.iter().copied().collect();

    let mut entries: Vec<GroupHighlightEntry> = Vec::new();
    let mut seen_entry_ids: std::collections::HashSet<uuid::Uuid> =
        std::collections::HashSet::new();

    for (idx, item) in cfg.items.iter().enumerate() {
        let step_num = idx + 1;
        let mut step_leaves = Vec::new();
        let mut vis = std::collections::HashSet::new();
        vis.insert(sel_id);
        expand_group_leaves(item.anim_id, &state.anims, &mut step_leaves, &mut vis);
        for lid in step_leaves {
            let is_foc = focused_set.contains(&lid);
            if seen_entry_ids.insert(lid) {
                entries.push(GroupHighlightEntry {
                    anim_id: lid,
                    step_number: Some(step_num),
                    is_focused_candidate: is_foc,
                });
            } else if is_foc
                && let Some(existing) = entries.iter_mut().find(|e| e.anim_id == lid)
            {
                existing.is_focused_candidate = true;
            }
        }
    }

    // Append hovered candidate(s) not yet in the group so user sees them light up on the map!
    for lid in focused_leaves {
        if seen_entry_ids.insert(lid) {
            entries.push(GroupHighlightEntry {
                anim_id: lid,
                step_number: None,
                is_focused_candidate: true,
            });
        }
    }

    Some((mode, entries))
}

/// Resolves a representative `(lng, lat)` camera focus target for any animation type
/// (Route, Particle, Shoot, Pointer Arrow, Spawn/Hide, or Group).
pub fn anim_primary_focus_lnglat(
    anim: &twg_shared::models::Anim,
    all_anims: &[twg_shared::models::Anim],
    markers: &[twg_shared::models::Marker],
) -> Option<(f64, f64)> {
    if twg_shared::traffic_anim::is_traffic_anim(anim) {
        let cfg = twg_shared::traffic_anim::parse_traffic_anim_config(anim);
        if let Some(p) = cfg
            .lanes
            .iter()
            .find_map(|l| l.geom.first())
        {
            return Some((p[0], p[1]));
        }
    }
    if twg_shared::traffic_queue::is_traffic_queue_anim(anim) {
        let cfg = twg_shared::traffic_queue::parse_traffic_queue_config(anim);
        if let Some(p) = cfg
            .corridors
            .iter()
            .find_map(|c| c.queue_geom.first().or_else(|| c.clear_geom.first()))
        {
            return Some((p[0], p[1]));
        }
    }
    if twg_shared::traffic_diversion::is_traffic_diversion_anim(anim) {
        let cfg = twg_shared::traffic_diversion::parse_traffic_diversion_config(anim);
        if let Some(p) = cfg.corridors.iter().find_map(|c| {
            c.approach_geom
                .first()
                .or_else(|| c.main_geom.first())
                .or_else(|| c.divert_geom.first())
        }) {
            return Some((p[0], p[1]));
        }
    }
    if let Some(p) = anim.geom.first() {
        return Some((p[0], p[1]));
    }
    if twg_shared::group_anim::is_group_anim(anim) {
        let mut leaves = Vec::new();
        let mut vis = std::collections::HashSet::new();
        expand_group_leaves(anim.id, all_anims, &mut leaves, &mut vis);
        for lid in leaves {
            if let Some(child) = all_anims.iter().find(|a| a.id == lid)
                && let Some(pt) = anim_primary_focus_lnglat(child, all_anims, markers)
            {
                return Some(pt);
            }
        }
        return None;
    }
    twg_shared::route_formation::anim_bound_marker_ids(anim)
        .first()
        .and_then(|mid| markers.iter().find(|m| m.id == *mid))
        .map(|m| (m.lng, m.lat))
}
