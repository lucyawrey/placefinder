use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, poll_once};
use place::{Address, Manifest, manifest};

use crate::labels::{WorldText, sign_height};
use crate::places::{Arrived, Status, Travel};
use crate::player::{Movement, Player};

/// Shows each Portal in the manifest as a glowing box and travels when the player walks in.
pub struct PortalsPlugin;

impl Plugin for PortalsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (spawn_portals, fetch_names, enter));
    }
}

#[derive(Component)]
pub struct Portal {
    target: String,
    half_size: Vec3,
    /// Off until the player has been outside it once, so arriving inside one doesn't bounce back.
    armed: bool,
}

/// The player's feet, facing and velocity in the frame of the Portal they stepped into.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Crossing {
    pub offset: Vec3,
    pub yaw: f32,
    pub velocity: Vec3,
}

/// Where and how the player stands on arrival.
#[derive(Debug, PartialEq)]
pub struct Pose {
    pub feet: Vec3,
    /// Radians about the vertical axis; at 0 the player faces -Z.
    pub yaw: f32,
    pub velocity: Vec3,
}

/// A Portal's centre and turn, in which its front faces +Z.
fn frame(portal: &manifest::Portal) -> Transform {
    Transform::from_translation(Vec3::from_array(portal.center.map(|v| v as f32)))
        .with_rotation(Quat::from_rotation_y((portal.yaw as f32).to_radians()))
}

/// Arriving through the Portal named `name`: as the player crossed into the Portal they left,
/// or, with no crossing, just past its exit side facing away, as if they had walked through.
/// `None` when the Place has no Portal by that name.
pub fn arrival(
    manifest: &Manifest,
    name: &str,
    crossing: Option<Crossing>,
    body_radius: f32,
) -> Option<Pose> {
    let portal = manifest
        .portals
        .iter()
        .find(|p| p.name.as_deref() == Some(name))?;
    let frame = frame(portal);
    let yaw = (portal.yaw as f32).to_radians();
    let size = Vec3::from_array(portal.size.map(|v| v as f32));
    let crossing = crossing.unwrap_or(Crossing {
        offset: Vec3::new(0.0, -size.y / 2.0, -(size.z / 2.0 + body_radius)),
        yaw: 0.0,
        velocity: Vec3::ZERO,
    });
    Some(Pose {
        feet: frame.transform_point(crossing.offset),
        yaw: yaw + crossing.yaw,
        velocity: frame.rotation * crossing.velocity,
    })
}

/// A sign showing its Portal's target until the target Place's name arrives.
#[derive(Component)]
struct TakesName(Address);

/// Fetches a Portal target's manifest for its name.
#[derive(Component)]
struct NameFetch {
    address: Address,
    task: Option<Task<Option<String>>>,
}

const LABEL_HEIGHT: f32 = 0.25;
const LABEL_GAP: f32 = 0.1;
const MAX_NAME_FETCHES: usize = 4;

/// What a Portal's sign first shows, and whether it then takes the target Place's name.
fn first_label(portal: &manifest::Portal) -> Option<(&str, bool)> {
    if !portal.show_label {
        return None;
    }
    Some(match &portal.label {
        Some(label) => (label, false),
        None => (&portal.target, true),
    })
}

/// The address a sign takes its name from, if it takes one and its target resolves.
fn name_source(portal: &manifest::Portal, here: &Address) -> Option<Address> {
    let (_, takes_name) = first_label(portal)?;
    takes_name
        .then(|| here.resolve(&portal.target).ok())?
        .map(|address| address.without_fragment())
}

/// Each address whose name a sign needs, once.
fn names_to_fetch(portals: &[manifest::Portal], here: &Address) -> Vec<Address> {
    let mut addresses = Vec::new();
    for address in portals.iter().filter_map(|p| name_source(p, here)) {
        if !addresses.contains(&address) {
            addresses.push(address);
        }
    }
    addresses
}

fn spawn_portals(
    mut commands: Commands,
    mut arrived: MessageReader<Arrived>,
    status: Res<Status>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for arrived in arrived.read() {
        let Some(current) = &status.current else {
            continue;
        };
        let material = materials.add(StandardMaterial {
            base_color: Color::srgba(0.4, 0.7, 1.0, 0.5),
            emissive: LinearRgba::rgb(0.5, 1.2, 2.5),
            alpha_mode: AlphaMode::Blend,
            ..default()
        });
        for portal in &arrived.manifest.portals {
            let size = Vec3::from_array(portal.size.map(|v| v as f32));
            let entity = commands
                .spawn((
                    Portal {
                        target: portal.target.clone(),
                        half_size: size / 2.0,
                        armed: false,
                    },
                    Mesh3d(meshes.add(Cuboid::from_size(size))),
                    MeshMaterial3d(material.clone()),
                    frame(portal),
                    ChildOf(arrived.root),
                ))
                .id();
            if let Some((text, _)) = first_label(portal) {
                let y = size.y / 2.0 + LABEL_GAP + sign_height(LABEL_HEIGHT) / 2.0;
                let mut sign = commands.spawn((
                    WorldText::sign(text, LABEL_HEIGHT),
                    Transform::from_xyz(0.0, y, 0.0),
                    ChildOf(entity),
                ));
                if let Some(address) = name_source(portal, &current.address) {
                    sign.insert(TakesName(address));
                }
            }
        }
        for address in names_to_fetch(&arrived.manifest.portals, &current.address) {
            commands.entity(arrived.root).with_child(NameFetch {
                address,
                task: None,
            });
        }
    }
}

