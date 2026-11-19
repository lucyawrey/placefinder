use bevy::asset::{LoadState, RecursiveDependencyLoadState};
use bevy::gltf::Gltf;
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, poll_once};
use bevy::world_serialization::WorldAssetRoot;
use place::{Address, Base, LoadError, Manifest};

use crate::source::{Bases, SOURCE};

/// Loads Places and moves between them. Travel only swaps Places once the new one has fully
/// loaded; a failed load leaves the current Place as it was.
pub struct PlacesPlugin;

impl Plugin for PlacesPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Travel>()
            .add_message::<Arrived>()
            .init_resource::<Status>()
            .add_systems(
                Update,
                (start_travel, fetch_manifest, finish_travel).chain(),
            );
    }
}

/// A request to go to an address. Typed addresses stand alone; a Portal's target is relative
/// to the current Place.
#[derive(Message)]
pub struct Travel {
    pub target: String,
    pub from_portal: bool,
}

/// The current Place changed.
#[derive(Message)]
pub struct Arrived {
    pub root: Entity,
    pub manifest: Manifest,
}

/// The current Place and what's happening to it, for the address bar to show.
#[derive(Resource, Default)]
pub struct Status {
    pub current: Option<Current>,
    pub loading: Option<String>,
    pub error: Option<String>,
}

pub struct Current {
    pub address: Address,
    number: u32,
    pub root: Entity,
}

#[derive(Component)]
struct Fetching {
    address: Address,
    task: Task<Result<(Base, Manifest), LoadError>>,
}

#[derive(Component)]
struct Pending {
    address: Address,
    number: u32,
    manifest: Manifest,
    gltf: Handle<Gltf>,
}

fn start_travel(
    mut commands: Commands,
    mut requests: MessageReader<Travel>,
    mut status: ResMut<Status>,
    fetching: Query<Entity, With<Fetching>>,
    pending: Query<(Entity, &Pending)>,
    bases: Res<Bases>,
) {
    let Some(Travel {
        target,
        from_portal,
    }) = requests.read().last()
    else {
        return;
    };
    let address = match (&status.current, from_portal) {
        (Some(current), true) => current.address.resolve(target),
        _ => Address::parse(target),
    };
    let address = match address {
        Ok(a) => a,
        Err(e) => {
            status.error = Some(e.to_string());
            return;
        }
    };
    for entity in &fetching {
        commands.entity(entity).despawn();
    }
    for (entity, place) in &pending {
        commands.entity(entity).despawn();
        bases.remove(place.number);
    }
    status.loading = Some(address.to_string());
    status.error = None;
    let fetch_address = address.clone();
    let task = AsyncComputeTaskPool::get().spawn(async move { place::load(&fetch_address) });
    commands.spawn(Fetching { address, task });
}

fn fetch_manifest(
    mut commands: Commands,
    mut fetching: Query<(Entity, &mut Fetching)>,
    mut status: ResMut<Status>,
    mut next_number: Local<u32>,
    bases: Res<Bases>,
    assets: Res<AssetServer>,
) {
    for (entity, mut fetch) in &mut fetching {
        let Some(result) = block_on(poll_once(&mut fetch.task)) else {
            continue;
        };
        commands.entity(entity).despawn();
        match result {
            Ok((base, manifest)) => {
                *next_number += 1;
                bases.insert(*next_number, base);
                let path = format!("{SOURCE}://{}/{}", *next_number, manifest.scene);
                commands.spawn(Pending {
                    address: fetch.address.clone(),
                    number: *next_number,
                    gltf: assets.load(path),
                    manifest,
                });
            }
            Err(e) => fail(&mut status, e.to_string()),
        }
    }
}

fn finish_travel(
    mut commands: Commands,
    pending: Query<(Entity, &Pending)>,
    mut status: ResMut<Status>,
    mut arrived: MessageWriter<Arrived>,
    bases: Res<Bases>,
    assets: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
) {
    for (entity, place) in &pending {
        let failed = match (
            assets.load_state(&place.gltf),
            assets.recursive_dependency_load_state(&place.gltf),
        ) {
            (LoadState::Failed(e), _) => Some(e.to_string()),
            (_, RecursiveDependencyLoadState::Failed(e)) => Some(e.to_string()),
            (_, RecursiveDependencyLoadState::Loaded) => None,
            _ => continue,
        };
        let scene = gltfs
            .get(&place.gltf)
            .and_then(|g| g.default_scene.clone().or(g.scenes.first().cloned()));
        let Some(scene) = scene.filter(|_| failed.is_none()) else {
            commands.entity(entity).despawn();
            bases.remove(place.number);
            fail(
                &mut status,
                failed.unwrap_or_else(|| {
                    format!(
                        "[a scene file with no scene in it: {}]",
                        place.manifest.scene
                    )
                }),
            );
            continue;
        };
        commands.entity(entity).despawn();
        if let Some(old) = status.current.take() {
            commands.entity(old.root).despawn();
            bases.remove(old.number);
        }
        let root = commands
            .spawn((
                Name::new(place.manifest.name.clone()),
                Transform::default(),
                Visibility::default(),
            ))
            .with_child(WorldAssetRoot(scene))
            .id();
        status.current = Some(Current {
            address: place.address.clone(),
            number: place.number,
            root,
        });
        info!("arrived at {}", place.address);
        status.loading = None;
        arrived.write(Arrived {
            root,
            manifest: place.manifest.clone(),
        });
    }
}

fn fail(status: &mut Status, reason: String) {
    warn!("failed to load a Place: {reason}");
    status.loading = None;
    status.error = Some(reason);
}
