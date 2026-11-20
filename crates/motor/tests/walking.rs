use motor::{Input, Settings, State, Vec3, World, spawn, step};
use parry3d::glamx::Quat;

const DT: f32 = 1.0 / 64.0;

/// Triangles for a test world. Floors are split into many triangles, so walking crosses seams.
#[derive(Default)]
struct Shapes {
    vertices: Vec<Vec3>,
    triangles: Vec<[u32; 3]>,
}

impl Shapes {
    fn quad(&mut self, corners: [Vec3; 4]) {
        let first = self.vertices.len() as u32;
        self.vertices.extend(corners);
        self.triangles
            .extend([[first, first + 1, first + 2], [first, first + 2, first + 3]]);
    }

    /// A level floor at `y` from (-size, -size) to (size, size), in 1 m tiles.
    fn floor(mut self, y: f32, size: i32) -> Self {
        for x in -size..size {
            for z in -size..size {
                let (x, z) = (x as f32, z as f32);
                self.quad([
                    Vec3::new(x, y, z),
                    Vec3::new(x, y, z + 1.0),
                    Vec3::new(x + 1.0, y, z + 1.0),
                    Vec3::new(x + 1.0, y, z),
                ]);
            }
        }
        self
    }

    fn cuboid(mut self, center: Vec3, half: Vec3, rotation: Quat) -> Self {
        let corner = |x: f32, y: f32, z: f32| center + rotation * (half * Vec3::new(x, y, z));
        for (axis, sign) in [
            (0, 1.0),
            (0, -1.0),
            (1, 1.0),
            (1, -1.0),
            (2, 1.0),
            (2, -1.0),
        ] {
            let face = |a: f32, b: f32| {
                let mut p = [0.0; 3];
                p[axis] = sign;
                p[(axis + 1) % 3] = a;
                p[(axis + 2) % 3] = b;
                corner(p[0], p[1], p[2])
            };
            self.quad([
                face(-1.0, -1.0),
                face(1.0, -1.0),
                face(1.0, 1.0),
                face(-1.0, 1.0),
            ]);
        }
        self
    }

    fn block(self, min: Vec3, max: Vec3) -> Self {
        self.cuboid((min + max) / 2.0, (max - min) / 2.0, Quat::IDENTITY)
    }

    fn world(self) -> World {
        World::new(self.vertices, self.triangles)
    }
}

/// Stands the player at `feet`, then walks toward `wish` for `seconds`.
fn walk(world: &World, feet: Vec3, wish: Vec3, seconds: f32) -> State {
    let settings = Settings::default();
    let mut state = settle(world, State::at(feet));
    let input = Input {
        wish: wish.normalize_or_zero(),
        jump: false,
    };
    for _ in 0..(seconds / DT) as usize {
        state = step(world, &settings, state, input, DT);
    }
    state
}

fn settle(world: &World, mut state: State) -> State {
    for _ in 0..64 {
        state = step(world, &Settings::default(), state, Input::default(), DT);
    }
    state
}

fn ramp(degrees: f32) -> World {
    // A long slab tilted up toward -z, starting at the floor near z = 0.
    let rotation = Quat::from_rotation_x(degrees.to_radians());
    let center = rotation * Vec3::new(0.0, -0.5, -5.0);
    Shapes::default()
        .floor(0.0, 8)
        .cuboid(center, Vec3::new(2.0, 0.5, 5.0), rotation)
        .world()
}

#[test]
fn stands_on_a_floor_of_many_triangles() {
    let world = Shapes::default().floor(0.0, 4).world();
    let state = settle(&world, State::at(Vec3::new(0.5, 1.0, 0.5)));
    assert!(state.standing, "{state:?}");
    assert!(state.feet.y.abs() < 0.05, "{state:?}");
}

#[test]
fn walks_across_seams_at_full_speed() {
    let world = Shapes::default().floor(0.0, 6).world();
    let state = walk(&world, Vec3::new(0.1, 0.0, 0.3), Vec3::X, 1.0);
    assert!(state.feet.x > 3.9, "{state:?}");
    assert!(state.feet.y.abs() < 0.05, "{state:?}");
}

