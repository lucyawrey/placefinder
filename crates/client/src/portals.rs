use bevy::prelude::*;

use crate::places::{Arrived, Travel};
use crate::player::{Movement, Player};

/// Shows each Portal in the manifest as a glowing box and travels when the player walks in.
pub struct PortalsPlugin;

impl Plugin for PortalsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (spawn_portals, enter));
    }
}

#[derive(Component)]
pub struct Portal {
    target: String,
    half_size: Vec3,
    /// Off until the player has been outside it once, so arriving inside one doesn't bounce back.
    armed: bool,
}

fn spawn_portals(
    mut commands: Commands,
    mut arrived: MessageReader<Arrived>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for arrived in arrived.read() {
        let material = materials.add(StandardMaterial {
            base_color: Color::srgba(0.4, 0.7, 1.0, 0.5),
            emissive: LinearRgba::rgb(0.5, 1.2, 2.5),
            alpha_mode: AlphaMode::Blend,
            ..default()
        });
        for portal in &arrived.manifest.portals {
            let size = Vec3::from_array(portal.size.map(|v| v as f32));
            commands.entity(arrived.root).with_child((
                Portal {
                    target: portal.target.clone(),
                    half_size: size / 2.0,
                    armed: false,
                },
                Mesh3d(meshes.add(Cuboid::from_size(size))),
                MeshMaterial3d(material.clone()),
                Transform::from_translation(Vec3::from_array(portal.center.map(|v| v as f32))),
            ));
        }
    }
}

fn enter(
    mut portals: Query<(&mut Portal, &GlobalTransform)>,
    player: Single<&Transform, With<Player>>,
    movement: Res<Movement>,
    mut travel: MessageWriter<Travel>,
) {
    let body = &movement.0;
    let half_size = Vec3::new(body.radius, body.height / 2.0, body.radius);
    let center = player.translation + Vec3::Y * half_size.y;
    for (mut portal, global) in &mut portals {
        let gap = (center - global.translation()).abs();
        let inside = gap.cmplt(portal.half_size + half_size).all();
        if inside && portal.armed {
            portal.armed = false;
            travel.write(Travel {
                target: portal.target.clone(),
                from_portal: true,
            });
        } else if !inside {
            portal.armed = true;
        }
    }
}
