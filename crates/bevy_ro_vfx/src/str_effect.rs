use crate::EffectRepeat;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use ro_files::{StrFile, StrKeyframe};
use std::path::Path;

/// Marker placed on each STR layer mesh entity so [`orient_str_billboards`] rotates them
/// to face the camera each frame.
#[derive(Component)]
pub struct StrBillboard;

/// Per-layer animation state stored inside [`StrEffectAnimator`].
pub struct StrLayerAnim {
    pub entity: Entity,
    pub mesh_handle: Handle<Mesh>,
    /// One handle per texture listed in the layer (pre-loaded at spawn).
    pub tex_handles: Vec<Handle<Image>>,
    pub keyframes: Vec<StrKeyframe>,
    pub tex_count: usize,
    /// Small Z offset applied to this layer's translation to separate coplanar XY quads.
    /// Set to `layer_index * 0.001` at spawn time.
    pub z_offset: f32,
}

/// Drives STR effect animation. Attached to the root STR entity (same as the emitter).
#[derive(Component)]
pub struct StrEffectAnimator {
    pub fps: f32,
    pub maxkey: i32,
    pub elapsed: f32,
    /// Remaining play count; `None` = loop forever.
    pub remaining: Option<u32>,
    pub layers: Vec<StrLayerAnim>,
    /// World-unit scale applied on top of the base STR coordinate conversion (`/ 35.0`).
    pub scale: f32,
}

/// Reads a `.str` file from disk, parses it, and spawns child mesh entities for each layer.
///
/// Attaches [`StrEffectAnimator`] to `parent_entity` so the [`animate_str`] system can
/// drive the animation.
pub fn spawn_str_effect(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    server: &AssetServer,
    assets_root: &Path,
    parent_entity: Entity,
    str_file_stem: &str,
    scale: f32,
    repeat: EffectRepeat,
) {
    let path = assets_root.join(format!("tex/effect/{str_file_stem}.str"));
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            warn!("[RoVfx] Could not read STR file {:?}: {e}", path);
            return;
        }
    };

    let str_file = match StrFile::parse(&bytes) {
        Ok(f) => f,
        Err(e) => {
            warn!("[RoVfx] Could not parse STR file {:?}: {e}", path);
            return;
        }
    };

    let mut layer_anims: Vec<StrLayerAnim> = Vec::new();
    let mut layer_index = 0usize;

    commands
        .entity(parent_entity)
        .insert(Visibility::Inherited)
        .with_children(|parent| {
            for layer in &str_file.layers {
                if layer.textures.is_empty() && layer.keyframes.is_empty() {
                    continue;
                }

                let tex_handles: Vec<Handle<Image>> = layer
                    .textures
                    .iter()
                    .map(|name| server.load(texture_asset_path(name)))
                    .collect();

                let mesh_handle = meshes.add(build_quad_mesh());

                let mat_handle = materials.add(StandardMaterial {
                    unlit: true,
                    double_sided: true,
                    cull_mode: None,
                    alpha_mode: AlphaMode::Add,
                    ..default()
                });

                let z_offset = layer_index as f32 * 0.001;

                let entity = parent
                    .spawn((
                        Mesh3d(mesh_handle.clone()),
                        MeshMaterial3d(mat_handle.clone()),
                        Transform::default(),
                        Visibility::Hidden,
                        StrBillboard,
                    ))
                    .id();

                layer_anims.push(StrLayerAnim {
                    entity,
                    mesh_handle,
                    tex_count: layer.textures.len(),
                    tex_handles,
                    keyframes: layer.keyframes.clone(),
                    z_offset,
                });
                layer_index += 1;
            }
        });

    if !layer_anims.is_empty() {
        commands.entity(parent_entity).insert(StrEffectAnimator {
            fps: str_file.fps as f32,
            maxkey: str_file.maxkey,
            elapsed: 0.0,
            remaining: match repeat {
                EffectRepeat::Infinite => None,
                EffectRepeat::Times(n) => Some(n),
            },
            layers: layer_anims,
            scale,
        });
    }
}

