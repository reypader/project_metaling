mod assets;
mod bgm;
mod loader;
mod render;
mod navigation;
mod terrain_material;

pub use assets::RoMapAsset;
pub use bgm::BgmTable;
pub use loader::RoMapLoader;
pub use render::{MapLightingReady, RoEffectEmitter, RoMapLight, RoMapMesh, RoMapRoot};
pub use navigation::NavMesh;
pub use terrain_material::{TerrainLightmapExtension, TerrainMaterial, TERRAIN_LIGHTMAP_SHADER_HANDLE};

pub mod prelude {
    pub use crate::{BgmTable, MapLightingReady, NavMesh, RoEffectEmitter, RoMapAsset, RoMapLight, RoMapLoader, RoMapMesh, RoMapRoot, RoMapsPlugin};
    pub use ro_files::TerrainType;
}

use bevy::pbr::ExtendedMaterial;
use bevy::prelude::*;
use std::path::PathBuf;

pub struct RoMapsPlugin {
    /// Filesystem path to the Bevy asset root (same value as `AssetPlugin::file_path`).
    /// Used to locate `misc/mp3nametable.txt` for BGM lookup.
    pub assets_root: PathBuf,
}

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
        app.insert_resource(bgm::load_bgm_table(&self.assets_root));
        app.add_systems(
            Update,
            (render::spawn_map_meshes, render::animate_water),
        );
    }
}
