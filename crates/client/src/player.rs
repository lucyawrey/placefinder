use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::mesh::PrimitiveTopology;
use bevy::pbr::ScreenSpaceTransmission;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions};
use bevy::world_serialization::WorldInstanceReady;
use motor::{Input, Settings, State};

use crate::labels::TextQuad;
use crate::places::{Arrived, Status, Traveling};
use crate::portals::Portal;
use crate::sky::{self, Gradient};

/// A first-person player that walks on and bumps into the current Place's meshes.
pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Movement>()
            .init_resource::<Collision>()
            .add_systems(Startup, spawn)
            .add_systems(
                Update,
                // Same frame as arrival, before the scene spawns and marks itself ready.
                (place_at_spawn, build_world, look, read_input, show)
                    .chain()
                    .after(Traveling),
            )
            .add_systems(FixedUpdate, walk)
            .add_observer(scene_ready);
    }
}

/// The player's `Transform` is at their feet.
#[derive(Component)]
pub struct Player {
    yaw: f32,
    pitch: f32,
    state: State,
    /// Where the feet were before the last tick, for drawing between ticks.
    previous: Vec3,
    input: Input,
}

#[derive(Resource, Default)]
pub struct Movement(pub Settings);

/// The current Place's collision world, `None` until its scene has spawned.
#[derive(Resource, Default)]
enum Collision {
    #[default]
    None,
    /// The scene has spawned; its world transforms are ready next frame.
    Spawned,
    Built(Box<motor::World>),
}

#[derive(Component)]
struct Eye;

/// The camera's height above the feet.
const EYE: f32 = 1.6;
const SENSITIVITY: f32 = 0.002;

fn spawn(mut commands: Commands) {
    commands
        .spawn((
            Player {
                yaw: 0.0,
                pitch: 0.0,
                state: State::at(Vec3::ZERO),
                previous: Vec3::ZERO,
                input: Input::default(),
            },
            Transform::default(),
            Visibility::default(),
        ))
        .with_child((
            Eye,
            Camera3d::default(),
            ScreenSpaceTransmission::default(),
            Transform::from_xyz(0.0, EYE, 0.0),
        ));
}

fn place_at_spawn(
    mut arrived: MessageReader<Arrived>,
    mut player: Single<(&mut Transform, &mut Player)>,
    mut collision: ResMut<Collision>,
    eye: Single<Entity, With<Eye>>,
    gradient: Res<Gradient>,
    mut commands: Commands,
) {
    if let Some(arrived) = arrived.read().last() {
        sky::show(&mut commands.entity(*eye), arrived.manifest.sky, &gradient);
        let spawn = Vec3::from_array(arrived.manifest.spawn.map(|v| v as f32));
        player.0.translation = spawn;
        player.1.state = State::at(spawn);
        player.1.previous = spawn;
        *collision = Collision::None;
    }
}

