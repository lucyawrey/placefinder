use bevy::pbr::ScreenSpaceTransmission;
use bevy::prelude::*;
use bevy_mod_openxr::action_binding::{OxrSendActionBindings, OxrSuggestActionBinding};
use bevy_mod_openxr::action_set_attaching::OxrAttachActionSet;
use bevy_mod_openxr::action_set_syncing::{OxrActionSetSyncSet, OxrSyncActionSet};
use bevy_mod_openxr::openxr_session_running;
use bevy_mod_openxr::resources::OxrInstance;
use bevy_mod_openxr::session::OxrSession;
use bevy_mod_openxr::spaces::OxrSpaceLocationFlags;
use bevy_mod_xr::camera::XrCamera;
use bevy_mod_xr::session::{
    XrPreDestroySession, XrSessionCreated, XrTrackingRoot, session_available,
};
use openxr::{Action, ActionSet, Path, Posef, ReferenceSpaceType, Vector2f};
use std::f32::consts::PI;

use crate::places::{Arrived, Status};
use crate::player::{Arriving, Eye, Player, Showing};
use crate::sky::{self, Gradient};

/// Plays in a headset through OpenXR: the headset's floor follows the player's feet and yaw.
pub struct VrPlugin;

impl Plugin for VrPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Recenter(true))
            .init_resource::<SnapTurn>()
            .add_systems(Startup, unavailable.run_if(not(session_available)))
            .add_systems(Startup, create_actions.run_if(session_available))
            .add_systems(PostStartup, follow_player)
            .add_systems(OxrSendActionBindings, suggest_bindings)
            .add_systems(XrSessionCreated, (start, show_eye::<false>))
            .add_systems(XrPreDestroySession, show_eye::<true>)
            .add_systems(
                PreUpdate,
                sync_actions
                    .before(OxrActionSetSyncSet)
                    .run_if(openxr_session_running),
            )
            .add_systems(
                Update,
                (
                    arrive,
                    (recenter, turn, walk).run_if(openxr_session_running),
                )
                    .chain()
                    .after(Arriving)
                    .before(Showing),
            )
            .add_observer(camera_added);
    }
}

/// The head lands on the player's feet and yaw once it's tracked, after the session starts and
/// after arrivals that don't come through a linked Portal.
#[derive(Resource)]
struct Recenter(bool);

/// Whether the turn stick has come back to the middle since the last snap turn.
#[derive(Resource)]
struct SnapTurn(bool);

impl Default for SnapTurn {
    fn default() -> Self {
        SnapTurn(true)
    }
}

const SNAP: f32 = PI / 6.0;
/// A turn needs the stick pushed past `PUSH`, and back inside `RELEASE` before the next.
const PUSH: f32 = 0.7;
const RELEASE: f32 = 0.3;

#[derive(Resource)]
struct Actions {
    set: ActionSet,
    walk: Action<Vector2f>,
    turn: Action<Vector2f>,
    aim: Action<Posef>,
}

#[derive(Component)]
struct Head;

#[derive(Component)]
struct LeftAim;

fn unavailable() {
    error!("{}", place::Text::vr_unavailable.get());
}

fn create_actions(instance: Res<OxrInstance>, mut commands: Commands) {
    let set = instance
        .create_action_set("player", "Player", 0)
        .expect("valid action set name");
    let actions = Actions {
        walk: set
            .create_action("walk", "Walk", &[])
            .expect("valid action"),
        turn: set
            .create_action("turn", "Turn", &[])
            .expect("valid action"),
        aim: set.create_action("aim", "Aim", &[]).expect("valid action"),
        set,
    };
    commands.insert_resource(actions);
}

fn suggest_bindings(actions: Res<Actions>, mut bindings: MessageWriter<OxrSuggestActionBinding>) {
    // Vive wands have trackpads where the others have thumbsticks.
    for (profile, stick) in [
        (
            "/interaction_profiles/oculus/touch_controller",
            "thumbstick",
        ),
        ("/interaction_profiles/valve/index_controller", "thumbstick"),
        (
            "/interaction_profiles/microsoft/motion_controller",
            "thumbstick",
        ),
        ("/interaction_profiles/htc/vive_controller", "trackpad"),
    ] {
        for (action, path) in [
            (
                actions.walk.as_raw(),
                format!("/user/hand/left/input/{stick}"),
            ),
            (
                actions.turn.as_raw(),
                format!("/user/hand/right/input/{stick}"),
            ),
            (
                actions.aim.as_raw(),
                "/user/hand/left/input/aim/pose".into(),
            ),
        ] {
            bindings.write(OxrSuggestActionBinding {
                action,
                interaction_profile: profile.into(),
                bindings: vec![path.into()],
            });
        }
    }
}

fn follow_player(
    root: Single<Entity, With<XrTrackingRoot>>,
    player: Single<Entity, With<Player>>,
    mut commands: Commands,
) {
    commands.entity(*player).add_child(*root);
}

fn start(
    actions: Res<Actions>,
    session: Res<OxrSession>,
    mut attach: MessageWriter<OxrAttachActionSet>,
    mut recenter: ResMut<Recenter>,
    mut commands: Commands,
) {
    attach.write(OxrAttachActionSet(actions.set.clone()));
    let head = session
        .create_reference_space(ReferenceSpaceType::VIEW, Isometry3d::IDENTITY)
        .expect("every runtime has a view space");
    let aim = session
        .create_action_space(&actions.aim, Path::NULL, Isometry3d::IDENTITY)
        .expect("aim is a pose action");
    commands.spawn((head.0, Head));
    commands.spawn((aim, LeftAim));
    recenter.0 = true;
}

