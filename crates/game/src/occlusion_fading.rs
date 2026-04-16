// ─────────────────────────────────────────────────────────────
// Model fade-out when occluding sprite billboards
// ─────────────────────────────────────────────────────────────

use crate::player_control::PlayerControl;
use bevy::camera::primitives::Aabb;
use bevy::prelude::*;
use bevy_ro_models::{RoModelInstance, RoModelMesh};
use bevy_ro_sprites::prelude::RoComposite;
use bevy_ro_vfx::EffectBillboard;
use std::collections::{HashMap, HashSet};

/// Controls how model-vs-billboard occlusion is detected.
#[derive(Clone, Debug, Default)]
#[allow(dead_code)]
pub enum OcclusionMode {
    /// Project AABB corners to NDC and test bounding rect overlap.
    /// Cheaper but less precise; may fade models that only overlap at the box level.
    BoundingBox,
    /// Subsample mesh vertices, project each to NDC, and test point-in-rect.
    /// More precise but requires a one-time vertex cache per mesh entity.
    #[default]
    VertexProjection,
}

/// Runtime configuration for the occlusion fading plugin, inserted as a `Resource`.
#[derive(Resource, Clone, Debug)]
pub struct OcclusionFadingConfig {
    /// Detection strategy. Default: `OcclusionMode::VertexProjection`.
    pub mode: OcclusionMode,
    /// Maximum distance from the player at which model-vs-billboard overlap is tested.
    /// Models farther than this are never faded, regardless of screen overlap.
    /// Default: `500.0`.
    pub cull_distance: f32,
}

impl Default for OcclusionFadingConfig {
    fn default() -> Self {
        Self {
            mode: OcclusionMode::default(),
            cull_distance: 500.0,
        }
    }
}

#[derive(Default)]
pub struct OcclusionFadingPlugin {
    pub config: OcclusionFadingConfig,
}

impl Plugin for OcclusionFadingPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.config.clone());
        match self.config.mode {
            OcclusionMode::VertexProjection => {
                app.add_systems(
                    Update,
                    (cache_model_vertices, fade_occluded_models).chain(),
                );
            }
            OcclusionMode::BoundingBox => {
                app.add_systems(Update, fade_occluded_models);
            }
        }
    }
}

/// Subsampled local-space vertex positions cached from a model's `Mesh` asset.
/// Populated once by `cache_model_vertices` and used by `fade_occluded_models`
/// in place of the coarser AABB rect test.
#[derive(Component)]
struct CachedMeshVertices(Vec<Vec3>);

/// Projects the top-half corners of a billboard quad (local Y from 0 to 0.5) into NDC
/// and returns their 2D bounding rect plus the NDC z of the billboard center (for depth
/// comparison against models). Only the top half is used so that models overlapping the
/// bottom half (which is sunken into the ground) do not trigger fading.
/// Returns `None` if no corner falls within the view frustum.
fn billboard_ndc_rect(
    camera: &Camera,
    cam_gt: &GlobalTransform,
    billboard_gt: &GlobalTransform,
) -> Option<(Rect, f32)> {
    const CORNERS: [Vec3; 4] = [
        // Vec3::new(-0.5, -0.5, 0.0), //uncomment this if the entire height of the billboard should be checked
        // Vec3::new(0.5, -0.5, 0.0), //uncomment this if the entire height of the billboard should be checked
        Vec3::new(-0.5, 0.0, 0.0), //uncomment this if only the top-half should be checked for obstruction
        Vec3::new(0.5, 0.0, 0.0), //uncomment this if only the top-half should be checked for obstruction
        Vec3::new(-0.5, 0.5, 0.0),
        Vec3::new(0.5, 0.5, 0.0),
    ];
    let mut min = Vec2::splat(f32::INFINITY);
    let mut max = Vec2::splat(f32::NEG_INFINITY);
    let mut any = false;
    for c in CORNERS {
        let world = billboard_gt.transform_point(c);
        if let Some(ndc) = camera.world_to_ndc(cam_gt, world) {
            // z in [0, 1] means within frustum (0 = far plane, 1 = near plane).
            if ndc.z >= 0.0 && ndc.z <= 1.0 {
                min = min.min(ndc.truncate());
                max = max.max(ndc.truncate());
                any = true;
            }
        }
    }
    if !any {
        return None;
    }
    // Center depth for depth-ordering: project the quad center (local origin).
    let center_ndc_z = camera
        .world_to_ndc(cam_gt, billboard_gt.transform_point(Vec3::ZERO))
        .map(|n| n.z)
        .unwrap_or(0.0);
    Some((Rect::new(min.x, min.y, max.x, max.y), center_ndc_z))
}

