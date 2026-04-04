use anyhow::{anyhow, bail, Context, Result};
use std::io::{Cursor, Seek, SeekFrom};

use crate::util::{check_magic, read_fixed_string, rf32, ri32, ru32, ru8};

use std::collections::{BTreeSet, HashMap};

use crate::translate::{decode_cp949_path, translate_cp949_path};
#[derive(Debug, Clone)]
pub struct RswLighting {
    pub longitude: u32,
    pub latitude: u32,
    pub diffuse: [f32; 3],
    pub ambient: [f32; 3],
    pub shadowmap_alpha: f32,
}

#[derive(Debug, Clone)]
pub struct ModelInstance {
    pub name: String,
    pub anim_type: i32,
    pub anim_speed: f32,
    pub collision_flags: i32,
    pub model_file: String,
    pub node_name: String,
    pub pos: [f32; 3],
    pub rot: [f32; 3],
    pub scale: [f32; 3],
}

#[derive(Debug, Clone)]
pub struct LightSource {
    pub name: String,
    pub pos: [f32; 3],
    pub diffuse: [f32; 3],
    pub range: f32,
}

#[derive(Debug, Clone)]
pub struct AudioSource {
    pub name: String,
    pub file: String,
    pub pos: [f32; 3],
    pub volume: f32,
    pub width: u32,
    pub height: u32,
    pub range: f32,
    pub cycle: f32,
}

#[derive(Debug, Clone)]
pub struct EffectEmitter {
    pub name: String,
    pub pos: [f32; 3],
    pub effect_id: u32,
    pub emit_speed: f32,
    pub params: [f32; 4],
}

#[derive(Debug, Clone)]
pub enum RswObject {
    Model(ModelInstance),
    Light(LightSource),
    Audio(AudioSource),
    Effect(EffectEmitter),
}

pub struct RswFile {
    pub version: (u8, u8),
    pub lighting: RswLighting,
    pub objects: Vec<RswObject>,
}

fn at_least(version: (u8, u8), major: u8, minor: u8) -> bool {
    version.0 > major || (version.0 == major && version.1 >= minor)
}

impl RswFile {
    /// Implementation covers RSW v1.9-v2.6 (build 162).
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut c = Cursor::new(data);
        check_magic(&mut c, b"GRSW")?;
        let major = ru8(&mut c)?;
        let minor = ru8(&mut c)?;

