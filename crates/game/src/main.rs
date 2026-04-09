mod camera_orbit;
mod map_interaction;

use crate::camera_orbit::{CameraFollower, OrbitCameraPlugin};
use crate::map_interaction::{MapInteractionPlugin, MapMarker};
use bevy::camera::primitives::Aabb;
use bevy::light::CascadeShadowConfigBuilder;
use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy_ro_maps::{MapLightingReady, RoMapRoot, RoMapsPlugin};
use bevy_ro_models::{RoModelInstance, RoModelMesh, RoModelsPlugin};
use bevy_ro_sounds::RoSoundsPlugin;
use bevy_ro_sprites::SpriteFrameEvent;
use bevy_ro_sprites::prelude::{
    Action, ActorBillboard, ActorDirection, ActorState, CompositeLayerDef, RoComposite,
    RoCompositeMaterial, RoSpritePlugin, SpriteRole, composite_tag, direction_index,
};
use bevy_ro_vfx::{EffectBillboard, EffectRepeat, RoEffectEmitter, RoVfxPlugin};
use std::collections::{HashMap, HashSet};

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(AssetPlugin {
                    file_path: "/Users/rmpader/code_projects/project_metaling/target/assets"
                        .to_string(),
                    ..default()
                })
                .set(bevy::audio::AudioPlugin {
                    // RO map coordinates are in the hundreds of world units; scale down so that
                    // spatial attenuation kicks in at game-relevant distances rather than at 1 unit.
                    // This value is tunable: lower = sounds carry farther globally.
                    default_spatial_scale: bevy::audio::SpatialScale::new(0.04),
                    ..default()
                }),
        )
        .add_plugins(OrbitCameraPlugin)
        // .add_plugins(DefaultPickingPlugins)
        .add_systems(Startup, setup)
        .add_plugins(RoSpritePlugin)
        .add_plugins(RoMapsPlugin {
            assets_root: "/Users/rmpader/code_projects/project_metaling/target/assets".into(),
        })
        .add_plugins(RoModelsPlugin)
        .add_plugins(RoSoundsPlugin)
        .add_plugins(RoVfxPlugin {
            assets_root: "/Users/rmpader/code_projects/project_metaling/target/assets".into(),
            config_path: "/Users/rmpader/code_projects/project_metaling/config/EffectTable.json"
                .into(),
        })
        .add_plugins(MeshPickingPlugin)
        .add_plugins(MapInteractionPlugin)
        .add_observer(apply_map_lighting)
        .add_systems(PostStartup, attach_composite)
        .insert_resource(ModelFadeCullDistance(500.0))
        .add_systems(
            Update,
            (select_action, cache_model_vertices, fade_occluded_models).chain(),
        )
        .add_systems(Update, spawn_emitter)
        .add_observer(|trigger: On<SpriteFrameEvent>| {
            let e = trigger.event();
            info!(
                "ACT event '{}' on {:?} during {:?}",
                e.event, e.entity, e.tag
            )
        })
        .run();
}

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        RoMapRoot {
            // asset: asset_server.load("maps/prt_fild08/prt_fild08.gnd"),
            // asset: asset_server.load("maps/payon/payon.gnd"),
            // asset: asset_server.load("maps/geffen/geffen.gnd"),
            // asset: asset_server.load("maps/pay_fild01/pay_fild01.gnd"),
            // asset: asset_server.load("maps/aldebaran/aldebaran.gnd"),
            // asset: asset_server.load("maps/pay_dun02/pay_dun02.gnd"),
            asset: asset_server.load("maps/prontera/prontera.gnd"),
            spawned: false,
        },
        Transform::default(),
        Visibility::default(),
    ));

    commands.spawn((
        MapMarker,
        Mesh3d(meshes.add(Cylinder::new(2.5, 0.5))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.1, 0.1),
            ..default()
        })),
        Transform::from_xyz(0.0, -100.0, 0.0),
        Pickable {
            should_block_lower: false,
            is_hoverable: false,
        },
    ));

    commands.spawn((
        ActorSprite {
            body: "sprite/human_female_knight/body.spr",
            head: "sprite/human_female_head/head/11.spr",
            weapon: Some("sprite/human_female_knight/weapon/spear/weapon.spr"),
            weapon_slash: Some("sprite/human_female_knight/weapon/spear/slash.spr"),
        },
        ActorState {
            action: Action::Idle,
        },
        ActorDirection(-Vec3::Z.xz()),
        Transform::from_xyz(0.0, 0.0, 100.0).with_scale(Vec3::new(0.15, 0.15, 0.15)),
        PlayerControl,
        SpatialListener::new(1.0),
    ));
    commands.spawn((Transform::from_xyz(0.0, 0.0, 0.0), CameraFollower));

    // Actor — body.spr + head 17.spr, composited in one quad
    commands.spawn((
        ActorSprite {
            body: "sprite/human_male_novice/body.spr",
            head: "sprite/human_male_head/head/10.spr",
            weapon: Some("sprite/human_male_novice/weapon/sword/weapon.spr"),
            weapon_slash: Some("sprite/human_male_novice/weapon/sword/slash.spr"),
        },
        ActorState {
            action: Action::Idle,
        },
        ActorDirection(Vec2::Y),
        Transform::from_xyz(-20.0, 0.0, 100.0).with_scale(Vec3::new(0.15, 0.15, 0.15)),
    ));

    commands.spawn((
        ActorSprite {
            body: "sprite/human_female_assassin/body.spr",
            head: "sprite/human_female_head/head/5.spr",
            weapon: Some("sprite/human_female_assassin/weapon/katar_katar/weapon.spr"),
            weapon_slash: Some("sprite/human_female_assassin/weapon/katar_katar/slash.spr"),
        },
        ActorState {
            action: Action::Idle,
        },
        ActorDirection(Vec2::Y),
        Transform::from_xyz(20.0, 0.0, 100.0).with_scale(Vec3::new(0.15, 0.15, 0.15)),
    ));
    // Directional sun light — direction and color will be overwritten by apply_map_lighting
    // once the map asset finishes loading.
    commands.spawn((
        DirectionalLight {
            illuminance: 0.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::default(),
        CascadeShadowConfigBuilder {
            maximum_distance: 1000.0,
            first_cascade_far_bound: 300.0,
            ..default()
        }
        .build(),
    ));

    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 800.0, 600.0).looking_at(Vec3::new(0.0, -17.0, 0.0), Vec3::Y),
    ));
}

