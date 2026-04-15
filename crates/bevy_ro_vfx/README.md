# bevy_ro_vfx

Visual effects plugin for Ragnarok Online. Renders STR animations, sprite-based effects,
cylinder effects, and 2D/3D plane effects driven by `EffectTable.json` definitions.

## Plugin Setup

```rust
use bevy_ro_vfx::RoVfxPlugin;

app.add_plugins(RoVfxPlugin {
    assets_root: "/path/to/target/assets".into(),
    config_path: "/path/to/config/EffectTable.json".into(),
    effect_sprite_scale_divisor: 35.0, // default
});
```

### RoVfxPlugin Fields

| Field                        | Type       | Default | Description                                                 |
|------------------------------|------------|---------|-------------------------------------------------------------|
| `assets_root`                | `PathBuf`  | (required) | Filesystem path to Bevy asset root (same as `AssetPlugin::file_path`). |
| `config_path`                | `PathBuf`  | (required) | Path to `config/EffectTable.json`.                        |
| `effect_sprite_scale_divisor`| `f32`      | `35.0`  | Divisor for normalizing ACT pixel scales to world units.    |

## Spawning Effects

Place an `RoEffectEmitter` component on an entity with a `Transform` and `GlobalTransform`.
The plugin reacts to `Added<RoEffectEmitter>` and spawns the appropriate visuals and sounds
as children.

```rust
use bevy_ro_vfx::{RoEffectEmitter, EffectRepeat};

// Play a named effect once at a position
commands.spawn((
    RoEffectEmitter {
        effect_id: "ef_firebolt".to_string(),
        repeat: EffectRepeat::Times(1),
    },
    Transform::from_xyz(100.0, 5.0, -50.0),
    GlobalTransform::default(),
    Visibility::default(),
));

// Permanent looping effect (e.g. map emitters)
commands.spawn((
    RoEffectEmitter {
        effect_id: "121".to_string(),
        repeat: EffectRepeat::Infinite,
    },
    Transform::from_xyz(200.0, 0.0, -100.0),
    GlobalTransform::default(),
    Visibility::default(),
));
```

### RoEffectEmitter Fields

| Field       | Type           | Description                                      |
|-------------|----------------|--------------------------------------------------|
| `effect_id` | `String`       | Effect name or numeric ID matching an `EffectTable.json` key. |
| `repeat`    | `EffectRepeat` | How many times to play before auto-despawn.      |

### EffectRepeat

| Variant          | Description                                                      |
|------------------|------------------------------------------------------------------|
| `Infinite`       | Loop forever; the emitter entity is never automatically destroyed. |
| `Times(u32)`     | Play exactly `n` times, then despawn the emitter and all children. |

## Effect Types

The plugin dispatches effects based on entries in `EffectTable.json`. Each entry can contain
one or more of these types:

| Type        | Description                                                        |
|-------------|--------------------------------------------------------------------|
| `AudioOnly` | Fires a `PlaySound` event only (no visuals).                      |
| `Cylinder`  | Truncated cone mesh with texture cycling and optional Y-rotation.  |
| `STR`       | STR animation file (multi-layer billboard keyframes).              |
| `SPR`       | Sprite billboard via `RoComposite` (uses the sprite plugin).      |
| `Plane2D`   | Camera-facing billboard with size/position/color animation.        |
| `Plane3D`   | World-space panel with size/rotation animation.                    |
| `Func`      | Procedural effect (not yet implemented, logs a warning).           |

Effects with `%d` in texture or file paths are resolved with a random integer from the
entry's `rand: [min, max]` range.

## Effect Lifecycle

1. **Spawn**: attach `RoEffectEmitter` + `Transform` + `GlobalTransform` + `Visibility`.
2. **Dispatch**: the plugin detects `Added<RoEffectEmitter>`, looks up the effect ID in
   `EffectTable`, and spawns visual children (meshes, billboards) and/or fires `PlaySound`.
3. **Animation**: per-type systems (`animate_str`, `animate_cylinders`,
   `animate_plane_effects`, `update_effect_composites`) drive the animation each frame.
4. **Cleanup**: when `EffectRepeat::Times(n)` reaches zero loops, the emitter entity and all
   children are despawned. `Infinite` effects persist until manually despawned.

## Marker Components

| Component         | Placed on                         | Description                               |
|-------------------|-----------------------------------|-------------------------------------------|
| `EffectBillboard` | SPR effect billboard children     | Carries scale factor and remaining count. |
| `StrBillboard`    | STR layer quads and 2D planes     | Marks billboards for camera-facing orient.|

### EffectBillboard Fields

| Field          | Type           | Description                                          |
|----------------|----------------|------------------------------------------------------|
| `scale_factor` | `f32`          | Pixel-to-world scale (from `effect_sprite_scale_divisor`). |
| `remaining`    | `Option<u32>`  | Remaining play count; `None` for infinite.           |
| `prev_frame`   | `u16`          | Previous frame index for loop boundary detection.    |

## Integration with bevy_ro_maps

You typically do not spawn effect emitters manually for map effects. The map crate reads
RSW files and spawns `RoEffectEmitter` entities at each effect emitter position defined in
the map. The VFX plugin then takes over automatically.

For gameplay effects (skills, hits, etc.), spawn emitters directly as shown above.

## Public API Summary

| Item                | Kind       | Description                                          |
|---------------------|------------|------------------------------------------------------|
| `RoVfxPlugin`       | Plugin     | Registers effect dispatch and animation systems.     |
| `RoEffectEmitter`   | Component  | Trigger effect spawning at an entity's position.     |
| `EffectRepeat`      | Enum       | Controls effect loop count.                          |
| `EffectBillboard`   | Component  | Marker on SPR effect billboard children.             |
| `StrBillboard`      | Component  | Marker on camera-facing effect billboards.           |
| `EffectTable`       | Resource   | Parsed effect definitions (read-only at runtime).    |
