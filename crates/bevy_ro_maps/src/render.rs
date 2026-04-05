use crate::assets::RoMapAsset;
use bevy::{
    asset::{LoadState, RenderAssetUsages},
    mesh::{Indices, PrimitiveTopology},
    picking::Pickable,
    prelude::*,
};
use bevy_ro_models::RsmAsset;
use ro_files::{ModelInstance, RsmMesh, RswLighting, RswObject, ShadeType};
use std::collections::HashMap;
use crate::navigation::NavMesh;

/// Vertex data accumulated per texture group while building mesh geometry.
type MeshGroup = (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<u32>);

/// Marker component placed on each terrain mesh entity spawned by the plugin.
#[derive(Component)]
pub struct RoMapMesh;

/// Place this component on a root entity to have the plugin spawn terrain mesh children once
/// the referenced [`RoMapAsset`] has loaded.
///
/// ```rust,no_run
/// # use bevy::prelude::*;
/// # use bevy_ro_maps::render::RoMapRoot;
/// # fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
/// commands.spawn((
///     RoMapRoot { asset: asset_server.load("maps/prontera.gnd"), spawned: false },
///     Transform::default(),
///     Visibility::default(),
/// ));
/// # }
/// ```
#[derive(Component)]
pub struct RoMapRoot {
    pub asset: Handle<RoMapAsset>,
    /// Set to `true` by the plugin once mesh children have been spawned. Prevents re-spawning
    /// on subsequent frames.
    pub spawned: bool,
}

/// Map grid dimensions in world units, derived once from the GND header.
#[derive(Clone, Copy)]
pub(crate) struct MapDims {
    /// GND tile scale (always 10.0 in practice).
    pub scale: f32,
    /// Half the map width (= width * scale * 0.5).
    pub cx: f32,
    /// Half the map height (= height * scale * 0.5).
    pub cz: f32,
}

/// Animates the water plane by cycling through pre-loaded texture frames.
#[derive(Component)]
pub(crate) struct WaterAnimator {
    pub frames: Vec<Handle<Image>>,
    pub interval_secs: f32,
    pub elapsed: f32,
    pub current_frame: usize,
}

/// Tracks RSM model instances that are still waiting for their asset to finish loading.
#[derive(Component)]
pub(crate) struct PendingModels {
    pub instances: Vec<(Handle<RsmAsset>, ModelInstance)>,
    pub dims: MapDims,
}

/// Fired once when a map finishes spawning. Carries the lighting parameters from the RSW file.
#[derive(Event, Clone)]
pub struct MapLightingReady(pub RswLighting);

