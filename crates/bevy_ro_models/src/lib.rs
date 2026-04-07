mod assets;
mod loader;
mod render;

pub use assets::RsmAsset;
pub use loader::RsmLoader;
pub use render::{PendingModel, RoModelInstance, RoModelMesh};

use bevy::prelude::*;

/// Registers the RSM asset loader and the model-materialization systems.
///
/// Add this plugin to your app instead of [`RsmPlugin`] when you also want
/// the `PendingModel` → mesh-geometry pipeline.
pub struct RoModelsPlugin;

impl Plugin for RoModelsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(RsmPlugin);
        app.add_systems(
            Update,
            (
                render::start_loading_pending_models,
                render::materialize_loading_models,
                render::animate_rsm,
            ),
        );
    }
}

/// Lower-level plugin: only registers the RSM asset type and its loader.
/// Use [`RoModelsPlugin`] instead unless you need the asset without the rendering pipeline.
pub struct RsmPlugin;

impl Plugin for RsmPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<RsmAsset>();
        app.register_asset_loader(RsmLoader);
    }
}
