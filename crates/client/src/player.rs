use bevy::camera::primitives::Aabb;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions};

use crate::places::{Arrived, Status};
use crate::portals::Portal;

/// A first-person player that walks on and bumps into the current Place's meshes as boxes.
pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn)
            .add_systems(Update, (place_at_spawn, look, walk).chain());
    }
}

#[derive(Component, Default)]
pub struct Player {
    yaw: f32,
    pitch: f32,
    fall_speed: f32,
}

/// Half the player's width and height; the camera sits near the top.
pub const HALF_SIZE: Vec3 = Vec3::new(0.3, 0.9, 0.3);
const EYE: f32 = 0.7;
const SPEED: f32 = 4.0;
const GRAVITY: f32 = 20.0;
/// How far the player is kept from a box they were pushed out of, since f32 rounding can
/// otherwise leave them just inside it.
const GAP: f32 = 0.001;
const SENSITIVITY: f32 = 0.002;

fn spawn(mut commands: Commands) {
    commands
        .spawn((
            Player::default(),
            Transform::from_xyz(0.0, HALF_SIZE.y, 0.0),
            Visibility::default(),
        ))
        .with_child((Camera3d::default(), Transform::from_xyz(0.0, EYE, 0.0)));
    commands.spawn((
        DirectionalLight::default(),
        Transform::from_xyz(2.0, 6.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn place_at_spawn(
    mut arrived: MessageReader<Arrived>,
    mut player: Single<(&mut Transform, &mut Player)>,
) {
    if let Some(arrived) = arrived.read().last() {
        let [x, y, z] = arrived.manifest.spawn.map(|v| v as f32);
        player.0.translation = Vec3::new(x, y + HALF_SIZE.y + GAP, z);
        player.1.fall_speed = 0.0;
    }
}

fn look(
    motion: Res<AccumulatedMouseMotion>,
    cursor: Single<&CursorOptions>,
    mut player: Single<(&mut Transform, &mut Player, &Children)>,
    mut cameras: Query<&mut Transform, (With<Camera3d>, Without<Player>)>,
) {
    if cursor.grab_mode == CursorGrabMode::None {
        return;
    }
    let (transform, state, children) = &mut *player;
    state.yaw -= motion.delta.x * SENSITIVITY;
    state.pitch = (state.pitch - motion.delta.y * SENSITIVITY).clamp(-1.5, 1.5);
    transform.rotation = Quat::from_rotation_y(state.yaw);
    for child in children.iter() {
        if let Ok(mut camera) = cameras.get_mut(child) {
            camera.rotation = Quat::from_rotation_x(state.pitch);
        }
    }
}

fn walk(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    cursor: Single<&CursorOptions>,
    status: Res<Status>,
    mut player: Single<(&mut Transform, &mut Player)>,
    solids: Query<(Entity, &Aabb, &GlobalTransform), (With<Mesh3d>, Without<Portal>)>,
    parents: Query<&ChildOf>,
) {
    let Some(current) = &status.current else {
        return;
    };
    let (transform, state) = &mut *player;
    let mut wish = Vec3::ZERO;
    if cursor.grab_mode != CursorGrabMode::None {
        for (key, dir) in [
            (KeyCode::KeyW, -Vec3::Z),
            (KeyCode::KeyS, Vec3::Z),
            (KeyCode::KeyA, -Vec3::X),
            (KeyCode::KeyD, Vec3::X),
        ] {
            if keys.pressed(key) {
                wish += dir;
            }
        }
    }
    let dt = time.delta_secs();
    let horizontal = transform.rotation * wish.normalize_or_zero() * SPEED * dt;
    state.fall_speed += GRAVITY * dt;
    let boxes: Vec<(Vec3, Vec3)> = solids
        .iter()
        .filter(|(entity, ..)| parents.iter_ancestors(*entity).any(|a| a == current.root))
        .map(|(_, aabb, global)| world_box(aabb, global))
        .collect();
    let mut position = transform.translation;
    for (axis, delta) in [
        (0, horizontal.x),
        (2, horizontal.z),
        (1, -state.fall_speed * dt),
    ] {
        if delta == 0.0 {
            continue;
        }
        position[axis] += delta;
        for &(min, max) in &boxes {
            if overlaps(position, min, max) {
                position[axis] = if delta > 0.0 {
                    min[axis] - HALF_SIZE[axis] - GAP
                } else {
                    max[axis] + HALF_SIZE[axis] + GAP
                };
                if axis == 1 {
                    state.fall_speed = 0.0;
                }
            }
        }
    }
    transform.translation = position;
}

fn world_box(aabb: &Aabb, global: &GlobalTransform) -> (Vec3, Vec3) {
    let affine = global.affine();
    let center: Vec3 = affine.transform_point3(aabb.center.into());
    let m = affine.matrix3;
    let half = Vec3::from(aabb.half_extents);
    let half: Vec3 =
        (m.x_axis.abs() * half.x + m.y_axis.abs() * half.y + m.z_axis.abs() * half.z).into();
    (center - half, center + half)
}

fn overlaps(center: Vec3, min: Vec3, max: Vec3) -> bool {
    (center - HALF_SIZE).cmplt(max).all() && (center + HALF_SIZE).cmpgt(min).all()
}
