use bevy::camera::{ImageRenderTarget, RenderTarget};
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;

use crate::places::Arrived;

/// Draws `WorldText` as flat signs in the world, and spawns each Place's `labels`.
pub struct LabelsPlugin;

impl Plugin for LabelsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SignFont>()
            .add_systems(Update, (spawn_labels, draw, start_drawing).chain())
            .add_observer(clean_up);
    }
}

/// Text on a flat sign whose front faces +Z.
#[derive(Component)]
#[require(Transform, Visibility)]
pub struct WorldText {
    pub text: String,
    /// Line height, in metres.
    pub height: f32,
    pub color: Color,
    /// The panel behind the text; `None` for no panel.
    pub background: Option<Color>,
    pub two_sided: bool,
}

impl WorldText {
    /// The look the code-free labels use.
    pub fn sign(text: impl Into<String>, height: f32) -> Self {
        WorldText {
            text: text.into(),
            height,
            color: Color::WHITE,
            background: Some(Color::srgba(0.0, 0.0, 0.0, 0.6)),
            two_sided: true,
        }
    }
}

/// The full Fira Mono, since Bevy's bundled copy only has ASCII.
#[derive(Resource)]
struct SignFont(Handle<Font>);

impl FromWorld for SignFont {
    fn from_world(world: &mut World) -> Self {
        let font = Font::from_bytes(include_bytes!("../fonts/FiraMono-Regular.ttf").to_vec());
        SignFont(world.resource_mut::<Assets<Font>>().add(font))
    }
}

/// A mesh showing a `WorldText`, which walking collision skips.
#[derive(Component)]
pub struct TextQuad;

const FONT_PX: f32 = 48.0;
const PADDING_PX: f32 = 8.0;
/// Bevy's default line height, relative to the font size.
const LINE_PER_FONT: f32 = 1.2;
const MAX_TEXTURE_WIDTH: f32 = 4096.0;

/// A sign's full height for a given line height, panel padding included.
pub fn sign_height(line_height: f32) -> f32 {
    line_height * (FONT_PX * LINE_PER_FONT + 2.0 * PADDING_PX) / (FONT_PX * LINE_PER_FONT)
}

/// The camera, UI and meshes drawing one `WorldText` into its texture. The camera renders every
/// frame: one that renders only after a change can run before Bevy has the text's shaders and
/// glyphs ready, leaving the sign blank.
#[derive(Component)]
struct Drawing {
    image: Handle<Image>,
    material: Handle<StandardMaterial>,
    camera: Entity,
    panel: Entity,
    text: Entity,
    quads: Vec<Entity>,
    /// Texture pixels per layout pixel, below 1 only for text wider than the texture limit.
    scale: f32,
    /// Waiting for the UI layout to measure the text, which happens after the frame it changed.
    measuring: bool,
}

fn spawn_labels(mut commands: Commands, mut arrived: MessageReader<Arrived>) {
    for arrived in arrived.read() {
        for label in &arrived.manifest.labels {
            commands.entity(arrived.root).with_child((
                WorldText::sign(&label.text, label.height as f32),
                Transform::from_translation(Vec3::from_array(label.center.map(|v| v as f32)))
                    .with_rotation(Quat::from_rotation_y((label.yaw as f32).to_radians())),
            ));
        }
    }
}