// ─────────────────────────────────────────────────────────────
// Input
// ─────────────────────────────────────────────────────────────

fn select_action(keys: Res<ButtonInput<KeyCode>>, mut q: Query<&mut ActorState>) {
    let action = if keys.pressed(KeyCode::Digit1) {
        Some(Action::Idle)
    } else if keys.pressed(KeyCode::Digit2) {
        Some(Action::Walk)
    } else if keys.pressed(KeyCode::Digit3) {
        Some(Action::Sit)
    } else if keys.pressed(KeyCode::Digit4) {
        Some(Action::PickUp)
    } else if keys.pressed(KeyCode::Digit5) {
        Some(Action::Alert)
    } else if keys.pressed(KeyCode::Digit6) {
        Some(Action::Skill)
    } else if keys.pressed(KeyCode::Digit7) {
        Some(Action::Flinch)
    } else if keys.pressed(KeyCode::Digit8) {
        Some(Action::Frozen)
    } else if keys.pressed(KeyCode::Digit9) {
        Some(Action::Dead)
    } else if keys.pressed(KeyCode::KeyQ) {
        Some(Action::Attack1)
    } else if keys.pressed(KeyCode::KeyE) {
        Some(Action::Attack2)
    } else if keys.pressed(KeyCode::KeyR) {
        Some(Action::Spell)
    } else {
        None
    };

    if let Some(a) = action {
        for mut state in &mut q {
            state.action = a;
        }
    }
}

fn spawn_emitter(
    mut commands: Commands,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    marker: Single<&Transform, With<MapMarker>>,
) {
    let m = marker.clone();
    let effect = if keys.clear_just_pressed(KeyCode::Comma) {
        Some(RoEffectEmitter { effect_id: 1, repeat: EffectRepeat::Times(1) })
    } else if keys.clear_just_pressed(KeyCode::Period) {
        Some(RoEffectEmitter { effect_id: 11, repeat: EffectRepeat::Times(1) })
    } else if keys.clear_just_pressed(KeyCode::Slash) {
        Some(RoEffectEmitter { effect_id: 41, repeat: EffectRepeat::Times(1) })
    } else if keys.clear_just_pressed(KeyCode::Semicolon) {
        Some(RoEffectEmitter { effect_id: 315, repeat: EffectRepeat::Times(1) })
    } else if keys.clear_just_pressed(KeyCode::KeyL) {
        Some(RoEffectEmitter { effect_id: 121, repeat: EffectRepeat::Times(1) })
    } else {
        None
    };

    if let Some(e) = effect {
        println!("Playing effect {:?}", e.effect_id);
        commands.spawn((m, GlobalTransform::from(m), Visibility::default(), e));
    };
}

#[derive(Component)]
struct PlayerControl;

