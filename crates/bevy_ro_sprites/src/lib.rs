pub mod actor;
mod animation;
pub mod composite;
mod loader;

pub use animation::{
    render_animation, AnimationRepeat, RenderAnimation, RoAnimation, RoAnimationControl,
    RoAnimationPlugin, RoAnimationState, SpriteFrameEvent,
};
pub use loader::{RoAtlas, RoAtlasLoader, RoAtlasLoaderSettings, TagMeta};

/// Controls how sprite billboards orient toward the camera.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum BillboardMode {
    /// Each billboard faces the camera position directly (spherical orientation).
    /// Matches the original RO client behaviour but introduces slight angular
    /// divergence at the screen edges.
    #[default]
    Spherical,
    /// All billboards are parallel to the camera plane. Eliminates angular
    /// divergence at screen edges.
    CameraParallel,
}

/// Runtime configuration for the sprite plugin, inserted as a `Resource`.
#[derive(Resource, Clone, Debug)]
pub struct RoSpriteConfig {
    /// Path to the shadow sprite asset. Default: `"sprite/shadow/shadow.spr"`.
    pub shadow_sprite_path: String,
    /// Billboard orientation mode. Default: `BillboardMode::Spherical`.
    pub billboard_mode: BillboardMode,
    /// Maximum tilt angle (degrees) for spherical billboard mode. Clamped pitch
    /// is limited to this value. Has no effect when billboard_mode is CameraParallel.
    /// Default: `30.0`.
    pub spherical_max_tilt: f32,
}

impl Default for RoSpriteConfig {
    fn default() -> Self {
        Self {
            shadow_sprite_path: "sprite/shadow/shadow.spr".to_string(),
            billboard_mode: BillboardMode::default(),
            spherical_max_tilt: 30.0,
        }
    }
}

pub mod prelude {
    pub use crate::actor::{Action, ActorDirection, ActorSprite, ActorState};
    pub use crate::animation::{
        render_animation, AnimationRepeat, RenderAnimation, RoAnimation, RoAnimationControl,
        RoAnimationPlugin, RoAnimationState, SpriteFrameEvent,
    };
    pub use crate::composite::{
        advance_and_update_composite, composite_tag, direction_index, orient_billboard,
        ActorBillboard, CompositeLayerDef, CompositeLayout, RoComposite, RoCompositeMaterial,
        RoCompositePlugin, SpriteRole, MAX_LAYERS,
    };
    pub use crate::loader::{RoAtlas, RoAtlasLoader, TagMeta};
    pub use crate::{BillboardMode, RoSpriteConfig, RoSpritePlugin};
}

use bevy::prelude::*;

#[derive(Default)]
pub struct RoSpritePlugin {
    pub config: RoSpriteConfig,
}

impl Plugin for RoSpritePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.config.clone());
        app.init_asset::<RoAtlas>();
        app.register_asset_loader(RoAtlasLoader);
        app.add_plugins(RoAnimationPlugin);
        app.add_plugins(composite::RoCompositePlugin);
        app.add_systems(Update, actor::update_composite_tag);
        app.add_systems(Update, actor::spawn_actor_billboard);
    }
}

/// Human-readable action tag label for a flat ACT action index.
///
/// Uses [`Action::from_flat_index`] to resolve the action and direction.
/// Falls back to a generic `"action_NNN_dir"` label for indices that don't
/// map to a known `Action` variant (e.g. extra monster action groups beyond
/// the known 5).
pub(crate) fn action_label(idx: usize, total_actions: usize) -> String {
    use crate::actor::{ACT_DIR_SUFFIXES, Action};
    if let Some((action, dir)) = Action::from_flat_index(idx, total_actions) {
        format!("{}_{}", action.tag_name(), ACT_DIR_SUFFIXES[dir as usize])
    } else {
        let dir = idx % 8;
        format!("action_{idx:03}_{}", ACT_DIR_SUFFIXES[dir])
    }
}
