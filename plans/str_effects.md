# Plan: STR Effect Rendering

## Overview

Six files touched in total:

1. `crates/ro_files/src/str.rs` — new binary parser + importer rewrite helper
2. `crates/ro_files/src/lib.rs` — re-export additions
3. `crates/bevy_ro_vfx/Cargo.toml` — add `ro_files` dependency
4. `crates/bevy_ro_vfx/src/str_effect.rs` — spawner + animator
5. `crates/bevy_ro_vfx/src/lib.rs` — VfxConfig resource, module, system wiring
6. `crates/asset_importer/src/batch.rs` — rewrite STR texture names (.bmp → .png) during import

---

## File 1: `crates/ro_files/src/str.rs` (new)

### Imports

```rust
use anyhow::{bail, Result};
use std::collections::BTreeSet;
use std::io::Cursor;
use crate::util::{check_magic, ri32, rf32, skip};
use crate::translate::decode_cp949_path;
```

### Public types

```rust
#[derive(Debug, Clone)]
pub struct StrKeyframe {
    pub frame: i32,
    pub kf_type: i32,        // 0 = base, 1 = delta
    pub position: [f32; 2],  // (x, y) in RO coords (center = 320)
    pub xy: [[f32; 2]; 4],   // remapped quad vertices
    pub aniframe: f32,
    pub anitype: i32,
    pub delay: f32,
    pub angle: f32,          // degrees, already converted from raw
    pub color: [f32; 4],     // (r, g, b, a)
    pub src_alpha: i32,
    pub dst_alpha: i32,
    pub mt_preset: i32,
}

#[derive(Debug, Clone)]
pub struct StrLayer {
    pub textures: Vec<String>,  // raw filenames, e.g. "lens_b.png" (after importer rewrite)
    pub keyframes: Vec<StrKeyframe>,
}

#[derive(Debug, Clone)]
pub struct StrFile {
    pub fps: i32,
    pub maxkey: i32,
    pub layers: Vec<StrLayer>,
}
```

### `StrFile::parse(data: &[u8]) -> Result<Self>`

Parse sequence:

1. `check_magic(&mut c, b"STRM")?`
2. `version = ri32(&mut c)?` — bail if `!= 148`
3. `fps = ri32`, `maxkey = ri32`, `layer_count = ri32`
4. `skip(&mut c, 16)?` — 16 reserved bytes
5. Loop `layer_count` times → `parse_layer`

`parse_layer`:
1. `tex_count = ri32`
2. Loop tex_count: read 128 raw bytes, `decode_cp949_path(&buf[..128])` → filename string
3. `kf_count = ri32`
4. Loop kf_count → `parse_keyframe`

`parse_keyframe` (124 bytes total):

| Field | Call | Notes |
|---|---|---|
| `frame` | `ri32` | |
| `kf_type` | `ri32` | |
| `position[0..2]` | `rf32` × 2 | |
| uv_raw (skip) | `rf32` × 8 | Discard — UVs always hardcoded to [(0,0),(1,0),(0,1),(1,1)] |
| xy_raw | `rf32` × 8 | Must remap (see below) |
| `aniframe` | `rf32` | |
| `anitype` | `ri32` | |
| `delay` | `rf32` | |
| angle_raw | `rf32` | `angle = raw * 360.0 / 1024.0` |
| `color[0..4]` | `rf32` × 4 | r, g, b, a |
| `src_alpha` | `ri32` | |
| `dst_alpha` | `ri32` | |
| `mt_preset` | `ri32` | |

XY remap:
```
xy[0] = [xy_raw[0], -xy_raw[4]]
xy[1] = [xy_raw[1], -xy_raw[5]]
xy[2] = [xy_raw[3], -xy_raw[7]]   // index 3, not 2
xy[3] = [xy_raw[2], -xy_raw[6]]   // index 2, not 3
```

### `rewrite_textures(data: &[u8]) -> Result<Vec<u8>>`

Called by the importer to replace `.bmp` with `.png` in texture name slots.

Header layout for offset calculation:
- `[0..4]`: STRM magic
- `[4..8]`: version (i32)
- `[8..12]`: fps (i32)
- `[12..16]`: maxkey (i32)
- `[16..20]`: layer_count (i32)
- `[20..36]`: 16 reserved bytes
- `[36..]`: layers

