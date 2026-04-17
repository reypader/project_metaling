mod camera_orbit;
mod interaction;
mod map_interaction;
mod occlusion_fading;
mod player_control;
use crate::camera_orbit::{CameraFollower, OrbitCameraPlugin};
use crate::interaction::{InteractionPlugin, InteractionTarget, LookTarget};
use crate::map_interaction::{MapInteractionPlugin, MapMarker};
use crate::occlusion_fading::{OcclusionFadingConfig, OcclusionFadingPlugin, OcclusionMode};
use crate::player_control::{PlayerControl, PlayerControlPlugin};

use bevy::light::CascadeShadowConfigBuilder;
use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy_ro_maps::{RoMapRoot, RoMapsPlugin};
use bevy_ro_models::RoModelsPlugin;
use bevy_ro_sounds::RoSoundsPlugin;
use bevy_ro_sprites::SpriteFrameEvent;
use bevy_ro_sprites::prelude::{Action, ActorDirection, ActorSprite, ActorState, RoSpritePlugin};
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
        .add_plugins(RoSpritePlugin::default())
        .add_plugins(RoMapsPlugin {
            assets_root: "/Users/rmpader/code_projects/project_metaling/target/assets".into(),
        })
        .add_plugins(RoModelsPlugin)
        .add_plugins(RoSoundsPlugin::default())
        .add_plugins(RoVfxPlugin {
            assets_root: "/Users/rmpader/code_projects/project_metaling/target/assets".into(),
            config_path: "/Users/rmpader/code_projects/project_metaling/config/EffectTable.json"
                .into(),
            effect_sprite_scale_divisor: 35.0,
        })
        .add_plugins(MeshPickingPlugin)
        .add_plugins(MapInteractionPlugin)
        .add_plugins(OcclusionFadingPlugin {
            config: OcclusionFadingConfig {
                mode: OcclusionMode::BoundingBox,
                ..default()
            },
        })
        .add_plugins(PlayerControlPlugin)
        .add_plugins(InteractionPlugin)
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
            body: "sprite/human/female_knight/body.spr".into(),
            head: Some("sprite/human/female_head/head/11.spr".into()),
            weapon: Some("sprite/human/female_knight/weapon/spear/weapon.spr".into()),
            weapon_slash: Some("sprite/human/female_knight/weapon/spear/slash.spr".into()),
        },
        ActorState {
            action: Action::Idle,
        },
        ActorDirection(-Vec3::Z.xz()),
        Transform::from_xyz(0.0, 0.0, 100.0).with_scale(Vec3::new(0.15, 0.15, 0.15)),
        PlayerControl,
        SpatialListener::new(1.0),
        InteractionTarget,
        LookTarget::default(),
    ));
    commands.spawn((Transform::from_xyz(0.0, 0.0, 0.0), CameraFollower));

    // Actor — body.spr + head 17.spr, composited in one quad
    commands.spawn((
        ActorSprite {
            body: "sprite/human/male_novice/body.spr".into(),
            head: Some("sprite/human/male_head/head/10.spr".into()),
            weapon: Some("sprite/human/male_novice/weapon/sword/weapon.spr".into()),
            weapon_slash: Some("sprite/human/male_novice/weapon/sword/slash.spr".into()),
        },
        ActorState {
            action: Action::Idle,
        },
        ActorDirection(Vec2::Y),
        Transform::from_xyz(-20.0, 0.0, 100.0).with_scale(Vec3::new(0.15, 0.15, 0.15)),
        InteractionTarget,
        LookTarget::default(),
    ));

    commands.spawn((
        ActorSprite {
            body: "sprite/monster/andre/body.spr".into(),
            head: None,
            weapon: None,
            weapon_slash: None,
        },
        ActorState {
            action: Action::Idle,
        },
        ActorDirection(Vec2::Y),
        Transform::from_xyz(-20.0, 0.0, 200.0).with_scale(Vec3::new(0.15, 0.15, 0.15)),
        InteractionTarget,
    ));

    commands.spawn((
        ActorSprite {
            body: "sprite/human/female_assassin/body.spr".into(),
            head: Some("sprite/human/female_head/head/5.spr".into()),
            weapon: Some("sprite/human/female_assassin/weapon/katar_katar/weapon.spr".into()),
            weapon_slash: Some("sprite/human/female_assassin/weapon/katar_katar/slash.spr".into()),
        },
        ActorState {
            action: Action::Idle,
        },
        ActorDirection(Vec2::Y),
        Transform::from_xyz(20.0, 0.0, 100.0).with_scale(Vec3::new(0.15, 0.15, 0.15)),
        InteractionTarget,
        LookTarget::default(),
    ));

    commands.spawn((
        ActorSprite {
            body: "sprite/monster/poring/body.spr".into(),
            head: None,
            weapon: None,
            weapon_slash: None,
        },
        ActorState {
            action: Action::Idle,
        },
        ActorDirection(Vec2::Y),
        Transform::from_xyz(40.0, 0.0, 100.0).with_scale(Vec3::new(0.15, 0.15, 0.15)),
        InteractionTarget,
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
