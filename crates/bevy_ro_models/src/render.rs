use crate::assets::RsmAsset;
use bevy::{
    asset::{LoadState, RenderAssetUsages},
    mesh::{Indices, PrimitiveTopology},
    picking::Pickable,
    prelude::*,
};
use ro_files::{RsmMesh, ShadeType};
use std::collections::HashMap;

/// Vertex data accumulated per texture group while building mesh geometry.
type MeshGroup = (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<[f32; 2]>, Vec<u32>);

/// Marker component placed on each RSM model geometry mesh entity.
/// Query for this to identify model meshes (e.g. for occlusion fade).
#[derive(Component)]
pub struct RoModelMesh;

/// Marker placed on the root entity of each materialized RSM model instance.
/// Used to group all `RoModelMesh` children under a single model for whole-model fading.
#[derive(Component)]
pub struct RoModelInstance;

/// Placed on entities by the map crate to request model geometry spawning.
/// Once the RSM asset loads, the model crate replaces this with rendered mesh children
/// and removes the component.
///
/// The entity must already have a `Transform` set to the correct world position, rotation,
/// and scale (converted from RSW space by the map crate).
#[derive(Component)]
pub struct PendingModel {
    pub asset_path: String,
    pub anim_speed: f32,
}

/// Internal: tracks a `PendingModel` entity that is waiting for its RSM asset to load.
#[derive(Component)]
pub(crate) struct LoadingModel {
    pub handle: Handle<RsmAsset>,
    pub anim_speed: f32,
}

/// Per-mesh rotation-keyframe data used by [`RsmAnimator`].
struct AnimNode {
    entity: Entity,
    frames: Vec<(i32, Quat)>,
}

/// Drives RSM1 per-mesh rotation-keyframe animation on a model instance entity.
#[derive(Component)]
pub(crate) struct RsmAnimator {
    anim_speed: f32,
    elapsed_ms: f32,
    nodes: Vec<AnimNode>,
}

/// Watches for newly added [`PendingModel`] components, starts asset loading,
/// and transitions each entity to the [`LoadingModel`] state.
pub(crate) fn start_loading_pending_models(
    mut commands: Commands,
    pending: Query<(Entity, &PendingModel), Without<LoadingModel>>,
    server: Res<AssetServer>,
) {
    for (entity, pending) in &pending {
        let handle: Handle<RsmAsset> = server.load(pending.asset_path.clone());
        commands
            .entity(entity)
            .remove::<PendingModel>()
            .insert(LoadingModel {
                handle,
                anim_speed: pending.anim_speed,
            });
    }
}

/// Polls [`LoadingModel`] entities and, once the RSM asset is loaded, builds the
/// model geometry as child entities and removes the [`LoadingModel`] component.
pub(crate) fn materialize_loading_models(
    mut commands: Commands,
    loading: Query<(Entity, &LoadingModel, &Transform)>,
    rsm_assets: Res<Assets<RsmAsset>>,
    server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, loading, transform) in &loading {
        match server.get_load_state(&loading.handle) {
            Some(LoadState::Loaded) => {
                let Some(rsm_asset) = rsm_assets.get(&loading.handle) else {
                    continue;
                };
                let rsm = &rsm_asset.rsm;
                let inst_scale_neg =
                    transform.scale.x * transform.scale.y * transform.scale.z < 0.0;

                if rsm.version < 0x0200
                    && loading.anim_speed > 0.0
                    && rsm.meshes.iter().any(|m| m.frames.len() > 1)
                {
                    build_animated_rsm1_on(
                        entity,
                        rsm_asset,
                        loading.anim_speed,
                        &mut commands,
                        &mut meshes,
                        &mut materials,
                        &server,
                    );
                } else {
                    if rsm.version >= 0x0200 {
                        let has_rot_anim = rsm.meshes.iter().any(|m| m.frames.len() > 1);
                        warn!(
                            "[RoModel] RSM2 model rendered via static RSM1-style path — \
                             matrix chain differs; {} mesh(es){}, anim_speed={}",
                            rsm.meshes.len(),
                            if has_rot_anim {
                                ", has rotation keyframes (not animated)"
                            } else {
                                ""
                            },
                            loading.anim_speed,
                        );
                    }
                    build_static_model_on(
                        entity,
                        rsm_asset,
                        inst_scale_neg,
                        &mut commands,
                        &mut meshes,
                        &mut materials,
                        &server,
                    );
                }

                commands.entity(entity).insert(RoModelInstance).remove::<LoadingModel>();
            }
            Some(LoadState::Failed(err)) => {
                warn!("[RoModel] failed to load RSM asset: {err}");
                commands.entity(entity).remove::<LoadingModel>();
            }
            _ => {}
        }
    }
}

