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
#[derive(Debug)]
pub enum EffectKind {
    AudioOnly,
    Cylinder(CylinderDef),
    Str { file: String },
    Spr { file: String },
    Plane2D,
    Plane3D,
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
                Some("2D") => EffectKind::Plane2D,
                Some("3D") => EffectKind::Plane3D,
                Some("FUNC") => EffectKind::Func,
                Some(other) => {
                    bevy::log::debug!("[RoVfx] Unknown effect type '{}', skipping", other);
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
