# bevy_ro_maps

Map terrain rendering, model instancing, navigation, lighting, audio, and visual effects
for Ragnarok Online maps. Consumes GND (ground mesh), RSW (resource world), and GAT
(altitude/terrain) files.

## Plugin Setup

```rust
use bevy_ro_maps::{RoMapsPlugin, prelude::*};

app.add_plugins(RoMapsPlugin {
    assets_root: "/path/to/target/assets".into(),
});
```

### RoMapsPlugin Fields

| Field         | Type     | Description                                                  |
|---------------|----------|--------------------------------------------------------------|
| `assets_root` | `PathBuf`| Filesystem path to the Bevy asset root (same as `AssetPlugin::file_path`). Used to locate `misc/mp3nametable.json` for BGM lookup. |

The plugin registers:
- `RoMapAsset` custom asset type and its `RoMapLoader`.
- Terrain lightmap shader and `TerrainMaterial`.
- Systems for mesh spawning and water animation.
- An observer for `MapLightingReady` that configures sun direction, color, and ambient light.

## Loading a Map

Spawn an entity with `RoMapRoot`, `Transform`, and `Visibility`. The plugin spawns terrain
meshes, models, lights, audio, and effects as children once the asset loads.

```rust
use bevy_ro_maps::prelude::*;

commands.spawn((
    RoMapRoot {
        asset: asset_server.load("maps/prontera/prontera.gnd"),
        spawned: false,
    },
    Transform::default(),
    Visibility::default(),
));
```

### RoMapRoot Fields

| Field    | Type                | Description                                             |
|----------|---------------------|---------------------------------------------------------|
| `asset`  | `Handle<RoMapAsset>`| Handle to the `.gnd` file. The loader co-loads `.gat` and `.rsw` automatically. |
| `spawned`| `bool`              | Set to `true` by the plugin after spawning. Prevents re-spawning. Must be initialized to `false`. |

### What Happens on Load

When the `RoMapAsset` finishes loading, the plugin:

1. Fires `MapLightingReady` with the RSW lighting parameters.
2. Triggers BGM playback if `BgmTable` has an entry for the map name.
3. Inserts a `NavMesh` component on the root entity for terrain queries.
4. Builds terrain mesh geometry grouped by texture, spawned as children with `RoMapMesh`.
5. Builds water plane geometry (if present) with animated texture cycling.
6. Spawns `PendingModel` entities for every RSW model instance (handled by `bevy_ro_models`).
7. Spawns `PointLight` entities for every RSW light source, marked with `RoMapLight`.
8. Spawns `RoEffectEmitter` entities for every RSW effect emitter (handled by `bevy_ro_vfx`).
9. Spawns spatial audio for every RSW audio emitter via `PlaySound`.

## Navigation and Terrain Queries

The `NavMesh` component is the primary interface for terrain passability, height queries,
and pathfinding. It is inserted on the `RoMapRoot` entity after the map loads.

### Accessing the NavMesh

```rust
use bevy_ro_maps::prelude::*;

fn my_system(
    nav_query: Query<&NavMesh, With<RoMapRoot>>,
) {
    let nav = nav_query.single().unwrap();
    // use nav...
}
```

### Checking Terrain Passability

Query whether a world-space position is walkable:

```rust
use ro_files::TerrainType;

let terrain = nav.terrain_at_world(world_x, world_z);
match terrain {
    TerrainType::Walkable => { /* can walk here */ }
    TerrainType::Blocked  => { /* impassable */ }
    TerrainType::Snipable => { /* ranged attacks pass through, walking blocked */ }
    _ => {}
}
```

You can also query by tile coordinates directly:

```rust
let terrain = nav.terrain_at_nav(col, row);
```

### Getting Terrain Height

Get the interpolated Y height at a world-space XZ position. Uses bilinear interpolation
across the four corner altitudes of the GAT tile. Returns Bevy Y-up coordinates.

```rust
let y = nav.height_at(world_x, world_z);
// Place an entity on the ground:
transform.translation = Vec3::new(world_x, y, world_z);
```