#[test]
fn walks_up_a_gentle_ramp() {
    let state = walk(&ramp(20.0), Vec3::new(0.0, 0.0, 1.0), -Vec3::Z, 2.0);
    assert!(state.feet.y > 1.0, "{state:?}");
    assert!(state.standing, "{state:?}");
}

#[test]
fn is_stopped_by_a_ramp_too_steep_to_walk() {
    let state = walk(&ramp(70.0), Vec3::new(0.0, 0.0, 1.0), -Vec3::Z, 2.0);
    assert!(state.feet.y < 0.4, "{state:?}");
}

#[test]
fn cannot_climb_a_too_steep_ramp_by_jumping_into_it() {
    let world = ramp(62.0);
    let settings = Settings::default();
    let mut state = settle(&world, State::at(Vec3::new(0.0, 0.0, 1.0)));
    let mut input = Input {
        wish: -Vec3::Z,
        jump: true,
    };
    let mut peak: f32 = 0.0;
    for _ in 0..64 * 5 {
        state = step(&world, &settings, state, input, DT);
        input.jump = false;
        peak = peak.max(state.feet.y);
    }
    assert!(peak < settings.jump_height + 0.1, "{peak}");
}

#[test]
fn climbs_a_step_under_the_step_height_but_not_over_it() {
    let low = Shapes::default()
        .floor(0.0, 6)
        .block(Vec3::new(1.0, 0.0, -3.0), Vec3::new(4.0, 0.3, 3.0))
        .world();
    let state = walk(&low, Vec3::ZERO, Vec3::X, 0.6);
    assert!(
        state.feet.x > 2.0 && (state.feet.y - 0.3).abs() < 0.05,
        "{state:?}"
    );

    let high = Shapes::default()
        .floor(0.0, 6)
        .block(Vec3::new(1.0, 0.0, -3.0), Vec3::new(4.0, 0.5, 3.0))
        .world();
    let state = walk(&high, Vec3::ZERO, Vec3::X, 1.5);
    assert!(state.feet.x < 0.75 && state.feet.y < 0.05, "{state:?}");
}

#[test]
fn slides_along_a_wall_hit_at_an_angle() {
    let world = Shapes::default()
        .floor(0.0, 6)
        .block(Vec3::new(1.0, 0.0, -6.0), Vec3::new(2.0, 3.0, 6.0))
        .world();
    let state = walk(&world, Vec3::ZERO, Vec3::new(1.0, 0.0, 1.0), 1.0);
    assert!(state.feet.x < 0.75, "{state:?}");
    assert!(state.feet.z > 1.5, "{state:?}");
}

#[test]
fn passes_through_a_doorway_but_not_its_frame() {
    // A wall at x = 2 with a 1.2 m wide, 2.2 m tall opening around z = 0.
    let world = Shapes::default()
        .floor(0.0, 6)
        .block(Vec3::new(2.0, 0.0, -6.0), Vec3::new(2.2, 3.0, -0.6))
        .block(Vec3::new(2.0, 0.0, 0.6), Vec3::new(2.2, 3.0, 6.0))
        .block(Vec3::new(2.0, 2.2, -0.6), Vec3::new(2.2, 3.0, 0.6))
        .world();
    let through = walk(&world, Vec3::ZERO, Vec3::X, 1.5);
    assert!(through.feet.x > 3.0, "{through:?}");
    let frame = walk(&world, Vec3::new(0.0, 0.0, 1.5), Vec3::X, 1.5);
    assert!(frame.feet.x < 1.75, "{frame:?}");
}

#[test]
fn slides_around_a_rotated_box_instead_of_stopping_at_its_bounds() {
    // A 2 m box turned 45° about y: its world-aligned bounds reach x = 1.59 at every z.
    let world = Shapes::default()
        .floor(0.0, 6)
        .cuboid(
            Vec3::new(3.0, 1.0, 0.0),
            Vec3::ONE,
            Quat::from_rotation_y(45f32.to_radians()),
        )
        .world();
    let state = walk(&world, Vec3::new(0.0, 0.0, 1.0), Vec3::X, 2.0);
    assert!(state.feet.x > 3.0, "{state:?}");
}