fn fetch_names(
    mut commands: Commands,
    mut fetches: Query<(Entity, &mut NameFetch)>,
    mut signs: Query<(&TakesName, &mut WorldText)>,
) {
    let mut running = fetches.iter().filter(|(_, f)| f.task.is_some()).count();
    for (entity, mut fetch) in &mut fetches {
        let Some(task) = &mut fetch.task else {
            if running < MAX_NAME_FETCHES {
                let address = fetch.address.clone();
                fetch.task = Some(AsyncComputeTaskPool::get().spawn(async move {
                    place::load(&address)
                        .ok()
                        .map(|(_, manifest)| manifest.name)
                }));
                running += 1;
            }
            continue;
        };
        let Some(name) = block_on(poll_once(task)) else {
            continue;
        };
        commands.entity(entity).despawn();
        running -= 1;
        let Some(name) = name else {
            continue;
        };
        for (takes_name, mut text) in &mut signs {
            if takes_name.0 == fetch.address {
                text.text = name.clone();
            }
        }
    }
}

fn enter(
    mut portals: Query<(&mut Portal, &GlobalTransform)>,
    player: Single<(&Transform, &Player)>,
    movement: Res<Movement>,
    mut travel: MessageWriter<Travel>,
) {
    let (transform, player) = *player;
    let body = &movement.0;
    let half_size = Vec3::new(body.radius, body.height / 2.0, body.radius);
    for (mut portal, global) in &mut portals {
        let to_portal = global.compute_transform().compute_affine().inverse();
        let feet = to_portal.transform_point3(transform.translation);
        // The body as a box in the Portal's frame: close enough for a trigger.
        let inside = (feet + Vec3::Y * half_size.y)
            .abs()
            .cmplt(portal.half_size + half_size)
            .all();
        if inside && portal.armed {
            portal.armed = false;
            travel.write(Travel {
                target: portal.target.clone(),
                from_portal: true,
                crossing: Some(Crossing {
                    offset: feet,
                    yaw: player.yaw() - global.rotation().to_euler(EulerRot::YXZ).0,
                    velocity: to_portal.transform_vector3(player.velocity()),
                }),
            });
        } else if !inside {
            portal.armed = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn portal(target: &str, label: Option<&str>, show_label: bool) -> manifest::Portal {
        manifest::Portal {
            target: target.into(),
            center: [0.0; 3],
            size: [1.0; 3],
            name: None,
            yaw: 0.0,
            label: label.map(Into::into),
            show_label,
        }
    }

    #[test]
    fn a_sign_shows_its_label_else_the_target_until_the_name_arrives() {
        assert_eq!(
            first_label(&portal("a", Some("To A"), true)),
            Some(("To A", false))
        );
        assert_eq!(first_label(&portal("a", None, true)), Some(("a", true)));
        assert_eq!(first_label(&portal("a", Some("To A"), false)), None);
    }

    #[test]
    fn fetches_each_needed_name_once() {
        let here = Address::folder("/places/home");
        let portals = [
            portal("a", None, true),
            portal("./a", None, true),
            portal("b", Some("To B"), true),
            portal("c", None, false),
            portal("d", None, true),
        ];
        let expected = ["a", "d"].map(|t| here.resolve(t).unwrap());
        assert_eq!(names_to_fetch(&portals, &here), expected);
    }

    /// A manifest with one Portal named `gate` at `center`, turned `yaw` degrees.
    fn linked(center: [f64; 3], yaw: f64) -> Manifest {
        let mut gate = portal("x", None, true);
        gate.name = Some("gate".into());
        gate.center = center;
        gate.size = [1.5, 2.0, 0.4];
        gate.yaw = yaw;
        Manifest {
            version: manifest::VERSION.into(),
            name: "A".into(),
            scene: "s.gltf".into(),
            spawn: manifest::Spawn {
                center: [0.0; 3],
                yaw: 0.0,
            },
            portals: vec![gate],
            labels: vec![],
            capture_mouse: false,
            sky: place::Sky::None,
        }
    }

    fn close(a: Vec3, b: Vec3) -> bool {
        a.distance(b) < 1e-5
    }

    #[test]
    fn a_crossing_comes_out_of_the_linked_portal_turned_with_it() {
        let crossing = Crossing {
            offset: Vec3::new(0.2, -1.0, 0.1),
            yaw: 0.3,
            velocity: Vec3::new(0.0, 0.0, -2.0),
        };
        let pose = arrival(&linked([10.0, 1.0, 0.0], 90.0), "gate", Some(crossing), 0.3).unwrap();
        // Turned a quarter counter-clockwise, the Portal's -Z (the way through) is world -X.
        assert!(
            close(pose.feet, Vec3::new(10.1, 0.0, -0.2)),
            "{:?}",
            pose.feet
        );
        assert!((pose.yaw - (std::f32::consts::FRAC_PI_2 + 0.3)).abs() < 1e-5);
        assert!(close(pose.velocity, Vec3::new(-2.0, 0.0, 0.0)));
    }

    #[test]
    fn a_typed_name_arrives_just_past_the_exit_side_facing_away() {
        let pose = arrival(&linked([0.0, 1.0, 5.0], 0.0), "gate", None, 0.3).unwrap();
        assert!(
            close(pose.feet, Vec3::new(0.0, 0.0, 4.5)),
            "{:?}",
            pose.feet
        );
        assert_eq!((pose.yaw, pose.velocity), (0.0, Vec3::ZERO));
        assert_eq!(arrival(&linked([0.0; 3], 0.0), "other", None, 0.3), None);
    }
}