### Snapping to Tile Center

Convert a world position to its tile center (snapped) plus terrain type:

```rust
let (snapped_pos, terrain_type) = nav.to_map_tile(world_x, world_z);
// snapped_pos is Vec3(tile_center_x, height_at_center, tile_center_z)
```

### Pathfinding (A*)

Find a walkable path between two world-space XZ positions. Returns a `VecDeque<Vec2>` of
waypoints (XZ) in world coordinates, excluding the start position. Returns `None` if no
path exists.

```rust
let start = (entity_x, entity_z);
let target = (click_x, click_z);

if let Some(path) = nav.path(start, target) {
    // path is VecDeque<Vec2> of XZ waypoints
    for waypoint in &path {
        let y = nav.height_at(waypoint.x, waypoint.y);
        // Move entity toward Vec3(waypoint.x, y, waypoint.y)
    }
}
```

### Complete Movement Example

```rust
use bevy_ro_maps::prelude::*;
use ro_files::TerrainType;
use std::collections::VecDeque;

#[derive(Component)]
struct MoveTarget {
    path: VecDeque<Vec2>,
    speed: f32,
}

fn click_to_move(
    nav_query: Query<&NavMesh, With<RoMapRoot>>,
    click_pos: Vec2, // world XZ from raycast
    mut commands: Commands,
    entity: Entity,
    transform: &Transform,
) {
    let nav = nav_query.single().unwrap();

    // Check if destination is walkable
    if nav.terrain_at_world(click_pos.x, click_pos.y) == TerrainType::Blocked {
        return;
    }

    // Find path
    let start = (transform.translation.x, transform.translation.z);
    let target = (click_pos.x, click_pos.y);
    if let Some(path) = nav.path(start, target) {
        commands.entity(entity).insert(MoveTarget {
            path,
            speed: 50.0,
        });
    }
}

fn follow_path(
    mut movers: Query<(&mut Transform, &mut MoveTarget)>,
    nav_query: Query<&NavMesh>,
    time: Res<Time>,
) {
    let nav = nav_query.single().unwrap();
    for (mut tf, mut target) in &mut movers {
        let Some(next) = target.path.front() else {
            continue;
        };
        let goal = Vec3::new(next.x, nav.height_at(next.x, next.y), next.y);
        let dir = (goal - tf.translation).normalize_or_zero();
        tf.translation += dir * target.speed * time.delta_secs();

        if tf.translation.xz().distance(next.clone()) < 1.0 {
            target.path.pop_front();
        }
    }
}
```

### NavMesh Fields

| Field            | Type           | Description                                     |
|------------------|----------------|-------------------------------------------------|
| `terrain_width`  | `f32`          | Map width in world units (`gnd_width * scale`). |
| `terrain_height` | `f32`          | Map height in world units (`gnd_height * scale`).|
| `nav_width`      | `i32`          | Number of GAT tile columns.                     |
| `nav_height`     | `i32`          | Number of GAT tile rows.                        |
| `tiles`          | `Vec<GatTile>` | Row-major tile data; index = `row * nav_width + col`. |

### NavMesh Methods

| Method                                           | Returns             | Description                                |
|--------------------------------------------------|---------------------|--------------------------------------------|
| `tile(col, row)`                                 | `Option<&GatTile>`  | Raw GAT tile at grid coordinates.          |
| `terrain_at_nav(col, row)`                       | `TerrainType`       | Terrain type at grid coordinates.          |
| `terrain_at_world(world_x, world_z)`             | `TerrainType`       | Terrain type at world XZ position.         |
| `height_at(world_x, world_z)`                    | `f32`               | Interpolated Y height at world XZ.         |
| `to_map_tile(world_x, world_z)`                  | `(Vec3, TerrainType)` | Tile-center position + terrain type.     |
| `path(start, target)`                            | `Option<VecDeque<Vec2>>` | A* path between world XZ positions.  |

## Lighting