/// Projects the eight corners of a mesh AABB into NDC and returns their 2D bounding rect
/// plus the NDC z of the AABB center (for depth comparison against billboards).
/// Returns `None` if no corner falls within the view frustum.
fn model_ndc_rect(
    camera: &Camera,
    cam_gt: &GlobalTransform,
    model_gt: &GlobalTransform,
    aabb: &Aabb,
) -> Option<(Rect, f32)> {
    let c = Vec3::from(aabb.center);
    let h = Vec3::from(aabb.half_extents);
    let corners = [
        c + Vec3::new(-h.x, -h.y, -h.z),
        c + Vec3::new(h.x, -h.y, -h.z),
        c + Vec3::new(-h.x, h.y, -h.z),
        c + Vec3::new(h.x, h.y, -h.z),
        c + Vec3::new(-h.x, -h.y, h.z),
        c + Vec3::new(h.x, -h.y, h.z),
        c + Vec3::new(-h.x, h.y, h.z),
        c + Vec3::new(h.x, h.y, h.z),
    ];
    let mut min = Vec2::splat(f32::INFINITY);
    let mut max = Vec2::splat(f32::NEG_INFINITY);
    let mut any = false;
    for corner in corners {
        let world = model_gt.transform_point(corner);
        if let Some(ndc) = camera.world_to_ndc(cam_gt, world)
            && ndc.z >= 0.0
            && ndc.z <= 1.0
        {
            min = min.min(ndc.truncate());
            max = max.max(ndc.truncate());
            any = true;
        }
    }
    if !any {
        return None;
    }
    // Use the nearest corner's z (highest NDC z = closest to camera) so that a model
    // clipping through the billboard — where only part of its geometry is in front —
    // is still detected.
    let nearest_z = corners
        .iter()
        .filter_map(|&corner| {
            camera
                .world_to_ndc(cam_gt, model_gt.transform_point(corner))
                .map(|n| n.z)
        })
        .fold(f32::NEG_INFINITY, f32::max);
    Some((Rect::new(min.x, min.y, max.x, max.y), nearest_z))
}

/// Extracts and caches a subsampled list of local-space vertex positions for each
/// `RoModelMesh` entity that does not yet have `CachedMeshVertices`. Runs once per
/// entity (on the first frame after the mesh asset is available).
fn cache_model_vertices(
    mut commands: Commands,
    meshes: Res<Assets<Mesh>>,
    query: Query<(Entity, &Mesh3d), (With<RoModelMesh>, Without<CachedMeshVertices>)>,
) {
    const MAX_SAMPLES: usize = 128;
    for (entity, mesh3d) in &query {
        let Some(mesh) = meshes.get(&mesh3d.0) else {
            continue;
        };
        let Some(attr) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
            continue;
        };
        let raw: Vec<Vec3> = attr
            .as_float3()
            .map(|s| s.iter().map(|&p| Vec3::from(p)).collect())
            .unwrap_or_default();
        let step = (raw.len() / MAX_SAMPLES).max(1);
        let sampled: Vec<Vec3> = raw.into_iter().step_by(step).collect();
        commands.entity(entity).insert(CachedMeshVertices(sampled));
    }
}

