use bevy::math::Vec2;
use bevy::prelude::Resource;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug)]
pub struct CylinderDef {
    pub texture_name: String,
    pub height: f32,
    pub bottom_size: f32,
    pub top_size: f32,
    /// [r, g, b, alpha_max]
    pub color: [f32; 4],
    /// true when blendMode == 2
    pub blend_additive: bool,
    /// `animation` field from entry; 1 means no frame cycling
    pub animation_frames: u32,
    pub rotate: bool,
    pub duration_ms: f32,
}

/// Shared definition for `Plane2D` and `Plane3D` effect types.
///
/// Sizes (`size_start`, `size_end`) are in raw pixel units; divide by 35 for world units.
/// Positions (`pos_start`, `pos_end`) are in world units, relative to the emitter.
/// For `Plane3D`, `posz` is a fixed Y height offset; `angle`/`to_angle` are Y-axis degrees.
/// For `Plane2D`, `angle` is a Z-rotation tilt; the effect faces the camera.
#[derive(Debug, Clone)]
pub struct PlaneDef {
    /// Texture file with extension, `"effect/"` prefix stripped.
    pub file: String,
    pub duration_ms: f32,
    pub alpha_max: f32,
    pub fade_in: bool,
    pub fade_out: bool,
    /// `blendMode == 2`
    pub blend_additive: bool,
    /// RGB tint (default `[1.0, 1.0, 1.0]`).
    pub color: [f32; 3],
    /// Start size in raw pixel units (divide by 35 for world units).
    pub size_start: Vec2,
    /// End size in raw pixel units.
    pub size_end: Vec2,
    /// Start position offset from emitter in world units `(posxStart, poszStart)`.
    pub pos_start: Vec2,
    /// End position offset from emitter in world units `(posxEnd, poszEnd)`.
    pub pos_end: Vec2,
    /// Fixed Y height offset (for 3D effects; ignored by 2D).
    pub posz: f32,
    /// Initial angle in degrees.
    pub angle: f32,
    /// Final angle in degrees (equals `angle` when there is no rotation animation).
    pub to_angle: f32,
}

#[derive(Debug)]
pub enum EffectKind {
    AudioOnly,
    Cylinder(CylinderDef),
    Str { file: String },
    Spr { file: String },
    Plane2D(PlaneDef),
    Plane3D(PlaneDef),
    Func,
}

#[derive(Debug)]
pub struct EffectEntry {
    pub kind: EffectKind,
    pub wav: Option<String>,
}

#[derive(Resource, Default)]
pub struct EffectTable(pub HashMap<u32, Vec<EffectEntry>>);

pub fn load_effect_table(path: &Path) -> EffectTable {
    let raw = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            bevy::log::warn!("[RoVfx] Could not read EffectTable at {:?}: {}", path, e);
            return EffectTable::default();
        }
    };

    let root: Value = match json5::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            bevy::log::warn!("[RoVfx] Failed to parse EffectTable: {}", e);
            return EffectTable::default();
        }
    };

    let Some(root_obj) = root.as_object() else {
        bevy::log::warn!("[RoVfx] EffectTable root is not an object");
        return EffectTable::default();
    };

    let mut table: HashMap<u32, Vec<EffectEntry>> = HashMap::new();

    for (key, entries_val) in root_obj {
        let Ok(effect_id) = key.parse::<u32>() else {
            continue;
        };
        let Some(entries) = entries_val.as_array() else {
            continue;
        };

        let mut parsed_entries: Vec<EffectEntry> = Vec::new();
        for entry_val in entries {
            let Some(entry) = entry_val.as_object() else {
                continue;
            };

            let wav = entry
                .get("wav")
                .and_then(|v| v.as_str())
                .map(|s| s.to_owned());

            let type_str = entry.get("type").and_then(|v| v.as_str());

            let kind = match type_str {
                None => EffectKind::AudioOnly,
                Some("CYLINDER") => {
                    let texture_name = entry
                        .get("textureName")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .trim_start_matches("effect/")
                        .to_owned();
                    let height = f32_field(entry, "height", 10.0);
                    let bottom_size = f32_field(entry, "bottomSize", 1.0);
                    let top_size = f32_field(entry, "topSize", 1.0);
                    let r = f32_field(entry, "red", 1.0);
                    let g = f32_field(entry, "green", 1.0);
                    let b = f32_field(entry, "blue", 1.0);
                    let alpha_max = f32_field(entry, "alphaMax", 1.0);
                    let blend_mode = u32_field(entry, "blendMode", 1);
                    let animation_frames = u32_field(entry, "animation", 1).max(1);
                    let rotate = entry
                        .get("rotate")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let duration_ms = f32_field(entry, "duration", 0.0);
                    EffectKind::Cylinder(CylinderDef {
                        texture_name,
                        height,
                        bottom_size,
                        top_size,
                        color: [r, g, b, alpha_max],
                        blend_additive: blend_mode == 2,
                        animation_frames,
                        rotate,
                        duration_ms,
                    })
                }
                Some("STR") => {
                    let file = entry
                        .get("file")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_owned();
                    EffectKind::Str { file }
                }
                Some("SPR") => {
                    let file = entry
                        .get("file")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_owned();
                    EffectKind::Spr { file }
                }
                Some("2D") => {
                    if entry.contains_key("duplicate") {
                        bevy::log::debug!(
                            "[RoVfx] Skipping 2D effect with 'duplicate' (not yet supported)"
                        );
                        continue;
                    }
                    EffectKind::Plane2D(parse_plane_def(entry))
                }
                Some("3D") => {
                    if entry.contains_key("duplicate") {
                        bevy::log::debug!(
                            "[RoVfx] Skipping 3D effect with 'duplicate' (not yet supported)"
                        );
                        continue;
                    }
                    EffectKind::Plane3D(parse_plane_def(entry))
                }
                Some("FUNC") => EffectKind::Func,
                Some(other) => {
                    bevy::log::warn!("[RoVfx] Unknown effect type '{}', skipping", other);
                    continue;
                }
            };

            parsed_entries.push(EffectEntry { kind, wav });
        }

        if !parsed_entries.is_empty() {
            table.insert(effect_id, parsed_entries);
        }
    }

    bevy::log::info!("[RoVfx] Loaded {} effect table entries", table.len());
    EffectTable(table)
}