fn apply_map_lighting(
    trigger: On<MapLightingReady>,
    mut sun_query: Query<(&mut DirectionalLight, &mut Transform)>,
    mut ambient: ResMut<GlobalAmbientLight>,
) {
    let lighting = &trigger.event().0;

    // Convert spherical coordinates to a sun ray direction.
    // Start from (0, -1, 0) (straight down), rotate around X by -latitude then around Y
    // by longitude. The resulting vector is the direction the light travels.
    let lat_rad = (lighting.latitude as f32).to_radians();
    let lon_rad = (lighting.longitude as f32).to_radians();
    let rot = Quat::from_rotation_y(-lon_rad) * Quat::from_rotation_x(lat_rad);
    let sun_dir = rot * Vec3::NEG_Y;

    let [dr, dg, db] = lighting.diffuse;
    let [ar, ag, ab] = lighting.ambient;

    if let Ok((mut light, mut transform)) = sun_query.single_mut() {
        // light.color = Color::srgb(dr, dg * 0.92, db * 0.78);
        light.color = Color::srgb(dr, dg, db);

        light.illuminance = lighting.shadowmap_alpha * 8_000.0;
        *transform = Transform::IDENTITY.looking_to(sun_dir, Vec3::Y);
    }

    ambient.color = Color::srgb(ar, ag, ab);
    ambient.brightness = 800.0;
}

// ─────────────────────────────────────────────────────────────
/// Marker: this entity hosts body + head layers composited on a billboard child entity.
#[derive(Component)]
pub struct ActorSprite {
    pub body: &'static str,
    pub head: &'static str,
    pub weapon: Option<&'static str>,
    pub weapon_slash: Option<&'static str>,
}

// ─────────────────────────────────────────────────────────────
// Spawn
// ─────────────────────────────────────────────────────────────

fn attach_composite(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<RoCompositeMaterial>>,
    actors: Query<(Entity, &ActorSprite, &ActorState, &ActorDirection)>,
    server: Res<AssetServer>,
    camera: Single<&Transform, With<Camera3d>>,
) {
    for (entity, sprite, state, dir) in &actors {
        let tag = composite_tag(
            state.action.tag_name(),
            direction_index(dir.0, camera.forward().as_vec3().xz().normalize()),
        );

        let mut layers = vec![
            CompositeLayerDef {
                atlas: server.load(sprite.body),
                role: SpriteRole::Body,
            },
            CompositeLayerDef {
                atlas: server.load(sprite.head),
                role: SpriteRole::Head,
            },
        ];
        if let Some(weapon) = sprite.weapon {
            layers.push(CompositeLayerDef {
                atlas: server.load(weapon),
                role: SpriteRole::Weapon { slot: 0 },
            })
        }
        if let Some(weapon_slash) = sprite.weapon_slash {
            layers.push(CompositeLayerDef {
                atlas: server.load(weapon_slash),
                role: SpriteRole::Weapon { slot: 1 },
            })
        }
        commands.entity(entity).with_children(|parent| {
            parent.spawn((
                RoComposite {
                    layers,
                    tag: Some(tag),
                    playing: true,
                    ..Default::default()
                },
                Mesh3d(meshes.add(Rectangle::new(1.0, 1.0))),
                MeshMaterial3d(mats.add(RoCompositeMaterial::default())),
                Transform::default(),
                ActorBillboard { feet_lift: 10.0 },
            ));
        });
    }
}

// ─────────────────────────────────────────────────────────────
// Systems
// ─────────────────────────────────────────────────────────────

//
// Events
//

// // Global observer — fires for any sprite entity
// app.add_observer(|trigger: On<SpriteFrameEvent>| {
// let e = trigger.event();
// info!("ACT event '{}' on {:?} during {:?}", e.event, e.entity, e.tag);
// });
//
// // Or, entity-specific observer at spawn time:
// commands.spawn(RoComposite { ... })
// .observe(|trigger: On<SpriteFrameEvent>| {
// let e = trigger.event();
// // play e.event as a sound cue
// });

// ─────────────────────────────────────────────────────────────
// Model fade-out when occluding sprite billboards
// ─────────────────────────────────────────────────────────────

/// Maximum distance from the player at which model-vs-billboard overlap is tested.
/// Models farther than this are never faded, regardless of screen overlap.
#[derive(Resource)]
pub struct ModelFadeCullDistance(pub f32);

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
    cull_dist: Res<ModelFadeCullDistance>,
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
    let cull_dist_sq = cull_dist.0 * cull_dist.0;

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

            let occluding = if let Some(CachedMeshVertices(verts)) = cached_verts {
                // Vertex-based test: project each sampled local-space vertex to NDC and
                // check if it lands inside a billboard rect while being in front of it.
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
                // Fallback: AABB rect test for entities whose vertex cache isn't ready yet.
                let Some((model_rect, model_z)) = model_ndc_rect(camera, cam_gt, model_gt, aabb)
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

//
// Events
//