/// Builds static (non-animated) RSM model geometry as children of `parent`.
///
/// Bakes the full RSM1 vertex transform chain (offset × pos_ × scale × rotation ×
/// pos [non-root] × Y-negate × Z-negate × bb-pivot) into vertex positions so the
/// parent entity's Transform is the only runtime transform needed.
fn build_static_model_on(
    parent: Entity,
    rsm_asset: &RsmAsset,
    inst_scale_neg: bool,
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    asset_server: &AssetServer,
) {
    let rsm = &rsm_asset.rsm;

    // Compute real bounding box across all mesh vertices using the full static transform chain.
    let mut actual_min_y = f32::MAX;
    let mut bb_x_min = f32::MAX;
    let mut bb_x_max = f32::MIN;
    let mut bb_z_min = f32::MAX;
    let mut bb_z_max = f32::MIN;
    for mesh in &rsm.meshes {
        let is_root = mesh.parent_name.is_empty();
        for &raw in &mesh.vertices {
            let m = &mesh.offset;
            let mut p = [
                m[0][0] * raw[0] + m[1][0] * raw[1] + m[2][0] * raw[2],
                m[0][1] * raw[0] + m[1][1] * raw[1] + m[2][1] * raw[2],
                m[0][2] * raw[0] + m[1][2] * raw[1] + m[2][2] * raw[2],
            ];
            p[0] += mesh.pos_[0];
            p[1] += mesh.pos_[1];
            p[2] += mesh.pos_[2];
            p[0] *= mesh.scale[0];
            p[1] *= mesh.scale[1];
            p[2] *= mesh.scale[2];
            if !mesh.frames.is_empty() {
                let rot = Quat::from_array(mesh.frames[0].quaternion).normalize();
                p = (rot * Vec3::from(p)).to_array();
            } else if mesh.rot_angle.abs() > 0.001 {
                let axis = Vec3::from(mesh.rot_axis);
                if axis.length_squared() > 0.0001 {
                    let rot = Quat::from_axis_angle(axis.normalize(), mesh.rot_angle);
                    p = (rot * Vec3::from(p)).to_array();
                }
            }
            if !is_root {
                p[0] += mesh.pos[0];
                p[1] += mesh.pos[1];
                p[2] += mesh.pos[2];
            }
            actual_min_y = actual_min_y.min(-p[1]);
            bb_x_min = bb_x_min.min(p[0]);
            bb_x_max = bb_x_max.max(p[0]);
            bb_z_min = bb_z_min.min(p[2]);
            bb_z_max = bb_z_max.max(p[2]);
        }
    }
    if actual_min_y == f32::MAX {
        actual_min_y = 0.0;
        bb_x_min = 0.0;
        bb_x_max = 0.0;
        bb_z_min = 0.0;
        bb_z_max = 0.0;
    }
    let real_bbrange_x = (bb_x_min + bb_x_max) * 0.5;
    let real_bbrange_z = (bb_z_min + bb_z_max) * 0.5;

    // Pre-compute per-vertex smooth normals when shade_type is Smooth.
    let smooth = matches!(rsm.shade_type, ShadeType::Smooth);
    let mesh_smooth_normals: Vec<HashMap<(u16, i32), Vec3>> = if smooth {
        rsm.meshes
            .iter()
            .map(|mesh| {
                let is_root = mesh.parent_name.is_empty();
                let mut acc: HashMap<(u16, i32), Vec3> = HashMap::new();
                for face in &mesh.faces {
                    let p = |i: usize| {
                        transform_rsm_vertex(
                            mesh.vertices
                                .get(face.vertex_ids[i] as usize)
                                .copied()
                                .unwrap_or_default(),
                            mesh,
                            is_root,
                            actual_min_y,
                            real_bbrange_x,
                            real_bbrange_z,
                        )
                    };
                    let face_normal = (p(1) - p(0)).cross(p(2) - p(0));
                    for corner in 0..3 {
                        *acc.entry((face.vertex_ids[corner], face.smooth_group))
                            .or_insert(Vec3::ZERO) += face_normal;
                    }
                }
                acc
            })
            .collect()
    } else {
        Vec::new()
    };

    let tex_count = rsm.textures.len();
    let mut groups: Vec<MeshGroup> = (0..tex_count.max(1))
        .map(|_| (vec![], vec![], vec![], vec![], vec![]))
        .collect();

    for (mesh_idx, mesh) in rsm.meshes.iter().enumerate() {
        let is_root = mesh.parent_name.is_empty();
        let smooth_normals = mesh_smooth_normals.get(mesh_idx);
        let mesh_scale_neg = mesh.scale[0] * mesh.scale[1] * mesh.scale[2] < 0.0;
        let flip_winding = inst_scale_neg ^ mesh_scale_neg;

        for face in &mesh.faces {
            let tex_slot = face.texture_id as usize;
            let resolved_tex =
                mesh.texture_indices.get(tex_slot).copied().unwrap_or(0) as usize;
            if resolved_tex >= groups.len() {
                continue;
            }

            let (positions, normals, uvs, _uvs1, indices) = &mut groups[resolved_tex];
            let mut tri_verts = [[0.0f32; 3]; 3];
            let mut tri_uvs = [[0.0f32; 2]; 3];

            for corner in 0..3 {
                let vid = face.vertex_ids[corner] as usize;
                let tcid = face.texcoord_ids[corner] as usize;
                let raw = mesh.vertices.get(vid).copied().unwrap_or([0.0; 3]);
                tri_verts[corner] = transform_rsm_vertex(
                    raw,
                    mesh,
                    is_root,
                    actual_min_y,
                    real_bbrange_x,
                    real_bbrange_z,
                )
                .to_array();
                tri_uvs[corner] =
                    mesh.tex_coords.get(tcid).copied().unwrap_or([0.0; 2]);
            }

            let v0 = Vec3::from(tri_verts[0]);
            let v1 = Vec3::from(tri_verts[1]);
            let v2 = Vec3::from(tri_verts[2]);
            let face_normal = (v1 - v0).cross(v2 - v0).normalize();

            let corner_normals: [[f32; 3]; 3] = std::array::from_fn(|corner| {
                if let Some(sn) = smooth_normals {
                    let key = (face.vertex_ids[corner], face.smooth_group);
                    sn.get(&key)
                        .copied()
                        .unwrap_or(face_normal)
                        .normalize()
                        .to_array()
                } else {
                    face_normal.to_array()
                }
            });

            let base = positions.len() as u32;
            for corner in 0..3 {
                positions.push(tri_verts[corner]);
                normals.push(corner_normals[corner]);
                uvs.push(tri_uvs[corner]);
            }
            if flip_winding {
                indices.push(base + 2);
                indices.push(base + 1);
                indices.push(base);
            } else {
                indices.push(base);
                indices.push(base + 1);
                indices.push(base + 2);
            }

            if face.two_sided {
                let base2 = positions.len() as u32;
                let back_order: [usize; 3] = if flip_winding { [0, 1, 2] } else { [2, 1, 0] };
                for corner in back_order {
                    positions.push(tri_verts[corner]);
                    normals.push((-Vec3::from(corner_normals[corner])).to_array());
                    uvs.push(tri_uvs[corner]);
                }
                indices.push(base2);
                indices.push(base2 + 1);
                indices.push(base2 + 2);
            }
        }
    }

    for (tex_idx, (positions, normals, uvs_data, _uvs1, face_indices)) in
        groups.into_iter().enumerate()
    {
        if positions.is_empty() {
            continue;
        }

        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs_data);
        mesh.insert_indices(Indices::U32(face_indices));

        let tex_name = rsm.textures.get(tex_idx).map(|s| s.as_str()).unwrap_or("");
        let texture: Handle<Image> = asset_server.load(tex_name.to_string());
        let material = materials.add(StandardMaterial {
            base_color_texture: Some(texture),
            alpha_mode: AlphaMode::Mask(0.5),
            perceptual_roughness: 1.0,
            reflectance: 0.0,
            double_sided: true,
            cull_mode: None,
            unlit: matches!(rsm.shade_type, ShadeType::Black),
            ..default()
        });

        let mesh_entity = commands
            .spawn((
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(material),
                Transform::default(),
                RoModelMesh,
                Pickable {
                    should_block_lower: false,
                    is_hoverable: false,
                },
            ))
            .id();
        commands.entity(parent).add_child(mesh_entity);
    }
}

