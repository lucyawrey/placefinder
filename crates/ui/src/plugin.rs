use std::collections::HashMap;

use accesskit::{Node as Access, Role};
use bevy::a11y::AccessibilityNode;
use bevy::input_focus::tab_navigation::{TabGroup, TabIndex};
use bevy::prelude::*;
use bevy::text::{EditableText, FontSize};
use bevy::ui::{Pressed, UiSystems};
use bevy::ui_widgets::{Activate, Button as Pressable, TextInput as Editable};

use crate::element::{self, Align, Direction, Element, Justify, Kind, Length, Space};
use crate::tree::{Id, Tree};

/// Draws each `UiRoot`'s tree with Bevy UI, redrawing only what changed.
pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<FollowLink>()
            .add_message::<Activated>()
            .add_systems(
                PostUpdate,
                (start, draw, show_pressed)
                    .chain()
                    .before(UiSystems::Prepare),
            )
            .add_observer(activate);
    }
}

/// A UI tree, drawn inside this entity's `Node`, which places it.
#[derive(Component)]
pub struct UiRoot {
    pub tree: Tree,
    /// What images' `src` is joined to for loading: an asset path ending in `/`.
    pub assets: String,
}

/// A link was followed. The client decides what its target means.
#[derive(Message)]
pub struct FollowLink {
    pub target: String,
}

/// A pressable box was pressed.
#[derive(Message)]
pub struct Activated {
    pub root: Entity,
    pub id: Id,
}

#[derive(Component, Default)]
struct Drawn(HashMap<Id, Entity>);

#[derive(Component)]
struct Draws {
    root: Entity,
    id: Id,
}

fn start(new: Query<Entity, Added<UiRoot>>, mut commands: Commands) {
    for root in &new {
        commands.entity(root).insert((
            Drawn::default(),
            TabGroup::new(0),
            BackgroundColor(color(element::Color::Background)),
        ));
    }
}

fn draw(
    mut roots: Query<(Entity, &mut UiRoot, &mut Drawn)>,
    assets: Res<AssetServer>,
    mut commands: Commands,
) {
    for (root, mut ui, mut drawn) in &mut roots {
        let ui = &mut *ui;
        let changes = ui.tree.take_changes();
        for id in changes.removed {
            if let Some(entity) = drawn.0.remove(&id) {
                commands.entity(entity).try_despawn();
            }
        }
        for &id in &changes.updated {
            let entity = *drawn
                .0
                .entry(id)
                .or_insert_with(|| commands.spawn(Draws { root, id }).id());
            let image = &ui.tree.element(id).src;
            let image = (!image.is_empty()).then(|| assets.load(format!("{}{image}", ui.assets)));
            draw_element(&mut commands.entity(entity), &ui.tree, id, image);
        }
        for parent in changes.reordered {
            let children: Vec<Entity> = ui
                .tree
                .children(parent)
                .iter()
                .map(|c| drawn.0[c])
                .collect();
            commands
                .entity(drawn.0[&parent])
                .replace_children(&children);
        }
        let top = ui.tree.root();
        if changes.updated.contains(&top) {
            commands.entity(root).add_child(drawn.0[&top]);
        }
    }
}

fn draw_element(entity: &mut EntityCommands, tree: &Tree, id: Id, image: Option<Handle<Image>>) {
    let element = tree.element(id);
    let kind = element.kind;
    let font = TextFont {
        font_size: FontSize::Px(tree.font_size(id).px()),
        ..default()
    };
    let words = (font, TextColor(color(text_color(tree, id))));
    let inline = tree
        .parent(id)
        .is_some_and(|p| tree.element(p).kind == Kind::Text);
    if inline {
        entity.insert(TextSpan::new(element.text.clone()));
    } else {
        entity.insert((
            node(element),
            BackgroundColor(background(element)),
            BorderColor::all(element.border_color.map_or(Color::NONE, color)),
        ));
        if matches!(kind, Kind::Text | Kind::Button | Kind::Link) {
            entity.insert(Text::new(element.text.clone()));
        }
    }
    if inline
        || matches!(
            kind,
            Kind::Text | Kind::Button | Kind::Link | Kind::TextInput
        )
    {
        entity.insert(words);
    }
    match kind {
        Kind::TextInput => {
            entity.insert((Editable, EditableText::new(&element.value)));
        }
        Kind::Image => {
            entity.insert(ImageNode::new(image.unwrap_or_default()));
        }
        _ => {}
    }
    if let Some(role) = role(element) {
        let mut access = Access::new(role);
        let label = label(tree, id);
        if !label.is_empty() {
            access.set_label(label);
        }
        entity.insert(AccessibilityNode(access));
    }
    if element.pressable() {
        entity.insert(Pressable);
    } else {
        entity.remove::<Pressable>();
    }
    if element.focusable() {
        entity.insert(TabIndex(0));
    } else {
        entity.remove::<TabIndex>();
    }
}

