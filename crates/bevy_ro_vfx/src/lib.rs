mod cylinder;
mod effect_table;
mod plane_effect;
mod str_effect;

use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy_ro_sounds::PlaySound;
use bevy_ro_sprites::prelude::{
    CompositeLayerDef, RoComposite, RoCompositeMaterial, SpriteRole, advance_and_update_composite,
};
use cylinder::{animate_cylinders, spawn_cylinder_effect};
use effect_table::{EffectKind, EffectTable, load_effect_table};
use plane_effect::{animate_plane_effects, spawn_plane_effect};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use str_effect::{animate_str, orient_str_billboards, spawn_str_effect};

/// Controls how many times a VFX effect plays before its entity is destroyed.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum EffectRepeat {
    /// Loop forever; the emitter entity is never automatically destroyed.
    #[default]
    Infinite,
    /// Play exactly `n` times, then despawn the emitter entity and all children.
    Times(u32),
}

/// Placed on each RSW effect-emitter entity by `bevy_ro_maps`.
/// The VFX plugin reacts to `Added<RoEffectEmitter>` and spawns the appropriate visuals/sounds.
#[derive(Component, Clone)]
pub struct RoEffectEmitter {
    pub effect_id: u32,
    pub repeat: EffectRepeat,
}

/// Marker placed on effect billboard child entities.
///
/// Used by game-side systems (e.g. occlusion fade) to distinguish effect billboards from
/// actor billboards without depending on game-crate internals.
#[derive(Component)]
pub struct EffectBillboard {
    pub scale_factor: f32,
    /// Remaining play count; `None` = infinite. Decremented each time the animation loops.
    pub remaining: Option<u32>,
    /// Frame index from the previous tick, used to detect loop boundaries.
    pub prev_frame: u16,
}

/// Holds init-time paths needed by runtime VFX systems.
#[derive(Resource)]
struct VfxConfig {
    assets_root: std::path::PathBuf,
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
        app.insert_resource(VfxConfig {
            assets_root: self.assets_root.clone(),
        });
        app.add_systems(Update, dispatch_effects);
        app.add_systems(Update, update_effect_composites);
        app.add_systems(Update, animate_cylinders);
        app.add_systems(Update, animate_str.before(orient_str_billboards));
        app.add_systems(Update, animate_plane_effects);
        app.add_systems(Update, orient_str_billboards);
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
    config: Res<VfxConfig>,
    new_effects: Query<(Entity, &RoEffectEmitter, &GlobalTransform), Added<RoEffectEmitter>>,
) {
    for (entity, emitter, gtf) in &new_effects {
        let id = emitter.effect_id;
        let repeat = emitter.repeat;
        let mut has_visual = false;

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
                        has_visual = true;
                        spawn_cylinder_effect(
                            &mut commands,
                            &mut meshes,
                            &mut std_mats,
                            &server,
                            entity,
                            def,
                            repeat,
                        );
                    }
                    EffectKind::Str { file } => {
                        has_visual = true;
                        let stem = file.trim_start_matches("effect/");
                        spawn_str_effect(
                            &mut commands,
                            &mut meshes,
                            &mut std_mats,
                            &server,
                            &config.assets_root,
                            entity,
                            stem,
                            5.0,
                            repeat,
                        );
                    }
                    EffectKind::Spr { file } => {
                        has_visual = true;
                        spawn_spr_effect(
                            &mut commands,
                            &mut meshes,
                            &mut composite_mats,
                            &server,
                            entity,
                            file,
                            repeat,
                        );
                    }
                    EffectKind::Plane2D(def) => {
                        has_visual = true;
                        spawn_plane_effect(
                            &mut commands,
                            &mut meshes,
                            &mut std_mats,
                            &server,
                            entity,
                            def,
                            true,
                            repeat,
                        );
                    }
                    EffectKind::Plane3D(def) => {
                        has_visual = true;
                        spawn_plane_effect(
                            &mut commands,
                            &mut meshes,
                            &mut std_mats,
                            &server,
                            entity,
                            def,
                            false,
                            repeat,
                        );
                    }
                    EffectKind::Func => {
                        warn!("[RoVfx] FUNC effect {id} not yet implemented");
                    }
                }
            }
        }


        // No visual animator was attached; clean up non-infinite emitters immediately.
        if !has_visual && repeat != EffectRepeat::Infinite {
            commands.entity(entity).despawn();
        }
    }
}

fn spawn_wav_effect(commands: &mut Commands, wav: &str, gtf: &GlobalTransform) {
    let tf = gtf.compute_transform();
    commands.trigger(PlaySound {
        path: format!("{wav}.wav"),
        looping: false,
        location: Some(tf),
        volume: Some(1.0),
        range: Some(1000.0),
    });
}

fn spawn_spr_effect(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<RoCompositeMaterial>,
    server: &AssetServer,
    parent_entity: Entity,
    stem: &str,
    repeat: EffectRepeat,
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
                    remaining: match repeat {
                        EffectRepeat::Infinite => None,
                        EffectRepeat::Times(n) => Some(n),
                    },
                    prev_frame: 0,
                },
                Pickable {
                    should_block_lower: false,
                    is_hoverable: false,
                },
            ));
        });
}

/// Applies layout and positioning for effect billboard children (those with [`EffectBillboard`]).
/// Also counts animation loop completions and despawns the parent emitter entity when done.
fn update_effect_composites(
    mut composites: Query<
        (
            &ChildOf,
            Entity,
            &mut RoComposite,
            &MeshMaterial3d<RoCompositeMaterial>,
            &mut Transform,
            &GlobalTransform,
            &mut EffectBillboard,
        ),
        Without<Camera3d>,
    >,
    atlases: Res<Assets<bevy_ro_sprites::RoAtlas>>,
    layouts: Res<Assets<TextureAtlasLayout>>,
    mut mats: ResMut<Assets<RoCompositeMaterial>>,
    time: Res<Time>,
    mut commands: Commands,
) {
    for (
        child_of,
        entity,
        mut composite,
        mat_handle,
        mut transform,
        global_transform,
        mut effect,
    ) in &mut composites
    {
        let prev_frame = effect.prev_frame;
        let Some(layout) = advance_and_update_composite(
            entity,
            &mut composite,
            mat_handle,
            &atlases,
            &layouts,
            &mut mats,
            &time,
            &mut commands,
            global_transform,
        ) else {
            continue;
        };

        // Detect when the animation wraps back to the start frame (loop boundary).
        effect.prev_frame = composite.current_frame;
        if composite.current_frame < prev_frame
            && let Some(remaining) = effect.remaining
        {
            let next = remaining - 1;
            if next == 0 {
                commands.entity(child_of.parent()).despawn();
                continue;
            }
            effect.remaining = Some(next);
        }

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
