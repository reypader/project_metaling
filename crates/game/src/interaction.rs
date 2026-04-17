//! Humanoid interaction / look-at system.
//!
//! See `plans/player_interaction.md` for the design. In short:
//!   * `InteractionTarget` marks actor entities that are right-click targetable.
//!   * `LookTarget` lives on humanoid actors and gates Idle/Sit animation.
//!     `Some(e)` plays the head-look animation and nudges facing toward `e`;
//!     `None` freezes the neutral frame.
//!   * Monsters (monster-layout sprites) don't receive `LookTarget`; their
//!     idle keeps looping via the sprite library default.
//!
//! Test-scene policy: a confirmed right-click broadcasts the clicked entity
//! to every `LookTarget` in the world. This stands in for a future NPC AI
//! system that will pick look-targets per actor.

use bevy::prelude::*;
use bevy_ro_sprites::actor::{update_composite_tag, ActorDirection, ActorState};
use bevy_ro_sprites::composite::{update_actor_composites, RoComposite, SpriteRole};
use bevy_ro_sprites::prelude::Action;
use bevy_ro_sprites::RoAtlas;

// ─────────────────────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────────────────────

/// Maximum angle (radians) between an actor's facing direction and its look
/// target. If exceeded, facing is rotated so the remaining offset equals
/// `±MAX_HEAD_TURN_ANGLE`. 60° keeps the body mostly steady while the head-
/// sweep frames reach the target. Combat targeting will later call the same
/// helper with `0.0` (fully face).
const MAX_HEAD_TURN_ANGLE: f32 = std::f32::consts::FRAC_PI_3;

/// Angular deadzone around facing where the "center" look frame is used
/// instead of a side-glance frame. Below this, the head stays level; above
/// it, we commit to a left/right frame.
const LOOK_CENTER_DEADZONE: f32 = std::f32::consts::PI / 12.0; // 15°

// ─────────────────────────────────────────────────────────────
// Components
// ─────────────────────────────────────────────────────────────

/// Marker on actor entities that humanoids are allowed to target.
#[derive(Component)]
pub struct InteractionTarget;

/// Attached to humanoid actors (player-layout sprites). `Some(e)` engages
/// the head-look on `e`; `None` freezes Idle/Sit on the neutral frame.
#[derive(Component, Default)]
pub struct LookTarget(pub Option<Entity>);

/// Internal marker preventing `attach_interaction_observers` from re-observing
/// an entity that already has the pointer observers wired up.
#[derive(Component)]
struct InteractionObserved;

// ─────────────────────────────────────────────────────────────
// Plugin
// ─────────────────────────────────────────────────────────────

pub struct InteractionPlugin;

impl Plugin for InteractionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                attach_interaction_observers,
                attach_map_clear_observer,
                cleanup_stale_look_targets,
            ),
        );
        // Must run after the sprite library sets `playing = true` on every
        // actor composite and before the frame-advance reads it — otherwise
        // our `playing = false` override is clobbered or read too late,
        // producing the visible head-shake jitter.
        app.add_systems(
            Update,
            update_look_state
                .after(update_composite_tag)
                .before(update_actor_composites),
        );
    }
}

// ─────────────────────────────────────────────────────────────
// Observers
// ─────────────────────────────────────────────────────────────

/// Attaches a `Pointer<Click>` observer on every `InteractionTarget` entity.
/// `Pointer<Click>` only fires when press and release land on the same entity
/// without the pointer dragging off, which matches the "release on same
/// target" contract in the plan — camera orbit from right-click-drag is left
/// untouched because its drag cancels the click.
fn attach_interaction_observers(
    mut commands: Commands,
    new_targets: Query<Entity, (With<InteractionTarget>, Without<InteractionObserved>)>,
) {
    for entity in &new_targets {
        commands
            .entity(entity)
            .insert(InteractionObserved)
            .observe(on_interaction_click);
    }
}

fn on_interaction_click(
    trigger: On<Pointer<Click>>,
    mut look_q: Query<(Entity, &mut LookTarget)>,
) {
    if trigger.button != PointerButton::Secondary {
        return;
    }
    let target = trigger.entity;
    for (entity, mut look) in &mut look_q {
        if entity == target {
            continue;
        }
        look.0 = Some(target);
    }
    info!("interaction target acquired: {target:?}");
}

/// Attaches a one-shot secondary-click observer on the `MapMarker` so right-
/// clicking empty terrain clears every `LookTarget`. The marker is spawned in
/// `setup` and sits on the terrain hit point; using it avoids a second
/// observer on `RoMapRoot` (which is also used by `map_interaction`).
///
/// We actually attach to whatever entity carries the navmesh terrain: the
/// same observer pattern used in `map_interaction.rs`. A dedicated system
/// here avoids intertwining unrelated concerns in that file.
fn attach_map_clear_observer(
    mut commands: Commands,
    new_marker: Query<Entity, (With<bevy_ro_maps::RoMapRoot>, Without<InteractionObserved>)>,
) {
    for entity in &new_marker {
        commands
            .entity(entity)
            .insert(InteractionObserved)
            .observe(
                |trigger: On<Pointer<Click>>, mut look_q: Query<&mut LookTarget>| {
                    if trigger.button != PointerButton::Secondary {
                        return;
                    }
                    for mut look in &mut look_q {
                        look.0 = None;
                    }
                    info!("look targets cleared (right-click on terrain)");
                },
            );
    }
}

// ─────────────────────────────────────────────────────────────
// Systems
// ─────────────────────────────────────────────────────────────

