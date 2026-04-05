mod assets;
mod loader;
mod render;
mod navigation;

pub use assets::RoMapAsset;
pub use loader::RoMapLoader;
pub use render::{RoMapMesh, RoMapRoot};
pub use navigation::{NavMesh};

pub mod prelude {
    pub use crate::{NavMesh, RoMapAsset, RoMapLoader, RoMapMesh, RoMapRoot, RoMapsPlugin};
    pub use ro_files::TerrainType;
}

use bevy::prelude::*;
use bevy_ro_models::RsmPlugin;

pub struct RoMapsPlugin;

impl Plugin for RoMapsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(RsmPlugin);
        app.init_asset::<RoMapAsset>();
        app.register_asset_loader(RoMapLoader);
        app.add_systems(
            Update,
            (render::spawn_map_meshes, render::spawn_model_meshes),
        );
    }
}