#[test]
fn stands_still_partly_over_a_platform_edge() {
    let world = Shapes::default()
        .floor(0.0, 6)
        .block(Vec3::new(-2.0, 0.0, -2.0), Vec3::new(0.0, 1.0, 2.0))
        .world();
    let feet = Vec3::new(0.1, 1.0, 0.0);
    let state = settle(&world, State::at(feet));
    assert!(state.standing, "{state:?}");
    assert!((state.feet - feet).length() < 0.05, "{state:?}");
}

#[test]
fn stands_still_partly_over_a_platform_corner() {
    let world = Shapes::default()
        .floor(0.0, 6)
        .block(Vec3::new(-2.0, 0.0, -2.0), Vec3::new(0.0, 1.0, 0.0))
        .world();
    let feet = Vec3::new(0.07, 1.0, 0.07);
    let state = settle(&world, State::at(feet));
    assert!(state.standing, "{state:?}");
    assert!((state.feet - feet).length() < 0.05, "{state:?}");
}

#[test]
fn walks_around_inside_a_hollow_room() {
    let mut shapes = Shapes::default().floor(0.0, 6);
    // Walls and roof of a 6 m room, as one mesh's faces seen from inside.
    let corners = [(-3.0, -3.0), (3.0, -3.0), (3.0, 3.0), (-3.0, 3.0)];
    for i in 0..4 {
        let (ax, az) = corners[i];
        let (bx, bz) = corners[(i + 1) % 4];
        shapes.quad([
            Vec3::new(ax, 0.0, az),
            Vec3::new(bx, 0.0, bz),
            Vec3::new(bx, 3.0, bz),
            Vec3::new(ax, 3.0, az),
        ]);
    }
    let world = shapes.world();
    let state = walk(&world, Vec3::new(-1.0, 0.0, 0.0), Vec3::X, 2.0);
    assert!((state.feet.x - 2.7).abs() < 0.05, "{state:?}");
    let state = walk(&world, Vec3::new(0.0, 0.0, 1.0), -Vec3::Z, 2.0);
    assert!((state.feet.z + 2.7).abs() < 0.05, "{state:?}");
}

#[test]
fn jumps_about_the_jump_height_and_lands() {
    let world = Shapes::default().floor(0.0, 4).world();
    let settings = Settings::default();
    let mut state = settle(&world, State::at(Vec3::Y * 0.5));
    let jump = Input {
        jump: true,
        ..Input::default()
    };
    state = step(&world, &settings, state, jump, DT);
    let mut peak: f32 = 0.0;
    for _ in 0..128 {
        state = step(&world, &settings, state, Input::default(), DT);
        peak = peak.max(state.feet.y);
    }
    assert!((peak - settings.jump_height).abs() < 0.1, "{peak}");
    assert!(state.standing && state.feet.y.abs() < 0.05, "{state:?}");
}

#[test]
fn is_pushed_out_when_starting_inside_a_box() {
    let world = Shapes::default()
        .floor(0.0, 6)
        .block(Vec3::new(-0.5, 0.0, -0.5), Vec3::new(0.5, 0.4, 0.5))
        .world();
    let state = settle(&world, State::at(Vec3::new(0.0, 0.2, 0.0)));
    assert!(state.standing, "{state:?}");
    assert!(state.feet.y > 0.35, "{state:?}");
}

#[test]
fn spawns_on_top_of_a_solid_mesh_it_starts_inside() {
    let world = Shapes::default()
        .floor(0.0, 6)
        .block(Vec3::new(-3.0, 0.0, -3.0), Vec3::new(3.0, 2.0, 3.0))
        .world();
    let state = settle(&world, spawn(&world, Vec3::new(0.0, 0.5, 0.0)));
    assert!(
        state.standing && (state.feet.y - 2.0).abs() < 0.05,
        "{state:?}"
    );
}

#[test]
fn respawns_after_falling_past_the_depth() {
    let world = Shapes::default().floor(0.0, 1).world();
    let spawn = Vec3::new(5.0, 0.0, 5.0);
    let mut state = State::at(spawn);
    let mut respawned = false;
    for _ in 0..64 * 6 {
        let next = step(&world, &Settings::default(), state, Input::default(), DT);
        respawned |= next.feet.y > state.feet.y;
        state = next;
    }
    assert!(respawned, "{state:?}");
}
