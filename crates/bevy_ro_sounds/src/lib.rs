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

/// Plugin that handles [`PlaySound`] events by spawning Bevy audio entities.
pub struct RoSoundsPlugin;

impl Plugin for RoSoundsPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(handle_play_sound);
        app.add_systems(Last, mute_audio_on_exit);
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
    match ev.location {
        Some(tf) => {
            println!("playing {:?} {:?}", resolved, tf);
            // Spatial scale is intentionally left to the global default_spatial_scale (set in
            // AudioPlugin). Per-sound overrides via with_spatial_scale() REPLACE the global value
            // rather than multiply it, so using one here would bypass the world-scale correction.
            commands.spawn((
                tf,
                GlobalTransform::from(tf),
                AudioPlayer::new(handle),
                settings
                    .with_spatial(true)
                    .with_volume(Volume::Linear(ev.volume.unwrap_or(1.0))),
            ))
        }
        None => commands.spawn((AudioPlayer::new(handle), settings)),
    };
}

/// Normalises a raw sound reference into an asset-relative path.
/// Takes only the filename after the last `/` or `\`, then prefixes
/// `wav/` for `.wav` files and `bgm/` for `.mp3` files.
/// Returns `None` for strings that are not sound file references.
fn resolve_sound_path(raw: &str) -> Option<String> {
    let filename = raw
        .rsplit(|c| c == '/' || c == '\\')
        .next()
        .unwrap_or(raw)
        .trim();
    if filename.is_empty() {
        return None;
    }
    let lower = filename.to_ascii_lowercase();
    println!("{:?}", lower);
    if lower.ends_with(".wav") {
        Some(format!("wav/{filename}"))
    } else if lower.ends_with(".mp3") {
        Some(format!("bgm/{filename}"))
    } else {
        println!("{:?}", lower);
        None
    }
}
