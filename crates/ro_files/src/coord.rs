//! Coordinate conversion helpers for translating Ragnarok Online coordinate
//! space into Bevy's right-handed Y-up coordinate space.
//!
//! Gated behind the `bevy` feature flag so that non-Bevy consumers
//! (grf_extractor, asset_importer) do not pull in bevy_math.

use bevy_math::{EulerRot, Quat, Vec3};

/// Converts an RSW object position to Bevy local-space (child of the map root entity).
///
/// The map root entity carries a centering transform (see [`map_center_offset`]),
/// so children use this function which places them relative to the un-centered
/// BrowEdit3 world-space grid.
///
/// Used for models, lights, and effects placed via RSW.
pub fn rsw_local_pos(pos: [f32; 3], cx: f32, cz: f32, scale: f32) -> Vec3 {
    Vec3::new(cx + pos[0], -pos[1], scale + cz - pos[2])
}

/// Converts an RSW object position to final Bevy world-space.
///
/// Used for root-level entities (e.g. audio emitters) that are not parented
/// to the map root and therefore do not inherit its centering transform.
pub fn rsw_world_pos(pos: [f32; 3]) -> Vec3 {
    Vec3::new(pos[0], -pos[1], -pos[2])
}

/// Converts RSW model rotation (degrees, stored as `[rot_x, rot_y, rot_z]`)
/// to a Bevy `Quat`.
///
/// Applies the YXZ Euler convention with sign flips that match the
/// original RO/BrowEdit3 coordinate system.
pub fn rsw_rotation(rot: [f32; 3]) -> Quat {
    Quat::from_euler(
        EulerRot::YXZ,
        (-rot[1]).to_radians(),
        rot[0].to_radians(),
        (-rot[2]).to_radians(),
    )
}

/// Returns the translation offset applied to the map root entity to center
/// the terrain at the world origin.
///
/// Terrain geometry is built in BrowEdit3 world-space where X is in
/// `[0, 2*cx]` and Z is in `[scale, 2*cz + scale]`. This offset shifts
/// the root so the terrain spans `[-cx..cx]` x `[-cz..cz]`.
pub fn map_center_offset(cx: f32, cz: f32, scale: f32) -> Vec3 {
    Vec3::new(-cx, 0.0, -(scale + cz))
}

/// Converts RSW lighting spherical coordinates (longitude, latitude in degrees)
/// to a Bevy directional-light direction vector.
///
/// Starts from straight down (`-Y`), rotates around X by latitude, then
/// around Y by negative longitude. The resulting vector is the direction
/// the light ray travels.
pub fn lighting_direction(longitude: u32, latitude: u32) -> Vec3 {
    let lat_rad = (latitude as f32).to_radians();
    let lon_rad = (longitude as f32).to_radians();
    let rot = Quat::from_rotation_y(-lon_rad) * Quat::from_rotation_x(lat_rad);
    rot * Vec3::NEG_Y
}

/// Converts a GND cube height value from RO's Y-down convention to Bevy's Y-up.
#[inline]
pub fn gnd_height(height: f32) -> f32 {
    -height
}

/// Converts a GAT tile altitude value from RO's Y-down convention to Bevy's Y-up.
#[inline]
pub fn gat_altitude(altitude: f32) -> f32 {
    -altitude
}

/// Applies the RSM model-space Y-negate and Z-negate step that converts
/// from RO's internal RSM coordinate system to Bevy's Y-up, Z-forward space.
///
/// This is the final step in the RSM vertex transform chain, after the
/// offset, pos_, scale, rotation, and pos transforms have been applied.
#[inline]
pub fn rsm_flip_yz(p: [f32; 3]) -> [f32; 3] {
    [p[0], -p[1], -p[2]]
}
