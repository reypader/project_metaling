# Plan: Expand RoVfxPlugin with multi-effect-type support

## Context

- `RoEffectEmitter` currently lives in `bevy_ro_maps`, causing `bevy_ro_vfx` to depend on `bevy_ro_maps`.
  Moving it to `bevy_ro_vfx` reverses this to `bevy_ro_maps` depends on `bevy_ro_vfx`.
- `emit_speed` and `params` are unused and get dropped.
- `EffectTable.json` uses JS-style syntax (comments, single quotes, numeric keys, JS function
  literals). Use the `json5` crate plus pre-processing to strip `func: function ...` blocks before
  parsing.
- Priority: CYLINDER, wav-only, STR, 2D/3D, FUNC (deferred).

## Effect type universe (from EffectTable.json)

All observed `type` values: `CYLINDER`, `STR`, `SPR`, `FUNC`, `2D`, `3D`, and absent (AudioOnly).
No other types exist in the file.

- **AudioOnly** (no type field): play wav only, no visual.
- **CYLINDER**: textured truncated cone mesh with optional rotation and frame animation.
- **STR**: binary keyframe animation file (`ro_files`). Deferred.
- **SPR** (EffectTable variant): sprite defined inside EffectTable, distinct from `effect_sprites.json`.
  Deferred, log warning.
- **2D**: short-lived animated camera-space billboard quad (particle-like, one-shot). Deferred.
- **3D**: short-lived animated world-space billboard quad with position/size tween. Deferred.
- **FUNC**: procedural JS callback. Deferred, log warning.

Note: `2D` and `3D` are one-shot transient effects (combat/skill hit sparks) not relevant to
RSW map emitters. Deferred after STR, before FUNC.

## Step 1: Move RoEffectEmitter to bevy_ro_vfx, reverse dependency

**`bevy_ro_vfx/src/lib.rs`**
- Define `#[derive(Component)] pub struct RoEffectEmitter { pub effect_id: u32 }` here.
- Remove `use bevy_ro_maps::RoEffectEmitter`.

**`bevy_ro_vfx/Cargo.toml`**
- Remove `bevy_ro_maps` dependency.
- Add `bevy_ro_sounds` dependency (for wav dispatch).
- Add `json5 = "0.4"` dependency.
- Add `regex = "1"` dependency (for pre-processing FUNC blocks).
- Add `tga` to Bevy features (for CYLINDER and STR texture loading).

**`bevy_ro_maps/Cargo.toml`**
- Add `bevy_ro_vfx` dependency.

**`bevy_ro_maps/src/render.rs`**
- Change import to `use bevy_ro_vfx::RoEffectEmitter`.
- Remove `emit_speed` and `params` fields from the spawn call.

**`bevy_ro_maps/src/lib.rs`**
- Change re-export: `pub use bevy_ro_vfx::RoEffectEmitter;` (keeps downstream users working).

## Step 2: Parse EffectTable.json into an EffectTable resource

**New file `bevy_ro_vfx/src/effect_table.rs`**

```rust
pub struct CylinderDef {
    pub texture_name: String,
    pub height: f32,
    pub bottom_size: f32,
    pub top_size: f32,
    pub color: [f32; 4],       // [r, g, b, alpha_max]
    pub blend_additive: bool,  // blendMode == 2
    pub animation_frames: u32, // `animation` field; 1 = no animation
    pub rotate: bool,
    pub duration_ms: f32,
}

pub enum EffectKind {
    AudioOnly,
    Cylinder(CylinderDef),
    Str { file: String },
    Spr { file: String },
    Plane2D,
    Plane3D,
    Func,
}

pub struct EffectEntry {
    pub kind: EffectKind,
    pub wav: Option<String>,
}

#[derive(Resource, Default)]
pub struct EffectTable(pub HashMap<u32, Vec<EffectEntry>>);
```

`load_effect_table(path) -> EffectTable`:
1. Read file to string.
2. Pre-process: use `regex` to strip `func:\s*function\s*\w*\s*\([^)]*\)\s*\{[^}]*(?:\{[^}]*\}[^}]*)?\}` blocks.
3. Parse with `json5::from_str`.
4. For each effect ID array, iterate sub-entries:
   - Classify `kind` by `type` field (absent = `AudioOnly`, `"CYLINDER"` = `Cylinder`, etc.).
   - Extract `wav` if present.
   - `"FUNC"` entries: store as `EffectKind::Func` (not silently ignored).

**`RoVfxPlugin`** gains a `config_path: PathBuf` field (path to `config/EffectTable.json`).
Inserts `EffectTable` resource at startup.

## Step 3: Unified dispatch system with pattern-matched delegation

