use parry3d::math::{Pose, Vector};
use parry3d::query::{self, Ray, RayCast, ShapeCastOptions};
use parry3d::shape::{Capsule, TriMesh, TriMeshFlags};

pub use parry3d::math::Vector as Vec3;

/// Kept between the body and what it rests against, so moving along a surface doesn't hit it.
const SKIN: f32 = 0.01;
const SLIDES: usize = 4;

#[derive(Clone, Debug)]
pub struct Settings {
    /// Meters, feet to top.
    pub height: f32,
    pub radius: f32,
    /// Meters per second.
    pub speed: f32,
    /// Meters per second squared.
    pub gravity: f32,
    /// The highest ledge walked up without jumping, in meters.
    pub step_height: f32,
    /// Degrees from level; anything steeper is a wall.
    pub max_slope: f32,
    pub jump_height: f32,
    /// Falling this many meters below the spawn point returns the player to it.
    pub respawn_depth: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            height: 1.8,
            radius: 0.3,
            speed: 4.0,
            gravity: 20.0,
            step_height: 0.35,
            max_slope: 60.0,
            jump_height: 1.0,
            respawn_depth: 100.0,
        }
    }
}

/// What the player collides with: triangles in world space.
pub struct World(Option<TriMesh>);

impl World {
    pub fn new(vertices: Vec<Vec3>, triangles: Vec<[u32; 3]>) -> World {
        if triangles.is_empty() {
            return World(None);
        }
        let flags = TriMeshFlags::FIX_INTERNAL_EDGES_TWO_SIDED;
        World(TriMesh::with_flags(vertices, triangles, flags).ok())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct State {
    pub feet: Vec3,
    pub velocity: Vec3,
    pub standing: bool,
    pub spawn: Vec3,
}

impl State {
    pub fn at(spawn: Vec3) -> State {
        State {
            feet: spawn,
            velocity: Vec3::ZERO,
            standing: false,
            spawn,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Input {
    /// Horizontal, in world space, at most 1 long.
    pub wish: Vec3,
    pub jump: bool,
}

/// Advances the player by `dt` seconds.
pub fn step(world: &World, settings: &Settings, state: State, input: Input, dt: f32) -> State {
    let Some(mesh) = &world.0 else {
        return fall(settings, state, dt);
    };
    let body = Body { mesh, settings };
    let mut feet = body.push_out(state.feet);
    let mut velocity = Vec3::new(input.wish.x, 0.0, input.wish.z) * settings.speed;
    let mut standing = state.standing;
    if standing && input.jump {
        velocity.y = (2.0 * settings.gravity * settings.jump_height).sqrt();
        standing = false;
    } else if !standing {
        velocity.y = state.velocity.y - settings.gravity * dt;
    }

    let across = Vec3::new(velocity.x, 0.0, velocity.z) * dt;
    if across != Vec3::ZERO {
        feet = if standing {
            body.walk(feet, across)
        } else {
            body.slide(feet, across, Climb::Any)
        };
    }

    if standing {
        match body.ground(feet, settings.step_height) {
            Some((drop, _)) => feet.y -= drop,
            None => standing = false,
        }
    } else {
        let moved = body.slide(feet, Vec3::Y * velocity.y * dt, Climb::Any);
        let blocked = (moved.y - feet.y).abs() < (velocity.y * dt).abs() - SKIN;
        feet = moved;
        if blocked && velocity.y < 0.0 && body.ground(feet, SKIN).is_some() {
            standing = true;
        }
        if blocked {
            velocity.y = 0.0;
        }
    }
    if standing {
        velocity.y = 0.0;
    }

    let state = State {
        feet,
        velocity,
        standing,
        ..state
    };
    respawn_if_lost(settings, state)
}

fn fall(settings: &Settings, state: State, dt: f32) -> State {
    let velocity = Vec3::Y * (state.velocity.y - settings.gravity * dt);
    let state = State {
        feet: state.feet + velocity * dt,
        velocity,
        standing: false,
        ..state
    };
    respawn_if_lost(settings, state)
}

fn respawn_if_lost(settings: &Settings, state: State) -> State {
    if state.feet.y < state.spawn.y - settings.respawn_depth {
        State::at(state.spawn)
    } else {
        state
    }
}

/// Which surfaces a slide may ride up.
#[derive(Clone, Copy, PartialEq)]
enum Climb {
    Any,
    Walkable,
    None,
}

struct Body<'a> {
    mesh: &'a TriMesh,
    settings: &'a Settings,
}

impl Body<'_> {
    fn capsule(&self) -> Capsule {
        let s = self.settings;
        Capsule::new_y(s.height / 2.0 - s.radius, s.radius)
    }

    fn pose(&self, feet: Vec3) -> Pose {
        Pose::from_translation(feet + Vec3::Y * self.settings.height / 2.0)
    }

    fn walkable(&self, normal: Vector) -> bool {
        normal.y >= self.settings.max_slope.to_radians().cos()
    }

    /// Moves the body out of anything it overlaps, as when spawning inside a mesh.
    fn push_out(&self, mut feet: Vec3) -> Vec3 {
        for _ in 0..SLIDES {
            let contact = query::contact(
                &Pose::IDENTITY,
                self.mesh,
                &self.pose(feet),
                &self.capsule(),
                0.0,
            );
            match contact {
                Ok(Some(c)) if c.dist < 0.0 => feet += c.normal1 * (SKIN - c.dist),
                _ => break,
            }
        }
        feet
    }

    /// How far along `motion` the body moves before it would come within `SKIN` of something,
    /// and the surface normal there.
    fn cast(&self, feet: Vec3, motion: Vec3) -> (f32, Option<Vector>) {
        let length = motion.length();
        let options = ShapeCastOptions {
            max_time_of_impact: length + SKIN,
            stop_at_penetration: false,
            ..ShapeCastOptions::default()
        };
        let hit = query::cast_shapes(
            &Pose::IDENTITY,
            Vec3::ZERO,
            self.mesh,
            &self.pose(feet),
            motion / length,
            &self.capsule(),
            options,
        );
        match hit {
            Ok(Some(hit)) => ((hit.time_of_impact - SKIN).max(0.0), Some(hit.normal1)),
            _ => (length, None),
        }
    }

    /// Moves along `motion`, sliding along what it hits. Surfaces it may not climb act as vertical
    /// walls.
    fn slide(&self, mut feet: Vec3, mut motion: Vec3, climb: Climb) -> Vec3 {
        for _ in 0..SLIDES {
            let length = motion.length();
            if length < 1e-5 {
                break;
            }
            let (travel, normal) = self.cast(feet, motion);
            feet += motion / length * travel;
            let Some(mut normal) = normal else { break };
            if climb == Climb::None || climb == Climb::Walkable && !self.walkable(normal) {
                normal = Vec3::new(normal.x, 0.0, normal.z).normalize_or_zero();
            }
            let rest = motion * (1.0 - travel / length);
            motion = rest - normal * rest.dot(normal).min(0.0);
        }
        feet
    }

    /// Moves a standing body, stepping up ledges up to the step height.
    fn walk(&self, feet: Vec3, across: Vec3) -> Vec3 {
        let flat = self.slide(feet, across, Climb::Walkable);
        if flat.distance_squared(feet + across) < 1e-6 {
            return flat;
        }
        let up = self.slide(feet, Vec3::Y * self.settings.step_height, Climb::Any);
        let over = self.slide(up, across, Climb::None);
        let Some((drop, support)) = self.ground(over, up.y - feet.y) else {
            return flat;
        };
        let stepped = over - Vec3::Y * drop;
        // The ledge's top, not where the rounded bottom rests on its edge, decides the height.
        if support > feet.y + self.settings.step_height + SKIN {
            return flat;
        }
        let gained = |to: Vec3| Vec3::new(to.x - feet.x, 0.0, to.z - feet.z).length();
        if gained(stepped) > gained(flat) + 1e-4 {
            stepped
        } else {
            flat
        }
    }

    /// How far down walkable ground is within `reach`, and the height of the highest walkable
    /// face under the body. The ground's own face decides whether it's walkable, not the contact
    /// on the capsule's rounded bottom, so standing partly over a platform's edge still counts.
    fn ground(&self, feet: Vec3, reach: f32) -> Option<(f32, f32)> {
        let (travel, normal) = self.cast(feet, -Vec3::Y * (reach + SKIN));
        normal?;
        let below = feet - Vec3::Y * travel;
        // A sphere resting on an edge hangs below the edge's top by up to its radius.
        let r = self.settings.radius;
        let probe = |x: f32, z: f32| {
            let ray = Ray::new(below + Vec3::new(x, r, z), -Vec3::Y);
            self.mesh
                .cast_ray_and_get_normal(&Pose::IDENTITY, &ray, r + SKIN * 3.0, false)
                .filter(|hit| self.walkable(hit.normal) || self.walkable(-hit.normal))
                .map(|hit| below.y + r - hit.time_of_impact)
        };
        let r = r * 0.9;
        [(0.0, 0.0), (r, 0.0), (-r, 0.0), (0.0, r), (0.0, -r)]
            .into_iter()
            .filter_map(|(x, z)| probe(x, z))
            .reduce(f32::max)
            .map(|support| (travel, support))
    }
}