/// Applies the full RSM1 vertex transform chain and returns the position in Bevy model space.
fn transform_rsm_vertex(
    raw: [f32; 3],
    mesh: &RsmMesh,
    is_root: bool,
    actual_min_y: f32,
    real_bbrange_x: f32,
    real_bbrange_z: f32,
) -> Vec3 {
    let m = &mesh.offset;
    let mut p = [
        m[0][0] * raw[0] + m[1][0] * raw[1] + m[2][0] * raw[2],
        m[0][1] * raw[0] + m[1][1] * raw[1] + m[2][1] * raw[2],
        m[0][2] * raw[0] + m[1][2] * raw[1] + m[2][2] * raw[2],
    ];
    p[0] += mesh.pos_[0];
    p[1] += mesh.pos_[1];
    p[2] += mesh.pos_[2];
    p[0] *= mesh.scale[0];
    p[1] *= mesh.scale[1];
    p[2] *= mesh.scale[2];
    if !mesh.frames.is_empty() {
        let rot = Quat::from_array(mesh.frames[0].quaternion).normalize();
        p = (rot * Vec3::from(p)).to_array();
    } else if mesh.rot_angle.abs() > 0.001 {
        let axis = Vec3::from(mesh.rot_axis);
        if axis.length_squared() > 0.0001 {
            let rot = Quat::from_axis_angle(axis.normalize(), mesh.rot_angle);
            p = (rot * Vec3::from(p)).to_array();
        }
    }
    if !is_root {
        p[0] += mesh.pos[0];
        p[1] += mesh.pos[1];
        p[2] += mesh.pos[2];
    }
    p[1] = -p[1];
    p[2] = -p[2];
    p[0] -= real_bbrange_x;
    p[1] -= actual_min_y;
    p[2] += real_bbrange_z;
    Vec3::from(p)
}