/// Advances STR effect animations. Runs every frame for all [`StrEffectAnimator`] entities.
pub fn animate_str(
    mut animators: Query<(Entity, &mut StrEffectAnimator)>,
    mut layer_queries: Query<(
        &mut MeshMaterial3d<StandardMaterial>,
        &mut Transform,
        &mut Visibility,
    )>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
    time: Res<Time>,
) {
    for (entity, mut anim) in &mut animators {
        anim.elapsed += time.delta_secs();
        let mut current_frame = (anim.elapsed * anim.fps).floor() as i32;

        if current_frame > anim.maxkey {
            match anim.remaining {
                None => {
                    anim.elapsed = 0.0;
                    current_frame = 0;
                }
                Some(n) => {
                    let next = n - 1;
                    if next == 0 {
                        commands.entity(entity).despawn();
                        continue;
                    }
                    anim.remaining = Some(next);
                    anim.elapsed = 0.0;
                    current_frame = 0;
                }
            }
        }

        for layer in &anim.layers {
            update_layer(
                layer,
                current_frame,
                anim.scale,
                &mut layer_queries,
                &mut meshes,
                &mut materials,
            );
        }
    }
}

fn update_layer(
    layer: &StrLayerAnim,
    current_frame: i32,
    scale: f32,
    layer_queries: &mut Query<(
        &mut MeshMaterial3d<StandardMaterial>,
        &mut Transform,
        &mut Visibility,
    )>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let mut start_anim: i32 = -1;
    let mut next_anim: i32 = -1;
    let mut last_frame: i32 = 0;
    let mut last_source: i32 = 0;

    for (i, kf) in layer.keyframes.iter().enumerate() {
        if kf.frame < current_frame {
            if kf.kf_type == 0 {
                start_anim = i as i32;
            }
            if kf.kf_type == 1 {
                next_anim = i as i32;
            }
        }
        last_frame = last_frame.max(kf.frame);
        if kf.kf_type == 0 {
            last_source = last_source.max(kf.frame);
        }
    }

    if start_anim < 0 || (next_anim < 0 && last_frame < current_frame) {
        if let Ok((_, _, mut vis)) = layer_queries.get_mut(layer.entity) {
            *vis = Visibility::Hidden;
        }
        return;
    }

    let from = &layer.keyframes[start_anim as usize];
    let tex_max = layer.tex_count.saturating_sub(1);

    let is_static = next_anim < 0
        || next_anim != start_anim + 1
        || layer.keyframes[next_anim as usize].frame != from.frame;

    if is_static && next_anim >= 0 && last_source <= from.frame {
        if let Ok((_, _, mut vis)) = layer_queries.get_mut(layer.entity) {
            *vis = Visibility::Hidden;
        }
        return;
    }

    let (pos, angle, color, xy, tex_index) = if is_static {
        let tex_idx = (from.aniframe as usize).min(tex_max);
        (from.position, from.angle, from.color, from.xy, tex_idx)
    } else {
        let to = &layer.keyframes[next_anim as usize];
        let delta = (current_frame - from.frame) as f32;
        let n = layer.tex_count as f32;

        let pos = [
            from.position[0] + to.position[0] * delta,
            from.position[1] + to.position[1] * delta,
        ];
        let angle = from.angle + to.angle * delta;
        let color = std::array::from_fn(|i| from.color[i] + to.color[i] * delta);
        let xy = std::array::from_fn(|i| {
            std::array::from_fn(|j| from.xy[i][j] + to.xy[i][j] * delta)
        });

        let tex_idx = match to.anitype {
            0 => from.aniframe as usize,
            1 => (from.aniframe + to.aniframe * delta).floor() as usize,
            2 => (from.aniframe + to.delay * delta).min(n - 1.0).floor() as usize,
            3 => ((from.aniframe + to.delay * delta).rem_euclid(n)).floor() as usize,
            4 => ((from.aniframe - to.delay * delta).rem_euclid(n)).floor() as usize,
            _ => 0,
        }
        .min(tex_max);

        (pos, angle, color, xy, tex_idx)
    };

    if let Some(mesh) = meshes.get_mut(&layer.mesh_handle) {
        let angle_rad = -angle.to_radians();
        // Vertices in the local XY plane, matching the Unity reference (Z=0).
        let positions: Vec<[f32; 3]> = xy
            .iter()
            .map(|&[x, y]| {
                let (rx, ry) = rotate2d(x, y, angle_rad);
                [rx / 35.0 * scale, ry / 35.0 * scale, 0.0]
            })
            .collect();
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    }

    if let Ok((mat_handle, mut transform, mut vis)) = layer_queries.get_mut(layer.entity) {
        // Layers stand vertically in the local XY plane, matching the Unity reference.
        // z_offset separates coplanar quads to prevent z-fighting.
        transform.translation = Vec3::new(
            (pos[0] - 320.0) / 35.0 * scale,
            -(pos[1] - 320.0) / 35.0 * scale,
            layer.z_offset,
        );
        *vis = Visibility::Inherited;

        if !layer.tex_handles.is_empty()
            && let Some(mat) = materials.get_mut(&mat_handle.0)
        {
            mat.base_color_texture = Some(layer.tex_handles[tex_index].clone());
            mat.base_color = Color::srgba(
                color[0] / 255.0,
                color[1] / 255.0,
                color[2] / 255.0,
                color[3] / 255.0,
            );
        }
    }
}