/// Each frame, fades RSM model meshes whose projected vertices overlap any billboard sprite.
/// Only models within [`ModelFadeCullDistance`] of the player are tested.
/// Falls back to the AABB rect test for entities whose vertex cache is not yet ready.
fn fade_occluded_models(
    mut fade_alphas: Local<HashMap<Entity, f32>>,
    time: Res<Time>,
    config: Res<OcclusionFadingConfig>,
    camera_q: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    player_q: Query<(&GlobalTransform, &Children), With<PlayerControl>>,
    billboards: Query<&GlobalTransform, (With<RoComposite>, Without<EffectBillboard>)>,
    model_meshes: Query<
        (
            Entity,
            &GlobalTransform,
            &Aabb,
            &MeshMaterial3d<StandardMaterial>,
            Option<&CachedMeshVertices>,
        ),
        With<RoModelMesh>,
    >,
    parent_q: Query<&ChildOf>,
    instance_q: Query<(), With<RoModelInstance>>,
    mut std_mats: ResMut<Assets<StandardMaterial>>,
) {
    let Ok((camera, cam_gt)) = camera_q.single() else {
        return;
    };
    let Ok((player_gt, player_children)) = player_q.single() else {
        return;
    };
    let player_pos = player_gt.translation();
    let cull_dist_sq = config.cull_distance * config.cull_distance;

    // Project only the player's billboard to NDC. The billboard is a child of the player entity.
    let billboard_rects: Vec<(Rect, f32)> = player_children
        .iter()
        .filter_map(|child| billboards.get(child).ok())
        .filter_map(|gt| billboard_ndc_rect(camera, cam_gt, gt))
        .collect();

    // Determine which model instance roots occlude a billboard this frame.
    let mut should_fade_instances: HashSet<Entity> = HashSet::new();
    if !billboard_rects.is_empty() {
        for (entity, model_gt, aabb, _, cached_verts) in &model_meshes {
            if model_gt.translation().distance_squared(player_pos) > cull_dist_sq {
                continue;
            }

            let occluding = match config.mode {
                OcclusionMode::VertexProjection => {
                    if let Some(CachedMeshVertices(verts)) = cached_verts {
                        // Vertex-based test: project each sampled local-space vertex to NDC
                        // and check if it lands inside a billboard rect while being in front.
                        'vertex: {
                            for &(bill_rect, bill_z) in &billboard_rects {
                                for &local in verts {
                                    let world = model_gt.transform_point(local);
                                    let Some(ndc) = camera.world_to_ndc(cam_gt, world) else {
                                        continue;
                                    };
                                    if ndc.z < 0.0 || ndc.z > 1.0 {
                                        continue;
                                    }
                                    if ndc.z <= bill_z {
                                        continue;
                                    }
                                    if bill_rect.contains(ndc.truncate()) {
                                        break 'vertex true;
                                    }
                                }
                            }
                            false
                        }
                    } else {
                        // Fallback: AABB rect test while vertex cache loads.
                        let Some((model_rect, model_z)) =
                            model_ndc_rect(camera, cam_gt, model_gt, aabb)
                        else {
                            continue;
                        };
                        billboard_rects.iter().any(|&(bill_rect, bill_z)| {
                            model_z > bill_z
                                && model_rect.min.x < bill_rect.max.x
                                && model_rect.max.x > bill_rect.min.x
                                && model_rect.min.y < bill_rect.max.y
                                && model_rect.max.y > bill_rect.min.y
                        })
                    }
                }
                OcclusionMode::BoundingBox => {
                    let Some((model_rect, model_z)) =
                        model_ndc_rect(camera, cam_gt, model_gt, aabb)
                    else {
                        continue;
                    };
                    billboard_rects.iter().any(|&(bill_rect, bill_z)| {
                        model_z > bill_z
                            && model_rect.min.x < bill_rect.max.x
                            && model_rect.max.x > bill_rect.min.x
                            && model_rect.min.y < bill_rect.max.y
                            && model_rect.max.y > bill_rect.min.y
                    })
                }
            };

            if occluding {
                should_fade_instances.insert(model_instance_root(entity, &parent_q, &instance_q));
            }
        }
    }

    // Expand instance roots to the full set of mesh entities that should be fading.
    let should_fade: HashSet<Entity> = model_meshes
        .iter()
        .filter_map(|(entity, _, _, _, _)| {
            let root = model_instance_root(entity, &parent_q, &instance_q);
            should_fade_instances.contains(&root).then_some(entity)
        })
        .collect();

    // Seed any newly occluding entities into the alpha map so they get animated.
    for &entity in &should_fade {
        fade_alphas.entry(entity).or_insert(1.0);
    }

    // Step each tracked entity's alpha toward its target and update the material.
    // Entities whose alpha returns to 1.0 are removed from the map.
    const FADED_ALPHA: f32 = 0.1;
    const FADE_SPEED: f32 = 1.0; // alpha units per second
    let dt = time.delta_secs();

    fade_alphas.retain(|&entity, alpha| {
        let target = if should_fade.contains(&entity) {
            FADED_ALPHA
        } else {
            1.0
        };
        let step = FADE_SPEED * dt;
        if (*alpha - target).abs() <= step {
            *alpha = target;
        } else {
            *alpha += step * if target > *alpha { 1.0 } else { -1.0 };
        }

        if let Ok((_, _, _, mat_handle, _)) = model_meshes.get(entity)
            && let Some(mat) = std_mats.get_mut(&mat_handle.0)
        {
            if *alpha >= 1.0 {
                mat.base_color = Color::WHITE;
                mat.alpha_mode = AlphaMode::Mask(0.5);
            } else {
                mat.base_color = Color::srgba(1.0, 1.0, 1.0, *alpha);
                mat.alpha_mode = AlphaMode::Blend;
            }
        }

        *alpha < 1.0 // remove from map once fully restored
    });
}

/// Walks the `ChildOf` chain from `entity` upward until a `RoModelInstance` ancestor is found.
/// Returns that ancestor, or `entity` itself if no such ancestor exists.
fn model_instance_root(
    entity: Entity,
    parent_q: &Query<&ChildOf>,
    instance_q: &Query<(), With<RoModelInstance>>,
) -> Entity {
    let mut current = entity;
    loop {
        if instance_q.contains(current) {
            return current;
        }
        let Ok(child_of) = parent_q.get(current) else {
            return current;
        };
        current = child_of.parent();
    }
}