/// Builds the entity hierarchy for an animated RSM1 model instance.
///
/// The `parent` entity IS the instance root (Transform already set by the map crate).
///
/// ```text
/// parent  (RSW world positioning — set by map crate)
/// └── outer_model  (Y/Z-flip + real_bbrange pivot)
///     └── [per mesh] anim_node  (matrix1: translate × rotate(t) × scale)
///         └── [per texture] geometry  (matrix2 baked into vertices)
/// ```
fn build_animated_rsm1_on(
    parent: Entity,
    rsm_asset: &RsmAsset,
    anim_speed: f32,
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    asset_server: &AssetServer,
) {
    let rsm = &rsm_asset.rsm;

    // Compute real bounding box at t=0.
    let mut actual_min_y = f32::MAX;
    let mut bb_x_min = f32::MAX;
    let mut bb_x_max = f32::MIN;
    let mut bb_z_min = f32::MAX;
    let mut bb_z_max = f32::MIN;
    for mesh in &rsm.meshes {
        let is_root = mesh.parent_name.is_empty();
        for &raw in &mesh.vertices {
            let m = &mesh.offset;
            let mut p = [
                m[0][0] * raw[0] + m[1][0] * raw[1] + m[2][0] * raw[2],
                m[0][1] * raw[0] + m[1][1] * raw[1] + m[2][1] * raw[2],
                m[0][2] * raw[0] + m[1][2] * raw[1] + m[2][2] * raw[2],
            ];
            p[0] += mesh.pos_[0];
            p[1] += mesh.pos_[1];
            p[2] += mesh.pos_[2];
            p[0] *= mesh.scale[0];
            p[1] *= mesh.scale[1];
            p[2] *= mesh.scale[2];
            if !mesh.frames.is_empty() {
                let rot = Quat::from_array(mesh.frames[0].quaternion).normalize();
                p = (rot * Vec3::from(p)).to_array();
            } else if mesh.rot_angle.abs() > 0.001 {
                let axis = Vec3::from(mesh.rot_axis);
                if axis.length_squared() > 0.0001 {
                    p = (Quat::from_axis_angle(axis.normalize(), mesh.rot_angle)
                        * Vec3::from(p))
                    .to_array();
                }
            }
            if !is_root {
                p[0] += mesh.pos[0];
                p[1] += mesh.pos[1];
                p[2] += mesh.pos[2];
            }
            actual_min_y = actual_min_y.min(-p[1]);
            bb_x_min = bb_x_min.min(p[0]);
            bb_x_max = bb_x_max.max(p[0]);
            bb_z_min = bb_z_min.min(p[2]);
            bb_z_max = bb_z_max.max(p[2]);
        }
    }
    if actual_min_y == f32::MAX {
        actual_min_y = 0.0;
        bb_x_min = 0.0;
        bb_x_max = 0.0;
        bb_z_min = 0.0;
        bb_z_max = 0.0;
    }
    let real_bbrange_x = (bb_x_min + bb_x_max) * 0.5;
    let real_bbrange_z = (bb_z_min + bb_z_max) * 0.5;

    // Build name→index map for parent-child wiring.
    let mesh_index_by_name: HashMap<&str, usize> = rsm
        .meshes
        .iter()
        .enumerate()
        .map(|(i, m)| (m.name.as_str(), i))
        .collect();

    // Outer model: Y/Z-flip + real_bbrange pivot. The combined per-vertex steps
    // (Y-flip, Z-flip, pivot) equal Transform { translation: (-rbx,-min_y,rbz), scale: (1,-1,-1) }.
    let outer_model = commands
        .spawn(
            Transform::from_translation(Vec3::new(
                -real_bbrange_x,
                -actual_min_y,
                real_bbrange_z,
            ))
            .with_scale(Vec3::new(1.0, -1.0, -1.0)),
        )
        .id();
    commands.entity(parent).add_child(outer_model);

    // Pre-compute per-mesh smooth normals in matrix2 space.
    let smooth = matches!(rsm.shade_type, ShadeType::Smooth);
    let mesh_smooth_normals: Vec<Option<HashMap<(u16, i32), Vec3>>> = rsm
        .meshes
        .iter()
        .map(|mesh| {
            if !smooth {
                return None;
            }
            let mut acc: HashMap<(u16, i32), Vec3> = HashMap::new();
            for face in &mesh.faces {
                let bake = |i: usize| -> Vec3 {
                    let raw = mesh
                        .vertices
                        .get(face.vertex_ids[i] as usize)
                        .copied()
                        .unwrap_or_default();
                    let m = &mesh.offset;
                    Vec3::new(
                        m[0][0] * raw[0] + m[1][0] * raw[1] + m[2][0] * raw[2] + mesh.pos_[0],
                        m[0][1] * raw[0] + m[1][1] * raw[1] + m[2][1] * raw[2] + mesh.pos_[1],
                        m[0][2] * raw[0] + m[1][2] * raw[1] + m[2][2] * raw[2] + mesh.pos_[2],
                    )
                };
                let fn_ = (bake(1) - bake(0)).cross(bake(2) - bake(0));
                for corner in 0..3 {
                    *acc.entry((face.vertex_ids[corner], face.smooth_group))
                        .or_insert(Vec3::ZERO) += fn_;
                }
            }
            Some(acc)
        })
        .collect();

    // Pass 1: create all anim-node entities with their initial (t=0) transforms.
    let anim_entities: Vec<Entity> = rsm
        .meshes
        .iter()
        .map(|mesh| {
            let is_root = mesh.parent_name.is_empty();
            let mesh_translation = if is_root { Vec3::ZERO } else { Vec3::from(mesh.pos) };
            commands
                .spawn(
                    Transform::from_translation(mesh_translation)
                        .with_rotation(rsm_mesh_initial_quat(mesh))
                        .with_scale(Vec3::from(mesh.scale)),
                )
                .id()
        })
        .collect();

    // Pass 2: wire parent-child, build geometry, collect animated nodes.
    let mut anim_nodes: Vec<AnimNode> = Vec::new();
    let tex_count = rsm.textures.len().max(1);

    for (mesh_idx, mesh) in rsm.meshes.iter().enumerate() {
        let anim_entity = anim_entities[mesh_idx];

        if mesh.parent_name.is_empty() {
            commands.entity(outer_model).add_child(anim_entity);
        } else {
            let parent_anim = mesh_index_by_name
                .get(mesh.parent_name.as_str())
                .and_then(|&pi| anim_entities.get(pi))
                .copied()
                .unwrap_or(outer_model);
            commands.entity(parent_anim).add_child(anim_entity);
        }

        let mut groups: Vec<MeshGroup> = (0..tex_count)
            .map(|_| (vec![], vec![], vec![], vec![], vec![]))
            .collect();
        let smooth_norms = mesh_smooth_normals.get(mesh_idx).and_then(|o| o.as_ref());

        for face in &mesh.faces {
            let tex_slot = face.texture_id as usize;
            let resolved_tex =
                mesh.texture_indices.get(tex_slot).copied().unwrap_or(0) as usize;
            if resolved_tex >= groups.len() {
                continue;
            }

            let (positions, normals, uvs, _uvs1, indices) = &mut groups[resolved_tex];
            let mut tri_verts = [[0.0f32; 3]; 3];
            let mut tri_uvs = [[0.0f32; 2]; 3];

            for corner in 0..3 {
                let vid = face.vertex_ids[corner] as usize;
                let tcid = face.texcoord_ids[corner] as usize;
                let raw = mesh.vertices.get(vid).copied().unwrap_or_default();
                let m = &mesh.offset;
                tri_verts[corner] = [
                    m[0][0] * raw[0] + m[1][0] * raw[1] + m[2][0] * raw[2] + mesh.pos_[0],
                    m[0][1] * raw[0] + m[1][1] * raw[1] + m[2][1] * raw[2] + mesh.pos_[1],
                    m[0][2] * raw[0] + m[1][2] * raw[1] + m[2][2] * raw[2] + mesh.pos_[2],
                ];
                tri_uvs[corner] = mesh.tex_coords.get(tcid).copied().unwrap_or_default();
            }

            let v0 = Vec3::from(tri_verts[0]);
            let v1 = Vec3::from(tri_verts[1]);
            let v2 = Vec3::from(tri_verts[2]);
            let face_normal = (v1 - v0).cross(v2 - v0).normalize();

            let corner_normals: [[f32; 3]; 3] = std::array::from_fn(|corner| {
                if let Some(sn) = smooth_norms {
                    sn.get(&(face.vertex_ids[corner], face.smooth_group))
                        .copied()
                        .unwrap_or(face_normal)
                        .normalize()
                        .to_array()
                } else {
                    face_normal.to_array()
                }
            });

            let base = positions.len() as u32;
            for corner in 0..3 {
                positions.push(tri_verts[corner]);
                normals.push(corner_normals[corner]);
                uvs.push(tri_uvs[corner]);
            }
            indices.extend([base, base + 1, base + 2]);

            if face.two_sided {
                let base2 = positions.len() as u32;
                for corner in [2usize, 1, 0] {
                    positions.push(tri_verts[corner]);
                    normals.push((-Vec3::from(corner_normals[corner])).to_array());
                    uvs.push(tri_uvs[corner]);
                }
                indices.extend([base2, base2 + 1, base2 + 2]);
            }
        }

        for (tex_idx, (pos_data, norm_data, uv_data, _uv1_data, idx_data)) in
            groups.into_iter().enumerate()
        {
            if pos_data.is_empty() {
                continue;
            }

            let mut mesh_geom = Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::default(),
            );
            mesh_geom.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos_data);
            mesh_geom.insert_attribute(Mesh::ATTRIBUTE_NORMAL, norm_data);
            mesh_geom.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv_data);
            mesh_geom.insert_indices(Indices::U32(idx_data));

            let tex_name = rsm.textures.get(tex_idx).map(|s| s.as_str()).unwrap_or("");
            let texture: Handle<Image> = asset_server.load(tex_name.to_string());
            let material = materials.add(StandardMaterial {
                base_color_texture: Some(texture),
                alpha_mode: AlphaMode::Mask(0.5),
                perceptual_roughness: 1.0,
                reflectance: 0.0,
                double_sided: true,
                cull_mode: None,
                unlit: matches!(rsm.shade_type, ShadeType::Black),
                ..default()
            });

            let geom = commands
                .spawn((
                    Mesh3d(meshes.add(mesh_geom)),
                    MeshMaterial3d(material),
                    Transform::default(),
                    RoModelMesh,
                    Pickable {
                        should_block_lower: false,
                        is_hoverable: false,
                    },
                ))
                .id();
            commands.entity(anim_entity).add_child(geom);
        }

        if mesh.frames.len() > 1 {
            let kf: Vec<(i32, Quat)> = mesh
                .frames
                .iter()
                .map(|f| (f.time, Quat::from_array(f.quaternion).normalize()))
                .collect();
            anim_nodes.push(AnimNode {
                entity: anim_entity,
                frames: kf,
            });
        }
    }

    if !anim_nodes.is_empty() {
        let resolved_speed = if anim_speed > 0.0 { anim_speed } else { 1.0 };
        commands.entity(parent).insert(RsmAnimator {
            anim_speed: resolved_speed,
            elapsed_ms: 0.0,
            nodes: anim_nodes,
        });
    }
}