Algorithm:
1. Validate magic `STRM` and length >= 36.
2. Read `version` from `[4..8]`. If `!= 148`, return unchanged `data.to_vec()`.
3. Read `layer_count` from `[16..20]`.
4. Clone to `out: Vec<u8>`. Set `offset = 36`.
5. For each layer:
   - Read `tex_count` as i32 from `out[offset..offset+4]`. `offset += 4`.
   - For each texture slot (128 bytes):
     - Find null terminator: `name_len = out[offset..offset+128].iter().position(|&b| b==0).unwrap_or(128)`.
     - If `name_len >= 4` and `out[offset+name_len-4..offset+name_len].eq_ignore_ascii_case(b".bmp")`:
       - Overwrite those 4 bytes with `b".png"`.
     - `offset += 128`.
   - Read `kf_count` as i32 from `out[offset..offset+4]`. `offset += 4`.
   - `offset += kf_count as usize * 124`. (Skip keyframes — not modified.)
6. Return `Ok(out)`.

---

## File 2: `crates/ro_files/src/lib.rs`

Add at the end of the module/use block:

```rust
pub mod str;
pub use str::{StrFile, StrKeyframe, StrLayer};
```

---

## File 3: `crates/bevy_ro_vfx/Cargo.toml`

Add under `[dependencies]`:

```toml
ro_files = { path = "../ro_files" }
```

---

## File 4: `crates/bevy_ro_vfx/src/str_effect.rs` (new)

### Imports

```rust
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use ro_files::{StrKeyframe, StrLayer};
use std::path::Path;
```

### `texture_asset_path(raw_name: &str) -> String`

Convert a raw texture filename from the STR binary to a Bevy asset path:
- After the importer runs, names are already `.png`. But support both for robustness.
- Rule: if extension is `.bmp`, replace with `.png`. Otherwise keep.

```rust
fn texture_asset_path(raw_name: &str) -> String {
    let stem = raw_name.rfind('.').map(|i| &raw_name[..i]).unwrap_or(raw_name);
    let ext = if raw_name.to_ascii_lowercase().ends_with(".bmp") { "png" } else {
        raw_name.rfind('.').map(|i| &raw_name[i+1..]).unwrap_or("tga")
    };
    format!("tex/effect/{}.{}", stem, ext)
}
```

### `StrLayerAnim`

```rust
pub struct StrLayerAnim {
    pub entity: Entity,
    pub mesh_handle: Handle<Mesh>,
    pub mat_handle: Handle<StandardMaterial>,
    pub tex_handles: Vec<Handle<Image>>,
    pub keyframes: Vec<StrKeyframe>,
    pub tex_count: usize,
}
```

### `StrEffectAnimator` component

```rust
#[derive(Component)]
pub struct StrEffectAnimator {
    pub fps: f32,
    pub maxkey: i32,
    pub elapsed: f32,
    pub looping: bool,
    pub layers: Vec<StrLayerAnim>,
}
```

### `build_quad_mesh() -> Mesh`

A placeholder deformable quad. Must use `RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD` so vertices can be updated on the CPU each frame.

```rust
fn build_quad_mesh() -> Mesh {
    let positions: Vec<[f32; 3]> = vec![[0.0, 0.0, 0.0]; 4];
    let normals: Vec<[f32; 3]> = vec![[0.0, 0.0, 1.0]; 4];
    let uvs: Vec<[f32; 2]> = vec![[0.0,0.0],[1.0,0.0],[0.0,1.0],[1.0,1.0]];
    let indices = vec![0u32, 1, 2, 1, 3, 2];
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}
```

### `spawn_str_effect`

```rust
pub fn spawn_str_effect(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    server: &AssetServer,
    assets_root: &Path,
    parent_entity: Entity,
    str_file_stem: &str,
)
```

Steps:
1. `let path = assets_root.join(format!("tex/effect/{}.str", str_file_stem))`.
2. `let bytes = std::fs::read(&path)` — warn and return on failure.
3. `let str_file = StrFile::parse(&bytes)` — warn and return on failure.
4. Build `layers: Vec<StrLayerAnim>` by iterating `str_file.layers`:
   - Skip layers where `layer.textures.is_empty() && layer.keyframes.is_empty()`.
   - Load tex handles: `server.load(texture_asset_path(name))` for each texture name.
   - Create mesh: `meshes.add(build_quad_mesh())`.
   - Create material: `materials.add(StandardMaterial { unlit: true, double_sided: true, cull_mode: None, alpha_mode: AlphaMode::Add, ..default() })`.
   - Spawn child entity under `parent_entity`: `(Mesh3d(mesh_handle.clone()), MeshMaterial3d(mat_handle.clone()), Transform::default(), Visibility::Hidden)`.
   - Collect into `StrLayerAnim { entity, mesh_handle, mat_handle, tex_handles, keyframes: layer.keyframes.clone(), tex_count: layer.textures.len() }`.
