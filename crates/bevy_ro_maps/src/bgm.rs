use bevy::prelude::*;
use std::collections::HashMap;
use std::path::Path;

/// Maps map names (lowercase, e.g. `"prontera"`) to BGM asset paths
/// (e.g. `"bgm/08.mp3"`). Populated at startup from `misc/mp3nametable.json`,
/// which is produced by the asset importer from `mp3nametable.txt`.
#[derive(Resource, Default)]
pub struct BgmTable(pub HashMap<String, String>);

pub fn load_bgm_table(assets_root: &Path) -> BgmTable {
    let path = assets_root.join("misc/mp3nametable.json");
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => {
            warn!("[RoMap] BGM table not found at {}", path.display());
            return BgmTable::default();
        }
    };
    match serde_json::from_str::<HashMap<String, String>>(&text) {
        Ok(map) => {
            info!("[RoMap] BGM table loaded: {} entries", map.len());
            BgmTable(map)
        }
        Err(e) => {
            warn!("[RoMap] Failed to parse BGM table: {e}");
            BgmTable::default()
        }
    }
}