/// Returns the initial (t=0) rotation quaternion for an RSM1 mesh.
fn rsm_mesh_initial_quat(mesh: &RsmMesh) -> Quat {
    if !mesh.frames.is_empty() {
        Quat::from_array(mesh.frames[0].quaternion).normalize()
    } else if mesh.rot_angle.abs() > 0.001 {
        let axis = Vec3::from(mesh.rot_axis);
        if axis.length_squared() > 0.0001 {
            Quat::from_axis_angle(axis.normalize(), mesh.rot_angle)
        } else {
            Quat::IDENTITY
        }
    } else {
        Quat::IDENTITY
    }
}

/// Advances per-mesh rotation keyframe animation for all [`RsmAnimator`] instances.
pub(crate) fn animate_rsm(
    time: Res<Time>,
    mut animators: Query<&mut RsmAnimator>,
    mut node_transforms: Query<&mut Transform, Without<RsmAnimator>>,
) {
    for mut animator in &mut animators {
        animator.elapsed_ms += time.delta_secs() * 1000.0 * animator.anim_speed;
        for node in &animator.nodes {
            let quat = rsm1_interpolate_rotation(&node.frames, animator.elapsed_ms);
            if let Ok(mut t) = node_transforms.get_mut(node.entity) {
                t.rotation = quat;
            }
        }
    }
}

/// BrowEdit-compatible RSM1 keyframe interpolation.
fn rsm1_interpolate_rotation(frames: &[(i32, Quat)], elapsed_ms: f32) -> Quat {
    if frames.is_empty() {
        return Quat::IDENTITY;
    }
    let last_time = frames.last().unwrap().0;
    if last_time == 0 || frames.len() == 1 {
        return frames[0].1;
    }

    let tick = (elapsed_ms as i32).rem_euclid(last_time);

    let mut current: i32 = 0;
    'find: {
        for (i, &(t, _)) in frames.iter().enumerate() {
            if t > tick {
                current = i as i32 - 1;
                break 'find;
            }
        }
    }
    if current < 0 {
        current = 0;
    }

    let mut next = current + 1;
    if next as usize >= frames.len() {
        next = 0;
    }

    let (t_curr, q_curr) = frames[current as usize];
    let (t_next, q_next) = frames[next as usize];

    let denom = (t_next - t_curr) as f32;
    let interval = if denom.abs() < f32::EPSILON {
        0.0
    } else {
        (tick - t_curr) as f32 / denom
    };

    q_curr.slerp(q_next, interval).normalize()
}