/// Rotates every [`StrBillboard`] layer entity to face the camera, using the parent
/// entity's world position as the pivot. Mirrors `orient_billboard` from `bevy_ro_sprites`.
pub fn orient_str_billboards(
    mut billboards: Query<(&mut Transform, &ChildOf), (With<StrBillboard>, Without<Camera3d>)>,
    parents: Query<&GlobalTransform>,
    camera_q: Query<&Transform, (With<Camera3d>, Without<StrBillboard>)>,
) {
    let Ok(cam) = camera_q.single() else { return };
    let cam_right = cam.rotation * Vec3::X;

    for (mut tf, child_of) in &mut billboards {
        let pivot = parents
            .get(child_of.parent())
            .map(|gt| gt.translation())
            .unwrap_or(Vec3::ZERO);

        let t = (pivot - cam.translation).dot(cam_right);
        let closest = cam.translation + t * cam_right;
        let face_dir = closest - pivot;
        let xz_len = Vec2::new(face_dir.x, face_dir.z).length();
        let yaw = f32::atan2(face_dir.x, face_dir.z);
        let max_tilt = 30_f32.to_radians();
        let pitch = (-f32::atan2(face_dir.y, xz_len)).clamp(-max_tilt, max_tilt);
        tf.rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch);
    }
}

fn rotate2d(x: f32, y: f32, angle_rad: f32) -> (f32, f32) {
    let (sin, cos) = angle_rad.sin_cos();
    (x * cos - y * sin, x * sin + y * cos)
}

/// Converts a raw texture filename from the STR binary to a Bevy asset path.
///
/// After the importer runs, `.bmp` extensions have already been rewritten to `.png`.
/// The `.bmp` fallback here handles assets extracted before the importer update.
fn texture_asset_path(raw_name: &str) -> String {
    let stem = raw_name.rfind('.').map(|i| &raw_name[..i]).unwrap_or(raw_name);
    let ext = if raw_name.to_ascii_lowercase().ends_with(".bmp") {
        "png"
    } else {
        raw_name
            .rfind('.')
            .map(|i| &raw_name[i + 1..])
            .unwrap_or("tga")
    };
    format!("tex/effect/{stem}.{ext}")
}

/// Creates a placeholder deformable quad mesh with `MAIN_WORLD | RENDER_WORLD` usage
/// so vertex positions can be updated every frame via `Assets<Mesh>::get_mut`.
fn build_quad_mesh() -> Mesh {
    let positions: Vec<[f32; 3]> = vec![[0.0, 0.0, 0.0]; 4];
    // Normals match Unity's Vector3.back: quads stand in the XY plane facing along -Z.
    // double_sided:true makes both faces visible regardless.
    let normals: Vec<[f32; 3]> = vec![[0.0, 0.0, -1.0]; 4];
    let uvs: Vec<[f32; 2]> = vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]];
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