fn start_drawing(
    mut commands: Commands,
    mut changed: Query<(Entity, &WorldText, Option<&mut Drawing>), Changed<WorldText>>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    font: Res<SignFont>,
) {
    for (entity, world_text, drawing) in &mut changed {
        let background = BackgroundColor(world_text.background.unwrap_or(Color::NONE));
        let text = (
            Text::new(world_text.text.clone()),
            TextColor(world_text.color),
        );
        if let Some(mut drawing) = drawing {
            commands.entity(drawing.panel).insert(background);
            commands.entity(drawing.text).insert(text);
            drawing.measuring = true;
            continue;
        }
        let image = images.add(Image::new_target_texture(
            1,
            1,
            TextureFormat::Bgra8UnormSrgb,
            None,
        ));
        let camera = commands
            .spawn((
                Camera2d,
                Camera {
                    order: -1,
                    clear_color: ClearColorConfig::Custom(Color::NONE),
                    ..default()
                },
                RenderTarget::Image(image.clone().into()),
            ))
            .id();
        let text = commands
            .spawn((
                text,
                TextFont::from_font_size(FONT_PX).with_font(font.0.clone()),
                TextLayout::no_wrap(),
            ))
            .id();
        let panel = commands
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    padding: UiRect::all(px(PADDING_PX)),
                    ..default()
                },
                background,
                UiTargetCamera(camera),
            ))
            .add_child(text)
            .id();
        let material = materials.add(StandardMaterial {
            base_color_texture: Some(image.clone()),
            unlit: true,
            // The UI blends onto a transparent clear, which leaves premultiplied colour.
            alpha_mode: AlphaMode::Premultiplied,
            ..default()
        });
        let sides: &[Quat] = if world_text.two_sided {
            // A back-facing copy, since one double-sided quad shows mirrored text from behind.
            &[Quat::IDENTITY, Quat::from_rotation_y(std::f32::consts::PI)]
        } else {
            &[Quat::IDENTITY]
        };
        let quads = sides
            .iter()
            .map(|&rotation| {
                commands
                    .spawn((
                        TextQuad,
                        Mesh3d(meshes.add(Rectangle::new(0.0, 0.0))),
                        MeshMaterial3d(material.clone()),
                        Transform::from_rotation(rotation),
                        ChildOf(entity),
                    ))
                    .id()
            })
            .collect();
        commands.entity(entity).insert(Drawing {
            image,
            material,
            camera,
            panel,
            text,
            quads,
            scale: 1.0,
            measuring: true,
        });
    }
}

fn draw(
    mut commands: Commands,
    mut drawings: Query<(&WorldText, &mut Drawing)>,
    nodes: Query<&ComputedNode>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (world_text, mut drawing) in &mut drawings {
        if !drawing.measuring {
            continue;
        }
        let (Ok(panel), Ok(text)) = (nodes.get(drawing.panel), nodes.get(drawing.text)) else {
            continue;
        };
        if text.size.y == 0.0 {
            continue;
        }
        let panel_size = panel.size / drawing.scale;
        let metres_per_px = world_text.height / (text.size.y / drawing.scale);
        let scale = (MAX_TEXTURE_WIDTH / panel_size.x).min(1.0);
        let pixels = (panel_size * scale).ceil().as_uvec2();
        // A new image rather than a resized one, which the material keeps sampling empty.
        drawing.image = images.add(Image::new_target_texture(
            pixels.x,
            pixels.y,
            TextureFormat::Bgra8UnormSrgb,
            None,
        ));
        materials
            .get_mut(&drawing.material)
            .unwrap()
            .base_color_texture = Some(drawing.image.clone());
        commands
            .entity(drawing.camera)
            .insert(RenderTarget::Image(ImageRenderTarget {
                handle: drawing.image.clone(),
                scale_factor: scale,
            }));
        let mesh = meshes.add(Rectangle::from_size(panel_size * metres_per_px));
        for &quad in &drawing.quads {
            commands.entity(quad).insert(Mesh3d(mesh.clone()));
        }
        drawing.scale = scale;
        drawing.measuring = false;
    }
}

/// The camera and UI aren't in the sign's hierarchy, so they go when it does.
fn clean_up(remove: On<Remove<Drawing>>, drawings: Query<&Drawing>, mut commands: Commands) {
    let drawing = drawings.get(remove.entity).unwrap();
    commands.entity(drawing.camera).despawn();
    commands.entity(drawing.panel).despawn();
}