When a map loads, `MapLightingReady(RswLighting)` is triggered as an event. The built-in
observer configures the first `DirectionalLight` in the scene:

- **Direction**: computed from RSW longitude/latitude via `ro_files::coord::lighting_direction`.
- **Color**: RSW diffuse color.
- **Illuminance**: based on RSW `shadowmap_alpha`.
- **Ambient**: set from RSW ambient color with brightness 800.

To customize lighting, add your own observer for `MapLightingReady`:

```rust
app.add_observer(|trigger: On<MapLightingReady>| {
    let lighting = &trigger.event().0;
    // lighting.diffuse, lighting.ambient, lighting.longitude, lighting.latitude
    // lighting.shadowmap_alpha
});
```

You must also spawn a `DirectionalLight` entity for the built-in observer to configure:

```rust
commands.spawn((
    DirectionalLight { illuminance: 0.0, shadows_enabled: true, ..default() },
    Transform::default(),
    CascadeShadowConfigBuilder {
        maximum_distance: 1000.0,
        first_cascade_far_bound: 300.0,
        ..default()
    }.build(),
));
```

## Background Music

The `BgmTable` resource maps map names to BGM asset paths. It is loaded from
`{assets_root}/misc/mp3nametable.json` at startup. When a map finishes loading, the plugin
looks up its name in the table and triggers `PlaySound { looping: true }`.

```rust
// Query the BGM table
fn check_bgm(bgm: Res<BgmTable>) {
    if let Some(path) = bgm.0.get("prontera") {
        println!("Prontera BGM: {}", path); // "bgm/08.mp3"
    }
}
```

BGM `.mp3` files must be manually placed in `assets/bgm/`; they are not part of the GRF
extraction pipeline.

## Marker Components

| Component          | Placed on                        | Description                            |
|--------------------|----------------------------------|----------------------------------------|
| `RoMapRoot`        | Map root entity                  | Triggers map loading and spawning.     |
| `RoMapMesh`        | Terrain mesh children            | Identifies terrain geometry entities.  |
| `RoMapLight`       | Point light children             | Identifies RSW-placed point lights.    |
| `NavMesh`          | Map root entity (after load)     | Terrain passability and height queries.|

## Coordinate System

The map root entity gets a centering transform `(-cx, 0, -(scale + cz))` applied after
spawning. All children inherit this offset.

- **Children of the root** (models, lights, effects): use `rsw_local_pos()` for their
  local transform. The centering transform is already applied by the parent.
- **Root-level entities** (audio emitters, standalone markers): must use final world
  coordinates directly, e.g. `Vec3::new(rsw_x, -rsw_y, -rsw_z)`.

All coordinate conversions from RO space to Bevy space are handled by `ro_files::coord`.

## Public API Summary

| Item                    | Kind        | Description                                       |
|-------------------------|-------------|---------------------------------------------------|
| `RoMapsPlugin`          | Plugin      | Main plugin (terrain, models, lights, audio, VFX).|
| `RoMapRoot`             | Component   | Map loading trigger.                              |
| `RoMapAsset`            | Asset       | Parsed GND + GAT + RSW data.                     |
| `RoMapLoader`           | AssetLoader | Bevy asset loader for `.gnd` files.               |
| `NavMesh`               | Component   | Terrain passability, heights, pathfinding.        |
| `MapLightingReady`      | Event       | Fired when map lighting data is available.        |
| `RoMapMesh`             | Component   | Marker on terrain mesh entities.                  |
| `RoMapLight`            | Component   | Marker on RSW-placed point light entities.        |
| `BgmTable`              | Resource    | Map name to BGM path lookup.                      |
| `TerrainMaterial`       | Material    | Terrain lightmap material (alias for extended material). |
| `TerrainLightmapExtension` | Struct   | Material extension carrying lightmap data.        |
| `RoEffectEmitter`       | Component   | Re-exported from `bevy_ro_vfx` for convenience.  |
| `TerrainType`           | Enum        | Re-exported from `ro_files` in prelude.           |
