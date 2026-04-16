use bevy::prelude::*;

use crate::composite::{
    composite_tag, direction_index, ActorBillboard, CompositeLayerDef, RoComposite,
    RoCompositeMaterial, SpriteRole,
};

// ─────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────

/// All possible actor animation actions (unified for players and monsters).
///
/// The player sprite layout has 13 action groups (104 actions = 13 groups x 8 directions).
/// Monster sprites have 5 action groups (40 actions = 5 groups x 8 directions).
/// When a monster sprite receives a player-only action, it falls back to a
/// suitable alternative via [`Action::monster_fallback`].
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

/// Direction suffixes in ACT file order (flat index within a group: 0=s, 1=sw, ..., 7=se).
/// Used by the loader when generating tag names from flat action indices.
pub(crate) const ACT_DIR_SUFFIXES: &[&str] = &["s", "sw", "w", "nw", "n", "ne", "e", "se"];

/// Direction suffixes in screen-space order (from [`direction_index`]: 0=e, 1=se, ..., 7=ne).
/// Used by [`composite_tag`] when converting a runtime direction index to a tag suffix.
pub(crate) const SCREEN_DIR_SUFFIXES: &[&str] = &["e", "se", "s", "sw", "w", "nw", "n", "ne"];

/// All player actions in layout order (base index = position * 8).
const PLAYER_ACTIONS: &[Action] = &[
    Action::Idle,
    Action::Walk,
    Action::Sit,
    Action::PickUp,
    Action::Alert,
    Action::Skill,
    Action::Flinch,
    Action::Frozen,
    Action::Dead,
    Action::Attack1,
    Action::Attack2,
    Action::Spell,
];

/// All monster actions in layout order (base index = position * 8).
const MONSTER_ACTIONS: &[Action] = &[
    Action::Idle,
    Action::Walk,
    Action::Attack1,
    Action::Flinch,
    Action::Dead,
];

impl Action {
    /// Short tag name for this action (used in composite tag strings like `"idle_s"`).
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

    /// Base action index in the player sprite layout (stride of 8 per action group).
    pub fn player_base_index(self) -> usize {
        match self {
            Action::Idle => 0,
            Action::Walk => 8,
            Action::Sit => 16,
            Action::PickUp => 24,
            Action::Alert => 32,
            Action::Skill => 40,
            Action::Flinch => 48,
            Action::Frozen => 56,
            Action::Dead => 64,
            // Player layout skips group 72..79 ("unknown"), handled at load time
            Action::Attack1 => 80,
            Action::Attack2 => 88,
            Action::Spell => 96,
        }
    }

    /// Maps this action to the closest available monster action.
    ///
    /// Monster sprites have only 5 groups: Idle(0), Walk(8), Attack1(16), Flinch(24), Dead(32).
    /// Player-only actions fall back:
    ///   Sit, Alert, Frozen -> Idle
    ///   PickUp, Skill, Spell, Attack2 -> Attack1
    pub fn monster_fallback(self) -> Action {
        match self {
            Action::Idle | Action::Sit | Action::Alert | Action::Frozen => Action::Idle,
            Action::Walk => Action::Walk,
            Action::PickUp | Action::Skill | Action::Attack1 | Action::Attack2 | Action::Spell => {
                Action::Attack1
            }
            Action::Flinch => Action::Flinch,
            Action::Dead => Action::Dead,
        }
    }

    /// Base action index in the monster sprite layout (stride of 8 per action group).
    pub fn monster_base_index(self) -> usize {
        match self.monster_fallback() {
            Action::Idle => 0,
            Action::Walk => 8,
            Action::Attack1 => 16,
            Action::Flinch => 24,
            Action::Dead => 32,
            _ => 0,
        }
    }

    /// Returns the base action index appropriate for the given total action count.
    ///
    /// Uses the monster layout when `total_actions` is a multiple of 8 but not 104
    /// (the player layout). Otherwise uses the player layout.
    pub fn base_index(self, total_actions: usize) -> usize {
        if Self::is_monster_layout(total_actions) {
            self.monster_base_index()
        } else {
            self.player_base_index()
        }
    }

    /// Resolves a flat action index (as stored in ACT files) back to an `Action`
    /// variant and a direction index (0..7).
    ///
    /// Returns `None` for indices that don't map to a known action (e.g. the
    /// "unknown" group 72..79 in the player layout, which is skipped at load time).
    pub fn from_flat_index(idx: usize, total_actions: usize) -> Option<(Action, u8)> {
        let dir = (idx % 8) as u8;
        let base = idx - idx % 8;
        let actions = if Self::is_monster_layout(total_actions) {
            MONSTER_ACTIONS
        } else {
            PLAYER_ACTIONS
        };
        // Find which action group this base corresponds to
        for (group_idx, &action) in actions.iter().enumerate() {
            let group_base = if Self::is_monster_layout(total_actions) {
                group_idx * 8
            } else {
                action.player_base_index()
            };
            if group_base == base {
                return Some((action, dir));
            }
        }
        None
    }

    /// Returns `true` if the given total action count indicates a monster sprite layout.
    pub fn is_monster_layout(total_actions: usize) -> bool {
        total_actions != 104 && total_actions.is_multiple_of(8)
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

/// Describes the sprite layers for an actor entity. When this component is added,
/// the sprite plugin automatically spawns a billboard child with the appropriate
/// [`RoComposite`] layers.
#[derive(Component)]
pub struct ActorSprite {
    pub body: String,
    pub head: Option<String>,
    pub weapon: Option<String>,
    pub weapon_slash: Option<String>,
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
            composite.playing = true; // !matches!(state.action, Action::Idle | Action::Sit);
        }
    }
}

/// Automatically spawns a billboard child entity with [`RoComposite`] layers for every
/// newly added [`ActorSprite`]. Registered by [`crate::RoSpritePlugin`].
pub(crate) fn spawn_actor_billboard(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<RoCompositeMaterial>>,
    new_actors: Query<
        (Entity, &ActorSprite, &ActorState, &ActorDirection),
        Added<ActorSprite>,
    >,
    server: Res<AssetServer>,
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

    for (entity, sprite, state, dir) in &new_actors {
        let tag = composite_tag(
            state.action.tag_name(),
            direction_index(dir.0, cam_fwd),
        );

        let mut layers = vec![CompositeLayerDef {
            atlas: server.load(sprite.body.clone()),
            role: SpriteRole::Body,
        }];
        if let Some(head) = &sprite.head {
            layers.push(CompositeLayerDef {
                atlas: server.load(head.clone()),
                role: SpriteRole::Head,
            });
        }
        if let Some(weapon) = &sprite.weapon {
            layers.push(CompositeLayerDef {
                atlas: server.load(weapon.clone()),
                role: SpriteRole::Weapon { slot: 0 },
            });
        }
        if let Some(weapon_slash) = &sprite.weapon_slash {
            layers.push(CompositeLayerDef {
                atlas: server.load(weapon_slash.clone()),
                role: SpriteRole::Weapon { slot: 1 },
            });
        }
        commands.entity(entity).with_children(|parent| {
            parent.spawn((
                RoComposite {
                    layers,
                    tag: Some(tag),
                    playing: true,
                    ..Default::default()
                },
                Mesh3d(meshes.add(Rectangle::new(1.0, 1.0))),
                MeshMaterial3d(mats.add(RoCompositeMaterial::default())),
                Transform::default(),
                ActorBillboard { feet_lift: 10.0 },
            ));
        });
    }
}
