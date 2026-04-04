use anyhow::{bail, Context, Result};
use std::collections::{BTreeSet, HashMap};
use std::io::{Cursor, Read, Seek, SeekFrom};

use crate::translate::translate_cp949_path;
use crate::util::{check_magic, rf32, ri16, ri32, ru8};

#[derive(Debug, Clone)]
pub struct GndLightmapSlice {
    /// 8×8 grayscale ambient occlusion (shadow) map.
    pub shadowmap: [u8; 64],
    /// 8×8 RGB baked light color.
    pub lightmap: [u8; 192],
}

/// A single textured surface (top or wall face of a cube). Indices into this list are stored
/// in `GndCube`.
#[derive(Debug, Clone)]
pub struct GndSurface {
    /// Diffuse U coordinates for the four corners in order [SW, SE, NW, NE].
    pub u: [f32; 4],
    /// Diffuse V coordinates for the four corners in order [SW, SE, NW, NE].
    pub v: [f32; 4],
    /// Index into `GndFile::texture_paths`. -1 means no texture.
    pub texture_id: i16,
    /// Index into `GndFile::lightmap_slices`.
    pub lightmap_id: i16,
    /// BGRA tile color (single per surface, not per-corner).
    pub color: [u8; 4],
}

/// One cell in the ground mesh grid. Each cube spans `scale` world units in X and Z.
#[derive(Debug, Clone)]
pub struct GndCube {
    /// Corner heights in order [SW, SE, NW, NE]. RO heights are Y-down; negate when converting
    /// to Bevy Y-up coordinates.
    pub heights: [f32; 4],
    /// Index into `GndFile::surfaces` for the top (horizontal) face. -1 = no surface.
    pub top_surface_id: i32,
    /// Index into `GndFile::surfaces` for the north (front) wall face. -1 = no surface.
    pub north_surface_id: i32,
    /// Index into `GndFile::surfaces` for the east (right) wall face. -1 = no surface.
    pub east_surface_id: i32,
}

#[derive(Debug, Clone)]
pub struct GndWaterPlane {
    pub level: f32,
    pub water_type: i32,
    pub wave_height: f32,
    pub wave_speed: f32,
    pub wave_pitch: f32,
    pub texture_cycling_interval: i32,
}

pub struct GndFile {
    pub version: (u8, u8),
    pub width: i32,
    pub height: i32,
    /// World units per cube edge. Always 10.0 in practice.
    pub scale: f32,
    /// Relative paths to diffuse textures (e.g. `"texture/유저인터페이스/map/...bmp"`).
    pub texture_paths: Vec<String>,
    /// Pre-computed lightmap slices baked into the map.
    pub lightmap_slices: Vec<GndLightmapSlice>,
    /// Surface definitions referenced by cube face IDs.
    pub surfaces: Vec<GndSurface>,
    /// Row-major grid of ground cubes. Index = row * width + col.
    pub cubes: Vec<GndCube>,
    /// Water plane configuration present in v1.8+.
    pub water: Option<GndWaterPlane>,
}

impl GndFile {
    /// Implementation covers GND v1.7-v1.9.
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut c = Cursor::new(data);

        check_magic(&mut c, b"GRGN")?;

        let major = ru8(&mut c)?;
        let minor = ru8(&mut c)?;

