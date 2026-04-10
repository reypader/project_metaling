mod camera_orbit;
mod map_interaction;
mod occlusion_fading;
mod player_control;
use crate::camera_orbit::{CameraFollower, OrbitCameraPlugin};
use crate::map_interaction::{MapInteractionPlugin, MapMarker};
use crate::occlusion_fading::OcclusionFadingPlugin;
use crate::player_control::{PlayerControl, PlayerControlPlugin};

use bevy::light::CascadeShadowConfigBuilder;
use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy_ro_maps::{RoMapRoot, RoMapsPlugin};
use bevy_ro_models::RoModelsPlugin;
use bevy_ro_sounds::RoSoundsPlugin;
use bevy_ro_sprites::SpriteFrameEvent;
use bevy_ro_sprites::prelude::{
    Action, ActorBillboard, ActorDirection, ActorState, CompositeLayerDef, RoComposite,
    RoCompositeMaterial, RoSpritePlugin, SpriteRole, composite_tag, direction_index,
};
use bevy_ro_vfx::RoVfxPlugin;

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
        .add_plugins(OcclusionFadingPlugin)
        .add_plugins(PlayerControlPlugin)
        .add_systems(PostStartup, attach_composite)
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
            head: Some("sprite/human_female_head/head/11.spr"),
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
            head: Some("sprite/human_male_head/head/10.spr"),
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
            head: Some("sprite/human_female_head/head/5.spr"),
            weapon: Some("sprite/human_female_assassin/weapon/katar_katar/weapon.spr"),
            weapon_slash: Some("sprite/human_female_assassin/weapon/katar_katar/slash.spr"),
        },
        ActorState {
            action: Action::Idle,
        },
        ActorDirection(Vec2::Y),
        Transform::from_xyz(20.0, 0.0, 100.0).with_scale(Vec3::new(0.15, 0.15, 0.15)),
    ));

     commands.spawn((
        ActorSprite {
            body: "sprite/human_female_assassin/body.spr",
            head: None,
            weapon: None,
            weapon_slash: None,
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
/// Marker: this entity hosts body + head layers composited on a billboard child entity.
#[derive(Component)]
pub struct ActorSprite {
    pub body: &'static str,
    pub head: Option<&'static str>,
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

        let mut layers = vec![CompositeLayerDef {
            atlas: server.load(sprite.body),
            role: SpriteRole::Body,
        }];
        if let Some(head) = sprite.head {
            layers.push(CompositeLayerDef {
                atlas: server.load(head),
                role: SpriteRole::Head,
            })
        }
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
