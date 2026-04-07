use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy_ro_maps::RoEffectEmitter;
use bevy_ro_sprites::prelude::{
    advance_and_update_composite, CompositeLayerDef, RoComposite, RoCompositeMaterial, SpriteRole,
};
use std::collections::HashMap;
use std::path::PathBuf;

/// Marker placed on effect billboard child entities.
///
/// Used by game-side systems (e.g. occlusion fade) to distinguish effect billboards from
/// actor billboards without depending on game-crate internals.
#[derive(Component)]
pub struct EffectBillboard {
    pub scale_factor: f32,
}

/// Bevy plugin that manages visual effects driven by [`RoEffectEmitter`] entities.
///
/// Pass the asset root path (same as `AssetPlugin::file_path`) so the plugin can locate
/// `sprite/effect/effect_sprites.json` at startup.
pub struct RoVfxPlugin {
    /// Filesystem path to the Bevy asset root (same value as `AssetPlugin::file_path`).
    pub assets_root: PathBuf,
}

impl Plugin for RoVfxPlugin {
    fn build(&self, app: &mut App) {
        let json_path = self.assets_root.join("sprite/effect/effect_sprites.json");
        let map = std::fs::read_to_string(&json_path)
            .ok()
            .and_then(|json| serde_json::from_str::<HashMap<u32, String>>(&json).ok())
            .unwrap_or_default();

        if map.is_empty() {
            warn!(
                "[RoVfx] effect_sprites.json not found or empty at {:?} — no effect sprites will render",
                json_path
            );
        } else {
            info!("[RoVfx] Loaded {} effect sprite mappings", map.len());
        }

        app.insert_resource(EffectSpriteMap(map));
        app.add_systems(Update, attach_effect_sprites);
        app.add_systems(Update, update_effect_composites);
    }
}

/// Maps RSW effect IDs to SPR file stems (e.g. `47 → "torch_01"`).
#[derive(Resource, Default)]
struct EffectSpriteMap(HashMap<u32, String>);

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
            debug!("[RoVfx] No sprite registered for effect ID {}", emitter.effect_id);
            continue;
        };
        info!("[RoVfx] Attaching sprite for effect ID {} ({})", emitter.effect_id, stem);

        let spr_path = format!("sprite/effect/{stem}.spr");
        // tag: None plays all frames in sequence — effect ACTs are non-directional loops.
        commands
            .entity(entity)
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
}

/// Applies layout and positioning for effect billboard children (those with [`EffectBillboard`]).
/// Calls [`advance_and_update_composite`] for animation/material, then sizes and places the quad
/// using the effect's `scale_factor` and no feet lift.
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
        ) else {
            continue;
        };

        let sf = effect.scale_factor;
        transform.scale = Vec3::new(layout.canvas_size.x * sf, layout.canvas_size.y * sf, 1.0);

        let local_x = (layout.canvas_feet.x - layout.canvas_size.x / 2.0) * sf;
        let local_y = (layout.canvas_size.y / 2.0 - layout.canvas_feet.y) * sf;
        let billboard_right = transform.rotation * Vec3::X;
        let billboard_up = transform.rotation * Vec3::Y;
        transform.translation = -billboard_right * local_x - billboard_up * local_y;
    }
}