pub(crate) fn spawn_map_meshes(
    mut commands: Commands,
    mut map_roots: Query<(Entity, &mut RoMapRoot)>,
    map_assets: Res<Assets<RoMapAsset>>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (root_entity, mut root) in &mut map_roots {
        if root.spawned {
            continue;
        }
        let load_state = asset_server.get_load_state(&root.asset);
        let Some(map) = map_assets.get(&root.asset) else {
            info!("[RoMap] asset not ready yet, load state: {:?}", load_state);
            continue;
        };
        info!(
            "[RoMap] asset loaded — grid {}x{}, scale {}, {} textures, {} surfaces, {} cubes",
            map.gnd.width,
            map.gnd.height,
            map.gnd.scale,
            map.gnd.texture_paths.len(),
            map.gnd.surfaces.len(),
            map.gnd.cubes.len()
        );
        root.spawned = true;
        commands.trigger(MapLightingReady(map.lighting.clone()));

        let gnd = &map.gnd;
        let scale = gnd.scale;


        commands.entity(root_entity).insert(NavMesh {
            terrain_width: gnd.width as f32 * scale,
            terrain_height: gnd.height as f32 * scale,
            nav_width: map.gat.width as i32,
            nav_height: map.gat.height as i32,
            tiles: map.gat.tiles.clone(),
        });

        // Half-extents used for the centering Transform applied to the root entity below,
        // and for converting model instance positions from BrowEdit3 world space.
        let cx = gnd.width as f32 * scale * 0.5;
        let cz = gnd.height as f32 * scale * 0.5;

        // Group top surfaces by texture_id so we emit one mesh per texture.
        // Each entry: (positions, normals, uvs, indices).
        let texture_count = gnd.texture_paths.len();
        let mut groups: Vec<MeshGroup> = (0..texture_count)
            .map(|_| (vec![], vec![], vec![], vec![]))
            .collect();

        // Pre-compute smooth normals at shared grid corners by accumulating (area-weighted)
        // face normals from all adjacent tiles. Corner (r, c) maps to index r*(width+1)+c.
        // Tile (row, col) contributes to corners NW=(row,col), NE=(row,col+1),
        // SW=(row+1,col), SE=(row+1,col+1).
        let corner_cols = (gnd.width + 1) as usize;
        let mut corner_normals: Vec<Vec3> =
            vec![Vec3::ZERO; (gnd.height + 1) as usize * corner_cols];
        for row in 0..gnd.height {
            for col in 0..gnd.width {
                let cube = &gnd.cubes[(row * gnd.width + col) as usize];
                let x0 = col as f32 * scale;
                let x1 = (col + 1) as f32 * scale;
                let z_nw = gnd.height as f32 * scale - row as f32 * scale;
                let z_sw = z_nw + scale;
                let sw = Vec3::new(x0, -cube.heights[0], z_sw);
                let se = Vec3::new(x1, -cube.heights[1], z_sw);
                let nw = Vec3::new(x0, -cube.heights[2], z_nw);
                let face_normal = (se - sw).cross(nw - sw); // area-weighted, not normalized
                let r = row as usize;
                let c = col as usize;
                corner_normals[r * corner_cols + c] += face_normal; // NW
                corner_normals[r * corner_cols + c + 1] += face_normal; // NE
                corner_normals[(r + 1) * corner_cols + c] += face_normal; // SW
                corner_normals[(r + 1) * corner_cols + c + 1] += face_normal; // SE
            }
        }
        for n in &mut corner_normals {
            *n = n.normalize_or_zero();
        }

        for row in 0..gnd.height {
            for col in 0..gnd.width {
                let cube = &gnd.cubes[(row * gnd.width + col) as usize];

                if cube.top_surface_id < 0 {
                    continue;
                }
                let surface = &gnd.surfaces[cube.top_surface_id as usize];
                if surface.texture_id < 0 {
                    continue;
                }
                let tex_idx = surface.texture_id as usize;
                if tex_idx >= texture_count {
                    continue;
                }

                // Build vertices in BrowEdit3 world space (no centering yet).
                // BrowEdit3 GndRenderer.cpp lines 316-319:
                //   NW (h3/heights[2]) at Z = 10*(height-y)
                //   SW (h1/heights[0]) at Z = 10*(height-y) + 10
                // The root entity's centering Transform (applied below) shifts everything
                // to [-cx..cx] x [-cz..cz] in final world space.
                let x0 = col as f32 * scale;
                let x1 = (col + 1) as f32 * scale;
                let z_nw = gnd.height as f32 * scale - row as f32 * scale;
                let z_sw = z_nw + scale;

                // Negate heights: RO is Y-down, Bevy is Y-up.
                // heights[0] = BL = SW, heights[1] = BR = SE,
                // heights[2] = TL = NW, heights[3] = TR = NE.
                let sw = Vec3::new(x0, -cube.heights[0], z_sw);
                let se = Vec3::new(x1, -cube.heights[1], z_sw);
                let nw = Vec3::new(x0, -cube.heights[2], z_nw);
                let ne = Vec3::new(x1, -cube.heights[3], z_nw);

                // Smooth normals: look up the averaged corner normal for each vertex.
                let r = row as usize;
                let c = col as usize;
                let n_sw = corner_normals[(r + 1) * corner_cols + c].to_array();
                let n_se = corner_normals[(r + 1) * corner_cols + c + 1].to_array();
                let n_nw = corner_normals[r * corner_cols + c].to_array();
                let n_ne = corner_normals[r * corner_cols + c + 1].to_array();

                let (positions, normals, uvs, indices) = &mut groups[tex_idx];

                let base = positions.len() as u32;

                // Vertices: 0=sw, 1=se, 2=nw, 3=ne
                positions.push(sw.to_array());
                positions.push(se.to_array());
                positions.push(nw.to_array());
                positions.push(ne.to_array());

                normals.push(n_sw);
                normals.push(n_se);
                normals.push(n_nw);
                normals.push(n_ne);

                // UVs: match vertex order above.
                uvs.push([surface.u[0], surface.v[0]]);
                uvs.push([surface.u[1], surface.v[1]]);
                uvs.push([surface.u[2], surface.v[2]]);
                uvs.push([surface.u[3], surface.v[3]]);

                // Two CCW triangles viewed from above (+Y): normal points +Y.
                // sw(0)→se(1)→nw(2)  and  se(1)→ne(3)→nw(2)
                indices.push(base);
                indices.push(base + 1);
                indices.push(base + 2);

                indices.push(base + 1);
                indices.push(base + 3);
                indices.push(base + 2);
            }
        }

        // Build wall geometry for north and east surfaces.
        for row in 0..gnd.height {
            for col in 0..gnd.width {
                let cube = &gnd.cubes[(row * gnd.width + col) as usize];

                let x0 = col as f32 * scale;
                let x1 = (col + 1) as f32 * scale;
                let z_nw = gnd.height as f32 * scale - row as f32 * scale;
                let z_sw = z_nw + scale;

                // North wall: shared edge at Z = z_nw. z_sw of tile (row+1) == z_nw of this
                // tile, so the correct neighbor is row+1, not row-1.
                if cube.north_surface_id >= 0 {
                    if let Some(surf) = gnd.surfaces.get(cube.north_surface_id as usize) {
                        let tex_idx = surf.texture_id as usize;
                        if surf.texture_id >= 0 && tex_idx < texture_count {
                            if let Some(north) = gnd.cube(col, row + 1) {
                                // v0=BL, v1=BR, v2=TL, v3=TR
                                // Top vertices come from neighbor's south edge (heights[0]=SW, heights[1]=SE).
                                let v0 = Vec3::new(x0, -cube.heights[2], z_nw);
                                let v1 = Vec3::new(x1, -cube.heights[3], z_nw);
                                let v2 = Vec3::new(x0, -north.heights[0], z_nw);
                                let v3 = Vec3::new(x1, -north.heights[1], z_nw);
                                // North wall lies in the Z-plane; normal is always +Z (faces south).
                                let n = [0.0_f32, 0.0, 1.0];

                                let (positions, normals, uvs, indices) = &mut groups[tex_idx];
                                let base = positions.len() as u32;
                                positions.extend([v0.to_array(), v1.to_array(), v2.to_array(), v3.to_array()]);
                                normals.extend([n, n, n, n]);
                                uvs.push([surf.u[0], surf.v[0]]);
                                uvs.push([surf.u[1], surf.v[1]]);
                                uvs.push([surf.u[2], surf.v[2]]);
                                uvs.push([surf.u[3], surf.v[3]]);
                                // T1: v0→v1→v2, T2: v1→v3→v2
                                indices.extend([base, base + 1, base + 2, base + 1, base + 3, base + 2]);
                            }
                        }
                    }
                }

                // East wall: shared edge at X = x1, between this tile and col+1.
                if cube.east_surface_id >= 0 {
                    if let Some(surf) = gnd.surfaces.get(cube.east_surface_id as usize) {
                        let tex_idx = surf.texture_id as usize;
                        if surf.texture_id >= 0 && tex_idx < texture_count {
                            if let Some(east) = gnd.cube(col + 1, row) {
                                // v0=BS, v1=BN, v2=TS, v3=TN
                                let v0 = Vec3::new(x1, -cube.heights[1], z_sw);
                                let v1 = Vec3::new(x1, -cube.heights[3], z_nw);
                                let v2 = Vec3::new(x1, -east.heights[0], z_sw);
                                let v3 = Vec3::new(x1, -east.heights[2], z_nw);
                                // East wall lies in the X-plane; normal is always -X (faces west).
                                let n = [-1.0_f32, 0.0, 0.0];

                                let (positions, normals, uvs, indices) = &mut groups[tex_idx];
                                let base = positions.len() as u32;
                                positions.extend([v0.to_array(), v1.to_array(), v2.to_array(), v3.to_array()]);
                                normals.extend([n, n, n, n]);
                                uvs.push([surf.u[0], surf.v[0]]);
                                uvs.push([surf.u[1], surf.v[1]]);
                                uvs.push([surf.u[2], surf.v[2]]);
                                uvs.push([surf.u[3], surf.v[3]]);
                                // T1: v1→v0→v2, T2: v1→v2→v3
                                indices.extend([base + 1, base, base + 2, base + 1, base + 2, base + 3]);
                            }
                        }
                    }
                }
            }
        }

        // Spawn water plane if present (sourced from RSW for older maps, or GND for v2.6+).
        if let Some(water) = &map.water {
            let y = -water.level;
            let n = [0.0_f32, 1.0, 0.0];
            let mut positions: Vec<[f32; 3]> = Vec::new();
            let mut normals: Vec<[f32; 3]> = Vec::new();
            let mut uvs: Vec<[f32; 2]> = Vec::new();
            let mut indices: Vec<u32> = Vec::new();

            for row in 0..gnd.height {
                for col in 0..gnd.width {
                    let x0 = col as f32 * scale;
                    let x1 = (col + 1) as f32 * scale;
                    let z_nw = gnd.height as f32 * scale - row as f32 * scale;
                    let z_sw = z_nw + scale;

                    let base = positions.len() as u32;
                    positions.extend([
                        [x0, y, z_sw], // SW
                        [x1, y, z_sw], // SE
                        [x0, y, z_nw], // NW
                        [x1, y, z_nw], // NE
                    ]);
                    normals.extend([n, n, n, n]);
                    uvs.extend([[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]]);
                    indices.extend([base, base + 1, base + 2, base + 1, base + 3, base + 2]);
                }
            }

            let mut water_mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
            water_mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
            water_mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
            water_mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
            water_mesh.insert_indices(Indices::U32(indices));

            let water_type = water.water_type;
            let frames: Vec<Handle<Image>> = (0..32)
                .map(|f| asset_server.load(format!("tex/e_water/water{}{:02}.jpg", water_type, f)))
                .collect();

            let interval_secs = if water.texture_cycling_interval > 0 {
                water.texture_cycling_interval as f32 / 60.0
            } else {
                1.0 / 30.0
            };

            let material = materials.add(StandardMaterial {
                base_color_texture: Some(frames[0].clone()),
                base_color: Color::srgba(1.0, 1.0, 1.0, 0.7),
                alpha_mode: AlphaMode::Blend,
                double_sided: true,
                cull_mode: None,
                ..default()
            });

            let water_entity = commands
                .spawn((
                    Mesh3d(meshes.add(water_mesh)),
                    MeshMaterial3d(material),
                    Transform::default(),
                    WaterAnimator { frames, interval_secs, elapsed: 0.0, current_frame: 0 },
                ))
                .id();
            commands.entity(root_entity).add_child(water_entity);
        }

        let total_verts: usize = groups.iter().map(|(p, _, _, _)| p.len()).sum();
        let non_empty = groups.iter().filter(|(p, _, _, _)| !p.is_empty()).count();
        let all_positions: Vec<[f32; 3]> = groups
            .iter()
            .flat_map(|(p, _, _, _)| p.iter().copied())
            .collect();
        if !all_positions.is_empty() {
            let min = all_positions.iter().fold([f32::MAX; 3], |acc, p| {
                [acc[0].min(p[0]), acc[1].min(p[1]), acc[2].min(p[2])]
            });
            let max = all_positions.iter().fold([f32::MIN; 3], |acc, p| {
                [acc[0].max(p[0]), acc[1].max(p[1]), acc[2].max(p[2])]
            });
            info!("[RoMap] mesh AABB  min {:?}  max {:?}", min, max);
        }
        info!(
            "[RoMap] built {} non-empty mesh groups, {} total vertices",
            non_empty, total_verts
        );

        // Spawn child mesh entities
        let mut children: Vec<Entity> = Vec::new();
        for (tex_idx, (positions, normals, uvs, indices)) in groups.into_iter().enumerate() {
            if positions.is_empty() {
                continue;
            }

            let vert_count = positions.len();
            let mut mesh = Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::default(),
            );
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
            mesh.insert_indices(Indices::U32(indices));

            let texture_path = &gnd.texture_paths[tex_idx];
            info!(
                "[RoMap] spawning mesh group {} — {} verts, texture: {}",
                tex_idx, vert_count, texture_path
            );

            let texture: Handle<Image> = asset_server.load(texture_path);

            let material = materials.add(StandardMaterial {
                base_color_texture: Some(texture),
                alpha_mode: AlphaMode::Mask(0.5),
                perceptual_roughness: 1.0,
                double_sided: true,
                cull_mode: None,
                ..default()
            });

            let child = commands
                .spawn((
                    Mesh3d(meshes.add(mesh)),
                    MeshMaterial3d(material),
                    Transform::default(),
                    RoMapMesh,
                    Pickable {
                        should_block_lower: true,
                        is_hoverable: true,
                    },
                ))
                .id();
            children.push(child);
        }

        if !children.is_empty() {
            commands.entity(root_entity).add_children(&children);
        }

        // Center the map at the world origin via the root entity's Transform.
        // Terrain is built in BrowEdit3 world space: X in [0, 2*cx], Z in [scale, 2*cz+scale].
        // This Transform shifts it to [-cx..cx] x [-cz..cz].
        commands
            .entity(root_entity)
            .insert(Transform::from_translation(Vec3::new(
                -cx,
                0.0,
                -(scale + cz),
            )));

        // Kick off RSM model instance loading.
        let model_instances: Vec<ModelInstance> = map
            .objects
            .iter()
            .filter_map(|obj| {
                if let RswObject::Model(inst) = obj {
                    Some(inst.clone())
                } else {
                    None
                }
            })
            .collect();

        if !model_instances.is_empty() {
            let unique_files: std::collections::HashSet<&str> = model_instances
                .iter()
                .map(|inst| inst.model_file.as_str())
                .collect();
            info!(
                "[RoMap] {} model instance(s) ({} unique file(s)) queued for loading",
                model_instances.len(),
                unique_files.len()
            );

            let pending: Vec<(Handle<RsmAsset>, ModelInstance)> = model_instances
                .into_iter()
                .map(|inst| {
                    let handle = asset_server.load(inst.model_file.clone());
                    (handle, inst)
                })
                .collect();

            commands.entity(root_entity).insert(PendingModels {
                instances: pending,
                dims: MapDims { scale, cx, cz },
            });
        }
    }
}