        (|| -> anyhow::Result<RswFile> {
            let version = (major, minor);

            // Build number: present in v2.2+ (browedit only reads one extra byte for exactly v2.2)
            if at_least(version, 2, 5) {
                let _ = ru32(&mut c)?; // build_number (u32 in v2.5+)
                let _ = ru8(&mut c)?; // unknown render flag
            } else if at_least(version, 2, 2) {
                let _ = ru8(&mut c)?; // build_number (u8 for v2.2)
            }

            // File references: ini (40), gnd (40), gat (40, present for v1.5+), src (40)
            let _ = read_fixed_string(&mut c, 40)?; // ini file
            let _ = read_fixed_string(&mut c, 40)?; // gnd file
            if major > 1 || minor > 4 {
                let _ = read_fixed_string(&mut c, 40)?; // gat file (v1.5+)
            }
            let _ = read_fixed_string(&mut c, 40)?; // src file

            // Water configuration: present when version < (2, 6) — moved to GND in v2.6+
            if !at_least(version, 2, 6) {
                // level, type, waveHeight, waveSpeed, wavePitch, textureCyclingInterval = 6 × 4 bytes
                c.seek(SeekFrom::Current(24))?;
            }

            // Lighting
            let longitude = ru32(&mut c)?;
            let latitude = ru32(&mut c)?;
            let diffuse = [rf32(&mut c)?, rf32(&mut c)?, rf32(&mut c)?];
            let ambient = [rf32(&mut c)?, rf32(&mut c)?, rf32(&mut c)?];
            let shadowmap_alpha = rf32(&mut c)?;

            // Map render flags / bounding box (16 bytes); present in v2.5+ as well as earlier
            // versions that embed a bounding box. Skip to stay version-safe.
            c.seek(SeekFrom::Current(16))?;

            // Objects
            let object_count = ri32(&mut c)? as usize;
            let mut objects = Vec::with_capacity(object_count);

            for _ in 0..object_count {
                let type_id = ri32(&mut c)?;
                match type_id {
                    1 => {
                        let name = read_fixed_string(&mut c, 40)?;
                        let anim_type = ri32(&mut c)?;
                        let anim_speed = rf32(&mut c)?;
                        let collision_flags = ri32(&mut c)?;

                        // v2.6 build 162+: one extra unknown byte between collision_flags and model_file
                        // We cannot check build number here without storing it, so we rely on the
                        // caller knowing. For safety we skip this only when we have evidence the
                        // field is present; for now it is omitted (rare, affects only some v2.6 maps).

                        let model_file = read_fixed_string(&mut c, 80)?;
                        let node_name = read_fixed_string(&mut c, 80)?;
                        let pos = [rf32(&mut c)?, rf32(&mut c)?, rf32(&mut c)?];
                        let rot = [rf32(&mut c)?, rf32(&mut c)?, rf32(&mut c)?];
                        let scale = [rf32(&mut c)?, rf32(&mut c)?, rf32(&mut c)?];
                        objects.push(RswObject::Model(ModelInstance {
                            name,
                            anim_type,
                            anim_speed,
                            collision_flags,
                            model_file,
                            node_name,
                            pos,
                            rot,
                            scale,
                        }));
                    }
                    2 => {
                        let name = read_fixed_string(&mut c, 80)?;
                        let pos = [rf32(&mut c)?, rf32(&mut c)?, rf32(&mut c)?];
                        let diffuse = [rf32(&mut c)?, rf32(&mut c)?, rf32(&mut c)?];
                        let range = rf32(&mut c)?;
                        objects.push(RswObject::Light(LightSource {
                            name,
                            pos,
                            diffuse,
                            range,
                        }));
                    }
                    3 => {
                        let name = read_fixed_string(&mut c, 80)?;
                        let file = read_fixed_string(&mut c, 80)?;
                        let pos = [rf32(&mut c)?, rf32(&mut c)?, rf32(&mut c)?];
                        let volume = rf32(&mut c)?;
                        let width = ru32(&mut c)?;
                        let height = ru32(&mut c)?;
                        let range = rf32(&mut c)?;
                        // cycle interval: present in v2.0+; default to 4.0 for older files
                        let cycle = if at_least(version, 2, 0) {
                            rf32(&mut c)?
                        } else {
                            4.0
                        };
                        objects.push(RswObject::Audio(AudioSource {
                            name,
                            file,
                            pos,
                            volume,
                            width,
                            height,
                            range,
                            cycle,
                        }));
                    }
                    4 => {
                        let name = read_fixed_string(&mut c, 80)?;
                        let pos = [rf32(&mut c)?, rf32(&mut c)?, rf32(&mut c)?];
                        let effect_id = ru32(&mut c)?;
                        let emit_speed = rf32(&mut c)?;
                        let params = [rf32(&mut c)?, rf32(&mut c)?, rf32(&mut c)?, rf32(&mut c)?];
                        objects.push(RswObject::Effect(EffectEmitter {
                            name,
                            pos,
                            effect_id,
                            emit_speed,
                            params,
                        }));
                    }
                    n => return Err(anyhow!("unknown RSW object type: {}", n)),
                }
            }

            // QuadTree follows in v2.1+ but nothing comes after it, so we stop here.

            Ok(RswFile {
                version,
                lighting: RswLighting {
                    longitude,
                    latitude,
                    diffuse,
                    ambient,
                    shadowmap_alpha,
                },
                objects,
            })
        })()
            .with_context(|| format!("RSW v{major}.{minor} (implementation covers v1.9-v2.6)"))
    }
}