        (|| -> anyhow::Result<GndFile> {
            let version = (major, minor);

            let width = ri32(&mut c)?;
            let height = ri32(&mut c)?;
            let scale = rf32(&mut c)?;

            // Textures
            let texture_count = ri32(&mut c)? as usize;
            let texture_path_len = ri32(&mut c)? as usize;
            let mut texture_paths = Vec::with_capacity(texture_count);
            for i in 0..texture_count {
                let mut buf = vec![0u8; texture_path_len];
                c.read_exact(&mut buf).with_context(|| {
                    format!("texture {i}/{texture_count} (path_len={texture_path_len})")
                })?;
                let end = buf.iter().position(|&b| b == 0).unwrap_or(texture_path_len);
                texture_paths.push(String::from_utf8_lossy(&buf[..end]).into_owned());
            }

            // Lightmap slices — the pixel_format/width/height are a single global header
            // that precedes all slices, not repeated per slice. Each slice is 256 bytes.
            let lightmap_count = ri32(&mut c)? as usize;
            let _pixel_format =
                ri32(&mut c).with_context(|| "lightmap global header: pixel_format")?;
            let _lm_width = ri32(&mut c).with_context(|| "lightmap global header: width")?;
            let _lm_height = ri32(&mut c).with_context(|| "lightmap global header: height")?;
            let mut lightmap_slices = Vec::with_capacity(lightmap_count);
            for i in 0..lightmap_count {
                let mut shadowmap = [0u8; 64];
                c.read_exact(&mut shadowmap)
                    .with_context(|| format!("lightmap slice {i}/{lightmap_count}: shadowmap"))?;
                let mut lightmap = [0u8; 192];
                c.read_exact(&mut lightmap)
                    .with_context(|| format!("lightmap slice {i}/{lightmap_count}: lightmap"))?;
                lightmap_slices.push(GndLightmapSlice {
                    shadowmap,
                    lightmap,
                });
            }

            // Surfaces (56 bytes each)
            let surface_count = ri32(&mut c).with_context(|| {
                format!("surface count (after {} lightmap slices)", lightmap_count)
            })? as usize;
            let mut surfaces = Vec::with_capacity(surface_count);
            for _ in 0..surface_count {
                let mut u = [0f32; 4];
                for v in u.iter_mut() {
                    *v = rf32(&mut c)?;
                }
                let mut v = [0f32; 4];
                for vv in v.iter_mut() {
                    *vv = rf32(&mut c)?;
                }
                // offset 32: texture_id (i16), offset 34: lightmap_id (i16) — no padding
                let texture_id = ri16(&mut c)?;
                let lightmap_id = ri16(&mut c)?;
                // offset 36: single BGRA tile color (4 bytes)
                let mut color = [0u8; 4];
                c.read_exact(&mut color)?;
                surfaces.push(GndSurface {
                    u,
                    v,
                    texture_id,
                    lightmap_id,
                    color,
                });
            }

            // Cubes (28 bytes each)
            let cube_count = (width as usize).checked_mul(height as usize).unwrap_or(0);
            let mut cubes = Vec::with_capacity(cube_count);
            for _ in 0..cube_count {
                let mut heights = [0f32; 4];
                for h in heights.iter_mut() {
                    *h = rf32(&mut c)?;
                }
                let top_surface_id = ri32(&mut c)?;
                let north_surface_id = ri32(&mut c)?;
                let east_surface_id = ri32(&mut c)?;
                cubes.push(GndCube {
                    heights,
                    top_surface_id,
                    north_surface_id,
                    east_surface_id,
                });
            }

            // v1.8+: water plane
            let water = if major > 1 || (major == 1 && minor >= 8) {
                let level = rf32(&mut c)?;
                let water_type = ri32(&mut c)?;
                let wave_height = rf32(&mut c)?;
                let wave_speed = rf32(&mut c)?;
                let wave_pitch = rf32(&mut c)?;
                let texture_cycling_interval = ri32(&mut c)?;

                // v1.9+: multiple water planes (u × v grid); read and discard the extra data
                if minor >= 9 {
                    let planes_u = ri32(&mut c)?;
                    let planes_v = ri32(&mut c)?;
                    let extra = (planes_u * planes_v) as usize;
                    c.seek(SeekFrom::Current((extra * 4) as i64))?; // per-plane level floats
                }

                Some(GndWaterPlane {
                    level,
                    water_type,
                    wave_height,
                    wave_speed,
                    wave_pitch,
                    texture_cycling_interval,
                })
            } else {
                None
            };

            Ok(GndFile {
                version,
                width,
                height,
                scale,
                texture_paths,
                lightmap_slices,
                surfaces,
                cubes,
                water,
            })
        })()
            .with_context(|| format!("GND v{major}.{minor} (implementation covers v1.7-v1.9)"))
    }

    pub fn cube(&self, col: i32, row: i32) -> Option<&GndCube> {
        if col >= 0 && row >= 0 && col < self.width && row < self.height {
            self.cubes.get((row * self.width + col) as usize)
        } else {
            None
        }
    }
}

