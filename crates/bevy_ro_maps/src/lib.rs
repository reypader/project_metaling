mod assets;
mod loader;
mod render;
mod navigation;
mod terrain_material;

pub use assets::RoMapAsset;
pub use loader::RoMapLoader;
pub use render::{MapLightingReady, RoEffectEmitter, RoMapLight, RoMapMesh, RoMapRoot};
pub use navigation::NavMesh;
pub use terrain_material::{TerrainLightmapExtension, TerrainMaterial, TERRAIN_LIGHTMAP_SHADER_HANDLE};

pub mod prelude {
    pub use crate::{MapLightingReady, NavMesh, RoEffectEmitter, RoMapAsset, RoMapLight, RoMapLoader, RoMapMesh, RoMapRoot, RoMapsPlugin};
    pub use ro_files::TerrainType;
}

use bevy::pbr::ExtendedMaterial;
use bevy::prelude::*;

pub struct RoMapsPlugin;

impl Plugin for RoMapsPlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::load_internal_asset!(
            app,
            TERRAIN_LIGHTMAP_SHADER_HANDLE,
            "shaders/terrain_lightmap.wgsl",
            Shader::from_wgsl
        );
        app.add_plugins(MaterialPlugin::<ExtendedMaterial<StandardMaterial, TerrainLightmapExtension>>::default());
        app.init_asset::<RoMapAsset>();
        app.register_asset_loader(RoMapLoader);
        app.add_systems(
            Update,
            (render::spawn_map_meshes, render::animate_water),
        );
    }
}