pub(crate) fn spawn_model_meshes(
    mut commands: Commands,
    mut pending_query: Query<(Entity, &mut PendingModels)>,
    rsm_assets: Res<Assets<RsmAsset>>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (root_entity, mut pending) in &mut pending_query {
        let mut still_pending: Vec<(Handle<RsmAsset>, ModelInstance)> = Vec::new();
        let instances = std::mem::take(&mut pending.instances);

        for (handle, inst) in instances {
            match asset_server.get_load_state(&handle) {
                Some(LoadState::Loaded) => {
                    if let Some(rsm_asset) = rsm_assets.get(&handle) {
                        let children = build_model_children(
                            &inst,
                            rsm_asset,
                            pending.dims,
                            &mut commands,
                            &mut meshes,
                            &mut materials,
                            &asset_server,
                        );
                        if !children.is_empty() {
                            commands.entity(root_entity).add_children(&children);
                        }
                    }
                }
                Some(LoadState::Failed(err)) => {
                    warn!("[RoModel] failed to load '{}': {err}", inst.model_file);
                }
                _ => {
                    still_pending.push((handle, inst));
                }
            }
        }

        pending.instances = still_pending;

        if pending.instances.is_empty() {
            commands.entity(root_entity).remove::<PendingModels>();
        }
    }
}