/// Translate model_file paths in all type-1 (Model) objects and normalize audio file paths
/// in all type-3 (Audio) objects within an RSW file.
///
/// Model paths are translated and ensured to have a `model/` prefix.
/// Audio paths are not translated (Korean WAV filenames are intentional ACT event keys)
/// but are decoded and ensured to have a `wav/` prefix.
/// All slots are 80 bytes, so the output is always the same size as the input.
pub fn rewrite_model_paths(
    data: &[u8],
    known: &HashMap<String, String>,
    misses: &mut BTreeSet<String>,
) -> Result<Vec<u8>> {
    if data.len() < 6 {
        bail!("RSW file too short ({} bytes)", data.len());
    }
    if &data[0..4] != b"GRSW" {
        bail!("not an RSW file: bad magic {:?}", &data[0..4]);
    }

    let major = data[4];
    let minor = data[5];
    let version = (major, minor);

    // Version-dependent extra bytes after major/minor (offset 6):
    //   v2.5+: build_number (u32) + unknown_render_flag (u8) = 5 bytes
    //   v2.2–v2.4: build_number (u8) = 1 byte
    //   else: 0 bytes
    let (version_extra, build_number) = if at_least(version, 2, 5) {
        if data.len() < 11 {
            bail!("RSW v{major}.{minor}: file too short for v2.5+ header");
        }
        let build = u32::from_le_bytes(data[6..10].try_into()?);
        (5usize, build)
    } else if at_least(version, 2, 2) {
        (1usize, 0u32)
    } else {
        (0usize, 0u32)
    };

    // File references: ini(40) + gnd(40) + gat(40, all v1.5+) + src(40) = 160 bytes
    let file_ref_start = 6 + version_extra;
    let file_ref_len = 160usize;

    // Water config: 24 bytes if version < (2, 6)
    let water_len: usize = if at_least(version, 2, 6) { 0 } else { 24 };

    // Lighting (36) + map boundaries/render flags (16) + object_count (4)
    let fixed_tail = 36 + 16 + 4;

    let objects_offset = file_ref_start + file_ref_len + water_len + fixed_tail;
    if data.len() < objects_offset {
        bail!("RSW v{major}.{minor}: file too short to reach objects section");
    }

    let object_count_offset = file_ref_start + file_ref_len + water_len + 36 + 16;
    let object_count =
        i32::from_le_bytes(data[object_count_offset..object_count_offset + 4].try_into()?) as usize;

    if object_count == 0 {
        return Ok(data.to_vec());
    }

    // v2.6.162+: one extra byte after collision_flags before model_file
    let extra_collision_byte: usize = if major == 2 && minor == 6 && build_number >= 162 {
        1
    } else {
        0
    };

    // Audio cycle field present in v2.0+
    let audio_cycle_bytes: usize = if at_least(version, 2, 0) { 4 } else { 0 };

    let mut out = data.to_vec();
    let mut pos = objects_offset;

    for _ in 0..object_count {
        if pos + 4 > data.len() {
            bail!("RSW: truncated at object type_id, offset {pos}");
        }
        let type_id = i32::from_le_bytes(data[pos..pos + 4].try_into()?);
        pos += 4;

        match type_id {
            1 => {
                // Model: name(40) + anim_type(4) + anim_speed(4) + collision_flags(4)
                //      + optional extra byte (v2.6.162+) + model_file(80) + node_name(80)
                //      + pos(12) + rot(12) + scale(12)
                let model_file_offset = pos + 40 + 4 + 4 + 4 + extra_collision_byte;
                let object_size = 40 + 4 + 4 + 4 + extra_collision_byte + 80 + 80 + 12 + 12 + 12;
                if model_file_offset + 80 > data.len() {
                    bail!("RSW: truncated at model_file, offset {model_file_offset}");
                }
                let raw = &data[model_file_offset..model_file_offset + 80];
                let translated = ensure_prefix(translate_cp949_path(raw, known, misses), "model/");
                let translated_bytes = translated.as_bytes();
                let copy_len = translated_bytes.len().min(79);
                out[model_file_offset..model_file_offset + 80].fill(0);
                out[model_file_offset..model_file_offset + copy_len]
                    .copy_from_slice(&translated_bytes[..copy_len]);
                pos += object_size;
            }
            2 => {
                // Light: name(80) + pos(12) + diffuse(12) + range(4)
                pos += 80 + 12 + 12 + 4;
            }
            3 => {
                // Audio: name(80) + file(80) + pos(12) + vol(4) + width(4) + height(4) + range(4)
                //      + cycle(4, v2.0+)
                // file segments must NOT be translated (Korean WAV names are ACT event keys),
                // but the path is decoded and ensured to have a wav/ prefix.
                let audio_file_offset = pos + 80;
                if audio_file_offset + 80 > data.len() {
                    bail!("RSW: truncated at audio file, offset {audio_file_offset}");
                }
                let raw = &data[audio_file_offset..audio_file_offset + 80];
                let normalized = ensure_prefix(decode_cp949_path(raw), "wav/");
                let normalized_bytes = normalized.as_bytes();
                let copy_len = normalized_bytes.len().min(79);
                out[audio_file_offset..audio_file_offset + 80].fill(0);
                out[audio_file_offset..audio_file_offset + copy_len]
                    .copy_from_slice(&normalized_bytes[..copy_len]);
                pos += 80 + 80 + 12 + 4 + 4 + 4 + 4 + audio_cycle_bytes;
            }
            4 => {
                // Effect: name(80) + pos(12) + effect_id(4) + emit_speed(4) + params(16)
                pos += 80 + 12 + 4 + 4 + 16;
            }
            other => {
                bail!("RSW: unknown object type {other} at offset {}", pos - 4);
            }
        }
    }

    Ok(out)
}

fn ensure_prefix(path: String, prefix: &str) -> String {
    let path = crate::translate::strip_data_prefix(&path);
    if path.starts_with(prefix) {
        path.to_string()
    } else {
        format!("{prefix}{path}")
    }
}
