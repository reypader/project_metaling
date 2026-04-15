# bevy_ro_sprites

Sprite billboard rendering plugin for Ragnarok Online actors (players, NPCs, monsters).
Composites multiple sprite layers (body, head, headgear, weapon, shadow) onto a single
billboard quad using a GPU shader, with direction-aware animation driven by ACT/SPR/IMF files.

## Plugin Setup

```rust
use bevy_ro_sprites::prelude::*;

// Default configuration
app.add_plugins(RoSpritePlugin::default());

// Custom configuration
app.add_plugins(RoSpritePlugin {
    config: RoSpriteConfig {
        shadow_sprite_path: "sprite/shadow/shadow.spr".to_string(),
        billboard_mode: BillboardMode::Spherical,
        spherical_max_tilt: 30.0,
    },
});
```

### RoSpriteConfig

| Field                | Type            | Default                       | Description                                |
|----------------------|-----------------|-------------------------------|--------------------------------------------|
| `shadow_sprite_path` | `String`        | `"sprite/shadow/shadow.spr"`  | Asset path for the shadow sprite.          |
| `billboard_mode`     | `BillboardMode` | `Spherical`                   | How billboards orient toward the camera.   |
| `spherical_max_tilt` | `f32`           | `30.0`                        | Max pitch (degrees) in Spherical mode.     |

### BillboardMode

| Variant          | Description                                                               |
|------------------|---------------------------------------------------------------------------|
| `Spherical`      | Each billboard faces the camera directly. Matches the original RO client. |
| `CameraParallel` | All billboards are parallel to the camera plane (no edge divergence).     |

The config is inserted as `Res<RoSpriteConfig>` and can be modified at runtime.

## Spawning Actor Sprites

Spawn an entity with `ActorSprite`, `ActorState`, `ActorDirection`, and a `Transform`.
The plugin automatically creates a billboard child with the appropriate composite layers.

```rust
use bevy_ro_sprites::prelude::*;

commands.spawn((
    ActorSprite {
        body: "sprite/human_female_knight/body.spr".into(),
        head: Some("sprite/human_female_head/head/11.spr".into()),
        weapon: Some("sprite/human_female_knight/weapon/spear/weapon.spr".into()),
        weapon_slash: Some("sprite/human_female_knight/weapon/spear/slash.spr".into()),
    },
    ActorState { action: Action::Idle },
    ActorDirection(Vec2::NEG_Y),  // facing south
    Transform::from_xyz(0.0, 0.0, 100.0)
        .with_scale(Vec3::splat(0.15)),
));
```

### ActorSprite Fields

| Field          | Type             | Description                                            |
|----------------|------------------|--------------------------------------------------------|
| `body`         | `String`         | Asset path to the body `.spr` file. Required.          |
| `head`         | `Option<String>` | Asset path to the head `.spr` file.                    |
| `weapon`       | `Option<String>` | Asset path to the weapon `.spr` file (slot 0).         |
| `weapon_slash` | `Option<String>` | Asset path to the weapon slash overlay `.spr` (slot 1).|

All paths are asset-relative. The plugin handles loading and compositing.

### Changing Action and Direction

Modify the `ActorState` and `ActorDirection` components; the plugin updates the billboard
tag automatically each frame.

```rust
fn control_actor(
    keys: Res<ButtonInput<KeyCode>>,
    mut actors: Query<(&mut ActorState, &mut ActorDirection)>,
) {
    for (mut state, mut dir) in &mut actors {
        if keys.pressed(KeyCode::KeyW) {
            state.action = Action::Walk;
            dir.0 = Vec2::Y; // face north
        } else {
            state.action = Action::Idle;
        }
    }
}
```

## Action Enum

All actor animation actions, unified across player and monster sprites.

```rust
pub enum Action {
    Idle, Walk, Sit, PickUp, Alert, Skill,
    Flinch, Frozen, Dead,
    Attack1, Attack2, Spell,
}
```

### Player Layout (13 groups, 104 actions = 13 x 8 directions)

All 12 Action variants are available.

### Monster Layout (5 groups, 40 actions = 5 x 8 directions)

Only `Idle`, `Walk`, `Attack1`, `Flinch`, `Dead` exist. Player-only actions fall back:

