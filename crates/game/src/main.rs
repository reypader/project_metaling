mod camera_orbit;
mod map_interaction;

use crate::camera_orbit::{CameraFollower, OrbitCamera, OrbitCameraPlugin};
use crate::map_interaction::{MapInteractionPlugin, MapMarker, Navigation};
use bevy::light::CascadeShadowConfigBuilder;
use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy::camera::primitives::Aabb;
use bevy_ro_maps::{MapLightingReady, NavMesh, RoEffectEmitter, RoMapRoot, RoMapsPlugin, RoModelMesh};
use std::collections::{HashMap, HashSet};
use bevy_ro_sprites::prelude::*;
use std::f32::consts::PI;
use std::ops::DerefMut;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(AssetPlugin {
            file_path: "/Users/rmpader/code_projects/project_metaling/target/assets".to_string(),
            ..default()
        }))
        .add_plugins(OrbitCameraPlugin)
        // .add_plugins(DefaultPickingPlugins)
        .add_systems(Startup, (setup, load_effect_sprite_map))
        .add_plugins(RoSpritePlugin)
        .add_plugins(RoMapsPlugin)
        .add_plugins(MeshPickingPlugin)
        .add_plugins(MapInteractionPlugin)
        .add_observer(apply_map_lighting)
        .add_systems(PostStartup, attach_composite)
        .insert_resource(ModelFadeCullDistance(500.0))
        .add_systems(Update, (select_action, update_composite_tag, fade_occluded_models, attach_effect_sprites))
        .add_observer(|trigger: On<SpriteFrameEvent>| {
            let e = trigger.event();
            info!(
                "ACT event '{}' on {:?} during {:?}",
                e.event, e.entity, e.tag
            );
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
            asset: asset_server.load("maps/pprontera/pprontera.gnd"),
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
        Transform::from_xyz(-20.0, 0.0, 100.0).with_scale(Vec3::new(0.15,0.15,0.15)),
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
        Transform::from_xyz(0.0, 0.0, 100.0).with_scale(Vec3::new(0.15,0.15,0.15)),
        PlayerControl,
    ));
    commands.spawn((
        Transform::from_xyz(0.0, 0.0, 0.0),
        CameraFollower,
    ));

    commands.spawn((
        ActorSprite {
            body: "sprite/human_female_knight/body.spr",
            head: "sprite/human_female_head/head/5.spr",
            weapon: Some("sprite/human_female_knight/weapon/two_handed_spear/weapon.spr"),
            weapon_slash: Some("sprite/human_female_knight/weapon/two_handed_spear/slash.spr"),
        },
        ActorState {
            action: Action::Idle,
        },
        ActorDirection(Vec2::Y),
        Transform::from_xyz(20.0, 0.0, 100.0).with_scale(Vec3::new(0.15,0.15,0.15)),
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
// Public types
// ─────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Default, Debug)]
pub enum Action {
    #[default]
    Idle,
    Walk,
    Sit,
    PickUp,
    Alert,
    Skill,
    Flinch,
    Frozen,
    Dead,
    Attack1,
    Attack2,
    Spell,
}

impl Action {
    pub fn tag_name(self) -> &'static str {
        match self {
            Action::Idle => "idle",
            Action::Walk => "walk",
            Action::Sit => "sit",
            Action::PickUp => "pickup",
            Action::Alert => "alert",
            Action::Skill => "skill",
            Action::Flinch => "flinch",
            Action::Frozen => "frozen",
            Action::Dead => "dead",
            Action::Attack1 => "attack1",
            Action::Attack2 => "attack2",
            Action::Spell => "spell",
        }
    }
}

/// Facing direction in world XZ space (length doesn't matter).
#[derive(Component, Clone, Copy, Default)]
pub struct ActorDirection(pub Vec2);

/// Current animation action.
#[derive(Component, Clone, Copy, Default)]
pub struct ActorState {
    pub action: Action,
}

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
                FeetLift(10.0),
            ));
        });
    }
}

// ─────────────────────────────────────────────────────────────
// Systems
// ─────────────────────────────────────────────────────────────

