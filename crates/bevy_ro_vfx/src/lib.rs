mod cylinder;
mod effect_table;

use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy_ro_sounds::PlaySound;
use bevy_ro_sprites::prelude::{
    CompositeLayerDef, RoComposite, RoCompositeMaterial, SpriteRole, advance_and_update_composite,
};
use cylinder::{animate_cylinders, spawn_cylinder_effect};
use effect_table::{EffectKind, EffectTable, load_effect_table};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Placed on each RSW effect-emitter entity by `bevy_ro_maps`.
/// The VFX plugin reacts to `Added<RoEffectEmitter>` and spawns the appropriate visuals/sounds.
#[derive(Component, Clone)]
pub struct RoEffectEmitter {
    pub effect_id: u32,
}

/// Marker placed on effect billboard child entities.
///
/// Used by game-side systems (e.g. occlusion fade) to distinguish effect billboards from
/// actor billboards without depending on game-crate internals.
#[derive(Component)]
pub struct EffectBillboard {
    pub scale_factor: f32,
}

/// Bevy plugin that manages visual effects driven by [`RoEffectEmitter`] entities.
pub struct RoVfxPlugin {
    /// Filesystem path to the Bevy asset root (same value as `AssetPlugin::file_path`).
    pub assets_root: PathBuf,
    /// Path to `config/EffectTable.json` (JS-style effect definitions).
    pub config_path: PathBuf,
}

impl Plugin for RoVfxPlugin {
    fn build(&self, app: &mut App) {
        let sprite_map = load_effect_sprite_map(&self.assets_root);
        let effect_table = load_effect_table(&self.config_path);

        app.insert_resource(sprite_map);
        app.insert_resource(effect_table);
        app.add_systems(Update, dispatch_effects);
        app.add_systems(Update, update_effect_composites);
        app.add_systems(Update, animate_cylinders);
    }
}

/// Maps RSW effect IDs to SPR file stems (e.g. `47 → "torch_01"`).
/// Loaded from `sprite/effect/effect_sprites.json` in the assets root.
#[derive(Resource, Default)]
struct EffectSpriteMap(HashMap<u32, String>);

fn load_effect_sprite_map(assets_root: &Path) -> EffectSpriteMap {
    let json_path = assets_root.join("sprite/effect/effect_sprites.json");
    let map = std::fs::read_to_string(&json_path)
        .ok()
        .and_then(|json| serde_json::from_str::<HashMap<u32, String>>(&json).ok())
        .unwrap_or_default();

    if map.is_empty() {
        warn!(
            "[RoVfx] effect_sprites.json not found or empty at {:?} — no SPR effect sprites will render",
            json_path
        );
    } else {
        info!("[RoVfx] Loaded {} effect sprite mappings", map.len());
    }

    EffectSpriteMap(map)
}

/// Divisor applied to effect billboard canvas size to normalize ACT-baked pixel scales
/// to world units.
const EFFECT_SPRITE_SCALE: f32 = 1.0 / 35.0;

