use ro_files::{GatFile, TerrainType};

/// Returns the [`TerrainType`] of the GAT tile at world position `(world_x, world_z)`.
///
/// `scale` is the world-units-per-tile value from `GndFile::scale` (always 10.0 in practice).
///
/// World coordinates are in the centered space produced by the map's root entity Transform:
/// X ranges from `-cx` to `+cx` and Z from `-cz` to `+cz`, where
/// `cx = gat.width * scale * 0.5` and `cz = gat.height * scale * 0.5`.
///
/// Returns [`TerrainType::Blocked`] for positions outside the map bounds.
pub fn terrain_at(gat: &GatFile, scale: f32, world_x: f32, world_z: f32) -> TerrainType {
    let cx = gat.width as f32 * scale * 0.5;
    let cz = gat.height as f32 * scale * 0.5;

    let col = ((world_x + cx) / scale).floor() as i32;
    // NW edge of tile row y is at centered Z = cz - (y+1)*scale;
    // SW edge at cz - y*scale. Row = ceil((cz - world_z) / scale).
    let row = ((cz - world_z) / scale).ceil() as i32;

    if col < 0 || col >= gat.width as i32 || row < 0 || row >= gat.height as i32 {
        return TerrainType::Blocked;
    }

    gat.tile(col as u32, row as u32)
        .map(|t| t.terrain_type)
        .unwrap_or(TerrainType::Blocked)
}

/// Returns `true` if the GAT tile at world position `(world_x, world_z)` is walkable.
///
/// `scale` is the world-units-per-tile value from `GndFile::scale` (always 10.0 in practice).
///
/// Returns `false` for positions outside the map bounds.
pub fn is_walkable(gat: &GatFile, scale: f32, world_x: f32, world_z: f32) -> bool {
    terrain_at(gat, scale, world_x, world_z) == TerrainType::Walkable
}