/// Propagate ActorState/ActorDirection changes to the RoComposite tag on the billboard child.
fn update_composite_tag(
    actors: Query<
        (&ActorState, &ActorDirection, &Children),
        // Or<(Changed<ActorState>, Changed<ActorDirection>)>,
        (With<ActorState>, With<ActorDirection>),
    >,
    mut billboards: Query<&mut RoComposite>,
    camera_q: Query<&Transform, With<Camera3d>>,
) {
    let cam_fwd = camera_q
        .single()
        .ok()
        .map(|t| {
            let f = t.forward().as_vec3();
            Vec2::new(f.x, f.z)
        })
        .unwrap_or(Vec2::NEG_Y);

    for (state, dir, children) in &actors {
        for child in children.iter() {
            let Ok(mut composite) = billboards.get_mut(child) else {
                continue;
            };
            let dir_idx = direction_index(dir.0, cam_fwd);
            let tag = composite_tag(state.action.tag_name(), dir_idx);
            composite.tag = Some(tag);
            composite.playing = !matches!(state.action, Action::Idle | Action::Sit);
        }
    }
}

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

/// Projects the four corners of a billboard quad (unit rectangle in local space,
/// scaled by `GlobalTransform`) into NDC and returns their 2D bounding rect plus
/// the NDC z of the billboard center (for depth comparison against models).
/// Returns `None` if no corner falls within the view frustum.
fn billboard_ndc_rect(
    camera: &Camera,
    cam_gt: &GlobalTransform,
    billboard_gt: &GlobalTransform,
) -> Option<(Rect, f32)> {
    const CORNERS: [Vec3; 4] = [
        Vec3::new(-0.5, -0.5, 0.0),
        Vec3::new(0.5, -0.5, 0.0),
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
        if let Some(ndc) = camera.world_to_ndc(cam_gt, world) {
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

/// Each frame, fades RSM model meshes whose screen-space AABB overlaps any billboard sprite.
/// Only models within [`ModelFadeCullDistance`] of the player are tested.
fn fade_occluded_models(
    mut previously_faded: Local<HashSet<Entity>>,
    cull_dist: Res<ModelFadeCullDistance>,
    camera_q: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    player_q: Query<&GlobalTransform, With<PlayerControl>>,
    billboards: Query<&GlobalTransform, (With<RoComposite>, Without<EffectComposite>)>,
    model_meshes: Query<
        (Entity, &GlobalTransform, &Aabb, &MeshMaterial3d<StandardMaterial>),
        With<RoModelMesh>,
    >,
    mut std_mats: ResMut<Assets<StandardMaterial>>,
) {
    let Ok((camera, cam_gt)) = camera_q.single() else {
        return;
    };
    let Ok(player_gt) = player_q.single() else {
        return;
    };
    let player_pos = player_gt.translation();
    let cull_dist_sq = cull_dist.0 * cull_dist.0;

    // Project all visible billboards to 2D screen rects + center depth.
    let billboard_rects: Vec<(Rect, f32)> = billboards
        .iter()
        .filter_map(|gt| billboard_ndc_rect(camera, cam_gt, gt))
        .collect();

    // Determine which model meshes overlap any billboard this frame.
    let mut should_fade: HashSet<Entity> = HashSet::new();
    if !billboard_rects.is_empty() {
        for (entity, model_gt, aabb, _) in &model_meshes {
            if model_gt.translation().distance_squared(player_pos) > cull_dist_sq
            {
                continue;
            }
            let Some((model_rect, model_z)) = model_ndc_rect(camera, cam_gt, model_gt, aabb) else {
                continue;
            };
            for &(bill_rect, bill_z) in &billboard_rects {
                // Only fade the model if it is in front of the billboard (closer to camera).
                // In NDC reverse-z: higher z = closer to camera.
                if model_z <= bill_z {
                    continue;
                }
                if model_rect.min.x < bill_rect.max.x
                    && model_rect.max.x > bill_rect.min.x
                    && model_rect.min.y < bill_rect.max.y
                    && model_rect.max.y > bill_rect.min.y
                {
                    should_fade.insert(entity);
                    break;
                }
            }
        }
    }

    // Restore materials that were faded last frame but no longer need to be.
    for &entity in previously_faded.iter() {
        if !should_fade.contains(&entity) {
            if let Ok((_, _, _, mat_handle)) = model_meshes.get(entity) {
                if let Some(mat) = std_mats.get_mut(&mat_handle.0) {
                    mat.base_color = Color::WHITE;
                    mat.alpha_mode = AlphaMode::Mask(0.5);
                }
            }
        }
    }

    // Apply fade to newly overlapping models.
    for &entity in &should_fade {
        if !previously_faded.contains(&entity) {
            if let Ok((_, _, _, mat_handle)) = model_meshes.get(entity) {
                if let Some(mat) = std_mats.get_mut(&mat_handle.0) {
                    mat.base_color = Color::srgba(1.0, 1.0, 1.0, 0.1);
                    mat.alpha_mode = AlphaMode::Blend;
                }
            }
        }
    }

    *previously_faded = should_fade;
}

/// Marker placed on effect billboard entities to exclude them from actor-occlusion fade checks.
#[derive(Component)]
struct EffectComposite;

/// Maps RSW effect IDs to SPR file stems (e.g. `47 → "torch_01"`).
/// Loaded from `sprite/effect/effect_sprites.json` at startup.
#[derive(Resource, Default)]
struct EffectSpriteMap(HashMap<u32, String>);

const ASSETS_PATH: &str =
    "/Users/rmpader/code_projects/project_metaling/target/assets";

fn load_effect_sprite_map(mut commands: Commands) {
    let path = format!("{ASSETS_PATH}/sprite/effect/effect_sprites.json");
    let map = std::fs::read_to_string(&path)
        .ok()
        .and_then(|json| serde_json::from_str::<HashMap<u32, String>>(&json).ok())
        .unwrap_or_default();
    if map.is_empty() {
        warn!("effect_sprites.json not found or empty — no effect sprites will render");
    } else {
        info!("Loaded {} effect sprite mappings", map.len());
    }
    commands.insert_resource(EffectSpriteMap(map));
}

/// Divisor applied to effect billboard canvas size to normalize ACT-baked pixel scales
/// to world units. Effect ACT files use large layer scale values (e.g. 7.46×) that make
/// canvases 10–20× larger than actor sprites. Tune this constant to adjust visual size.
const EFFECT_SPRITE_SCALE: f32 = 1.0 / 35.0;

/// Reacts to newly spawned [`RoEffectEmitter`] entities and attaches a [`RoComposite`] billboard
/// child for effect IDs that have a corresponding SPR sprite.
fn attach_effect_sprites(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<RoCompositeMaterial>>,
    server: Res<AssetServer>,
    effect_map: Res<EffectSpriteMap>,
    new_effects: Query<(Entity, &RoEffectEmitter), Added<RoEffectEmitter>>,
) {
    for (entity, emitter) in &new_effects {
        let Some(stem) = effect_map.0.get(&emitter.effect_id) else {
            println!("Not found {:?}", &emitter.effect_id);
            continue;
        };
        println!("Found {:?}", stem);
        let spr_path = format!("sprite/effect/{stem}.spr");
        // tag: None plays all frames in sequence — effect ACTs are non-directional loops.
        // A directional tag like "idle_s" silently skips rendering if the ACT has only
        // one action, so we let the composite cycle the full frame range instead.
        commands.entity(entity).insert(Visibility::Inherited).with_children(|parent| {
            parent.spawn((
                RoComposite {
                    layers: vec![CompositeLayerDef {
                        atlas: server.load(spr_path),
                        role: SpriteRole::Body,
                    }],
                    tag: None,
                    playing: true,
                    ..Default::default()
                },
                Mesh3d(meshes.add(Rectangle::new(1.0, 1.0))),
                MeshMaterial3d(mats.add(RoCompositeMaterial::default())),
                Transform::default(),
                Visibility::Visible,
                EffectComposite,
                NoShadowLayer,
                BillboardScale(EFFECT_SPRITE_SCALE),
                Pickable { should_block_lower: false, is_hoverable: false },
            ));
        });
    }
}
