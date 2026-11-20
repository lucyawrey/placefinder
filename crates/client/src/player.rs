use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions};
use bevy::world_serialization::WorldInstanceReady;
use motor::{Input, Settings, State};

use crate::places::{Arrived, Status, Traveling};
use crate::portals::Portal;

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
        .with_child((Camera3d::default(), Transform::from_xyz(0.0, EYE, 0.0)));
    commands.spawn((
        DirectionalLight::default(),
        Transform::from_xyz(2.0, 6.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn place_at_spawn(
    mut arrived: MessageReader<Arrived>,
    mut player: Single<(&mut Transform, &mut Player)>,
    mut collision: ResMut<Collision>,
) {
    if let Some(arrived) = arrived.read().last() {
        let spawn = Vec3::from_array(arrived.manifest.spawn.map(|v| v as f32));
        player.0.translation = spawn;
        player.1.state = State::at(spawn);
        player.1.previous = spawn;
        *collision = Collision::None;
    }
}

fn scene_ready(
    ready: On<WorldInstanceReady>,
    status: Res<Status>,
    parents: Query<&ChildOf>,
    mut collision: ResMut<Collision>,
) {
    let current = status.current.as_ref().map(|c| c.root);
    if parents.get(ready.entity).ok().map(|p| p.parent()) == current {
        *collision = Collision::Spawned;
    }
}

fn build_world(
    mut collision: ResMut<Collision>,
    status: Res<Status>,
    meshes: Res<Assets<Mesh>>,
    solids: Query<(Entity, &Mesh3d, &GlobalTransform), Without<Portal>>,
    parents: Query<&ChildOf>,
) {
    let (Collision::Spawned, Some(current)) = (&*collision, &status.current) else {
        return;
    };
    let mut vertices = Vec::new();
    let mut triangles = Vec::new();
    for (entity, handle, global) in &solids {
        if !parents.iter_ancestors(entity).any(|a| a == current.root) {
            continue;
        }
        if let Some(mesh) = meshes.get(handle) {
            add_triangles(mesh, global, &mut vertices, &mut triangles);
        }
    }
    *collision = Collision::Built(Box::new(motor::World::new(vertices, triangles)));
}

/// Adds a mesh's triangles, in world space, to the collision world being built.
fn add_triangles(
    mesh: &Mesh,
    global: &GlobalTransform,
    vertices: &mut Vec<Vec3>,
    triangles: &mut Vec<[u32; 3]>,
) {
    if mesh.primitive_topology() != PrimitiveTopology::TriangleList {
        return;
    }
    let Some(positions) = mesh
        .try_attribute_option(Mesh::ATTRIBUTE_POSITION)
        .ok()
        .flatten()
        .and_then(|p| p.as_float3())
    else {
        return;
    };
    let first = vertices.len() as u32;
    let affine = global.affine();
    vertices.extend(positions.iter().map(|&p| affine.transform_point3(p.into())));
    let indices: Vec<u32> = match mesh.indices() {
        Some(Indices::U16(i)) => i.iter().map(|&i| i as u32).collect(),
        Some(Indices::U32(i)) => i.clone(),
        None => (0..positions.len() as u32).collect(),
    };
    let mirrored = affine.matrix3.determinant() < 0.0;
    triangles.extend(indices.as_chunks::<3>().0.iter().map(|t| {
        let [a, b, c] = t.map(|i| first + i);
        if mirrored { [a, c, b] } else { [a, b, c] }
    }));
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
        // runs from the center toward +x, -z, and its bounds reach past its faces along +x.
        let half = 1.8 / 2f32.sqrt();
        assert!((rest_height(world, 8.0 + half, 2.0 - half) - 2.0).abs() < 0.05);
        assert!(rest_height(world, 9.8, 2.0).abs() < 0.05);
    }
}