/// `None` leaves the role Bevy gives, as for a text input.
fn role(element: &Element) -> Option<Role> {
    match element.kind {
        Kind::Box if element.pressable() => Some(Role::Button),
        Kind::Box => Some(Role::GenericContainer),
        Kind::Text => Some(Role::Paragraph),
        Kind::Span => None,
        Kind::Button => Some(Role::Button),
        Kind::Link => Some(Role::Link),
        Kind::TextInput => None,
        Kind::Image => Some(Role::Image),
    }
}

/// An element's words, with a `Text`'s spans and links joined.
fn label(tree: &Tree, id: Id) -> String {
    let own = tree.element(id).text.clone();
    tree.children(id)
        .iter()
        .fold(own, |words, &child| words + &tree.element(child).text)
}

fn node(element: &Element) -> Node {
    let space = |s: Option<Space>| px(s.map_or(0.0, Space::px));
    let default_padding = match element.kind {
        Kind::Button | Kind::TextInput => Some(Space::M),
        _ => None,
    };
    let gap = space(element.gap);
    Node {
        flex_direction: match element.direction {
            Some(Direction::Row) => FlexDirection::Row,
            Some(Direction::Column) | None => FlexDirection::Column,
        },
        row_gap: gap,
        column_gap: gap,
        padding: UiRect::all(space(element.padding.or(default_padding))),
        margin: UiRect::all(space(element.margin)),
        width: length(element.width),
        height: length(element.height),
        align_items: match element.align {
            Some(Align::Start) => AlignItems::FlexStart,
            Some(Align::Center) => AlignItems::Center,
            Some(Align::End) => AlignItems::FlexEnd,
            Some(Align::Stretch) | None => AlignItems::Stretch,
        },
        justify_content: match element.justify {
            Some(Justify::Start) | None => JustifyContent::FlexStart,
            Some(Justify::Center) => JustifyContent::Center,
            Some(Justify::End) => JustifyContent::FlexEnd,
            Some(Justify::Between) => JustifyContent::SpaceBetween,
        },
        border: UiRect::all(space(element.border)),
        border_radius: BorderRadius::all(space(element.radius)),
        ..default()
    }
}

fn length(length: Option<Length>) -> Val {
    match length {
        None | Some(Length::Auto) => Val::Auto,
        Some(Length::Fill) => percent(100),
        Some(Length::Px(n)) => px(n),
        Some(Length::Percent(n)) => percent(n),
    }
}

fn background(element: &Element) -> Color {
    let default = match element.kind {
        Kind::Button => Some(element::Color::Accent),
        Kind::TextInput => Some(element::Color::Surface),
        _ => None,
    };
    element.background.or(default).map_or(Color::NONE, color)
}

/// Links are accent-colored unless they set a color.
fn text_color(tree: &Tree, id: Id) -> element::Color {
    match (tree.element(id).kind, tree.element(id).color) {
        (Kind::Link, None) => element::Color::Accent,
        _ => tree.color(id),
    }
}

fn color(token: element::Color) -> Color {
    let [r, g, b, a] = token.rgba();
    Color::srgba(r, g, b, a)
}

/// Darkens what's pressed the frame it's pressed, before what pressing does.
fn show_pressed(
    pressed: Query<Entity, Added<Pressed>>,
    mut released: RemovedComponents<Pressed>,
    roots: Query<&UiRoot>,
    mut drawn: Query<(
        &Draws,
        Has<Pressed>,
        Option<&mut BackgroundColor>,
        &mut TextColor,
    )>,
) {
    for entity in pressed.iter().chain(released.read()) {
        let Ok((draws, is_pressed, background_color, mut words)) = drawn.get_mut(entity) else {
            continue;
        };
        let Ok(ui) = roots.get(draws.root) else {
            continue;
        };
        let element = ui.tree.element(draws.id);
        let shade = |c: Color| if is_pressed { c.darker(0.15) } else { c };
        if let Some(mut background_color) = background_color {
            background_color.0 = shade(background(element));
        }
        words.0 = shade(color(text_color(&ui.tree, draws.id)));
    }
}

fn activate(
    activate: On<Activate>,
    drawn: Query<&Draws>,
    roots: Query<&UiRoot>,
    mut follow: MessageWriter<FollowLink>,
    mut activated: MessageWriter<Activated>,
) {
    let Ok(draws) = drawn.get(activate.entity) else {
        return;
    };
    let Ok(ui) = roots.get(draws.root) else {
        return;
    };
    let element = ui.tree.element(draws.id);
    if element.kind == Kind::Link {
        follow.write(FollowLink {
            target: element.target.clone(),
        });
    } else {
        activated.write(Activated {
            root: draws.root,
            id: draws.id,
        });
    }
}
