mod camera_orbit;
mod map_interaction;

use crate::camera_orbit::{CameraFollower, OrbitCamera, OrbitCameraPlugin};
use crate::map_interaction::{MapInteractionPlugin, MapMarker, Navigation};
use bevy::light::CascadeShadowConfigBuilder;
use bevy::prelude::*;
use bevy_ro_maps::{MapLightingReady, NavMesh, RoMapRoot, RoMapsPlugin};
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
        .add_systems(Startup, setup)
        .add_plugins(RoSpritePlugin)
        .add_plugins(RoMapsPlugin)
        .add_plugins(MeshPickingPlugin)
        .add_plugins(MapInteractionPlugin)
        .add_observer(apply_map_lighting)
        .add_systems(PostStartup, attach_composite)
        .add_systems(Update, (select_action, update_composite_tag))
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
            // asset: asset_server.load("maps/pay_fild01/pay_fild01.gnd"),
            asset: asset_server.load("maps/aldebaran/aldebaran.gnd"),
            // asset: asset_server.load("maps/pprontera/pprontera.gnd"),
            spawned: false,
        },
        Transform::default(),
        Visibility::default(),
    ));

    commands.spawn((
        MapMarker,
        Mesh3d(meshes.add(Cylinder::new(2.5, 2.0))),
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
        Transform::from_xyz(-20.0, 0.0, 0.0),
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
        Transform::from_xyz(0.0, 0.0, 0.0).with_scale(Vec3::new(0.25,0.25,0.25)),
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
        Transform::from_xyz(20.0, 0.0, 0.0),
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
        CascadeShadowConfigBuilder::default().build(),
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
    let rot = Quat::from_rotation_y(lon_rad) * Quat::from_rotation_x(-lat_rad);
    let sun_dir = rot * Vec3::NEG_Y;

    let [dr, dg, db] = lighting.diffuse;
    let [ar, ag, ab] = lighting.ambient;

    if let Ok((mut light, mut transform)) = sun_query.single_mut() {
        light.color = Color::srgb(dr, dg * 0.92, db * 0.78);
        light.illuminance = lighting.shadowmap_alpha * 20_000.0;
        *transform = Transform::IDENTITY.looking_to(sun_dir, Vec3::Y);
    }

    ambient.color = Color::srgb(ar, ag, ab);
    ambient.brightness = 1500.0;
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
        Or<(Changed<ActorState>, Changed<ActorDirection>)>,
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