fn show_eye<const ACTIVE: bool>(mut eye: Single<&mut Camera, With<Eye>>) {
    eye.is_active = ACTIVE;
}

fn sync_actions(actions: Res<Actions>, mut sync: MessageWriter<OxrSyncActionSet>) {
    sync.write(OxrSyncActionSet(actions.set.clone()));
}

/// Gives a headset camera the same sky and transmission as the desktop camera.
fn camera_added(
    added: On<Add<XrCamera>>,
    status: Res<Status>,
    gradient: Res<Gradient>,
    mut commands: Commands,
) {
    let mut camera = commands.entity(added.entity);
    camera.insert(ScreenSpaceTransmission::default());
    if let Some(current) = &status.current {
        sky::show(&mut camera, current.manifest.sky, &gradient);
    }
}

fn arrive(
    mut arrived: MessageReader<Arrived>,
    cameras: Query<Entity, With<XrCamera>>,
    gradient: Res<Gradient>,
    mut recenter: ResMut<Recenter>,
    mut commands: Commands,
) {
    let Some(arrived) = arrived.read().last() else {
        return;
    };
    for camera in &cameras {
        sky::show(
            &mut commands.entity(camera),
            arrived.manifest.sky,
            &gradient,
        );
    }
    // A crossing carries the whole floor through the Portal, so the head already lands right.
    recenter.0 = arrived.crossing.is_none();
}

fn recenter(
    mut recenter: ResMut<Recenter>,
    head: Single<(&Transform, &OxrSpaceLocationFlags), With<Head>>,
    mut player: Single<(&mut Transform, &mut Player), Without<Head>>,
) {
    let (head, flags) = *head;
    if !recenter.0 || !flags.pos_valid() || !flags.rot_valid() {
        return;
    }
    let (transform, player) = &mut *player;
    let yaw = player.yaw() - yaw_of(head.rotation);
    let feet = feet_under(player.feet(), head.translation, yaw);
    player.teleport(transform, feet, yaw);
    recenter.0 = false;
}

fn turn(
    actions: Res<Actions>,
    session: Res<OxrSession>,
    mut armed: ResMut<SnapTurn>,
    head: Single<&Transform, With<Head>>,
    mut player: Single<(&mut Transform, &mut Player), Without<Head>>,
) {
    let Ok(stick) = actions.turn.state(&session, Path::NULL) else {
        return;
    };
    let x = stick.current_state.x;
    if x.abs() < RELEASE {
        armed.0 = true;
    }
    if !armed.0 || x.abs() < PUSH {
        return;
    }
    armed.0 = false;
    let (transform, player) = &mut *player;
    let spot = player.feet() + Quat::from_rotation_y(player.yaw()) * flat(head.translation);
    let yaw = player.yaw() - SNAP * x.signum();
    player.teleport(transform, feet_under(spot, head.translation, yaw), yaw);
}

fn walk(
    actions: Res<Actions>,
    session: Res<OxrSession>,
    aim: Single<&GlobalTransform, With<LeftAim>>,
    mut player: Single<&mut Player>,
) {
    let Ok(stick) = actions.walk.state(&session, Path::NULL) else {
        return;
    };
    let Vector2f { x, y } = stick.current_state;
    let forward = flat(*aim.forward()).normalize_or_zero();
    let right = Vec3::new(-forward.z, 0.0, forward.x);
    player.wish((forward * y + right * x).clamp_length_max(1.0));
}

fn flat(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z)
}

/// Radians about the vertical axis, 0 facing -Z, as `Player::yaw`.
fn yaw_of(rotation: Quat) -> f32 {
    let forward = rotation * Vec3::NEG_Z;
    (-forward.x).atan2(-forward.z)
}

/// The feet that put the head over `spot`, with the player turned to `yaw`. `head` is the
/// head's position on the headset's floor, which turns with the player about the feet.
fn feet_under(spot: Vec3, head: Vec3, yaw: f32) -> Vec3 {
    spot - Quat::from_rotation_y(yaw) * flat(head)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn head_in_world(feet: Vec3, head: Vec3, yaw: f32) -> Vec3 {
        feet + Quat::from_rotation_y(yaw) * head
    }

    #[test]
    fn recentering_puts_the_head_over_the_arrival_facing_its_yaw() {
        let head = Transform::from_xyz(0.4, 1.7, -0.3).with_rotation(Quat::from_rotation_y(0.5));
        let (arrival, arrival_yaw) = (Vec3::new(3.0, 1.0, -2.0), 1.2);
        let yaw = arrival_yaw - yaw_of(head.rotation);
        let feet = feet_under(arrival, head.translation, yaw);
        let landed = head_in_world(feet, head.translation, yaw);
        assert!(landed.xz().distance(arrival.xz()) < 1e-5);
        assert_eq!(feet.y, arrival.y);
        assert!((yaw + yaw_of(head.rotation) - arrival_yaw).abs() < 1e-5);
    }

    #[test]
    fn a_snap_turn_keeps_the_head_where_it_is() {
        let (feet, head, yaw) = (Vec3::new(1.0, 0.0, 2.0), Vec3::new(0.5, 1.6, 0.2), 0.3);
        let before = head_in_world(feet, head, yaw);
        let spot = feet + Quat::from_rotation_y(yaw) * flat(head);
        let turned = yaw - SNAP;
        let after = head_in_world(feet_under(spot, head, turned), head, turned);
        assert!(before.distance(after) < 1e-5);
    }
}
