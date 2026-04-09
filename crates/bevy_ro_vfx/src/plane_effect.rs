use crate::EffectRepeat;
use crate::effect_table::PlaneDef;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

/// Drives a simple 2D or 3D plane effect animation.
///
/// Attached to the child mesh entity spawned by [`spawn_plane_effect`].
/// Both 2D and 3D effects are camera-facing billboards; `angle`/`angle_end` is a Z-axis tilt
/// (clock-hand rotation) applied on top of the camera-facing rotation each frame.
#[derive(Component)]
pub struct PlaneEffectAnimator {
    pub elapsed: f32,
    pub duration: f32,
    pub alpha_max: f32,
    pub color: [f32; 3],
    pub fade_in: bool,
    pub fade_out: bool,
    /// Emitter-relative start position in world units (X offset, Y height offset, 0).
    pub pos_start: Vec3,
    /// Emitter-relative end position in world units.
    pub pos_end: Vec3,
    /// Start size in raw pixel units (divide by 35 for world units).
    pub size_start: Vec2,
    /// End size in raw pixel units.
    pub size_end: Vec2,
    /// Initial Z-tilt angle in radians.
    pub angle_start: f32,
    /// Final Z-tilt angle in radians (equals `angle_start` when there is no tilt animation).
    pub angle_end: f32,
    pub mat_handle: Handle<StandardMaterial>,
    /// Remaining play count; `None` = loop forever.
    pub remaining: Option<u32>,
}

/// Raw pixel units → world units (matches the STR and sprite effect scale).
const SIZE_SCALE: f32 = 1.0 / 35.0;

pub fn spawn_plane_effect(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    server: &AssetServer,
    parent_entity: Entity,
    def: &PlaneDef,
    _camera_facing: bool,
    repeat: EffectRepeat,
) {
    let texture_path = if def.file.is_empty() {
        None
    } else {
        let ext = if def.file.contains('.') { "" } else { ".tga" };
        Some(format!("tex/effect/{}{}", def.file, ext))
    };

    let tex_handle = texture_path.map(|p| server.load::<Image>(p));

    let alpha_mode = if def.blend_additive {
        AlphaMode::Add
    } else {
        AlphaMode::Blend
    };

    let mat = materials.add(StandardMaterial {
        base_color_texture: tex_handle,
        base_color: Color::srgba(
            def.color[0],
            def.color[1],
            def.color[2],
            if def.fade_in { 0.0 } else { def.alpha_max },
        ),
        unlit: true,
        double_sided: true,
        cull_mode: None,
        alpha_mode,
        ..default()
    });

    let mesh_handle = meshes.add(build_plane_mesh());

    // posz is a fixed Y height offset; pos_start/end animate X and Z-height.
    let pos_start = Vec3::new(def.pos_start.x, def.posz + def.pos_start.y, 0.0);
    let pos_end = Vec3::new(def.pos_end.x, def.posz + def.pos_end.y, 0.0);

    let initial_size = def.size_start * SIZE_SCALE;
    let initial_transform = Transform {
        translation: pos_start,
        scale: Vec3::new(initial_size.x, initial_size.y, 1.0),
        ..default()
    };

    let remaining = match repeat {
        EffectRepeat::Infinite => None,
        EffectRepeat::Times(n) => Some(n),
    };

    let animator = PlaneEffectAnimator {
        elapsed: 0.0,
        duration: def.duration_ms / 1000.0,
        alpha_max: def.alpha_max,
        color: def.color,
        fade_in: def.fade_in,
        fade_out: def.fade_out,
        pos_start,
        pos_end,
        size_start: def.size_start,
        size_end: def.size_end,
        angle_start: def.angle.to_radians(),
        angle_end: def.to_angle.to_radians(),
        mat_handle: mat.clone(),
        remaining,
    };

    commands
        .entity(parent_entity)
        .insert(Visibility::Inherited)
        .with_children(|parent| {
            parent.spawn((
                Mesh3d(mesh_handle),
                MeshMaterial3d(mat),
                initial_transform,
                Visibility::Visible,
                animator,
            ));
        });
}