5. Attach `StrEffectAnimator { fps: str_file.fps as f32, maxkey: str_file.maxkey, elapsed: 0.0, looping: true, layers }` to `parent_entity`.
6. Insert `Visibility::Inherited` on `parent_entity`.

### `rotate2d(x: f32, y: f32, angle_rad: f32) -> (f32, f32)` (private)

```rust
fn rotate2d(x: f32, y: f32, angle_rad: f32) -> (f32, f32) {
    let (sin, cos) = angle_rad.sin_cos();
    (x * cos - y * sin, x * sin + y * cos)
}
```

### `animate_str` system

```rust
pub fn animate_str(
    mut animators: Query<(Entity, &mut StrEffectAnimator)>,
    mut layer_queries: Query<(&mut MeshMaterial3d<StandardMaterial>, &mut Transform, &mut Visibility)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
    time: Res<Time>,
)
```

For each animator:
1. `animator.elapsed += time.delta_secs()`.
2. `current_frame = (animator.elapsed * animator.fps).floor() as i32`.
3. If `current_frame > animator.maxkey`:
   - If `looping`: `animator.elapsed = 0.0; current_frame = 0;`.
   - Else: `commands.entity(entity).despawn(); continue;`.
4. For each layer: call `update_layer(layer, current_frame, &mut layer_queries, &mut meshes, &mut materials)`.

### `update_layer` (private)

```rust
fn update_layer(
    layer: &StrLayerAnim,
    current_frame: i32,
    layer_queries: &mut Query<(&mut MeshMaterial3d<StandardMaterial>, &mut Transform, &mut Visibility)>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
)
```

**Step 1: Keyframe search**

```rust
let mut start_anim: i32 = -1;
let mut next_anim: i32 = -1;
let mut last_frame: i32 = 0;
let mut last_source: i32 = 0;

for (i, kf) in layer.keyframes.iter().enumerate() {
    if kf.frame < current_frame {
        if kf.kf_type == 0 { start_anim = i as i32; }
        if kf.kf_type == 1 { next_anim = i as i32; }
    }
    last_frame = last_frame.max(kf.frame);
    if kf.kf_type == 0 { last_source = last_source.max(kf.frame); }
}
```

**Step 2: Visibility check**

```rust
if start_anim < 0 || (next_anim < 0 && last_frame < current_frame) {
    // hide layer
    if let Ok((_, _, mut vis)) = layer_queries.get_mut(layer.entity) {
        *vis = Visibility::Hidden;
    }
    return;
}
```

**Step 3: Determine values (pos, angle, color, xy, tex_index)**

```rust
let from = &layer.keyframes[start_anim as usize];

let (pos, angle, color, xy, tex_index) = if next_anim < 0
    || next_anim != start_anim + 1
    || layer.keyframes[next_anim as usize].frame != from.frame
{
    // Static branch. Extra check from reference:
    if next_anim >= 0 && last_source <= from.frame {
        // hide
        return;
    }
    let tex_idx = (from.aniframe as usize).min(layer.tex_count.saturating_sub(1));
    (from.position, from.angle, from.color, from.xy, tex_idx)
} else {
    // Delta branch
    let to = &layer.keyframes[next_anim as usize];
    let delta = (current_frame - from.frame) as f32;

    let pos = [
        from.position[0] + to.position[0] * delta,
        from.position[1] + to.position[1] * delta,
    ];
    let angle = from.angle + to.angle * delta;
    let color = [
        from.color[0] + to.color[0] * delta,
        from.color[1] + to.color[1] * delta,
        from.color[2] + to.color[2] * delta,
        from.color[3] + to.color[3] * delta,
    ];
    let xy = std::array::from_fn(|i| [
        from.xy[i][0] + to.xy[i][0] * delta,
        from.xy[i][1] + to.xy[i][1] * delta,
    ]);

    let n = layer.tex_count as f32;
    let tex_idx = match to.anitype {
        0 => from.aniframe as usize,
        1 => (from.aniframe + to.aniframe * delta).floor() as usize,
        2 => (from.aniframe + to.delay * delta).min(n - 1.0).floor() as usize,
        3 => ((from.aniframe + to.delay * delta).rem_euclid(n)).floor() as usize,
        4 => ((from.aniframe - to.delay * delta).rem_euclid(n)).floor() as usize,
        _ => 0,
    };
    (pos, angle, color, xy, tex_idx.min(layer.tex_count.saturating_sub(1)))
};
```

