# bevy_ro_sounds

Sound playback plugin for Ragnarok Online assets. Handles `.wav` sound effects and `.mp3`
background music with spatial audio support.

## Plugin Setup

```rust
use bevy_ro_sounds::{RoSoundsPlugin, RoSoundsConfig};

app.add_plugins(RoSoundsPlugin::default());

// Or with custom configuration:
app.add_plugins(RoSoundsPlugin {
    config: RoSoundsConfig {
        default_sfx_volume: 0.8,
    },
});
```

### RoSoundsConfig

| Field                | Type  | Default | Description                                              |
|----------------------|-------|---------|----------------------------------------------------------|
| `default_sfx_volume` | `f32` | `1.0`   | Volume used when `PlaySound::volume` is `None`.          |

The config is inserted as a `Resource` and can be modified at runtime via `ResMut<RoSoundsConfig>`.

## Playing Sounds

Fire the `PlaySound` event via `commands.trigger()`:

```rust
use bevy_ro_sounds::PlaySound;

// Simple sound effect (non-spatial)
commands.trigger(PlaySound {
    path: "wav/attack.wav".to_string(),
    looping: false,
    location: None,
    volume: None,   // uses RoSoundsConfig::default_sfx_volume
    range: None,
});

// Spatial sound at a world position
commands.trigger(PlaySound {
    path: "wav/hit.wav".to_string(),
    looping: false,
    location: Some(transform),
    volume: Some(0.5),  // override default volume
    range: None,
});

// Looping background music
commands.trigger(PlaySound {
    path: "bgm/01.mp3".to_string(),
    looping: true,
    location: None,
    volume: None,
    range: None,
});
```

### PlaySound Fields

| Field      | Type                | Description                                                    |
|------------|---------------------|----------------------------------------------------------------|
| `path`     | `String`            | Asset-relative path or raw ACT event string (see Path Resolution). |
| `looping`  | `bool`              | `true` for BGM/ambient loops, `false` for one-shot SFX.       |
| `location` | `Option<Transform>` | World-space transform for spatial audio. `None` for global.    |
| `volume`   | `Option<f32>`       | Per-sound volume override. `None` falls back to config default.|
| `range`    | `Option<f32>`       | Reserved for future per-sound spatial range override.          |

### Path Resolution

Sound paths are normalized automatically:

- `.wav` files get a `wav/` prefix if not already present: `"attack.wav"` becomes `"wav/attack.wav"`.
- `.mp3` files are used as-is (expected to already carry a `bgm/` prefix).
- Backslashes are converted to forward slashes.
- Strings that are not `.wav` or `.mp3` are silently ignored.

This means ACT event strings like `"effect\\hit.wav"` are handled correctly.

### Spatial Audio

Spatial attenuation is controlled by Bevy's global `AudioPlugin::default_spatial_scale`, not
per-sound. The map crate typically sets this to `SpatialScale::new(0.04)` because RO map
coordinates are in the hundreds of world units.

```rust
// In your App setup:
.add_plugins(DefaultPlugins.set(bevy::audio::AudioPlugin {
    default_spatial_scale: bevy::audio::SpatialScale::new(0.04),
    ..default()
}))
```

### Sound Lifecycle

- **One-shot sounds** (`looping: false`): automatically despawned when playback finishes.
- **Looping sounds** (`looping: true`): persist until manually despawned.
- On `AppExit`, all audio sinks are muted to prevent audio artifacts during shutdown.

## Public API Summary

| Item              | Kind       | Description                                      |
|-------------------|------------|--------------------------------------------------|
| `RoSoundsPlugin`  | Plugin     | Registers the observer and cleanup systems.      |
| `RoSoundsConfig`  | Resource   | Runtime volume defaults.                         |
| `PlaySound`       | Event      | Trigger to play a sound.                         |