fn parse_plane_def(entry: &serde_json::Map<String, Value>) -> PlaneDef {
    let file = entry
        .get("file")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim_start_matches("effect/")
        .to_owned();

    let duration_ms = f32_field(entry, "duration", 500.0);
    let alpha_max = f32_field(entry, "alphaMax", 1.0);
    let fade_in = entry.get("fadeIn").and_then(|v| v.as_bool()).unwrap_or(false);
    let fade_out = entry.get("fadeOut").and_then(|v| v.as_bool()).unwrap_or(false);
    let blend_additive = u32_field(entry, "blendMode", 1) == 2;
    let color = [
        f32_field(entry, "red", 1.0),
        f32_field(entry, "green", 1.0),
        f32_field(entry, "blue", 1.0),
    ];

    // Resolve size from the various field patterns used in EffectTable.json.
    let size_start;
    let size_end;

    if entry.contains_key("sizeStartX") || entry.contains_key("sizeStartY") {
        // Explicit per-axis: sizeStartX/Y + sizeEndX/Y
        size_start = Vec2::new(
            f32_field(entry, "sizeStartX", 10.0),
            f32_field(entry, "sizeStartY", 10.0),
        );
        size_end = Vec2::new(
            f32_field(entry, "sizeEndX", f32_field(entry, "sizeStartX", 10.0)),
            f32_field(entry, "sizeEndY", f32_field(entry, "sizeStartY", 10.0)),
        );
    } else if entry.contains_key("sizeX") {
        // Fixed-width slash: sizeX + sizeStartY/sizeEndY
        let sx = f32_field(entry, "sizeX", 10.0);
        size_start = Vec2::new(sx, f32_field(entry, "sizeStartY", 10.0));
        size_end = Vec2::new(sx, f32_field(entry, "sizeEndY", f32_field(entry, "sizeStartY", 10.0)));
    } else if entry.contains_key("sizeStart") {
        // Uniform scale: sizeStart / sizeEnd
        let ss = f32_field(entry, "sizeStart", 10.0);
        let se = f32_field(entry, "sizeEnd", ss);
        size_start = Vec2::splat(ss);
        size_end = Vec2::splat(se);
    } else {
        // Single fixed size
        let s = f32_field(entry, "size", 10.0);
        size_start = Vec2::splat(s);
        size_end = Vec2::splat(s);
    }

    let posx_start = f32_field(entry, "posxStart", f32_field(entry, "posx", 0.0));
    let posx_end = f32_field(entry, "posxEnd", posx_start);
    let posz_start = f32_field(entry, "poszStart", 0.0);
    let posz_end = f32_field(entry, "poszEnd", posz_start);
    let posz = f32_field(entry, "posz", 0.0);

    let angle = f32_field(entry, "angle", 0.0);
    let to_angle = f32_field(entry, "toAngle", angle);

    PlaneDef {
        file,
        duration_ms,
        alpha_max,
        fade_in,
        fade_out,
        blend_additive,
        color,
        size_start,
        size_end,
        pos_start: Vec2::new(posx_start, posz_start),
        pos_end: Vec2::new(posx_end, posz_end),
        posz,
        angle,
        to_angle,
    }
}

fn f32_field(obj: &serde_json::Map<String, Value>, key: &str, default: f32) -> f32 {
    obj.get(key)
        .and_then(|v| v.as_f64())
        .map(|n| n as f32)
        .unwrap_or(default)
}

fn u32_field(obj: &serde_json::Map<String, Value>, key: &str, default: u32) -> u32 {
    obj.get(key)
        .and_then(|v| v.as_f64())
        .map(|n| n as u32)
        .unwrap_or(default)
}