**Step 4: Update mesh vertices**

```rust
if let Some(mesh) = meshes.get_mut(&layer.mesh_handle) {
    let angle_rad = -angle.to_radians();
    let positions: Vec<[f32; 3]> = xy.iter().map(|&[x, y]| {
        let (rx, ry) = rotate2d(x, y, angle_rad);
        [rx / 35.0, ry / 35.0, 0.0]
    }).collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
}
```

**Step 5: Update transform, material, visibility**

```rust
if let Ok((mut mat_handle, mut transform, mut vis)) = layer_queries.get_mut(layer.entity) {
    transform.translation = Vec3::new(
        (pos[0] - 320.0) / 35.0,
        -(pos[1] - 320.0) / 35.0,
        0.0,
    );
    *vis = Visibility::Inherited;

    if !layer.tex_handles.is_empty() {
        if let Some(mat) = materials.get_mut(&mat_handle.0) {
            mat.base_color_texture = Some(layer.tex_handles[tex_index].clone_weak());
            mat.base_color = Color::srgba(color[0], color[1], color[2], color[3]);
        }
    }
}
```

---

## File 5: `crates/bevy_ro_vfx/src/lib.rs`

### Add `VfxConfig` resource

```rust
#[derive(Resource)]
struct VfxConfig {
    assets_root: std::path::PathBuf,
}
```

Insert in `RoVfxPlugin::build`:
```rust
app.insert_resource(VfxConfig { assets_root: self.assets_root.clone() });
```

### Add module

```rust
mod str_effect;
```

### Register system

```rust
app.add_systems(Update, str_effect::animate_str);
```

### Add parameter and dispatch in `dispatch_effects`

Add `config: Res<VfxConfig>` to the system parameters.

Replace the `EffectKind::Str { file }` stub:
```rust
EffectKind::Str { file } => {
    let stem = file.trim_start_matches("effect/");
    str_effect::spawn_str_effect(
        &mut commands,
        &mut meshes,
        &mut std_mats,
        &server,
        &config.assets_root,
        entity,
        stem,
    );
}
```

---

## File 6: `crates/asset_importer/src/batch.rs`

### Add import to `batch.rs`

```rust
use ro_files::str as ro_str;
```

### Update `copy_dir_recursive`

Add a `.str` branch before the `else` fallback:

```rust
} else if name_str.to_ascii_lowercase().ends_with(".str") {
    let data = std::fs::read(&src_path)
        .with_context(|| format!("reading {}", src_path.display()))?;
    let rewritten = ro_str::rewrite_textures(&data)
        .with_context(|| format!("rewriting STR {}", src_path.display()))?;
    std::fs::write(dst.join(&name), rewritten)?;
```

### Update `copy_dir_translated`

Same `.str` branch added in the same position.

---

## Design Notes

**Why importer + runtime fallback both handle .bmp→.png:** Users who already have extracted assets won't have the STR files updated until they re-run the importer. The runtime `texture_asset_path` function keeps the `.bmp`→`.png` fallback for backwards compatibility. Once a full re-import is done, the importer-rewritten files will already have `.png` and the fallback is a no-op.

**Looping:** Map effect emitters are permanent, so `looping: true` is correct. The reference destroys the game object after `maxkey`, which is a one-shot model suited to combat effects. For persistent world effects (torch, portal), we loop instead of despawn.

**UV hardcoding:** The reference discards `uv_raw` and hardcodes `[(0,0),(1,0),(0,1),(1,1)]` because each layer holds individual textures directly (not atlas-packed). The static UV array in `build_quad_mesh` is correct and never changes.

**Mesh CPU update requirement:** `RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD` is required because vertex positions are updated every frame. Without `MAIN_WORLD`, the mesh is moved to the GPU after first upload and `meshes.get_mut()` returns `None`.

**`rem_euclid` for anitype 4:** Rust's `%` operator can return negative values for negative dividends. `.rem_euclid(n)` guarantees a non-negative result needed for a valid texture index.
