use anyhow::{bail, Result};
use std::io::Cursor;

use crate::translate::decode_cp949_path;
use crate::util::{check_magic, rf32, ri32, skip};

#[derive(Debug, Clone)]
pub struct StrKeyframe {
    pub frame: i32,
    pub kf_type: i32,
    pub position: [f32; 2],
    pub xy: [[f32; 2]; 4],
    pub aniframe: f32,
    pub anitype: i32,
    pub delay: f32,
    /// Degrees, already converted from the raw 0-1024 unit.
    pub angle: f32,
    /// (r, g, b, a)
    pub color: [f32; 4],
    pub src_alpha: i32,
    pub dst_alpha: i32,
    pub mt_preset: i32,
}

#[derive(Debug, Clone)]
pub struct StrLayer {
    /// Raw texture filenames from the binary (e.g. `"lens_b.png"` after importer rewrite).
    pub textures: Vec<String>,
    pub keyframes: Vec<StrKeyframe>,
}

#[derive(Debug, Clone)]
pub struct StrFile {
    pub fps: i32,
    pub maxkey: i32,
    pub layers: Vec<StrLayer>,
}

impl StrFile {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut c = Cursor::new(data);
        check_magic(&mut c, b"STRM")?;

        let version = ri32(&mut c)?;
        if version != 148 {
            bail!("STR version {version} not supported (expected 148)");
        }

        let fps = ri32(&mut c)?;
        let maxkey = ri32(&mut c)?;
        let layer_count = ri32(&mut c)? as usize;
        skip(&mut c, 16)?;

        let mut layers = Vec::with_capacity(layer_count);
        for _ in 0..layer_count {
            layers.push(parse_layer(&mut c)?);
        }

        Ok(StrFile { fps, maxkey, layers })
    }
}

fn parse_layer(c: &mut Cursor<&[u8]>) -> Result<StrLayer> {
    let tex_count = ri32(c)? as usize;
    let mut textures = Vec::with_capacity(tex_count);
    for _ in 0..tex_count {
        let mut buf = [0u8; 128];
        use std::io::Read;
        c.read_exact(&mut buf)?;
        textures.push(decode_cp949_path(&buf));
    }

    let kf_count = ri32(c)? as usize;
    let mut keyframes = Vec::with_capacity(kf_count);
    for _ in 0..kf_count {
        keyframes.push(parse_keyframe(c)?);
    }

    Ok(StrLayer { textures, keyframes })
}

fn parse_keyframe(c: &mut Cursor<&[u8]>) -> Result<StrKeyframe> {
    let frame = ri32(c)?;
    let kf_type = ri32(c)?;
    let position = [rf32(c)?, rf32(c)?];

    // uv_raw: 8 floats — discard. UVs are hardcoded to atlas corners.
    for _ in 0..8 {
        rf32(c)?;
    }

    // xy_raw: 8 floats — read then remap.
    let mut xy_raw = [0f32; 8];
    for v in &mut xy_raw {
        *v = rf32(c)?;
    }
    let xy = [
        [xy_raw[0], -xy_raw[4]],
        [xy_raw[1], -xy_raw[5]],
        [xy_raw[3], -xy_raw[7]], // index 3, not 2
        [xy_raw[2], -xy_raw[6]], // index 2, not 3
    ];

    let aniframe = rf32(c)?;
    let anitype = ri32(c)?;
    let delay = rf32(c)?;
    let angle = rf32(c)? * (360.0 / 1024.0);
    let color = [rf32(c)?, rf32(c)?, rf32(c)?, rf32(c)?];
    let src_alpha = ri32(c)?;
    let dst_alpha = ri32(c)?;
    let mt_preset = ri32(c)?;

    Ok(StrKeyframe {
        frame,
        kf_type,
        position,
        xy,
        aniframe,
        anitype,
        delay,
        angle,
        color,
        src_alpha,
        dst_alpha,
        mt_preset,
    })
}

/// Rewrites texture name slots inside an STR binary: replaces `.bmp` extensions with `.png`.
///
/// Called by the importer when copying `.str` files to the asset tree. This mirrors
/// how `rsm::rewrite_textures` patches texture paths in RSM files.
pub fn rewrite_textures(data: &[u8]) -> Result<Vec<u8>> {
    if data.len() < 36 {
        bail!("STR file too short ({} bytes)", data.len());
    }
    if &data[0..4] != b"STRM" {
        bail!("not an STR file: bad magic {:?}", &data[0..4]);
    }

    let version = i32::from_le_bytes(data[4..8].try_into()?);
    if version != 148 {
        return Ok(data.to_vec());
    }

    let layer_count = i32::from_le_bytes(data[16..20].try_into()?) as usize;

    let mut out = data.to_vec();
    let mut offset = 36usize;

    for _ in 0..layer_count {
        if offset + 4 > out.len() {
            bail!("STR rewrite: unexpected end of data at offset {offset}");
        }
        let tex_count = i32::from_le_bytes(out[offset..offset + 4].try_into()?) as usize;
        offset += 4;

        for _ in 0..tex_count {
            if offset + 128 > out.len() {
                bail!("STR rewrite: unexpected end of data at texture slot {offset}");
            }
            let name_end = offset + 128;
            let name_len = out[offset..name_end]
                .iter()
                .position(|&b| b == 0)
                .unwrap_or(128);

            if name_len >= 4
                && out[offset + name_len - 4..offset + name_len]
                    .eq_ignore_ascii_case(b".bmp")
            {
                out[offset + name_len - 4..offset + name_len].copy_from_slice(b".png");
            }

            offset += 128;
        }

        if offset + 4 > out.len() {
            bail!("STR rewrite: unexpected end of data at keyframe count {offset}");
        }
        let kf_count = i32::from_le_bytes(out[offset..offset + 4].try_into()?) as usize;
        offset += 4 + kf_count * 124;
    }

    Ok(out)
}