/// Translate texture paths in a GND file's binary content.
///
/// Each texture path slot is decoded from CP949, translated, and re-encoded as UTF-8.
/// Paths that already start with `texture/` are kept as-is; paths without that prefix
/// (some GND files omit it) have `texture/` prepended so all output paths are
/// asset-root-relative. `texture_path_len` in the output header is updated if any
/// translated path requires a larger slot.
pub fn rewrite_textures(
    data: &[u8],
    known: &HashMap<String, String>,
    misses: &mut BTreeSet<String>,
) -> Result<Vec<u8>> {
    if data.len() < 26 {
        bail!("GND file too short ({} bytes)", data.len());
    }
    if &data[0..4] != b"GRGN" {
        bail!("not a GND file: bad magic {:?}", &data[0..4]);
    }

    // GND header layout:
    //   0..4   GRGN magic
    //   4      major (u8)
    //   5      minor (u8)
    //   6..18  width (i32) + height (i32) + scale (f32)
    //  18..22  texture_count (i32 LE)
    //  22..26  texture_path_len (i32 LE)
    //  26..    texture_count × texture_path_len byte slots (null-terminated, zero-padded)
    //          followed by lightmaps, surfaces, cubes, water — all unchanged

    let texture_count = i32::from_le_bytes(data[18..22].try_into()?) as usize;
    let old_slot_len = i32::from_le_bytes(data[22..26].try_into()?) as usize;

    if texture_count == 0 {
        return Ok(data.to_vec());
    }

    let texture_block_end = 26 + texture_count * old_slot_len;
    if data.len() < texture_block_end {
        bail!(
            "GND file truncated: need {} bytes for texture block, have {}",
            texture_block_end,
            data.len()
        );
    }

    let mut translated: Vec<Vec<u8>> = Vec::with_capacity(texture_count);
    for i in 0..texture_count {
        let slot = &data[26 + i * old_slot_len..26 + (i + 1) * old_slot_len];
        let translated_path = translate_cp949_path(slot, known, misses);
        let prefixed = ensure_texture_prefix(translated_path);
        translated.push(prefixed.into_bytes());
    }

    let max_len = translated.iter().map(|p| p.len()).max().unwrap_or(0);
    let new_slot_len = old_slot_len.max(max_len + 1);

    let mut out = Vec::with_capacity(
        18 + 4 + 4 + texture_count * new_slot_len + (data.len() - texture_block_end),
    );
    out.extend_from_slice(&data[0..18]);
    out.extend_from_slice(&(texture_count as i32).to_le_bytes());
    out.extend_from_slice(&(new_slot_len as i32).to_le_bytes());
    for path_bytes in &translated {
        let mut slot = vec![0u8; new_slot_len];
        let copy_len = path_bytes.len().min(new_slot_len - 1);
        slot[..copy_len].copy_from_slice(&path_bytes[..copy_len]);
        out.extend_from_slice(&slot);
    }
    out.extend_from_slice(&data[texture_block_end..]);

    Ok(out)
}

fn ensure_texture_prefix(path: String) -> String {
    let path = crate::translate::strip_data_prefix(&path);
    // let prefixed = if path.starts_with("texture/") || path.starts_with("texture\\") {
    //     path.to_string()
    // } else {
    let prefixed = format!("tex/{path}");
    // };
    crate::translate::bmp_ext_to_png(&prefixed)
}
