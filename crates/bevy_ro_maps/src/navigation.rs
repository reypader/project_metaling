use bevy::math::Vec3;
use bevy::prelude::{Component, Vec2};
use pathfinding::prelude::astar;
use ro_files::TerrainType::Blocked;
use ro_files::{GatTile, TerrainType};
use std::collections::VecDeque;

#[derive(Component)]
pub struct NavMesh {
    pub terrain_width: f32,
    pub terrain_height: f32,
    pub nav_width: i32,
    pub nav_height: i32,
    /// Row-major: index = row * width + col.
    pub tiles: Vec<GatTile>,
}

impl NavMesh {
    pub fn tile(&self, col: i32, row: i32) -> Option<&GatTile> {
        if col < self.nav_width && row < self.nav_height {
            self.tiles.get((row * self.nav_width + col) as usize)
        } else {
            None
        }
    }

    fn as_nav(&self, world_x: f32, world_z: f32) -> (f32, f32) {
        let width = self.nav_width;
        let height = self.nav_height;
        let scale_x = self.terrain_width / width as f32;
        let scale_z = self.terrain_height / height as f32;
        let offset_w = -(width as f32 * 0.5 * scale_x);
        let offset_h = height as f32 * 0.5 * scale_z;
        let tile_x = (world_x - offset_w) / scale_x;
        let tile_y = (offset_h - world_z) / scale_z;
        (tile_x, tile_y)
    }

    fn as_world(&self, col: i32, row: i32) -> (f32, f32) {
        let width = self.nav_width;
        let height = self.nav_height;
        let scale_x = self.terrain_width / width as f32;
        let scale_z = self.terrain_height / height as f32;
        let offset_w = -(width as f32 * 0.5 * scale_x);
        let offset_h = height as f32 * 0.5 * scale_z;
        (
            offset_w + (scale_x * (col as f32 + 0.5)),
            offset_h - (scale_z * (row as f32 + 0.5)),
        )
    }

    pub fn terrain_at_nav(&self, col: i32, row: i32) -> TerrainType {
        if col < 0 || col >= self.nav_width || row < 0 || row >= self.nav_height {
            return Blocked;
        }
        self.tile(col, row)
            .map(|t| t.terrain_type)
            .unwrap_or(Blocked)
    }

    pub fn terrain_at_world(&self, world_x: f32, world_z: f32) -> TerrainType {
        let (col, row) = self.as_nav(world_x, world_z);
        let col = col.floor() as i32;
        let row = row.floor() as i32;
        self.terrain_at_nav(col, row)
    }

    pub fn path(&self, start: (f32, f32), target: (f32, f32)) -> Option<VecDeque<Vec2>> {
        let start_c = self.as_nav(start.0, start.1);
        let end_c = self.as_nav(target.0, target.1);
        let end_c = (end_c.0.floor() as i32, end_c.1.floor() as i32);

        astar(
            &(start_c.0.floor() as i32, start_c.1.floor() as i32),
            |&(x, y)| {
                vec![
                    (x + 1, y + 1),
                    (x + 1, y),
                    (x + 1, y - 1),
                    (x - 1, y + 1),
                    (x - 1, y),
                    (x - 1, y - 1),
                    (x, y + 1),
                    (x, y - 1),
                ]
                .into_iter()
                .map(|p| (p, 1))
            },
            |&(x, y)| match &self.terrain_at_nav(x, y) {
                TerrainType::Walkable => (end_c.0 - x).pow(2) + (end_c.1 - y).pow(2),
                _ => 1000000,
            },
            |&p| p == end_c,
        )
        .map(|result| {
            result
                .0
                .into_iter()
                .skip(1)
                .map(|x| Vec2::from(self.as_world(x.0, x.1)))
                .collect()
        })
    }

    pub fn to_map_tile(&self, world_x: f32, world_z: f32) -> (Vec3, TerrainType) {
        // hack
        let x = self.as_nav(world_x, world_z);
        let x = self.as_world(x.0.floor() as i32, x.1.floor() as i32);
        (
            Vec3::new(x.0, self.height_at(x.0, x.1), x.1),
            self.terrain_at_world(world_x, world_z),
        )
    }

    pub fn height_at(&self, world_x: f32, world_z: f32) -> f32 {
        let (tile_x, tile_y) = self.as_nav(world_x, world_z);
        let col = tile_x.floor() as i32;
        let row = tile_y.floor() as i32;
        let fx = tile_x.fract();
        let fz = (row as f32 - tile_y).clamp(0.0, 1.0);
        if col < 0 || col >= self.nav_width {
            return 0.0;
        }
        if row < 0 || row >= self.nav_height {
            return 0.0;
        }
        let Some(tile) = self.tile(col, row) else {
            return 0.0;
        };
        let sw = ro_files::coord::gat_altitude(tile.altitude_sw);
        let se = ro_files::coord::gat_altitude(tile.altitude_se);
        let nw = ro_files::coord::gat_altitude(tile.altitude_nw);
        let ne = ro_files::coord::gat_altitude(tile.altitude_ne);

        let north = nw + (ne - nw) * fx;
        let south = sw + (se - sw) * fx;
        north + (south - north) * fz
    }
}
