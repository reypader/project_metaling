use bevy::app::{App, Plugin, Update};
use bevy::prelude::Visibility::{Hidden, Visible};
use bevy::prelude::*;
use bevy_ro_maps::prelude::TerrainType;
use bevy_ro_maps::{NavMesh, RoMapRoot};

pub struct MapInteractionPlugin;
impl Plugin for MapInteractionPlugin {
    fn build(&self, app: &mut App) {
        // app.add_systems(Update, spawn_tiles.run_if(not(any_with_component::<Tile>)));
        app.add_systems(Update, attach_input_listeners);
    }
}

#[derive(Component)]
struct ClickObserved;

#[derive(Component)]
pub struct MapMarker;

fn attach_input_listeners(
    mut commands: Commands,
    query: Query<Entity, (With<RoMapRoot>, Without<ClickObserved>)>,
) {
    for entity in query.iter() {
        println!("Adding observer to {:?}", entity);
        commands
            .entity(entity)
            .insert(ClickObserved)
            .observe(
                |hover: On<Pointer<Move>>,
                 mut marker: Single<(&mut Visibility, &mut Transform), With<MapMarker>>,
                 query: Query<&NavMesh>,
                 map: Single<&RoMapRoot>| {
                    if let Ok(navmesh) = query.get(hover.entity) {
                        println!("hit?");
                        if let Some(hit_pos) = hover.hit.position {
                            let (target, terrain_type) =
                                navmesh.to_map_tile(hit_pos.x, hit_pos.z);
                            println!("target {:?}, hit {:?}", target, hit_pos);
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
                    }
                },
            )
            .observe(
                |click: On<Pointer<Click>>,
                 mut marker: Single<(&mut Visibility, &mut Transform), With<MapMarker>>,
                 query: Query<&NavMesh>,
                 map: Single<&RoMapRoot>| {
                    if let Ok(navmesh) = query.get(click.entity) {
                        println!("hit?");
                        if let Some(hit_pos) = click.hit.position {
                            let (target, terrain_type) =
                                navmesh.to_map_tile(hit_pos.x, hit_pos.z);
                            println!("target {:?}, hit {:?}", target, hit_pos);
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
                    }
                },
            );
    }
}
