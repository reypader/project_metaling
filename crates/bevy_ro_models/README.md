# bevy_ro_models

3D model rendering plugin for Ragnarok Online RSM (Resource Model) files. Handles static
and animated models used by maps and environments.

## Plugin Setup

```rust
use bevy_ro_models::RoModelsPlugin;

app.add_plugins(RoModelsPlugin);
```

`RoModelsPlugin` is a unit struct with no configuration. It registers:

- The `RsmAsset` custom asset type and its loader.
- Systems for loading, materializing, and animating RSM models.

A lower-level `RsmPlugin` is also available if you only need the asset type without the
rendering pipeline.

## How Models Work

Models follow a three-stage lifecycle: **pending**, **loading**, **materialized**.

### Spawning a Model

Attach the `PendingModel` component to an entity with a `Transform`. The plugin picks it up
automatically.

```rust
use bevy_ro_models::PendingModel;

commands.spawn((
    PendingModel {
        asset_path: "model/prontera/building01.rsm".to_string(),
        anim_speed: 1.0,
    },
    Transform::from_xyz(100.0, 0.0, -50.0),
    Visibility::default(),
));
```

### Lifecycle Stages

1. **Pending**: Entity has `PendingModel`. The plugin starts loading the RSM asset and
   transitions the entity to an internal `LoadingModel` state.
2. **Loading**: The RSM asset is being loaded from disk. The `PendingModel` component is
   removed and replaced internally.
3. **Materialized**: Once the RSM asset is ready, the plugin builds mesh geometry as child
   entities. Each child gets a `RoModelMesh` marker, and the root gets `RoModelInstance`.

### PendingModel Fields

| Field        | Type     | Description                                             |
|--------------|----------|---------------------------------------------------------|
| `asset_path` | `String` | Asset-relative path to the `.rsm` file.                 |
| `anim_speed` | `f32`    | Playback speed multiplier for keyframe animations.      |

### Animated Models

RSM files can contain per-node rotation keyframes (RSM1 format). If keyframes are present,
the plugin attaches an internal `RsmAnimator` that drives rotation interpolation each frame.
Set `anim_speed` to control playback speed (1.0 = normal, 0.0 = paused).

## Marker Components

Use these to query model entities:

| Component        | Placed on                          | Description                               |
|------------------|------------------------------------|-------------------------------------------|
| `RoModelInstance`| Root entity of a materialized model| Groups all mesh children under one model. |
| `RoModelMesh`    | Each geometry mesh child entity    | Identifies individual model mesh parts.   |

### Example: Fading All Model Meshes

```rust
fn fade_models(
    instances: Query<&Children, With<RoModelInstance>>,
    mut materials: Query<&MeshMaterial3d<StandardMaterial>, With<RoModelMesh>>,
) {
    for children in &instances {
        for child in children.iter() {
            if let Ok(mat_handle) = materials.get(*child) {
                // Modify the material for fading...
            }
        }
    }
}
```

## Integration with bevy_ro_maps

You do not typically spawn models manually. The map crate (`bevy_ro_maps`) reads RSW
(Resource World) files and spawns `PendingModel` entities for every model instance defined
in the map, with the correct world-space transform already applied. The models plugin then
materializes them automatically.

## Public API Summary

| Item              | Kind       | Description                                        |
|-------------------|------------|----------------------------------------------------|
| `RoModelsPlugin`  | Plugin     | Full rendering pipeline (loading + materialization).|
| `RsmPlugin`       | Plugin     | Asset-only (no rendering systems).                 |
| `RsmAsset`        | Asset      | Parsed RSM model data.                             |
| `RsmLoader`       | AssetLoader| Bevy asset loader for `.rsm` files.                |
| `PendingModel`    | Component  | Request model geometry spawning.                   |
| `RoModelInstance` | Component  | Marker on materialized model root entities.        |
| `RoModelMesh`     | Component  | Marker on individual model mesh entities.          |
