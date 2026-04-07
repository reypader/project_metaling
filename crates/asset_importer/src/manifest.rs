use serde::{Deserialize, Serialize};

/// Top-level manifest file structure.
#[derive(Deserialize, Serialize)]
pub struct Manifest {
    /// Path to the GRF data root directory (the "data" folder inside the GRF extraction).
    /// All sprite paths in entries are relative to this directory.
    pub data_root: String,
    /// Absolute or relative path where exported spritesheets are written.
    pub output_root: String,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub body: Vec<BodyEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub head: Vec<HeadEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headgear: Vec<HeadgearEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub garment: Vec<GarmentEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub weapon: Vec<WeaponEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shield: Vec<ShieldEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shadow: Vec<ShadowEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub projectile: Vec<ProjectileEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub map: Vec<MapEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effect: Vec<EffectEntry>,
}

#[derive(Deserialize, Serialize)]
pub struct BodyEntry {
    pub job: String,
    pub gender: String,
    /// SPR path relative to grf_root.
    pub spr: String,
    /// ACT path relative to grf_root.
    pub act: String,
    /// Optional IMF path relative to data_root (e.g. "imf/novice_male.imf").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imf: Option<String>,
}

#[derive(Deserialize, Serialize)]
pub struct HeadEntry {
    /// Numeric head sprite ID (the number in the filename, e.g. 1 for "1_male").
    pub id: u32,
    pub gender: String,
    pub spr: String,
    pub act: String,
    /// Body IMF path relative to data_root for head-behind-body z-order computation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imf: Option<String>,
}

#[derive(Deserialize, Serialize)]
pub struct HeadgearEntry {
    /// Sprite name (accname without gender prefix, e.g. "ribbon" for m_ribbon).
    pub name: String,
    /// Accname view ID; informational only, does not affect export.
    pub view: u32,
    /// Equipment slot: "Head_Top" | "Head_Mid" | "Head_Low".
    pub slot: String,
    pub gender: String,
    pub spr: String,
    pub act: String,
}

#[derive(Deserialize, Serialize)]
pub struct GarmentEntry {
    pub name: String,
    pub job: String,
    pub gender: String,
    pub spr: String,
    pub act: String,
}

#[derive(Deserialize, Serialize)]
pub struct WeaponEntry {
    pub name: String,
    pub job: String,
    pub gender: String,
    /// "weapon" for the main weapon sprite, "slash" for the slash effect overlay.
    pub slot: String,
    pub spr: String,
    pub act: String,
}

#[derive(Deserialize, Serialize)]
pub struct ShieldEntry {
    /// Shield sprite name (everything after `{job}_{gender}_` in the filename).
    pub name: String,
    pub job: String,
    pub gender: String,
    pub spr: String,
    pub act: String,
}

#[derive(Deserialize, Serialize)]
pub struct ShadowEntry {
    pub spr: String,
    pub act: String,
}

#[derive(Deserialize, Serialize)]
pub struct ProjectileEntry {
    /// Sprite name (filename stem, e.g. "canon_bullet").
    pub name: String,
    pub spr: String,
    pub act: String,
}

#[derive(Deserialize, Serialize)]
pub struct MapEntry {
    /// Map name (filename stem, e.g. "prontera").
    pub name: String,
    /// RSW path relative to data_root (scene definition: object placement, lighting, audio).
    pub rsw: String,
    /// GND path relative to data_root (terrain geometry and texture references).
    pub gnd: String,
    /// GAT path relative to data_root (collision/height data, self-contained).
    pub gat: String,
}

#[derive(Deserialize, Serialize)]
pub struct EffectEntry {
    /// Sprite name (filename stem, e.g. "torch_01").
    pub name: String,
    pub spr: String,
    pub act: String,
}
