use crate::assets::RoMapAsset;
use crate::bgm::BgmTable;
use crate::navigation::NavMesh;
use crate::terrain_material::{TerrainLightmapExtension, TerrainMaterial};
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    picking::Pickable,
    prelude::*,
};
use bevy_ro_models::PendingModel;
use bevy_ro_sounds::PlaySound;
use bevy_ro_vfx::{EffectRepeat, RoEffectEmitter};
use ro_files::{LightSource, ModelInstance, RswLighting, RswObject};

/// Vertex data accumulated per texture group while building mesh geometry.
/// Fields: (positions, normals, uv0, uv1, indices)
type MeshGroup = (
    Vec<[f32; 3]>,
    Vec<[f32; 3]>,
    Vec<[f32; 2]>,
    Vec<[f32; 2]>,
    Vec<u32>,
);

/// Marker component placed on each terrain mesh entity spawned by the plugin.
#[derive(Component)]
pub struct RoMapMesh;

/// Marker placed on each RSW point-light entity spawned by the plugin.
#[derive(Component)]
pub struct RoMapLight;


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

/// Fired once when a map finishes spawning. Carries the lighting parameters from the RSW file.
#[derive(Event, Clone)]
pub struct MapLightingReady(pub RswLighting);

pub(crate) fn spawn_map_meshes(
    mut commands: Commands,
    mut map_roots: Query<(Entity, &mut RoMapRoot)>,
    map_assets: Res<Assets<RoMapAsset>>,
    asset_server: Res<AssetServer>,
    bgm_table: Res<BgmTable>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut terrain_materials: ResMut<Assets<TerrainMaterial>>,
    mut images: ResMut<Assets<Image>>,
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

        // Trigger BGM for this map if the table has an entry.
        let map_name = asset_server
            .get_path(&root.asset)
            .and_then(|p| {
                p.path()
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_lowercase())
            })
            .unwrap_or_default();
        if let Some(bgm_path) = bgm_table.0.get(&*map_name) {
            commands.trigger(PlaySound {
                path: bgm_path.clone(),
                looping: true,
                location: None,
                volume: None,
                range: None,
            });
        }

        let gnd = &map.gnd;
        let scale = gnd.scale;

        let (lightmap_atlas, atlas_size_px) = build_lightmap_atlas(gnd, &mut images);

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
            .map(|_| (vec![], vec![], vec![], vec![], vec![]))
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

                let (positions, normals, uvs, uvs1, indices) = &mut groups[tex_idx];

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

                // UV0: diffuse texture UVs, match vertex order above.
                uvs.push([surface.u[0], surface.v[0]]);
                uvs.push([surface.u[1], surface.v[1]]);
                uvs.push([surface.u[2], surface.v[2]]);
                uvs.push([surface.u[3], surface.v[3]]);

                // UV1: lightmap atlas UVs.
                // Vertex order: SW=(lm1.x, lm1.y), SE=(lm2.x, lm1.y),
                //               NW=(lm1.x, lm2.y), NE=(lm2.x, lm2.y)
                let (lm1, lm2) = lightmap_uv_range(surface.lightmap_id, atlas_size_px);
                uvs1.push([lm1[0], lm1[1]]);
                uvs1.push([lm2[0], lm1[1]]);
                uvs1.push([lm1[0], lm2[1]]);
                uvs1.push([lm2[0], lm2[1]]);

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
                if cube.north_surface_id >= 0
                    && let Some(surf) = gnd.surfaces.get(cube.north_surface_id as usize)
                {
                    let tex_idx = surf.texture_id as usize;
                    if surf.texture_id >= 0
                        && tex_idx < texture_count
                        && let Some(north) = gnd.cube(col, row + 1)
                    {
                        // v0=BL, v1=BR, v2=TL, v3=TR
                        // Top vertices come from neighbor's south edge (heights[0]=SW, heights[1]=SE).
                        let v0 = Vec3::new(x0, -cube.heights[2], z_nw);
                        let v1 = Vec3::new(x1, -cube.heights[3], z_nw);
                        let v2 = Vec3::new(x0, -north.heights[0], z_nw);
                        let v3 = Vec3::new(x1, -north.heights[1], z_nw);
                        // North wall lies in the Z-plane; normal is always +Z (faces south).
                        let n = [0.0_f32, 0.0, 1.0];

                        let (positions, normals, uvs, uvs1, indices) = &mut groups[tex_idx];
                        let base = positions.len() as u32;
                        positions.extend([
                            v0.to_array(),
                            v1.to_array(),
                            v2.to_array(),
                            v3.to_array(),
                        ]);
                        normals.extend([n, n, n, n]);
                        uvs.push([surf.u[0], surf.v[0]]);
                        uvs.push([surf.u[1], surf.v[1]]);
                        uvs.push([surf.u[2], surf.v[2]]);
                        uvs.push([surf.u[3], surf.v[3]]);
                        // UV1 north wall: v0=(lm1.x,lm1.y), v1=(lm2.x,lm1.y),
                        //                v2=(lm1.x,lm2.y), v3=(lm2.x,lm2.y)
                        let (lm1, lm2) = lightmap_uv_range(surf.lightmap_id, atlas_size_px);
                        uvs1.extend([
                            [lm1[0], lm1[1]],
                            [lm2[0], lm1[1]],
                            [lm1[0], lm2[1]],
                            [lm2[0], lm2[1]],
                        ]);
                        // T1: v0→v1→v2, T2: v1→v3→v2
                        indices.extend([base, base + 1, base + 2, base + 1, base + 3, base + 2]);
                    }
                }

                // East wall: shared edge at X = x1, between this tile and col+1.
                if cube.east_surface_id >= 0
                    && let Some(surf) = gnd.surfaces.get(cube.east_surface_id as usize)
                {
                    let tex_idx = surf.texture_id as usize;
                    if surf.texture_id >= 0
                        && tex_idx < texture_count
                        && let Some(east) = gnd.cube(col + 1, row)
                    {
                        // v0=BS, v1=BN, v2=TS, v3=TN
                        let v0 = Vec3::new(x1, -cube.heights[1], z_sw);
                        let v1 = Vec3::new(x1, -cube.heights[3], z_nw);
                        let v2 = Vec3::new(x1, -east.heights[0], z_sw);
                        let v3 = Vec3::new(x1, -east.heights[2], z_nw);
                        // East wall lies in the X-plane; normal is always -X (faces west).
                        let n = [-1.0_f32, 0.0, 0.0];

                        let (positions, normals, uvs, uvs1, indices) = &mut groups[tex_idx];
                        let base = positions.len() as u32;
                        positions.extend([
                            v0.to_array(),
                            v1.to_array(),
                            v2.to_array(),
                            v3.to_array(),
                        ]);
                        normals.extend([n, n, n, n]);
                        uvs.push([surf.u[0], surf.v[0]]);
                        uvs.push([surf.u[1], surf.v[1]]);
                        uvs.push([surf.u[2], surf.v[2]]);
                        uvs.push([surf.u[3], surf.v[3]]);
                        // UV1 east wall: v0=(lm2.x,lm1.y), v1=(lm1.x,lm1.y),
                        //               v2=(lm2.x,lm2.y), v3=(lm1.x,lm2.y)
                        let (lm1, lm2) = lightmap_uv_range(surf.lightmap_id, atlas_size_px);
                        uvs1.extend([
                            [lm2[0], lm1[1]],
                            [lm1[0], lm1[1]],
                            [lm2[0], lm2[1]],
                            [lm1[0], lm2[1]],
                        ]);
                        // T1: v1→v0→v2, T2: v1→v2→v3
                        indices.extend([base + 1, base, base + 2, base + 1, base + 2, base + 3]);
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

            let mut water_mesh = Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::default(),
            );
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
                    WaterAnimator {
                        frames,
                        interval_secs,
                        elapsed: 0.0,
                        current_frame: 0,
                    },
                ))
                .id();
            commands.entity(root_entity).add_child(water_entity);
        }

        let total_verts: usize = groups.iter().map(|(p, _, _, _, _)| p.len()).sum();
        let non_empty = groups
            .iter()
            .filter(|(p, _, _, _, _)| !p.is_empty())
            .count();
        let all_positions: Vec<[f32; 3]> = groups
            .iter()
            .flat_map(|(p, _, _, _, _)| p.iter().copied())
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
        for (tex_idx, (positions, normals, uvs, uvs1, indices)) in groups.into_iter().enumerate() {
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
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, uvs1);
            mesh.insert_indices(Indices::U32(indices));

            let texture_path = &gnd.texture_paths[tex_idx];
            info!(
                "[RoMap] spawning mesh group {} — {} verts, texture: {}",
                tex_idx, vert_count, texture_path
            );

            let texture: Handle<Image> = asset_server.load(texture_path);

            let material = terrain_materials.add(TerrainMaterial {
                base: StandardMaterial {
                    base_color_texture: Some(texture),
                    alpha_mode: AlphaMode::Mask(0.5),
                    perceptual_roughness: 1.0,
                    reflectance: 0.0,
                    double_sided: true,
                    cull_mode: None,
                    ..default()
                },
                extension: TerrainLightmapExtension {
                    lightmap: lightmap_atlas.clone(),
                },
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

        // Spawn RSM model instances as child entities with PendingModel for the model crate
        // to pick up and render. The map crate pre-computes the world Transform from RSW data.
        let model_instances: Vec<&ModelInstance> = map
            .objects
            .iter()
            .filter_map(|obj| {
                if let RswObject::Model(inst) = obj {
                    Some(inst)
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

            for inst in model_instances {
                // Convert RSW position/rotation/scale to Bevy world space (BrowEdit3 convention).
                let translation =
                    Vec3::new(cx + inst.pos[0], -inst.pos[1], scale + cz - inst.pos[2]);
                let rotation = Quat::from_euler(
                    EulerRot::YXZ,
                    (-inst.rot[1]).to_radians(),
                    inst.rot[0].to_radians(),
                    (-inst.rot[2]).to_radians(),
                );
                let inst_scale = Vec3::new(inst.scale[0], inst.scale[1], inst.scale[2]);

                let child = commands
                    .spawn((
                        PendingModel {
                            asset_path: inst.model_file.clone(),
                            anim_speed: inst.anim_speed,
                        },
                        Transform::from_translation(translation)
                            .with_rotation(rotation)
                            .with_scale(inst_scale),
                        Visibility::default(),
                    ))
                    .id();
                commands.entity(root_entity).add_child(child);
            }
        }

        // Spawn RSW point lights and effect-emitter markers.
        let dims = MapDims { scale, cx, cz };
        let light_entities = spawn_lights(&map.objects, dims, &mut commands);
        let effect_entities = spawn_effects(&map.objects, dims, &mut commands);
        let audio_count = trigger_audio_sources(&map.objects, &mut commands);
        info!(
            "[RoMap] spawned {} point light(s), {} effect emitter(s), {} audio source(s)",
            light_entities.len(),
            effect_entities.len(),
            audio_count,
        );
        commands
            .entity(root_entity)
            .add_children(&light_entities)
            .add_children(&effect_entities);
    }
}

/// Converts an RSW object position to BrowEdit3 local space (same system as model instances).
/// The root entity's centering Transform brings this into Bevy world space.
fn rsw_local_pos(pos: [f32; 3], dims: MapDims) -> Vec3 {
    Vec3::new(dims.cx + pos[0], -pos[1], dims.scale + dims.cz - pos[2])
}

/// Spawns a `PointLight` child entity for every `RswObject::Light` in `objects`.
fn spawn_lights(objects: &[RswObject], dims: MapDims, commands: &mut Commands) -> Vec<Entity> {
    objects
        .iter()
        .filter_map(|obj| {
            let RswObject::Light(light) = obj else {
                return None;
            };
            Some(spawn_point_light(light, dims, commands))
        })
        .collect()
}

fn spawn_point_light(light: &LightSource, dims: MapDims, commands: &mut Commands) -> Entity {
    // RSW range is in GND world units (scale = 10). Bevy PointLight range is also in
    // world units, so we use it directly.
    // Intensity multiplier is large because the directional light uses ~10-20k lux and
    // point lights need to visually compete; tune down once placement is confirmed correct.
    let color = Color::linear_rgb(light.diffuse[0], light.diffuse[1], light.diffuse[2]);
    let bevy_range = light.range.max(50.0) * 2.0;
    let intensity = bevy_range * 200_000.0;

    commands
        .spawn((
            PointLight {
                color,
                intensity,
                range: bevy_range,
                shadows_enabled: false,
                ..default()
            },
            Transform::from_translation(rsw_local_pos(light.pos, dims)),
            RoMapLight,
        ))
        .id()
}

/// Spawns a positioned marker entity for every `RswObject::Effect` in `objects`.
fn spawn_effects(objects: &[RswObject], dims: MapDims, commands: &mut Commands) -> Vec<Entity> {
    objects
        .iter()
        .filter_map(|obj| {
            let RswObject::Effect(effect) = obj else {
                return None;
            };
            Some(
                commands
                    .spawn((
                        Transform::from_translation(rsw_local_pos(effect.pos, dims)),
                        Visibility::Hidden,
                        RoEffectEmitter {
                            effect_id: effect.effect_id,
                            repeat: EffectRepeat::Infinite,
                        },
                    ))
                    .id(),
            )
        })
        .collect()
}

/// Fires a [`PlaySound`] trigger for every `RswObject::Audio` in `objects` and spawns a debug
/// cylinder marker at each emitter position so they can be located visually.
///
/// Audio emitters are spawned as root-level entities (not children of the map root), so their
/// world position must already account for the root entity's centering transform (-cx, 0, -(scale+cz)).
/// This collapses to: world = (rsw.x, -rsw.y, -rsw.z) — no dims needed.
fn trigger_audio_sources(
    objects: &[RswObject],
    commands: &mut Commands,
) -> usize {
    let mut count = 0;
    for obj in objects {
        let RswObject::Audio(audio) = obj else {
            continue;
        };
        let [x, y, z] = audio.pos;
        let pos = Vec3::new(x, -y, -z);
        info!(
            "[Audio] emitter '{}' | rsw_pos={:?} → world={:.1?} | vol={} range={} width={} height={}",
            audio.file, audio.pos, pos, audio.volume, audio.range, audio.width, audio.height
        );
        commands.trigger(PlaySound {
            path: audio.file.clone(),
            looping: true,
            location: Some(Transform::from_translation(pos)),
            volume: Some(audio.volume),
            range: Some(audio.range),
        });
        count += 1;
    }
    count
}

/// Builds an RGBA lightmap atlas from all `GndLightmapSlice` entries.
///
/// Layout: slices are packed left-to-right, top-to-bottom in a square atlas.
/// Each 8×8 slice occupies its own 8×8 region.
/// RGB = baked light color, A = shadow/AO intensity.
///
/// Returns the atlas `Handle<Image>` and the atlas side length in pixels.
fn build_lightmap_atlas(
    gnd: &ro_files::GndFile,
    images: &mut Assets<Image>,
) -> (Handle<Image>, usize) {
    let n = gnd.lightmap_slices.len();
    if n == 0 {
        let img = Image::new(
            bevy::render::render_resource::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            bevy::render::render_resource::TextureDimension::D2,
            vec![255u8; 4],
            bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        return (images.add(img), 1);
    }

    // Pick the smallest power-of-two number of slices per row that fits all slices
    // in a square atlas.
    let slices_per_row = {
        let sq = (n as f32).sqrt().ceil() as usize;
        sq.next_power_of_two().max(1)
    };
    let atlas_size = slices_per_row * 8;

    let mut data = vec![0u8; atlas_size * atlas_size * 4];

    for (i, slice) in gnd.lightmap_slices.iter().enumerate() {
        let col = i % slices_per_row;
        let row = i / slices_per_row;
        for yy in 0..8usize {
            for xx in 0..8usize {
                let atlas_x = col * 8 + xx;
                let atlas_y = row * 8 + yy;
                let pixel = (atlas_y * atlas_size + atlas_x) * 4;
                let texel = xx + 8 * yy;
                data[pixel] = slice.lightmap[3 * texel];
                data[pixel + 1] = slice.lightmap[3 * texel + 1];
                data[pixel + 2] = slice.lightmap[3 * texel + 2];
                data[pixel + 3] = slice.shadowmap[texel];
            }
        }
    }

    let img = Image::new(
        bevy::render::render_resource::Extent3d {
            width: atlas_size as u32,
            height: atlas_size as u32,
            depth_or_array_layers: 1,
        },
        bevy::render::render_resource::TextureDimension::D2,
        data,
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    (images.add(img), atlas_size)
}

/// Computes the lightmap atlas UV range `(lm1, lm2)` for a surface's `lightmap_id`.
///
/// `lm1` is the top-left corner UV of the slice's inner 6×6 region.
/// `lm2` is the bottom-right corner UV.
///
/// Returns `([0,0], [0,0])` when `lightmap_id < 0`.
fn lightmap_uv_range(lightmap_id: i16, atlas_size: usize) -> ([f32; 2], [f32; 2]) {
    if lightmap_id < 0 || atlas_size == 0 {
        return ([0.0; 2], [0.0; 2]);
    }
    let id = lightmap_id as usize;
    let slices_per_row = atlas_size / 8;
    let col = id % slices_per_row;
    let row = id / slices_per_row;
    let inv = 1.0 / atlas_size as f32;
    let lm1 = [col as f32 * 8.0 * inv + inv, row as f32 * 8.0 * inv + inv];
    let lm2 = [lm1[0] + 6.0 * inv, lm1[1] + 6.0 * inv];
    (lm1, lm2)
}

pub(crate) fn animate_water(
    time: Res<Time>,
    mut query: Query<(&mut WaterAnimator, &MeshMaterial3d<StandardMaterial>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (mut anim, mat_handle) in &mut query {
        anim.elapsed += time.delta_secs();
        if anim.elapsed >= anim.interval_secs {
            anim.elapsed -= anim.interval_secs;
            anim.current_frame = (anim.current_frame + 1) % anim.frames.len();
            if let Some(mat) = materials.get_mut(mat_handle.id()) {
                mat.base_color_texture = Some(anim.frames[anim.current_frame].clone());
            }
        }
    }
}