Replace `attach_effect_sprites` with `dispatch_effects` handling `Added<RoEffectEmitter>`.

```rust
fn dispatch_effects(
    mut commands: Commands,
    effect_table: Res<EffectTable>,
    effect_sprite_map: Res<EffectSpriteMap>,
    new_effects: Query<(Entity, &RoEffectEmitter, &GlobalTransform), Added<RoEffectEmitter>>,
    // ...mesh/material/server resources...
) {
    for (entity, emitter, gtf) in &new_effects {
        let id = emitter.effect_id;
        // EffectTable path
        if let Some(entries) = effect_table.0.get(&id) {
            for entry in entries {
                if let Some(wav) = &entry.wav {
                    spawn_wav_effect(&mut commands, wav, gtf);
                }
                match &entry.kind {
                    EffectKind::AudioOnly => {}
                    EffectKind::Cylinder(def) => spawn_cylinder_effect(&mut commands, ..., entity, def),
                    EffectKind::Str { file } => warn!("[RoVfx] STR effect {id} not yet implemented: {file}"),
                    EffectKind::Spr { file } => warn!("[RoVfx] EffectTable SPR effect {id} not yet implemented: {file}"),
                    EffectKind::Plane2D | EffectKind::Plane3D => warn!("[RoVfx] 2D/3D effect {id} not yet implemented"),
                    EffectKind::Func => warn!("[RoVfx] FUNC effect {id} not yet implemented"),
                }
            }
        }
        // effect_sprites.json path (existing SPR)
        if let Some(stem) = effect_sprite_map.0.get(&id) {
            spawn_spr_effect(&mut commands, ..., entity, stem);
        }
    }
}
```

Each `spawn_*` is a standalone function taking only what it needs.

## Step 4: CYLINDER rendering

**New file `bevy_ro_vfx/src/cylinder.rs`**

`spawn_cylinder_effect(commands, meshes, materials, server, parent_entity, def)`:
- Build a `Mesh` programmatically: truncated cone with 24 radial segments.
  - Bottom ring at `y = 0`, top ring at `y = def.height * CYLINDER_SCALE`.
  - Bottom radius = `def.bottom_size * CYLINDER_SCALE`, top = `def.top_size * CYLINDER_SCALE`.
  - UVs: `u` wraps 0..1 around circumference, `v` goes 0..1 bottom to top.
- `StandardMaterial`:
  - `base_color_texture` from `tex/effect/<texture_name>.png`.
  - `base_color: Color::srgba(r, g, b, alpha_max)`.
  - `alpha_mode: AlphaMode::Blend` (normal) or `AlphaMode::Add` (when `blend_additive`).
  - `double_sided: true, cull_mode: None`.
- Attach as a child of the emitter entity.
- Attach `CylinderAnimator { animation_frames, elapsed: 0.0, duration_ms, rotate }`.

`CylinderAnimator` system:
- If `animation_frames > 1`: cycle `uv_transform` offset on the material (one frame = `1.0 / animation_frames` of v-height).
- If `rotate`: rotate entity around Y each frame.

## Step 5: Wav-only effects

Fully covered by Step 3 dispatch. No additional work.

`spawn_wav_effect(commands, wav_path, gtf)` emits:
```rust
commands.trigger(PlaySound {
    path: wav_path.clone(),
    looping: true,
    location: Some(Transform::from(gtf)),
    volume: None,
    range: None,
});
```

## Step 6: STR effects (deferred)

Reference implementation: `RoEffectRenderer.cs` in `reference/RagnarokRebuild`.

Key findings from the reference:
- Each STR layer = one quad (4 vertices). Layers are independent game objects updated every frame.
- Keyframes have two types: **type 0 = base frame** (absolute values), **type 1 = delta frame**
  (values are deltas, NOT absolute targets). Interpolation formula:
  `result = from.value + to_delta.value * (current_frame - from.frame)`
  This applies to position, XY vertices, UVs, angle, and color.
- Position offset: center = 320 units, divide by 35 to get world units. Matches the existing
  `EFFECT_SPRITE_SCALE = 1.0 / 35.0` constant already in the crate.
- Anitype controls texture frame advance:
  - 0: fixed frame
  - 1: `floor(from.aniframe + to.aniframe * delta)`
  - 2: `min(from.aniframe + to.delay * delta, textureCount - 1)` (clamped)
  - 3: `(from.aniframe + to.delay * delta) % textureCount` (looping forward)
  - 4: `(from.aniframe - to.delay * delta) % textureCount` (looping reverse)
- Blend: both src and dst = `One` (additive). No depth write.
- STR effects are one-shot by default (stop when `frame > maxKey`). Looping requires
  re-spawning at the caller's discretion.