/// Lights a scene that brings no lights of its own with a sun, which leaves with the Place.
fn scene_ready(
    ready: On<WorldInstanceReady>,
    status: Res<Status>,
    parents: Query<&ChildOf>,
    children: Query<&Children>,
    lights: Query<(), Or<(With<DirectionalLight>, With<PointLight>, With<SpotLight>)>>,
    mut collision: ResMut<Collision>,
    mut commands: Commands,
) {
    let Some(root) = status.current.as_ref().map(|c| c.root) else {
        return;
    };
    if parents.get(ready.entity).ok().map(|p| p.parent()) != Some(root) {
        return;
    }
    *collision = Collision::Spawned;
    if !children.iter_descendants(ready.entity).any(|e| lights.contains(e)) {
        commands.entity(root).with_child((
            DirectionalLight::default(),
            Transform::from_xyz(2.0, 6.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
        ));
    }
}

fn build_world(
    mut collision: ResMut<Collision>,
    mut player: Single<&mut Player>,
    status: Res<Status>,
    meshes: Res<Assets<Mesh>>,
    solids: Query<(Entity, &Mesh3d, &GlobalTransform), (Without<Portal>, Without<TextQuad>)>,
    parents: Query<&ChildOf>,
) {
    let (Collision::Spawned, Some(current)) = (&*collision, &status.current) else {
        return;
    };
    let mut parts = Vec::new();
    for (entity, handle, global) in &solids {
        if !parents.iter_ancestors(entity).any(|a| a == current.root) {
            continue;
        }
        if let Some(mesh) = meshes.get(handle) {
            parts.extend(triangles(mesh, global));
        }
    }
    let world = motor::World::new(parts);
    player.state = motor::spawn(&world, player.state.spawn);
    player.previous = player.state.feet;
    *collision = Collision::Built(Box::new(world));
}

/// A mesh's points, in world space, and triangles, for the collision world being built.
fn triangles(mesh: &Mesh, global: &GlobalTransform) -> Option<(Vec<Vec3>, Vec<[u32; 3]>)> {
    if mesh.primitive_topology() != PrimitiveTopology::TriangleList {
        warn!("no collision for a mesh that isn't a triangle list");
        return None;
    }
    let Some(positions) = mesh
        .try_attribute_option(Mesh::ATTRIBUTE_POSITION)
        .ok()
        .flatten()
        .and_then(|p| p.as_float3())
    else {
        warn!("no collision for a mesh without 32-bit float positions");
        return None;
    };
    let affine = global.affine();
    let points = positions.iter().map(|&p| affine.transform_point3(p.into())).collect();
    let indices: Vec<u32> = match mesh.indices() {
        Some(indices) => indices.iter().map(|i| i as u32).collect(),
        None => (0..positions.len() as u32).collect(),
    };
    let mirrored = affine.matrix3.determinant() < 0.0;
    let triangles = indices.as_chunks::<3>().0.iter().map(|&[a, b, c]| {
        if mirrored { [a, c, b] } else { [a, b, c] }
    });
    Some((points, triangles.collect()))
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

/// Reads input each frame for the next tick: the direction from the keys and facing now, and a
/// jump press kept until a tick uses it.
fn read_input(
    keys: Res<ButtonInput<KeyCode>>,
    cursor: Single<&CursorOptions>,
    mut player: Single<(&Transform, &mut Player)>,
) {
    let (transform, player) = &mut *player;
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
        player.input.jump |= keys.just_pressed(KeyCode::Space);
    }
    player.input.wish = transform.rotation * wish.normalize_or_zero();
}

fn walk(
    time: Res<Time>,
    settings: Res<Movement>,
    collision: Res<Collision>,
    mut player: Single<&mut Player>,
) {
    let Collision::Built(world) = &*collision else {
        return;
    };
    player.previous = player.state.feet;
    player.state = motor::step(
        world,
        &settings.0,
        player.state,
        player.input,
        time.delta_secs(),
    );
    player.input.jump = false;
}

/// Draws the player between their last two ticks, so movement is smooth at any frame rate.
fn show(time: Res<Time<Fixed>>, mut player: Single<(&mut Transform, &Player)>) {
    let (transform, player) = &mut *player;
    let t = time.overstep_fraction();
    transform.translation = player.previous.lerp(player.state.feet, t);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::places::tests::{app_with, example, travel};

    /// Where a body dropped at `x`, `z` from above comes to rest.
    fn rest_height(world: &motor::World, x: f32, z: f32) -> f32 {
        let settings = Settings::default();
        let mut state = State::at(Vec3::new(x, 4.0, z));
        for _ in 0..256 {
            state = motor::step(world, &settings, state, Input::default(), 1.0 / 64.0);
        }
        state.feet.y
    }

    #[test]
    fn builds_collision_from_the_scene_in_world_space() {
        let mut app = app_with(|app| _ = app.add_plugins(PlayerPlugin));
        travel(&mut app, &example("collision-test"));
        for _ in 0..100 {
            if matches!(app.world().resource::<Collision>(), Collision::Built(_)) {
                break;
            }
            app.update();
        }
        let Collision::Built(world) = app.world().resource::<Collision>() else {
            panic!("no collision world");
        };
        // The platform's top.
        assert!((rest_height(world, 0.0, -1.5) - 1.25).abs() < 0.05);
        // The rotated, scaled box under a parent at (8, 0, 2): its top is 2 m up, its long axis
        // runs from the center toward +x, -z, and its bounds reach 1.7 m past its +x, +z face.
        let half = 1.8 / 2f32.sqrt();
        assert!((rest_height(world, 8.0 + half, 2.0 - half) - 2.0).abs() < 0.05);
        assert!(rest_height(world, 9.9, 3.9).abs() < 0.05);
    }
}