| Player Action        | Monster Fallback |
|----------------------|------------------|
| `Sit`, `Alert`, `Frozen` | `Idle`       |
| `PickUp`, `Skill`, `Spell`, `Attack2` | `Attack1` |

Fallback is handled automatically when the sprite's action count indicates a monster layout.

### Key Action Methods

```rust
// Get the tag name for composite lookup
let tag = action.tag_name(); // e.g. "idle", "attack1", "walk"

// Check layout type from total action count
Action::is_monster_layout(total_actions) // true if monster layout

// Get base index for a given layout
action.base_index(total_actions) // picks monster or player base

// Reverse-lookup from flat ACT index
Action::from_flat_index(42, 104) // -> Some((Action::Skill, 2))
```

## ActorDirection

`ActorDirection(Vec2)` is a continuous world-space XZ facing vector. The plugin discretizes
it into one of 8 directions internally using `direction_index()`.

```rust
ActorDirection(Vec2::NEG_Y)  // south
ActorDirection(Vec2::X)      // east
ActorDirection(-Vec3::Z.xz()) // south (from 3D forward)
```

The direction is relative to the camera. As the camera orbits, the billboard direction
suffix updates automatically.

## Composite System (Advanced)

For manual composite control (effects, custom billboards), bypass `ActorSprite` and build
the components directly.

### RoComposite

The core component driving multi-layer billboard rendering.

```rust
use bevy_ro_sprites::prelude::*;

// Manual composite setup (ActorSprite does this automatically)
commands.spawn((
    RoComposite {
        layers: vec![
            CompositeLayerDef {
                atlas: server.load("sprite/human_male_novice/body.spr"),
                role: SpriteRole::Body,
            },
            CompositeLayerDef {
                atlas: server.load("sprite/human_male_head/head/10.spr"),
                role: SpriteRole::Head,
            },
        ],
        tag: Some("idle_s".to_string()),
        playing: true,
        ..Default::default()
    },
    Mesh3d(meshes.add(Rectangle::new(1.0, 1.0))),
    MeshMaterial3d(mats.add(RoCompositeMaterial::default())),
    Transform::default(),
    ActorBillboard { feet_lift: 10.0 },
));
```

### RoComposite Fields

| Field           | Type                    | Default        | Description                             |
|-----------------|-------------------------|----------------|-----------------------------------------|
| `layers`        | `Vec<CompositeLayerDef>`| `[]`           | Sprite layers to composite.             |
| `tag`           | `Option<String>`        | `None`         | Current animation tag (e.g. `"idle_s"`).|
| `playing`       | `bool`                  | `true`         | Whether animation advances.             |
| `speed`         | `f32`                   | `1.0`          | Playback speed multiplier.              |
| `current_frame` | `u16`                   | `0`            | Current frame index.                    |
| `elapsed`       | `Duration`              | `ZERO`         | Time elapsed in current frame.          |

### SpriteRole

Determines z-ordering within the composite.

| Role                    | Description                                           |
|-------------------------|-------------------------------------------------------|
| `Shadow`                | Shadow sprite (always behind, z = -1).                |
| `Body`                  | Body sprite.                                          |
| `Head`                  | Head sprite (z varies by direction/IMF data).         |
| `Headgear { slot: u8 }` | Headgear (slots 0-3 for upper/mid/lower/extra).     |
| `Weapon { slot: u8 }`   | Weapon (slot 0 = main, slot 1 = slash overlay).      |
| `Shield`                | Shield sprite.                                        |
| `Garment`               | Garment sprite (always on top, z = 35).               |

### Tag Format

Tags follow the pattern `"{action}_{direction}"`:

```
"idle_s"      -> Idle, facing south
"walk_ne"     -> Walk, facing north-east  
"attack1_e"   -> Attack1, facing east
```

Direction suffixes in screen-space order (from `direction_index`):
`e`, `se`, `s`, `sw`, `w`, `nw`, `n`, `ne`

Build a tag with the helper:

```rust
let tag = composite_tag("idle", direction_index(facing_vec, cam_forward_xz));
```

## Animation System (Standalone)

For non-composite 2D sprite animations (UI, flat sprites), use `RoAnimation` directly.

