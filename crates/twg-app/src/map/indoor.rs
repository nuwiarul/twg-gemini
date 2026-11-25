use bevy::prelude::*;
use twg_shared::models::{BaseLayer, MapMode};

use crate::state::AppState;

#[derive(Component)]
pub struct IndoorGroundPlane;

pub fn setup_indoor_ground(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mesh = meshes.add(Plane3d::default().mesh().size(4000.0, 4000.0));
    let mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.03, 0.05, 0.09),
        unlit: true,
        ..default()
    });
    commands.spawn((
        Mesh3d(mesh),
        MeshMaterial3d(mat),
        Transform::from_xyz(0.0, -0.1, 0.0),
        Visibility::Hidden,
        IndoorGroundPlane,
    ));
}

pub fn indoor_grid_system(
    state: Res<AppState>,
    mut gizmos: Gizmos,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut ground_q: Query<(&MeshMaterial3d<StandardMaterial>, &mut Visibility), With<IndoorGroundPlane>>,
) {
    let map_visible = state.is_3d_map_screen() || state.is_skenario_map_preview_active();
    let is_indoor = map_visible
        && (state.active_map_mode == MapMode::Indoor || !state.active_base_layer.is_outdoor_tile());

    let Ok((mat_handle, mut vis)) = ground_q.single_mut() else {
        return;
    };

    if !is_indoor {
        *vis = Visibility::Hidden;
        return;
    }

    *vis = Visibility::Visible;
    let is_light = matches!(
        state.active_base_layer,
        BaseLayer::GridLight | BaseLayer::BlankLight
    );
    let has_grid = !matches!(
        state.active_base_layer,
        BaseLayer::BlankDark | BaseLayer::BlankLight
    );

    if let Some(mut mat) = materials.get_mut(&mat_handle.0) {
        mat.base_color = if is_light {
            Color::srgb(0.95, 0.97, 0.99)
        } else {
            Color::srgb(0.03, 0.05, 0.08)
        };
    }

    if has_grid {
        let step = if state.is_skenario_map_preview_active() {
            (state.skenario_table.form_grid_size_m as f32).max(1.0)
        } else {
            state
                .active_skenario
                .as_ref()
                .map(|s| s.indoor_config.grid_size_m as f32)
                .filter(|v| *v >= 1.0)
                .unwrap_or(10.0)
        };
        let half_lines = 30;
        let span = step * half_lines as f32;
        let minor_color = if is_light {
            Color::srgba(0.15, 0.23, 0.35, 0.22)
        } else {
            Color::srgba(0.35, 0.52, 0.72, 0.22)
        };
        let major_color = if is_light {
            Color::srgba(0.05, 0.4, 0.65, 0.45)
        } else {
            Color::srgba(0.22, 0.74, 0.97, 0.45)
        };

        for i in -half_lines..=half_lines {
            let offset = i as f32 * step;
            let color = if i % 5 == 0 { major_color } else { minor_color };
            gizmos.line(
                Vec3::new(offset, 0.02, -span),
                Vec3::new(offset, 0.02, span),
                color,
            );
            gizmos.line(
                Vec3::new(-span, 0.02, offset),
                Vec3::new(span, 0.02, offset),
                color,
            );
        }
    }
}
