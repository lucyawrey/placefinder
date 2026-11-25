use parry3d::math::{Pose, Vector};
use parry3d::query::{self, PointQuery, Ray, RayCast, ShapeCastOptions};
use parry3d::shape::{Capsule, Compound, Shape, SharedShape, TriMesh, TriMeshFlags};

pub use parry3d::math::Vector as Vec3;

/// Kept between the body and what it rests against, so moving along a surface doesn't hit it.
const SKIN: f32 = 0.01;
const SLIDES: usize = 4;
/// Meshes smaller than this across, in meters, with more than `DENSE` triangles collide as a
/// convex hull of at most `HULL_POINTS` points: testing every triangle of a dense prop is slow.
const SMALL: f32 = 2.0;
const DENSE: usize = 64;
const HULL_POINTS: usize = 64;

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

/// What the player collides with, in world space: triangles, and convex hulls standing in for
/// small, dense meshes.
pub struct World {
    mesh: Option<TriMesh>,
    hulls: Option<Compound>,
}

impl World {
    /// Takes each mesh's points and triangles.
    pub fn new(meshes: impl IntoIterator<Item = (Vec<Vec3>, Vec<[u32; 3]>)>) -> World {
        let mut vertices = Vec::new();
        let mut triangles = Vec::new();
        let mut hulls = Vec::new();
        for (points, mesh) in meshes {
            match hull(&points, mesh.len()) {
                Some(hull) => hulls.push((Pose::IDENTITY, hull)),
                None => {
                    let first = vertices.len() as u32;
                    vertices.extend(points);
                    triangles.extend(mesh.into_iter().map(|t| t.map(|i| first + i)));
                }
            }
        }
        let flags = TriMeshFlags::FIX_INTERNAL_EDGES_TWO_SIDED;
        World {
            mesh: (!triangles.is_empty())
                .then(|| TriMesh::with_flags(vertices, triangles, flags).ok())
                .flatten(),
            hulls: (!hulls.is_empty()).then(|| Compound::new(hulls)),
        }
    }

    fn solids(&self) -> impl Iterator<Item = &dyn Shape> {
        let mesh = self.mesh.iter().map(|m| m as &dyn Shape);
        mesh.chain(self.hulls.iter().map(|h| h as &dyn Shape))
    }
}

