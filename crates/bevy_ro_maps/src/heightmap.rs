use ro_files::GatFile;

/// Returns the terrain height at world position `(world_x, world_z)` by bilinearly
/// interpolating the four corner altitudes of the enclosing GAT tile.
///
/// `scale` is the world-units-per-tile value from `GndFile::scale` (always 10.0 in practice).
///
/// World coordinates are in the centered space produced by the map's root entity Transform:
/// X ranges from `-cx` to `+cx` and Z from `-cz` to `+cz`, where
/// `cx = gat.width * scale * 0.5` and `cz = gat.height * scale * 0.5`.
///
/// RO altitudes use a Y-down convention (deeper = more negative). This function negates them
/// so the result is a positive Bevy Y-up height.
///
/// Returns `0.0` for positions outside the map bounds.
pub fn height_at(gat: &GatFile, scale: f32, world_x: f32, world_z: f32) -> f32 {
    let cx = gat.width as f32 * scale * 0.5;
    let cz = gat.height as f32 * scale * 0.5;

    // Column: X runs left-to-right from -cx to +cx.
    let tile_x = (world_x + cx) / scale;
    let col = tile_x.floor() as i32;
    if col < 0 || col >= gat.width as i32 {
        return 0.0;
    }
    let fx = tile_x.fract();

    // Row: NW edge of tile row y is at centered Z = cz - (y+1)*scale,
    //      SW edge is at cz - y*scale (higher Z value).
    // Given world_z, find row using: row = ceil((cz - world_z) / scale).
    // fz = row - (cz - world_z)/scale  gives 0 at NW edge, 1 at SW edge.
    let u = (cz - world_z) / scale;
    let row = u.ceil() as i32;
    if row < 0 || row >= gat.height as i32 {
        return 0.0;
    }
    let fz = (row as f32 - u).clamp(0.0, 1.0);

    let Some(tile) = gat.tile(col as u32, row as u32) else {
        return 0.0;
    };

    let sw = -tile.altitude_sw;
    let se = -tile.altitude_se;
    let nw = -tile.altitude_nw;
    let ne = -tile.altitude_ne;

    // Bilinear interpolation: fz=0 samples the NW/NE edge, fz=1 samples the SW/SE edge.
    let north = nw + (ne - nw) * fx;
    let south = sw + (se - sw) * fx;
    north + (south - north) * fz
}
