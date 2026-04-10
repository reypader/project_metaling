// ─────────────────────────────────────────────────────────────
// Camera orbit
// ─────────────────────────────────────────────────────────────

use crate::player_control::PlayerControl;
use bevy::app::{App, Plugin, Update};
use bevy::camera::Camera3d;
use bevy::input::ButtonInput;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::math::{StableInterpolate, Vec3};
use bevy::prelude::IntoScheduleConfigs;
use bevy::prelude::{
    Component, EulerRot, KeyCode, MouseButton, Quat, Res, ResMut, Resource, Single, Time,
    Transform, With, Without,
};
pub struct OrbitCameraPlugin;
impl Plugin for OrbitCameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OrbitCamera>();
        app.add_systems(Update, (follow_player, update_cam).chain());
        app.add_systems(Update, camera_orbit);
    }
}
#[derive(Resource)]
pub struct OrbitCamera {
    pub distance: f32,
    pub focus: Vec3,
}

#[derive(Component)]
pub struct CameraFollower;

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            distance: 5000.0,
            focus: Vec3::ZERO,
        }
    }
}

fn follow_player(
    q: Single<&Transform, (Without<CameraFollower>, With<PlayerControl>)>,
    mut follower: Single<&mut Transform, (With<CameraFollower>, Without<PlayerControl>)>,
    time: Res<Time>,
) {
    follower
        .translation
        .smooth_nudge(&q.translation, 5.0, time.delta_secs());
}
fn update_cam(
    mut orbit: ResMut<OrbitCamera>,
    follower: Single<&Transform, (With<CameraFollower>, Without<PlayerControl>)>,
) {
    orbit.focus = follower.translation
}
fn camera_orbit(
    mouse_button: Res<ButtonInput<MouseButton>>,
    kb_button: Res<ButtonInput<KeyCode>>,
    mouse: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    time: Res<Time>,
    mut orbit: ResMut<OrbitCamera>,
    mut camera: Single<&mut Transform, With<Camera3d>>,
) {
    let dt = time.delta_secs();

    let mut delta_pitch = 0.0;
    let mut delta_yaw = 0.0;
    if mouse_button.pressed(MouseButton::Right) && kb_button.pressed(KeyCode::ShiftLeft) {
        delta_pitch = mouse.delta.y * dt;
    } else if mouse_button.pressed(MouseButton::Right) {
        delta_yaw -= mouse.delta.x * dt;
    }

    let d = orbit.distance - (scroll.delta.y * 200.0 * dt);
    orbit.distance = d.clamp(50.0, 200.0);

    let (yaw, pitch, roll) = camera.rotation.to_euler(EulerRot::YXZ);

    // Clamp pitch: no shallower than 30° from ground (-PI/6), no steeper than 30° from vertical (-PI/3).
    let pitch =
        (pitch + delta_pitch).clamp(-std::f32::consts::FRAC_PI_3, -std::f32::consts::FRAC_PI_6);
    let yaw = yaw + delta_yaw;
    camera.rotation = Quat::from_euler(EulerRot::YXZ, yaw, pitch, roll);

    camera.translation = orbit.focus - camera.forward() * orbit.distance;
}