/// Advances all [`PlaneEffectAnimator`] entities each frame.
///
/// Both 2D and 3D plane effects are camera-facing billboards. `angle`/`angle_end` is a Z-axis
/// tilt composed on top of the camera-facing rotation, so the effect rotates like a clock hand
/// while always facing the camera.
pub fn animate_plane_effects(
    mut animators: Query<(&ChildOf, &mut PlaneEffectAnimator, &mut Transform)>,
    parents: Query<&GlobalTransform>,
    camera_q: Query<&Transform, (With<Camera3d>, Without<PlaneEffectAnimator>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    time: Res<Time>,
    mut commands: Commands,
) {
    let Ok(cam) = camera_q.single() else { return };
    let cam_translation = cam.translation;
    let cam_right = cam.rotation * Vec3::X;

    for (child_of, mut anim, mut transform) in &mut animators {
        anim.elapsed += time.delta_secs();

        if anim.elapsed >= anim.duration {
            match anim.remaining {
                None => {
                    anim.elapsed = anim.elapsed.rem_euclid(anim.duration);
                }
                Some(n) => {
                    let next = n.saturating_sub(1);
                    if next == 0 {
                        commands.entity(child_of.parent()).despawn();
                        continue;
                    }
                    anim.remaining = Some(next);
                    anim.elapsed = anim.elapsed.rem_euclid(anim.duration);
                }
            }
        }

        let lerp_t = (anim.elapsed / anim.duration).clamp(0.0, 1.0);

        let alpha = if anim.fade_in && anim.fade_out {
            if lerp_t < 0.5 {
                anim.alpha_max * (lerp_t * 2.0)
            } else {
                anim.alpha_max * (1.0 - lerp_t) * 2.0
            }
        } else if anim.fade_in {
            anim.alpha_max * lerp_t
        } else if anim.fade_out {
            anim.alpha_max * (1.0 - lerp_t)
        } else {
            anim.alpha_max
        };

        let pos = anim.pos_start.lerp(anim.pos_end, lerp_t);
        let size = anim.size_start.lerp(anim.size_end, lerp_t) * SIZE_SCALE;
        let angle = anim.angle_start + (anim.angle_end - anim.angle_start) * lerp_t;

        transform.translation = pos;
        transform.scale = Vec3::new(size.x, size.y, 1.0);

        // Compute camera-facing rotation using the approximate world position of this entity
        // (parent world pos + local offset). Ignores parent rotation/scale, which is fine
        // for map-level emitters that have identity rotation.
        let parent_pos = parents
            .get(child_of.parent())
            .map(|gt| gt.translation())
            .unwrap_or(Vec3::ZERO);
        let world_pos = parent_pos + pos;

        let proj = (world_pos - cam_translation).dot(cam_right);
        let closest = cam_translation + proj * cam_right;
        let face_dir = closest - world_pos;
        let xz_len = Vec2::new(face_dir.x, face_dir.z).length();
        let yaw = f32::atan2(face_dir.x, face_dir.z);
        let max_tilt = 30_f32.to_radians();
        let pitch = (-f32::atan2(face_dir.y, xz_len)).clamp(-max_tilt, max_tilt);
        let cam_rot = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch);

        // Z-tilt is applied on top of the camera-facing rotation (clock-hand rotation of the
        // billboard quad).
        transform.rotation = cam_rot * Quat::from_rotation_z(-angle);

        if let Some(mat) = materials.get_mut(&anim.mat_handle) {
            mat.base_color = Color::srgba(anim.color[0], anim.color[1], anim.color[2], alpha);
        }
    }
}

fn build_plane_mesh() -> Mesh {
    // Unit quad in the XY plane, facing +Z. Same topology as the STR quad mesh.
    let positions: Vec<[f32; 3]> = vec![
        [-0.5, -0.5, 0.0],
        [0.5, -0.5, 0.0],
        [-0.5, 0.5, 0.0],
        [0.5, 0.5, 0.0],
    ];
    let normals: Vec<[f32; 3]> = vec![[0.0, 0.0, 1.0]; 4];
    let uvs: Vec<[f32; 2]> = vec![[0.0, 1.0], [1.0, 1.0], [0.0, 0.0], [1.0, 0.0]];
    let indices = vec![0u32, 1, 2, 1, 3, 2];

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}
