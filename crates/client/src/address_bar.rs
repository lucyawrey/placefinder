use bevy::input_focus::{FocusCause, InputFocus};
use bevy::prelude::*;
use bevy::text::{EditableText, LineBreak, TextCursorStyle, TextEdit};
use bevy::ui_widgets::{Activate, Button, SelectAllOnFocus, TextInput};
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

use crate::places::{Status, Travel};

/// Temporary: a minimal address bar until the Placefinder UI system exists.
pub struct AddressBarPlugin;

impl Plugin for AddressBarPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn)
            .add_systems(Update, (show_status, spin, submit, grab_cursor))
            .add_observer(go);
    }
}

const HEIGHT: f32 = 36.0;

#[derive(Component)]
struct Field;

#[derive(Component)]
struct Go;

#[derive(Component)]
struct StatusText;

/// Spins while a Place loads.
#[derive(Component)]
struct Spinner;

fn spawn(mut commands: Commands) {
    commands.spawn((
        Node {
            width: percent(100),
            height: px(HEIGHT),
            column_gap: px(8),
            padding: UiRect::all(px(6)),
            ..default()
        },
        BackgroundColor(Color::srgb(0.12, 0.12, 0.14)),
        children![
            (
                Field,
                TextInput,
                SelectAllOnFocus,
                TextCursorStyle {
                    color: Color::srgb(0.9, 0.9, 0.95),
                    ..default()
                },
                TextLayout {
                    linebreak: LineBreak::NoWrap,
                    ..default()
                },
                Node {
                    flex_grow: 1.0,
                    overflow: Overflow::clip(),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.2, 0.2, 0.24))
            ),
            (
                Spinner,
                Node {
                    width: px(14),
                    height: px(14),
                    align_self: AlignSelf::Center,
                    border: UiRect::all(px(3)),
                    ..default()
                },
                BorderColor::all(Color::srgb(0.4, 0.6, 1.0)),
                Visibility::Hidden
            ),
            (
                Go,
                Button,
                Node {
                    padding: UiRect::horizontal(px(10)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.25, 0.35, 0.6)),
                children![Text::new(place::Text::go_button.get())]
            ),
            (
                StatusText,
                Text::default(),
                TextColor(Color::srgb(1.0, 0.6, 0.5)),
                Node {
                    position_type: PositionType::Absolute,
                    top: px(HEIGHT + 4.0),
                    left: px(8),
                    ..default()
                }
            ),
        ],
    ));
}

fn show_status(
    status: Res<Status>,
    focus: Res<InputFocus>,
    mut field: Single<(Entity, &mut EditableText), With<Field>>,
    mut text: Single<&mut Text, With<StatusText>>,
    mut spinner: Single<&mut Visibility, With<Spinner>>,
) {
    if !status.is_changed() {
        return;
    }
    let (entity, editable) = &mut *field;
    // Before any Place has loaded, the address being loaded stays shown, even after it fails.
    let shown = match &status.current {
        Some(current) => Some(current.address.to_string()),
        None => status.loading.clone(),
    };
    if let (Some(address), false) = (shown, focus.get() == Some(*entity)) {
        editable.queue_edit(TextEdit::SelectAll);
        editable.queue_edit(TextEdit::Insert(address.into()));
    }
    **spinner = if status.loading.is_some() {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    text.0 = match &status.error {
        Some(reason) => place::Text::load_failed.fill(&[("reason", reason)]),
        None => String::new(),
    };
}

fn spin(time: Res<Time>, mut spinner: Single<&mut UiTransform, With<Spinner>>) {
    spinner.rotation = Rot2::radians(time.elapsed_secs() * 6.0);
}

fn submit(
    keys: Res<ButtonInput<KeyCode>>,
    focus: Res<InputFocus>,
    field: Single<(Entity, &EditableText), With<Field>>,
    mut travel: MessageWriter<Travel>,
) {
    let (entity, text) = *field;
    if keys.just_pressed(KeyCode::Enter) && focus.get() == Some(entity) {
        travel.write(Travel {
            target: text.value().to_string(),
            from_portal: false,
        });
    }
}

fn go(
    event: On<Activate>,
    buttons: Query<(), With<Go>>,
    field: Single<&EditableText, With<Field>>,
    mut travel: MessageWriter<Travel>,
) {
    if buttons.contains(event.entity) {
        travel.write(Travel {
            target: field.value().to_string(),
            from_portal: false,
        });
    }
}

/// In a Place whose manifest asks for it, clicking the scene captures the mouse for looking
/// around; clicking the bar or pressing Escape releases it, so typing doesn't move the player.
fn grab_cursor(
    status: Res<Status>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut focus: ResMut<InputFocus>,
    field: Single<Entity, With<Field>>,
    window: Single<(&Window, &mut CursorOptions), With<PrimaryWindow>>,
) {
    let (window, mut cursor) = window.into_inner();
    let in_bar = window.cursor_position().is_some_and(|p| p.y < HEIGHT);
    let clicked_bar = mouse.just_pressed(MouseButton::Left) && in_bar;
    let allowed = status
        .current
        .as_ref()
        .is_some_and(|c| c.manifest.capture_mouse);
    let disallowed = !allowed && cursor.grab_mode != CursorGrabMode::None;
    if keys.just_pressed(KeyCode::Escape) || clicked_bar || disallowed {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
        if clicked_bar {
            focus.set(*field, FocusCause::Pressed);
        }
    } else if allowed
        && mouse.just_pressed(MouseButton::Left)
        && cursor.grab_mode == CursorGrabMode::None
    {
        focus.clear();
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
}