/// Applies the full RSM vertex transform chain (offset matrix → pos_ → scale →
/// rotation → pos [non-root] → Y-negate → Z-negate → bb pivot) and returns the
/// resulting position in Bevy world space (model-local, before the instance transform).
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

fn build_model_children(
    inst: &ModelInstance,
    rsm_asset: &RsmAsset,
    dims: MapDims,
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    asset_server: &AssetServer,
) -> Vec<Entity> {
    let rsm = &rsm_asset.rsm;
    let MapDims {
        scale: gnd_scale,
        cx,
        cz,
    } = dims;

    // Build the instance Transform in BrowEdit3 world space to match the terrain mesh,
    // which is also built in BrowEdit3 world space. The root entity's centering Transform
    // (Vec3::new(-cx, 0, -(scale+cz))) is inherited by all children including model instances.
    //
    // BrowEdit3 RsmRenderer.cpp line 117 (after outer Scale(1,1,-1)):
    //   X = 5*width + pos.x  = cx + pos.x
    //   Y = -pos.y
    //   Z = 10 + 5*height - pos.z  = gnd_scale + cz - pos.z
    let translation = Vec3::new(cx + inst.pos[0], -inst.pos[1], gnd_scale + cz - inst.pos[2]);
    // With Z negated in mesh geometry (step 7 below), the outer scale(1,1,-1) is baked in.
    // Conjugation by Scale(1,1,-1): Ry(+y)→Ry(-y), Rx(-x)→Rx(+x), Rz(-z)→Rz(-z).
    let rotation = Quat::from_euler(
        EulerRot::YXZ,
        (-inst.rot[1]).to_radians(), // -ry (conjugated by Z-flip)
        inst.rot[0].to_radians(),    // +rx (double-negated: -rx then conjugated)
        (-inst.rot[2]).to_radians(), // -rz (unchanged by Z-flip conjugation)
    );
    let scale = Vec3::new(inst.scale[0], inst.scale[1], inst.scale[2]);
    // Negative determinant = odd number of negative scale components.
    // In that case the instance transform reverses face winding, so we pre-flip the index
    // order in the baked geometry to keep faces front-facing in world space. Without this,
    // double_sided:true would see the (now back-facing) face and negate the normal a second
    // time, undoing Bevy's correct normal-matrix flip and making the model dark.
    let inst_scale_neg = inst.scale[0] * inst.scale[1] * inst.scale[2] < 0.0;

    // Compute the full-transform bounding box (replaces the offset-only rsm.bbrange). across all mesh vertices using the same transform
    // chain applied in the face loop (offset → pos_ → scale → rotation → pos for non-root).
    // This matches BrowEdit3's setBoundingBox2 (which applies Scale(1,-1,1)*matrix1*matrix2;
    // Scale(1,-1,1) doesn't affect X/Z, so the X/Z center is equivalent).
    //
    // actual_min_y corresponds to realbbmin.y (grounding the model base at Y=0).
    // real_bbrange_x/z corresponds to realbbrange.x/z (centering the model in X/Z).
    // Using these instead of the offset-only bbrange from rsm.bbrange is essential for meshes
    // with non-trivial per-mesh rotation or scale, where the offset-only center is wrong.
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
            actual_min_y = actual_min_y.min(-p[1]); // Y-negated Bevy Y
            bb_x_min = bb_x_min.min(p[0]);
            bb_x_max = bb_x_max.max(p[0]);
            bb_z_min = bb_z_min.min(p[2]); // RSM Z (pre-negate)
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
    let real_bbrange_z = (bb_z_min + bb_z_max) * 0.5; // RSM Z space; pivot adds this post-Z-negate

    // Pre-compute per-vertex smooth normals when shade_type is Smooth.
    // Keyed by (vertex_id, smooth_group) so hard edges between smooth groups are preserved.
    // Face normals are accumulated (area-weighted by magnitude) then normalized on lookup.
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

    // Build mesh geometry per texture, collecting all face data in model space.
    // Keyed by the resolved RsmFile::textures index.
    let tex_count = rsm.textures.len();
    let mut groups: Vec<MeshGroup> = (0..tex_count.max(1))
        .map(|_| (vec![], vec![], vec![], vec![]))
        .collect();

    for (mesh_idx, mesh) in rsm.meshes.iter().enumerate() {
        let is_root = mesh.parent_name.is_empty();
        let smooth_normals = mesh_smooth_normals.get(mesh_idx);
        // mesh.scale parity XOR inst parity: flip if exactly one of them is negative-det.
        let mesh_scale_neg = mesh.scale[0] * mesh.scale[1] * mesh.scale[2] < 0.0;
        let flip_winding = inst_scale_neg ^ mesh_scale_neg;

        for face in &mesh.faces {
            let tex_slot = face.texture_id as usize;
            let resolved_tex = mesh.texture_indices.get(tex_slot).copied().unwrap_or(0) as usize;
            if resolved_tex >= groups.len() {
                continue;
            }

            let (positions, normals, uvs, indices) = &mut groups[resolved_tex];

            let mut tri_verts = [[0.0f32; 3]; 3];
            let mut tri_uvs = [[0.0f32; 2]; 3];

            for corner in 0..3 {
                let vid = face.vertex_ids[corner] as usize;
                let tcid = face.texcoord_ids[corner] as usize;

                let raw = mesh.vertices.get(vid).copied().unwrap_or([0.0; 3]);

                // 1. Apply 3×3 offset matrix (column-major). Matches browedit matrix2.
                let m = &mesh.offset;
                let mut p = [
                    m[0][0] * raw[0] + m[1][0] * raw[1] + m[2][0] * raw[2],
                    m[0][1] * raw[0] + m[1][1] * raw[1] + m[2][1] * raw[2],
                    m[0][2] * raw[0] + m[1][2] * raw[1] + m[2][2] * raw[2],
                ];

                // 2. Secondary translation (pos_). Applied before scale/rotation in matrix2.
                p[0] += mesh.pos_[0];
                p[1] += mesh.pos_[1];
                p[2] += mesh.pos_[2];

                // 3. Per-mesh scale (browedit matrix1: Scale(scale) applied before rotation).
                p[0] *= mesh.scale[0];
                p[1] *= mesh.scale[1];
                p[2] *= mesh.scale[2];

                // 4. Static rotation or first keyframe quaternion (browedit matrix1).
                if !mesh.frames.is_empty() {
                    let q = mesh.frames[0].quaternion; // [x, y, z, w]
                    let rot = Quat::from_array(q).normalize();
                    p = (rot * Vec3::from(p)).to_array();
                } else if mesh.rot_angle.abs() > 0.001 {
                    let axis = Vec3::from(mesh.rot_axis);
                    if axis.length_squared() > 0.0001 {
                        let rot = Quat::from_axis_angle(axis.normalize(), mesh.rot_angle);
                        p = (rot * Vec3::from(p)).to_array();
                    }
                }

                // 5. Non-root: parent-relative translation (browedit matrix1: Translate(pos)).
                if !is_root {
                    p[0] += mesh.pos[0];
                    p[1] += mesh.pos[1];
                    p[2] += mesh.pos[2];
                }

                // 6. Negate Y (RO Y-down → Bevy Y-up).
                p[1] = -p[1];

                // 7. Negate Z (bakes browedit's outer Scale(1,1,-1) into mesh geometry).
                p[2] = -p[2];

                // 8. Apply bounding box pivot so the model sits centered and grounded.
                // BrowEdit3 RsmRenderer.cpp:126 applies (-realbbrange.x, realbbmin.y, -realbbrange.z)
                // as an instance-level pivot to ALL RSM v1.x models.
                // real_bbrange_x/z are computed from the full per-mesh transform chain, matching
                // BrowEdit3's setBoundingBox2. actual_min_y matches realbbmin.y.
                p[0] -= real_bbrange_x;
                p[1] -= actual_min_y;
                p[2] += real_bbrange_z; // ADD because Z was negated in step 7

                tri_verts[corner] = p;
                tri_uvs[corner] = mesh.tex_coords.get(tcid).copied().unwrap_or([0.0; 2]);
            }

            let v0 = Vec3::from(tri_verts[0]);
            let v1 = Vec3::from(tri_verts[1]);
            let v2 = Vec3::from(tri_verts[2]);
            let face_normal = (v1 - v0).cross(v2 - v0).normalize();

            // Per-corner normals: smooth (averaged per vertex+smooth_group) or flat.
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
            // Flip index order when combined scale determinant is negative so the face
            // stays front-facing in world space after the instance transform.
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
                // Back face: vertex order is reversed relative to main face winding so it is
                // front-facing from the opposite side. When flip_winding is true the main face
                // uses [2,1,0] so the back face must use [0,1,2], and vice versa.
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

    let non_empty = groups.iter().filter(|(p, _, _, _)| !p.is_empty()).count();
    let total_verts: usize = groups.iter().map(|(p, _, _, _)| p.len()).sum();
    // info!(
    //     "[RoModel] spawning '{}' — {} mesh(es), {} tex group(s), {} total verts, bb {:?}..{:?}, rsw_pos {:?}, rsw_scale {:?}, translation {:?}, rotation {:?}, real_bbrange [{:.2},{:.2}]",
    //     inst.model_file,
    //     rsm.meshes.len(),
    //     non_empty,
    //     total_verts,
    //     rsm.bbmin,
    //     rsm.bbmax,
    //     inst.pos,
    //     inst.scale,
    //     translation,
    //     rotation,
    //     real_bbrange_x,
    //     real_bbrange_z
    // );

    let instance_root = commands
        .spawn(
            Transform::from_translation(translation)
                .with_rotation(rotation)
                .with_scale(scale),
        )
        .id();

    let children: Vec<Entity> = vec![instance_root];

    for (tex_idx, (positions, normals, uvs_data, face_indices)) in groups.into_iter().enumerate() {
        if positions.is_empty() {
            continue;
        }

        let vert_count = positions.len();
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs_data);
        mesh.insert_indices(Indices::U32(face_indices));

        let tex_name = rsm.textures.get(tex_idx).map(|s| s.as_str()).unwrap_or("");
        // info!(
        //     "[RoModel]   tex group {} — {} verts, texture: {}",
        //     tex_idx, vert_count, tex_name
        // );

        let texture: Handle<Image> = asset_server.load(tex_name.to_string());
        let material = materials.add(StandardMaterial {
            base_color_texture: Some(texture),
            alpha_mode: AlphaMode::Mask(0.5),
            perceptual_roughness: 1.0,
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
            ))
            .id();

        commands.entity(instance_root).add_child(mesh_entity);
    }

    children
}

pub(crate) fn animate_water(
    time: Res<Time>,
    mut query: Query<(&mut WaterAnimator, &MeshMaterial3d<StandardMaterial>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    println!("Animating water");
    for (mut anim, mat_handle) in &mut query {
        anim.elapsed += time.delta_secs();
        if anim.elapsed >= anim.interval_secs {
            println!("anim.elapsed >= anim.interval_secs is true");
            anim.elapsed -= anim.interval_secs;
            anim.current_frame = (anim.current_frame + 1) % anim.frames.len();
            if let Some(mat) = materials.get_mut(mat_handle.id()) {
                println!("got material");
                mat.base_color_texture = Some(anim.frames[anim.current_frame].clone());
            }
        }
    }
}
