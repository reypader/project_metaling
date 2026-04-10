mod assets;
mod bgm;
mod loader;
mod navigation;
mod render;
mod terrain_material;

pub use assets::RoMapAsset;
pub use bevy_ro_vfx::RoEffectEmitter;
pub use bgm::BgmTable;
pub use loader::RoMapLoader;
pub use navigation::NavMesh;
pub use render::{MapLightingReady, RoMapLight, RoMapMesh, RoMapRoot};
pub use terrain_material::{
    TERRAIN_LIGHTMAP_SHADER_HANDLE, TerrainLightmapExtension, TerrainMaterial,
};

pub mod prelude {
    pub use crate::{
        BgmTable, MapLightingReady, NavMesh, RoMapAsset, RoMapLight, RoMapLoader, RoMapMesh,
        RoMapRoot, RoMapsPlugin,
    };
    pub use bevy_ro_vfx::RoEffectEmitter;
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
        app.add_plugins(MaterialPlugin::<
            ExtendedMaterial<StandardMaterial, TerrainLightmapExtension>,
        >::default());
        app.init_asset::<RoMapAsset>();
        app.register_asset_loader(RoMapLoader);
        app.insert_resource(bgm::load_bgm_table(&self.assets_root));
        app.add_systems(Update, (render::spawn_map_meshes, render::animate_water));
        app.add_observer(apply_map_lighting);
    }
}
fn apply_map_lighting(
    trigger: On<MapLightingReady>,
    mut sun_query: Query<(&mut DirectionalLight, &mut Transform)>,
    mut ambient: ResMut<GlobalAmbientLight>,
) {
    let lighting = &trigger.event().0;

    // Convert spherical coordinates to a sun ray direction.
    // Start from (0, -1, 0) (straight down), rotate around X by -latitude then around Y
    // by longitude. The resulting vector is the direction the light travels.
    let lat_rad = (lighting.latitude as f32).to_radians();
    let lon_rad = (lighting.longitude as f32).to_radians();
    let rot = Quat::from_rotation_y(-lon_rad) * Quat::from_rotation_x(lat_rad);
    let sun_dir = rot * Vec3::NEG_Y;

    let [dr, dg, db] = lighting.diffuse;
    let [ar, ag, ab] = lighting.ambient;

    if let Ok((mut light, mut transform)) = sun_query.single_mut() {
        // light.color = Color::srgb(dr, dg * 0.92, db * 0.78);
        light.color = Color::srgb(dr, dg, db);

        light.illuminance = lighting.shadowmap_alpha * 8_000.0;
        *transform = Transform::IDENTITY.looking_to(sun_dir, Vec3::Y);
    }

    ambient.color = Color::srgb(ar, ag, ab);
    ambient.brightness = 800.0;
}
