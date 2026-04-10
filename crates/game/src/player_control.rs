use bevy::prelude::*;
use bevy_ro_sprites::actor::{Action, ActorState};
use bevy_ro_vfx::{EffectRepeat, RoEffectEmitter};

use crate::map_interaction::MapMarker;

pub struct PlayerControlPlugin;
impl Plugin for PlayerControlPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, select_action);
        app.add_systems(Update, spawn_emitter);
    }
}
#[derive(Component)]
pub struct PlayerControl;

// ─────────────────────────────────────────────────────────────
// Input
// ─────────────────────────────────────────────────────────────

fn select_action(keys: Res<ButtonInput<KeyCode>>, mut q: Query<&mut ActorState>) {
    let action = if keys.pressed(KeyCode::Digit1) {
        Some(Action::Idle)
    } else if keys.pressed(KeyCode::Digit2) {
        Some(Action::Walk)
    } else if keys.pressed(KeyCode::Digit3) {
        Some(Action::Sit)
    } else if keys.pressed(KeyCode::Digit4) {
        Some(Action::PickUp)
    } else if keys.pressed(KeyCode::Digit5) {
        Some(Action::Alert)
    } else if keys.pressed(KeyCode::Digit6) {
        Some(Action::Skill)
    } else if keys.pressed(KeyCode::Digit7) {
        Some(Action::Flinch)
    } else if keys.pressed(KeyCode::Digit8) {
        Some(Action::Frozen)
    } else if keys.pressed(KeyCode::Digit9) {
        Some(Action::Dead)
    } else if keys.pressed(KeyCode::KeyQ) {
        Some(Action::Attack1)
    } else if keys.pressed(KeyCode::KeyE) {
        Some(Action::Attack2)
    } else if keys.pressed(KeyCode::KeyR) {
        Some(Action::Spell)
    } else {
        None
    };

    if let Some(a) = action {
        for mut state in &mut q {
            state.action = a;
        }
    }
}

fn spawn_emitter(
    mut commands: Commands,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    marker: Single<&Transform, With<MapMarker>>,
) {
    let m = marker.clone();
    let effect = if keys.clear_just_pressed(KeyCode::Comma) {
        Some(RoEffectEmitter {
            effect_id: "ef_firebolt".to_string(),
            repeat: EffectRepeat::Times(1),
        })
    } else if keys.clear_just_pressed(KeyCode::Period) {
        Some(RoEffectEmitter {
            effect_id: "92".to_string(),
            repeat: EffectRepeat::Times(1),
        })
    } else if keys.clear_just_pressed(KeyCode::Slash) {
        Some(RoEffectEmitter {
            effect_id: "41".to_string(),
            repeat: EffectRepeat::Times(1),
        })
    } else if keys.clear_just_pressed(KeyCode::Semicolon) {
        Some(RoEffectEmitter {
            effect_id: "315".to_string(),
            repeat: EffectRepeat::Times(1),
        })
    } else if keys.clear_just_pressed(KeyCode::KeyL) {
        Some(RoEffectEmitter {
            effect_id: "121".to_string(),
            repeat: EffectRepeat::Times(1),
        })
    } else {
        None
    };

    if let Some(e) = effect {
        println!("Playing effect {:?}", e.effect_id);
        commands.spawn((m, GlobalTransform::from(m), Visibility::default(), e));
    };
}
