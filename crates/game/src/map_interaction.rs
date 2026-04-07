use crate::{Action, ActorDirection, ActorState, PlayerControl};
use bevy::app::{App, Plugin, Update};
use bevy::prelude::Visibility::{Hidden, Visible};
use bevy::prelude::*;
use bevy_ro_maps::prelude::TerrainType;
use bevy_ro_maps::{NavMesh, RoMapRoot};
use std::collections::VecDeque;

pub struct MapInteractionPlugin;
impl Plugin for MapInteractionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, navigate);
        app.add_systems(Update, attach_input_listeners);
    }
}

#[derive(Component)]
struct ClickObserved;

#[derive(Component)]
pub struct MapMarker;

#[derive(Component, Default)]
pub struct Navigation {
    pub path: VecDeque<Vec2>,
}

fn navigate(
    mut query: Query<(
        &mut Transform,
        &mut Navigation,
        &mut ActorDirection,
        &mut ActorState,
    )>,
    navmesh: Single<&NavMesh>,
    time: Res<Time>,
) {
    for (mut tf, mut nav, mut dir, mut state) in query.iter_mut() {
        let p = &mut nav.path;
        if !p.is_empty() {
            let current_loc = tf.translation.xz();

            p.pop_front_if(|next| {
                let d = current_loc.distance(*next);
                d < 1.0
            });
            if let Some(target) = p.front() {
                let direction = (target - current_loc).normalize();
                dir.0 = direction;
                let x = tf.translation.x + direction.x * 50.0 * time.delta_secs();
                let z = tf.translation.z + direction.y * 50.0 * time.delta_secs();
                let y = navmesh.height_at(x, z);

                tf.translation = Vec3::new(x, y, z);
                state.action = Action::Walk;
            } else {
                state.action = Action::Idle;
            }
        }
    }
}
fn attach_input_listeners(
    mut commands: Commands,
    query: Query<Entity, (With<RoMapRoot>, Without<ClickObserved>)>,
) {
    for entity in query.iter() {
        commands
            .entity(entity)
            .insert(ClickObserved)
            .observe(
                |hover: On<Pointer<Move>>,
                 marker: Single<(&mut Visibility, &mut Transform), With<MapMarker>>,
                 query: Query<&NavMesh>,
                 _map: Single<&RoMapRoot>| {
                    if let Ok(navmesh) = query.get(hover.entity)
                        && let Some(hit_pos) = hover.hit.position {
                            let (target, terrain_type) = navmesh.to_map_tile(hit_pos.x, hit_pos.z);
                            let (mut vis, mut tf) = marker.into_inner();

                            tf.translation = target;

                            match terrain_type {
                                TerrainType::Walkable => {
                                    *vis = Visible;
                                    tf.rotation = Quat::default();
                                }
                                TerrainType::Blocked => *vis = Hidden,
                                TerrainType::Snipeable => {
                                    *vis = Visible;
                                    tf.rotation = Quat::from_rotation_x(90.0);
                                }
                                TerrainType::Unknown(_) => *vis = Hidden,
                            }
                        }
                },
            )
            .observe(
                |click: On<Pointer<Click>>,
                 player: Single<(Entity, &Transform), With<PlayerControl>>,
                 query: Query<&NavMesh>,
                 _map: Single<&RoMapRoot>,
                 mut commands: Commands| {
                    if click.button == PointerButton::Primary
                        && let Ok(navmesh) = query.get(click.entity)
                            && let Some(hit_pos) = click.hit.position {
                                let (payer_entity, player_tf) = *player;
                                let (target, terrain_type) =
                                    navmesh.to_map_tile(hit_pos.x, hit_pos.z);
                                if terrain_type == TerrainType::Walkable {
                                    let start = player_tf.translation.xz();
                                    if let Some(path) =
                                        navmesh.path((start.x, start.y), (target.x, target.z))
                                    {
                                        commands.entity(payer_entity).insert(Navigation { path });
                                    }
                                }
                            }
                },
            );
    }
}
