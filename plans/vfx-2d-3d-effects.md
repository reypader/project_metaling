# Plan: 2D and 3D Plane Effects

## Context

Steps 1-6 of `vfx-multi-effect-types.md` are complete. This plan covers step 7: implementing
`Plane2D` and `Plane3D` effect types from `EffectTable.json`.

## Field survey

**2D** (7 total entries in config): camera-facing billboard quads, typically combat/skill hit sparks.
Common fields: `file`, `duration`, `alphaMax`, `angle`, `fadeOut`.
Two sub-patterns:
- Moving lens: `posxStart/End`, `poszStart/End`, `sizeStartX/Y`, `sizeEndX/Y`
- Rotating panel (same fields as 3D): `sizeX`, `sizeStartY/EndY`, `toAngle`, `posz`

**3D** (many entries): world-space effects. Very varied, but most common fields:
- `file`, `duration`, `alphaMax`, `posz`, `blendMode`, `red/green/blue`, `fadeIn/Out`, `zIndex`
- `sizeStart/sizeEnd` (uniform billboard scale, most common)
- `sizeX + sizeStartY/EndY + angle/toAngle` (rotating slash panel)
- `size` (fixed uniform scale, no animation)
- Complex: `duplicate`, `nbOfRotation`, `rotatePosX/Y` — deferred, log warning

## Coordinate semantics

Sizes (`sizeStartX/Y`, `sizeEndX/Y`, `sizeStart`, `sizeEnd`, `sizeX`): raw pixel units; divide by 35
for world units (same divisor as STR effects and `EFFECT_SPRITE_SCALE`).

Positions (`posxStart/End`, `poszStart/End`, `posx`, `posz`): world units, relative to emitter.
For 2D: posX = world X, posz = world Y (height). For 3D: posz = fixed Y height offset.

Angle: degrees. For 2D effects, it's the quad's Z-rotation (tilt). For 3D effects, it's the
Y-axis rotation and `toAngle` is the target (interpolated over duration).

Color tint: red/green/blue fields (default 1.0). Alpha from alphaMax (default 1.0).

BlendMode: 2 = additive (`AlphaMode::Add`), otherwise `AlphaMode::Blend`. Same as CYLINDER.

## Data structures (`effect_table.rs`)

Replace `EffectKind::Plane2D` and `EffectKind::Plane3D` unit variants with:

```rust
pub struct PlaneDef {
    pub file: String,        // texture path, "effect/" prefix stripped
    pub duration_ms: f32,
    pub alpha_max: f32,
    pub fade_in: bool,
    pub fade_out: bool,
    pub blend_additive: bool,
    pub color: [f32; 3],     // [r, g, b] tint; multiply with alphaMax for final color
    pub size_start: Vec2,    // raw pixel units
    pub size_end: Vec2,
    pub pos_start: Vec2,     // (posxStart, poszStart) or (posx, posz) — world units
    pub pos_end: Vec2,       // (posxEnd, poszEnd) — same as pos_start if no movement
    pub angle: f32,          // initial angle degrees
    pub to_angle: f32,       // final angle degrees (equals `angle` if no rotation)
    pub posz: f32,           // Y height offset (for 3D fixed-position effects)
}

pub enum EffectKind {
    ...
    Plane2D(PlaneDef),
    Plane3D(PlaneDef),
    ...
}
```

Parsing logic for `"2D"` and `"3D"` entries:
- `size_start`/`size_end`: resolve from these in priority order:
  1. `sizeStartX/Y` + `sizeEndX/Y` (explicit per-axis)
  2. `sizeX` + `sizeStartY/EndY` (fixed-width slash)
  3. `sizeStart`/`sizeEnd` splat to Vec2 (uniform scale)
  4. `size` splat for both start and end
  5. fallback `Vec2::splat(10.0)`
- `pos_start` = (posxStart or posx or 0, poszStart or 0)
- `pos_end` = (posxEnd or posx or 0, poszEnd or poszStart or 0)
- `posz` = field `posz` (0.0 if absent)
- `angle` = field `angle` (0.0 if absent)
- `to_angle` = field `toAngle` (same as `angle` if absent)
- `color` = [red, green, blue] (1.0 defaults)
- Skip entries with `duplicate` field (log warning, not yet supported)

## Animation component (`plane_effect.rs`)

```rust
#[derive(Component)]
pub struct PlaneEffectAnimator {
    pub elapsed: f32,
    pub duration: f32,
    pub alpha_max: f32,
    pub fade_in: bool,
    pub fade_out: bool,
    pub pos_start: Vec3,    // emitter-relative (X=posxStart, Y=posz+poszStart, Z=0)
    pub pos_end: Vec3,
    pub size_start: Vec2,
    pub size_end: Vec2,
    pub angle_start: f32,
    pub angle_end: f32,
    pub mat_handle: Handle<StandardMaterial>,
    pub camera_facing: bool,
}
```

`spawn_plane_effect(commands, meshes, materials, server, parent_entity, def, camera_facing, repeat)`:
- Builds a unit quad mesh with `RenderAssetUsages::MAIN_WORLD | RENDER_WORLD` (reused, not mutated).
- Creates `StandardMaterial` with `unlit: true`, `double_sided: true`, `cull_mode: None`.
  AlphaMode: Add when `blend_additive`, else Blend.
- Spawns a child entity with: `PlaneEffectAnimator`, `Mesh3d`, `MeshMaterial3d`, `Transform`.
- If `camera_facing`: attaches `StrBillboard` so `orient_str_billboards` rotates it each frame.
- For 3D (world-space): no `StrBillboard`; initial rotation is `Quat::from_rotation_y(angle_rad)`.

`animate_plane_effects` system:
```
t = (elapsed / duration).clamp(0, 1)
pos = lerp(pos_start, pos_end, t)
size = lerp(size_start, size_end, t) / 35.0   (convert to world units)
angle = lerp(angle_start, angle_end, t)
alpha = match (fade_in, fade_out) {
    (true, _) if t < 0.5 => alpha_max * (t * 2.0)
    (_, true) => alpha_max * (1.0 - t)
    _ => alpha_max
}
Update transform: translation = pos, scale = (size.x, size.y, 1.0)
Update material: base_color alpha = alpha
For 3D: update rotation = Quat::from_rotation_y(angle.to_radians())
Despawn parent when elapsed >= duration (for finite effects; infinite = never despawn)
```

For infinite effects (`EffectRepeat::Infinite`): loop by resetting `elapsed` to 0.

## Integration (`lib.rs`)

1. Add `mod plane_effect;` and use `spawn_plane_effect`, `animate_plane_effects`.
2. In `dispatch_effects`: replace Plane2D/Plane3D warnings with:
   ```rust
   EffectKind::Plane2D(def) => spawn_plane_effect(..., def, true, repeat),
   EffectKind::Plane3D(def) => spawn_plane_effect(..., def, false, repeat),
   ```
3. In `RoVfxPlugin::build`: add `animate_plane_effects` to Update schedule.

## File changelist

| File | Change |
|---|---|
| `bevy_ro_vfx/src/effect_table.rs` | Add `PlaneDef`; change `Plane2D`/`Plane3D` to tuple variants; parse fields |
| `bevy_ro_vfx/src/plane_effect.rs` | New: `PlaneEffectAnimator`, `spawn_plane_effect`, `animate_plane_effects` |
| `bevy_ro_vfx/src/lib.rs` | Add `mod plane_effect`; add system; wire dispatch |