- All textures per layer are packed into an atlas; keyframe UVs are atlas-rect-remapped.
- **Exact binary parse sequence** (from `RagnarokEffectLoader.cs`):
  - Header: `STRM` magic (4 bytes), `version: i32` (only 148 supported), `fps: i32`,
    `maxkey: i32`, `layer_count: i32`, skip 16 bytes (display/group/type fields).
  - Per layer: `texture_count: i32`, then `texture_count` × 128-byte Korean strings (EUC-KR),
    `keyframe_count: i32`, then keyframes.
  - Per keyframe (in order): `frame: i32`, `type: i32`, `position: Vec2`, `uv_raw: [f32; 8]`
    (skip — UVs are hardcoded to atlas corners), `xy_raw: [f32; 8]`, `aniframe: f32`,
    `anitype: i32`, `delay: f32`, `angle_raw: f32`, `color: [f32; 4]`, `src_alpha: i32`,
    `dst_alpha: i32`, `mt_preset: i32` (store, ignore).
  - XY vertex remap from `xy_raw[0..8]` (Y is negated, indices 2/3 are swapped in second group):
    - `xy[0] = (xy_raw[0], -xy_raw[4])`
    - `xy[1] = (xy_raw[1], -xy_raw[5])`
    - `xy[2] = (xy_raw[3], -xy_raw[7])` ← index 3, not 2
    - `xy[3] = (xy_raw[2], -xy_raw[6])` ← index 2, not 3
  - Angle conversion: `raw * 360.0 / 1024.0` degrees (1024 raw units = full rotation).
  - UVs are always hardcoded to `[(0,0),(1,0),(0,1),(1,1)]`; the raw UV array is skipped.
- Texture deduplication: deduplicate filenames across all layers into a global list.
  `StrLayer.textures: Vec<usize>` stores indices into that global list. Bevy side loads each
  unique texture once and references by index.
- Binary STR layers store texture FILENAMES (128 chars, EUC-KR). Parse them as `Vec<String>` in
  `ro_files`. The Unity `Textures: List<int>` is a post-load atlas index — our Rust parser
  stores filenames and the Bevy side resolves them to asset handles.

Implementation steps when ready:
1. `ro_files/src/str.rs`: parse STR binary into `StrFile { fps, maxkey, layers: Vec<StrLayer> }`
   where each layer has `textures: Vec<String>` and `keyframes: Vec<StrKeyframe>`.
2. `bevy_ro_vfx/src/str.rs`: `spawn_str_effect` + `StrAnimator` component + `animate_str` system.
3. Texture loading: pack layer textures into an atlas or load individually from `tex/effect/`.
   Note: STR texture files are TGA format; add `tga` feature to Bevy (see TGA note below).

## Step 7: 2D/3D plane effects (deferred, after STR)

One-shot animated billboard quads. Require a timed animator that lerps position, scale, and
alpha from start to end values over `duration`. Both types share core machinery; the only
difference is coordinate space (camera-facing vs world). Implement together as a single effort.

## Step 8: FUNC effects (deferred, after 2D/3D)

Log warning in dispatcher. No implementation.

## TGA texture note

CYLINDER effect textures (`tex/effect/<name>.tga`) and STR layer textures are TGA files.
Bevy supports TGA via the `tga` feature on the `image` crate, exposed as a Bevy feature flag.
Add `tga` to the Bevy feature list in `bevy_ro_vfx/Cargo.toml` (and `game/Cargo.toml` if needed).
The `TGALoader.cs` reference confirms: 32-bit TGAs have an alpha channel; 24-bit do not.
Byte order in the file is BGR(A); standard TGA loaders handle this automatically.

## File changelist

| File | Change |
|---|---|
| `bevy_ro_vfx/src/lib.rs` | Define `RoEffectEmitter`; add `config_path` to plugin; unified dispatch |
| `bevy_ro_vfx/src/effect_table.rs` | New: EffectTable parsing |
| `bevy_ro_vfx/src/cylinder.rs` | New: cone mesh generation + CylinderAnimator |
| `bevy_ro_vfx/Cargo.toml` | Remove `bevy_ro_maps`; add `json5`, `bevy_ro_sounds`, `regex` |
| `bevy_ro_maps/src/render.rs` | Import `RoEffectEmitter` from `bevy_ro_vfx`; drop unused spawn fields |
| `bevy_ro_maps/src/lib.rs` | Re-export `RoEffectEmitter` from `bevy_ro_vfx` |
| `bevy_ro_maps/Cargo.toml` | Add `bevy_ro_vfx` dep |
| `crates/game/src/main.rs` | Pass `config_path` to `RoVfxPlugin` |