/// Removes `Some(e)` from any `LookTarget` whose referenced entity has been
/// despawned, keeping the invariant that `Some(e)` always points to a live
/// entity. Cheap: `get(e)` on the entity query.
fn cleanup_stale_look_targets(
    mut lookers: Query<&mut LookTarget>,
    entities: Query<Entity>,
) {
    for mut look in &mut lookers {
        if let Some(e) = look.0
            && entities.get(e).is_err()
        {
            look.0 = None;
        }
    }
}

/// Gates Idle/Sit animation playback and nudges facing direction for every
/// actor that has a `LookTarget`. Runs each frame.
fn update_look_state(
    mut lookers: Query<(
        Entity,
        &LookTarget,
        &Transform,
        &mut ActorDirection,
        &ActorState,
        &Children,
    )>,
    target_tf_q: Query<&Transform>,
    mut composite_q: Query<&mut RoComposite>,
    atlases: Res<Assets<RoAtlas>>,
) {
    for (self_entity, look, tf, mut dir, state, children) in &mut lookers {
        let Some(child) = children.iter().find(|c| composite_q.contains(*c)) else {
            continue;
        };
        let Ok(mut composite) = composite_q.get_mut(child) else {
            continue;
        };

        match (state.action, look.0) {
            (Action::Idle | Action::Sit, Some(target))
                if target != self_entity =>
            {
                let Ok(target_tf) = target_tf_q.get(target) else {
                    freeze_neutral(&mut composite, &atlases);
                    continue;
                };
                let self_xz = tf.translation.xz();
                let target_xz = target_tf.translation.xz();
                let to_target = (target_xz - self_xz).normalize_or_zero();
                if to_target == Vec2::ZERO {
                    freeze_neutral(&mut composite, &atlases);
                    continue;
                }
                dir.0 = clamp_direction_toward(dir.0, to_target, MAX_HEAD_TURN_ANGLE);
                let angle = signed_angle(dir.0, to_target);
                let offset = look_frame_offset(angle);
                set_frame_within_tag(&mut composite, &atlases, offset);
            }
            (Action::Idle | Action::Sit, _) => freeze_neutral(&mut composite, &atlases),
            _ => { /* Walk / Attack / etc. own their own animation state. */ }
        }
    }
}

fn freeze_neutral(composite: &mut RoComposite, atlases: &Assets<RoAtlas>) {
    set_frame_within_tag(composite, atlases, 0);
}

/// Frame-offset convention within a 3-frame look animation:
///   0 — head centered (target within deadzone of facing)
///   1 — head turned one way (target on positive-angle side)
///   2 — head turned the other way (target on negative-angle side)
///
/// Sprites with fewer frames clamp the offset. The positive/negative
/// convention is sprite-author-dependent; if 1 and 2 feel swapped during
/// testing, swap the two returns.
fn look_frame_offset(angle: f32) -> u16 {
    if angle.abs() < LOOK_CENTER_DEADZONE {
        0
    } else if angle > 0.0 {
        1
    } else {
        2
    }
}

/// Resolves the body atlas for the given composite and writes
/// `current_frame = tag_range.start() + offset`, clamped to the range.
/// `playing` is set to `false` so the library's frame-advance pass leaves
/// the explicit frame alone.
fn set_frame_within_tag(
    composite: &mut RoComposite,
    atlases: &Assets<RoAtlas>,
    offset: u16,
) {
    composite.playing = false;
    let Some(tag) = composite.tag.as_ref() else {
        composite.current_frame = 0;
        return;
    };
    let body_handle = composite
        .layers
        .iter()
        .find(|l| l.role == SpriteRole::Body)
        .map(|l| &l.atlas);
    let range = body_handle
        .and_then(|h| atlases.get(h))
        .and_then(|a| a.tags.get(tag))
        .map(|m| m.range.clone());
    let Some(range) = range else {
        // Atlas not loaded yet — let the library clamp on the next pass.
        composite.current_frame = 0;
        return;
    };
    let span = range.end() - range.start();
    let clamped = offset.min(span);
    composite.current_frame = *range.start() + clamped;
}

/// Signed angle (radians) from `from` to `to` in the XZ Vec2 plane, in
/// range [-PI, PI]. Returns 0 if either input is zero-length.
fn signed_angle(from: Vec2, to: Vec2) -> f32 {
    let from = from.normalize_or_zero();
    let to = to.normalize_or_zero();
    if from == Vec2::ZERO || to == Vec2::ZERO {
        return 0.0;
    }
    let dot = from.dot(to);
    let cross = from.x * to.y - from.y * to.x;
    cross.atan2(dot)
}

// ─────────────────────────────────────────────────────────────
// Math helpers
// ─────────────────────────────────────────────────────────────

/// Rotates `current` toward `target` only as much as needed so that the angle
/// between them is at most `max_angle` (radians). Returns `current` unchanged
/// when already within reach. Inputs may be unnormalized; the returned vector
/// is normalized. Combat targeting will later pass `max_angle = 0.0` to fully
/// face the target.
fn clamp_direction_toward(current: Vec2, target: Vec2, max_angle: f32) -> Vec2 {
    let current = current.normalize_or_zero();
    let target = target.normalize_or_zero();
    if current == Vec2::ZERO || target == Vec2::ZERO {
        return current;
    }
    let cur_angle = current.y.atan2(current.x);
    let tgt_angle = target.y.atan2(target.x);
    let mut diff = tgt_angle - cur_angle;
    // Normalize to [-PI, PI].
    while diff > std::f32::consts::PI {
        diff -= std::f32::consts::TAU;
    }
    while diff < -std::f32::consts::PI {
        diff += std::f32::consts::TAU;
    }
    if diff.abs() <= max_angle {
        return current;
    }
    let rotate_by = diff - diff.signum() * max_angle;
    let (sin, cos) = rotate_by.sin_cos();
    Vec2::new(
        current.x * cos - current.y * sin,
        current.x * sin + current.y * cos,
    )
}