/// Reacts to newly spawned [`RoEffectEmitter`] entities and dispatches visuals and sounds
/// based on entries in [`EffectTable`] and [`EffectSpriteMap`].
fn dispatch_effects(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut std_mats: ResMut<Assets<StandardMaterial>>,
    mut composite_mats: ResMut<Assets<RoCompositeMaterial>>,
    server: Res<AssetServer>,
    effect_table: Res<EffectTable>,
    sprite_map: Res<EffectSpriteMap>,
    new_effects: Query<(Entity, &RoEffectEmitter, &GlobalTransform), Added<RoEffectEmitter>>,
) {
    for (entity, emitter, gtf) in &new_effects {
        let id = emitter.effect_id;
        // EffectTable path: CYLINDER, STR, SPR (EffectTable variant), wav-only, etc.
        if let Some(entries) = effect_table.0.get(&id) {
            for entry in entries {
                println!("Spawning effect {:?}", entry.kind);

                if let Some(wav) = &entry.wav {
                    spawn_wav_effect(&mut commands, wav, gtf);
                }
                match &entry.kind {
                    EffectKind::AudioOnly => {}
                    EffectKind::Cylinder(def) => {
                        spawn_cylinder_effect(
                            &mut commands,
                            &mut meshes,
                            &mut std_mats,
                            &server,
                            entity,
                            def,
                        );
                    }
                    EffectKind::Str { file } => {
                        warn!("[RoVfx] STR effect {id} not yet implemented: {file}");
                    }
                    EffectKind::Spr { file } => {
                        warn!("[RoVfx] EffectTable SPR effect {id} not yet implemented: {file}");
                    }
                    EffectKind::Plane2D | EffectKind::Plane3D => {
                        warn!("[RoVfx] 2D/3D plane effect {id} not yet implemented");
                    }
                    EffectKind::Func => {
                        warn!("[RoVfx] FUNC effect {id} not yet implemented");
                    }
                }
            }
        }

        // effect_sprites.json path: SPR/ACT billboard composites.
        if let Some(stem) = sprite_map.0.get(&id) {
            spawn_spr_effect(
                &mut commands,
                &mut meshes,
                &mut composite_mats,
                &server,
                entity,
                stem,
            );
        }
    }
}

fn spawn_wav_effect(commands: &mut Commands, wav: &str, gtf: &GlobalTransform) {
    let tf = gtf.compute_transform();
    commands.trigger(PlaySound {
        path: format!("{wav}.wav"),
        looping: false,
        location: Some(tf),
        volume: Some(20.0),
        range: Some(100000.0),
    });
}

fn spawn_spr_effect(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<RoCompositeMaterial>,
    server: &AssetServer,
    parent_entity: Entity,
    stem: &str,
) {
    info!("[RoVfx] Attaching SPR sprite for stem '{}'", stem);
    let spr_path = format!("sprite/effect/{stem}.spr");
    commands
        .entity(parent_entity)
        .insert(Visibility::Inherited)
        .with_children(|parent| {
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
                EffectBillboard {
                    scale_factor: EFFECT_SPRITE_SCALE,
                },
                Pickable {
                    should_block_lower: false,
                    is_hoverable: false,
                },
            ));
        });
}

/// Applies layout and positioning for effect billboard children (those with [`EffectBillboard`]).
fn update_effect_composites(
    mut composites: Query<
        (
            Entity,
            &mut RoComposite,
            &MeshMaterial3d<RoCompositeMaterial>,
            &mut Transform,
            &EffectBillboard,
        ),
        Without<Camera3d>,
    >,
    atlases: Res<Assets<bevy_ro_sprites::RoAtlas>>,
    layouts: Res<Assets<TextureAtlasLayout>>,
    mut mats: ResMut<Assets<RoCompositeMaterial>>,
    time: Res<Time>,
    mut commands: Commands,
) {
    for (entity, mut composite, mat_handle, mut transform, effect) in &mut composites {
        let Some(layout) = advance_and_update_composite(
            entity,
            &mut composite,
            mat_handle,
            &atlases,
            &layouts,
            &mut mats,
            &time,
            &mut commands,
            &transform,
        ) else {
            continue;
        };

        let sf = effect.scale_factor;
        transform.scale = Vec3::new(layout.canvas_size.x * sf, layout.canvas_size.y * sf, 1.0);

        let local_x = (layout.canvas_feet.x - layout.canvas_size.x / 2.0) * sf;
        let local_y = (layout.canvas_size.y / 2.0 - layout.canvas_feet.y) * sf;
        let billboard_right = transform.rotation * Vec3::X;
        let billboard_up = transform.rotation * Vec3::Y;
        transform.translation =
            (Vec3::Y * 8.0) - billboard_right * local_x - billboard_up * local_y;
    }
}
