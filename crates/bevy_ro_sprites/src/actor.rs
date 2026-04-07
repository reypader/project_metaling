use bevy::prelude::*;

use crate::composite::{composite_tag, direction_index, RoComposite};

// ─────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────

/// Player/NPC animation action.
#[derive(Clone, Copy, PartialEq, Default, Debug)]
pub enum Action {
    #[default]
    Idle,
    Walk,
    Sit,
    PickUp,
    Alert,
    Skill,
    Flinch,
    Frozen,
    Dead,
    Attack1,
    Attack2,
    Spell,
}

impl Action {
    pub fn tag_name(self) -> &'static str {
        match self {
            Action::Idle => "idle",
            Action::Walk => "walk",
            Action::Sit => "sit",
            Action::PickUp => "pickup",
            Action::Alert => "alert",
            Action::Skill => "skill",
            Action::Flinch => "flinch",
            Action::Frozen => "frozen",
            Action::Dead => "dead",
            Action::Attack1 => "attack1",
            Action::Attack2 => "attack2",
            Action::Spell => "spell",
        }
    }
}

/// Facing direction in world XZ space (length doesn't matter).
#[derive(Component, Clone, Copy, Default)]
pub struct ActorDirection(pub Vec2);

/// Current animation action for an actor entity.
#[derive(Component, Clone, Copy, Default)]
pub struct ActorState {
    pub action: Action,
}

// ─────────────────────────────────────────────────────────────
// Systems
// ─────────────────────────────────────────────────────────────

/// Propagates [`ActorState`]/[`ActorDirection`] changes to the [`RoComposite`] tag on the
/// billboard child entity. Registered by [`crate::RoSpritePlugin`].
pub(crate) fn update_composite_tag(
    actors: Query<
        (&ActorState, &ActorDirection, &Children),
        (With<ActorState>, With<ActorDirection>),
    >,
    mut billboards: Query<&mut RoComposite>,
    camera_q: Query<&Transform, With<Camera3d>>,
) {
    let cam_fwd = camera_q
        .single()
        .ok()
        .map(|t| {
            let f = t.forward().as_vec3();
            Vec2::new(f.x, f.z)
        })
        .unwrap_or(Vec2::NEG_Y);

    for (state, dir, children) in &actors {
        for child in children.iter() {
            let Ok(mut composite) = billboards.get_mut(child) else {
                continue;
            };
            let dir_idx = direction_index(dir.0, cam_fwd);
            let tag = composite_tag(state.action.tag_name(), dir_idx);
            composite.tag = Some(tag);
            composite.playing = !matches!(state.action, Action::Idle | Action::Sit);
        }
    }
}