```rust
use bevy_ro_sprites::prelude::*;

commands.spawn((
    RoAnimation {
        atlas: server.load("sprite/effect/torch.spr"),
        animation: RoAnimationControl::tag("idle_s"),
    },
    Sprite::default(),
));
```

### RoAnimationControl Fields

| Field     | Type              | Default          | Description                  |
|-----------|-------------------|------------------|------------------------------|
| `tag`     | `Option<String>`  | `None`           | Animation tag to play.       |
| `playing` | `bool`            | `true`           | Whether animation advances.  |
| `speed`   | `f32`             | `1.0`            | Playback speed multiplier.   |
| `repeat`  | `AnimationRepeat` | `Loop`           | `Loop` or `Count(n)`.        |

## SpriteFrameEvent

Observe animation events (sound cues, attack triggers) from ACT files:

```rust
app.add_observer(|trigger: On<SpriteFrameEvent>| {
    let e = trigger.event();
    println!("Event '{}' on {:?} during {:?}", e.event, e.entity, e.tag);
});
```

### SpriteFrameEvent Fields

| Field    | Type             | Description                                              |
|----------|------------------|----------------------------------------------------------|
| `entity` | `Entity`         | The entity whose animation produced this event.          |
| `event`  | `String`         | ACT event string (e.g. `"atk"`, `"attack.wav"`).        |
| `tag`    | `Option<String>` | The animation tag active when the event fired.           |

Sound events (`.wav`/`.mp3` strings) are automatically forwarded to `bevy_ro_sounds` as
`PlaySound` triggers by the animation system.

## Marker Components

| Component        | Placed on                      | Description                                |
|------------------|--------------------------------|--------------------------------------------|
| `ActorBillboard` | Billboard child entities       | Enables shadow attachment and feet lift.   |
| `ActorSprite`    | Actor root entities            | Triggers automatic billboard spawning.     |
| `ActorState`     | Actor root entities            | Current animation action.                  |
| `ActorDirection` | Actor root entities            | World-space facing direction.              |

## Public API Summary

| Item                  | Kind       | Description                                         |
|-----------------------|------------|-----------------------------------------------------|
| `RoSpritePlugin`      | Plugin     | Main plugin (compositing, animation, billboard).    |
| `RoSpriteConfig`      | Resource   | Billboard mode, shadow path, tilt settings.         |
| `BillboardMode`       | Enum       | `Spherical` or `CameraParallel`.                    |
| `ActorSprite`         | Component  | Declares sprite layers for auto-billboard spawning. |
| `ActorState`          | Component  | Current `Action` for an actor.                      |
| `ActorDirection`      | Component  | World-space XZ facing vector.                       |
| `Action`              | Enum       | Animation action (Idle, Walk, Attack1, etc.).       |
| `RoComposite`         | Component  | Multi-layer composite billboard driver.             |
| `CompositeLayerDef`   | Struct     | Layer definition (atlas handle + role).             |
| `SpriteRole`          | Enum       | Layer identity (Body, Head, Weapon, etc.).          |
| `RoCompositeMaterial` | Material   | GPU material for composite rendering.               |
| `ActorBillboard`      | Component  | Billboard marker with feet lift offset.             |
| `CompositeLayout`     | Struct     | Canvas geometry output from composite update.       |
| `RoAnimation`         | Component  | Standalone sprite animation driver.                 |
| `RoAnimationControl`  | Struct     | Tag, speed, repeat settings for animation.          |
| `RoAnimationState`    | Component  | Current frame and elapsed time.                     |
| `AnimationRepeat`     | Enum       | `Loop` or `Count(n)`.                               |
| `SpriteFrameEvent`    | Event      | ACT event triggered during animation playback.      |
| `RoAtlas`             | Asset      | Parsed SPR atlas data.                              |
| `RoAtlasLoader`       | AssetLoader| Bevy asset loader for `.spr` files.                 |
| `composite_tag`       | Function   | Build tag string from action name + direction index.|
| `direction_index`     | Function   | Convert facing Vec2 + camera forward to 0-7 index. |
| `orient_billboard`    | Function   | System that keeps billboards facing the camera.     |
| `MAX_LAYERS`          | Constant   | Maximum composite layers (8).                       |
