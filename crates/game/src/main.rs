mod camera_orbit;
mod map_interaction;

use crate::camera_orbit::{OrbitCamera, OrbitCameraPlugin};
use crate::map_interaction::{MapInteractionPlugin, MapMarker};
use bevy::color::palettes::css::OLD_LACE;
use bevy::light::CascadeShadowConfigBuilder;
use bevy::prelude::*;
use bevy_ro_maps::{NavMesh, RoMapRoot, RoMapsPlugin};
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
        .add_systems(PostStartup, attach_composite)
        .add_systems(Update, (select_action, update_composite_tag, move_player))
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
            asset: asset_server.load("maps/prt_fild08/prt_fild08.gnd"),
            // asset: asset_server.load("maps/payon/payon.gnd"),
            // asset: asset_server.load("maps/pay_fild01/pay_fild01.gnd"),
            // asset: asset_server.load("maps/aldebaran/aldebaran.gnd"),
            spawned: false,
        },
        Transform::default(),
        Visibility::default(),
    ));

    commands.spawn((
        MapMarker,
        Mesh3d(meshes.add(Cylinder::new(5.0, 2.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.1, 0.1),
            ..default()
        })),
        Transform::from_xyz(0.0, -100.0, 0.0),
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
        Transform::from_xyz(0.0, 0.0, 0.0),
        PlayerControl,
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
    // directional 'sun' light
    commands.spawn((
        DirectionalLight {
            illuminance: light_consts::lux::AMBIENT_DAYLIGHT,
            shadows_enabled: false,
            ..default()
        },
        Transform {
            translation: Vec3::new(0.0, 000.0, 50000.0),
            rotation: Quat::from_rotation_x(-PI / 2.),
            ..default()
        },
        // The default cascade config is designed to handle large scenes.
        // As this example has a much smaller world, we can tighten the shadow
        // bounds for better visual quality.
        CascadeShadowConfigBuilder {
            first_cascade_far_bound: 4.0,
            maximum_distance: 10.0,
            ..default()
        }
        .build(),
    ));

    // ambient light
    // ambient lights' brightnesses are measured in candela per meter square, calculable as (color * brightness)
    commands.insert_resource(GlobalAmbientLight {
        color: OLD_LACE.into(),
        brightness: 100.0,
        ..default()
    });

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

fn move_player(
    keys: Res<ButtonInput<KeyCode>>,
    mut orbit_cam: ResMut<OrbitCamera>,
    time: Res<Time>,
    mut q: Single<(&mut Transform, &mut ActorState, &mut ActorDirection), With<PlayerControl>>,
    navmesh: Single<&NavMesh>,
) {
    let speed = 100.0 * time.delta_secs();
    let mut z = 0.0;
    if keys.pressed(KeyCode::KeyW) {
        z -= 1.0;
    }
    if keys.pressed(KeyCode::KeyS) {
        z += 1.0;
    }

    let mut x = 0.0;
    if keys.pressed(KeyCode::KeyA) {
        x -= 1.0;
    }
    if keys.pressed(KeyCode::KeyD) {
        x += 1.0;
    }
    let (tf, state, direction) = q.deref_mut();

    let d = Vec2::new(x,z);
    let x = tf.translation.x + x*speed;
    let z = tf.translation.z + z*speed;
    let y = navmesh.height_at(x, z);

    let transform = Vec3::new(x, y, z);
    if d != Vec2::ZERO {
        tf.translation = transform;
        orbit_cam.focus = tf.translation;
        state.action = Action::Walk;
        direction.0 = d;
    } else {
        state.action = Action::Idle;
    }
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