/// A simplified convex hull standing in for a small mesh of more than `DENSE` triangles, or `None`
/// to keep its triangles, as for a mesh collapsed to a point.
fn hull(points: &[Vec3], triangles: usize) -> Option<SharedShape> {
    let (min, max) = points
        .iter()
        .fold((Vec3::MAX, Vec3::MIN), |(lo, hi), &p| (lo.min(p), hi.max(p)));
    if triangles <= DENSE || (max - min).max_element() >= SMALL {
        return None;
    }
    // The farthest point in each of `HULL_POINTS` directions spread evenly over a sphere.
    let golden = std::f32::consts::PI * (3.0 - 5f32.sqrt());
    let extremes: Vec<Vec3> = (0..HULL_POINTS)
        .map(|i| {
            let y = 1.0 - 2.0 * (i as f32 + 0.5) / HULL_POINTS as f32;
            let (sin, cos) = (golden * i as f32).sin_cos();
            let ring = (1.0 - y * y).sqrt();
            let direction = Vec3::new(ring * cos, y, ring * sin);
            *points
                .iter()
                .max_by(|a, b| a.dot(direction).total_cmp(&b.dot(direction)))
                .expect("a dense mesh has points")
        })
        .collect();
    SharedShape::convex_hull(&extremes)
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

/// Places the player at `at`, lifted onto the top of any solid mesh `at` is inside.
pub fn spawn(world: &World, at: Vec3) -> State {
    State {
        feet: lift(world, at),
        ..State::at(at)
    }
}

/// The point, raised out of any solid it's inside.
pub fn lift(world: &World, at: Vec3) -> Vec3 {
    let mut feet = at;
    if let Some(hulls) = &world.hulls
        && hulls.contains_local_point(feet)
        && let Some(exit) = hulls.cast_local_ray(&Ray::new(feet, Vec3::Y), f32::MAX, false)
    {
        feet.y += exit + SKIN;
    }
    if let Some(mesh) = &world.mesh {
        // Inside a closed mesh, a ray straight up meets the back of its upward-facing top.
        // Only checked here: from below, an upper story's floor faces up too.
        while let Some(hit) =
            mesh.cast_ray_and_get_normal(&Pose::IDENTITY, &Ray::new(feet, Vec3::Y), f32::MAX, false)
        {
            // The hit's normal faces the ray; the triangle's own normal says which side was hit.
            let normal = mesh.triangle(hit.subshape).normal();
            if !normal.is_some_and(|n| n.y > 0.0) {
                break;
            }
            feet.y += hit.time_of_impact + SKIN;
        }
    }
    feet
}

/// Advances the player by `dt` seconds.
pub fn step(world: &World, settings: &Settings, state: State, input: Input, dt: f32) -> State {
    let state = match world.solids().next() {
        Some(_) => Body { world, settings }.advance(state, input, dt),
        None => fall(settings, state, dt),
    };
    if state.feet.y < state.spawn.y - settings.respawn_depth {
        return spawn(world, state.spawn);
    }
    state
}

fn fall(settings: &Settings, state: State, dt: f32) -> State {
    let velocity = Vec3::Y * (state.velocity.y - settings.gravity * dt);
    State {
        feet: state.feet + velocity * dt,
        velocity,
        standing: false,
        ..state
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
    world: &'a World,
    settings: &'a Settings,
}

impl Body<'_> {
    fn advance(&self, state: State, input: Input, dt: f32) -> State {
        let mut feet = self.push_out(state.feet);
        let mut velocity = Vec3::new(input.wish.x, 0.0, input.wish.z) * self.settings.speed;
        let mut standing = state.standing;
        if standing && input.jump {
            velocity.y = (2.0 * self.settings.gravity * self.settings.jump_height).sqrt();
            standing = false;
        } else if !standing {
            velocity.y = state.velocity.y - self.settings.gravity * dt;
        }

        let across = Vec3::new(velocity.x, 0.0, velocity.z) * dt;
        if across != Vec3::ZERO {
            feet = if standing {
                self.walk(feet, across)
            } else {
                self.slide(feet, across, Climb::Walkable)
            };
        }

        if standing {
            match self.ground(feet, self.settings.step_height) {
                Some((drop, _)) => feet.y -= drop,
                None => standing = false,
            }
        } else {
            let moved = self.slide(feet, Vec3::Y * velocity.y * dt, Climb::Any);
            let blocked = (moved.y - feet.y).abs() < (velocity.y * dt).abs() - SKIN;
            feet = moved;
            if blocked && velocity.y < 0.0 && self.ground(feet, SKIN).is_some() {
                standing = true;
            }
            if blocked {
                velocity.y = 0.0;
            }
        }
        if standing {
            velocity.y = 0.0;
        }

        State {
            feet,
            velocity,
            standing,
            ..state
        }
    }

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
            let deepest = self
                .world
                .solids()
                .filter_map(|solid| {
                    query::contact(&Pose::IDENTITY, solid, &self.pose(feet), &self.capsule(), 0.0)
                        .ok()
                        .flatten()
                })
                .min_by(|a, b| a.dist.total_cmp(&b.dist));
            match deepest {
                Some(c) if c.dist < 0.0 => feet += c.normal1 * (SKIN - c.dist),
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
        let first = self
            .world
            .solids()
            .filter_map(|solid| {
                query::cast_shapes(
                    &Pose::IDENTITY,
                    Vec3::ZERO,
                    solid,
                    &self.pose(feet),
                    motion / length,
                    &self.capsule(),
                    options,
                )
                .ok()
                .flatten()
            })
            .min_by(|a, b| a.time_of_impact.total_cmp(&b.time_of_impact));
        match first {
            Some(hit) => ((hit.time_of_impact - SKIN).max(0.0), Some(hit.normal1)),
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
        let gained = |to: Vec3| Vec3::new(to.x - feet.x, 0.0, to.z - feet.z).length();
        if gained(flat) > across.length() - 1e-4 {
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
            self.world
                .solids()
                .filter_map(|solid| {
                    solid.cast_ray_and_get_normal(&Pose::IDENTITY, &ray, r + SKIN * 3.0, false)
                })
                .min_by(|a, b| a.time_of_impact.total_cmp(&b.time_of_impact))
                .filter(|hit| self.walkable(hit.normal) || self.walkable(-hit.normal))
                .map(|hit| below.y + r - hit.time_of_impact)
        };
        let r = r * 0.9;
        let d = r * std::f32::consts::FRAC_1_SQRT_2;
        [
            (0.0, 0.0),
            (r, 0.0),
            (-r, 0.0),
            (0.0, r),
            (0.0, -r),
            (d, d),
            (d, -d),
            (-d, d),
            (-d, -d),
        ]
        .into_iter()
        .filter_map(|(x, z)| probe(x, z))
        .reduce(f32::max)
        .map(|support| (travel, support))
    }
}
