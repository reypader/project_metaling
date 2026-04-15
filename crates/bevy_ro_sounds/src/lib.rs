use bevy::{audio::Volume, prelude::*};

/// Trigger this event to play a sound. The `path` field accepts either a pre-resolved
/// asset-relative path (`"wav/foo.wav"`, `"bgm/17-1.mp3"`) or a raw ACT event string
/// (`"attack.wav"`). The handler normalises the path by taking only the filename after
/// the last `/` or `\`, then routing it to `wav/` (.wav) or `bgm/` (.mp3).
#[derive(Event)]
pub struct PlaySound {
    pub path: String,
    pub looping: bool,
    pub location: Option<Transform>,
    pub volume: Option<f32>,
    pub range: Option<f32>,
}

/// Marker for one-shot audio entities that should be despawned when playback finishes.
#[derive(Component)]
struct OneShotAudio;

/// Runtime configuration for the sound plugin, inserted as a `Resource`.
#[derive(Resource, Clone, Debug)]
pub struct RoSoundsConfig {
    /// Default volume for spatial sound effects when `PlaySound::volume` is `None`.
    /// Default: `1.0`.
    pub default_sfx_volume: f32,
}

impl Default for RoSoundsConfig {
    fn default() -> Self {
        Self {
            default_sfx_volume: 1.0,
        }
    }
}

/// Plugin that handles [`PlaySound`] events by spawning Bevy audio entities.
#[derive(Default)]
pub struct RoSoundsPlugin {
    pub config: RoSoundsConfig,
}

impl Plugin for RoSoundsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.config.clone());
        app.add_observer(handle_play_sound);
        app.add_systems(Last, (mute_audio_on_exit, despawn_finished_sounds));
    }
}

fn mute_audio_on_exit(
    mut exits: MessageReader<AppExit>,
    mut sinks: Query<&mut AudioSink>,
    mut spatial_sinks: Query<&mut SpatialAudioSink>,
) {
    if exits.is_empty() {
        return;
    }
    exits.clear();
    for mut sink in &mut sinks {
        sink.set_volume(Volume::Linear(0.0));
    }
    for mut sink in &mut spatial_sinks {
        sink.set_volume(Volume::Linear(0.0));
    }
}

fn handle_play_sound(
    trigger: On<PlaySound>,
    mut commands: Commands,
    server: Res<AssetServer>,
    config: Res<RoSoundsConfig>,
) {
    let ev = trigger.event();
    let Some(resolved) = resolve_sound_path(&ev.path) else {
        println!("can't find {:?}", &ev.path);
        return;
    };

    // Deduplicate only non-spatial looping sounds (e.g. BGM). Spatial emitters each need their
    // own independent instance at their own world position, so skip dedup when a location is set.
    // if ev.location.is_none() && ev.looping {
    //     let ps = &mut playing_sounds.sounds;
    //     if !ps.insert(resolved.clone()) {
    //         return;
    //     }
    // }
    let handle = server.load::<AudioSource>(resolved.clone());
    let settings = if ev.looping {
        PlaybackSettings::LOOP
    } else {
        PlaybackSettings::ONCE
    };
    let one_shot = if ev.looping { None } else { Some(OneShotAudio) };
    let volume = ev.volume.unwrap_or(config.default_sfx_volume);
    match ev.location {
        Some(tf) => {
            println!("playing {:?} {:?}", resolved, tf);
            // Spatial scale is intentionally left to the global default_spatial_scale (set in
            // AudioPlugin). Per-sound overrides via with_spatial_scale() REPLACE the global value
            // rather than multiply it, so using one here would bypass the world-scale correction.
            let mut cmd = commands.spawn((
                tf,
                GlobalTransform::from(tf),
                AudioPlayer::new(handle),
                settings
                    .with_spatial(true)
                    .with_volume(Volume::Linear(volume)),
            ));
            if let Some(marker) = one_shot {
                cmd.insert(marker);
            }
            cmd
        }
        None => {
            let mut cmd = commands.spawn((AudioPlayer::new(handle), settings));
            if let Some(marker) = one_shot {
                cmd.insert(marker);
            }
            cmd
        }
    };
}

fn despawn_finished_sounds(
    mut commands: Commands,
    spatial: Query<(Entity, &SpatialAudioSink), With<OneShotAudio>>,
    non_spatial: Query<(Entity, &AudioSink), With<OneShotAudio>>,
) {
    for (entity, sink) in &spatial {
        if sink.empty() {
            commands.entity(entity).despawn();
        }
    }
    for (entity, sink) in &non_spatial {
        if sink.empty() {
            commands.entity(entity).despawn();
        }
    }
}

/// Normalises a raw sound reference into an asset-relative path.
/// Normalizes a raw sound path into an asset-relative path.
/// Backslashes are converted to forward slashes. For `.wav` files, a `wav/`
/// prefix is added if not already present. `.mp3` files are returned as-is
/// (assumed to already carry a `bgm/` prefix or need none).
/// Returns `None` for strings that are not sound file references.
fn resolve_sound_path(raw: &str) -> Option<String> {
    let normalized = raw.replace('\\', "/");
    let trimmed = normalized.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.ends_with(".wav") {
        if lower.starts_with("wav/") {
            Some(trimmed.to_string())
        } else {
            Some(format!("wav/{trimmed}"))
        }
    } else if lower.ends_with(".mp3") {
        Some(trimmed.to_string())
    } else {
        None
    }
}
