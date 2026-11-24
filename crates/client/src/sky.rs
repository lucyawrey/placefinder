use bevy::asset::RenderAssetUsages;
use bevy::camera::Hdr;
use bevy::light::atmosphere::ScatteringMedium;
use bevy::light::{
    Atmosphere, AtmosphereEnvironmentMapLight, EnvironmentMapLight, GeneratedEnvironmentMapLight,
    Skybox,
};
use bevy::pbr::AtmosphereSettings;
use bevy::prelude::*;
use bevy::render::render_resource::{
    Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension,
};
use place::Sky;

/// The skies a Place's manifest can pick.
pub struct SkyPlugin;

impl Plugin for SkyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup);
    }
}

#[derive(Resource)]
pub struct Gradient(Handle<Image>);

const SIZE: u32 = 64;
/// In cd/m², picked by eye at the camera's default exposure.
const BRIGHTNESS: f32 = 500.0;

fn setup(
    mut commands: Commands,
    mut media: ResMut<Assets<ScatteringMedium>>,
    mut images: ResMut<Assets<Image>>,
) {
    commands.spawn(Atmosphere::earth(
        media.add(ScatteringMedium::earth(256, 256)),
    ));
    commands.insert_resource(Gradient(images.add(gradient())));
}

/// Shows `sky` behind the camera's view and lights the scene with it.
pub fn show(camera: &mut EntityCommands, sky: Sky, gradient: &Gradient) {
    camera.remove::<(
        AtmosphereSettings,
        AtmosphereEnvironmentMapLight,
        GeneratedEnvironmentMapLight,
        EnvironmentMapLight,
        Skybox,
        Hdr,
    )>();
    match sky {
        Sky::Atmosphere => {
            camera.insert((
                AtmosphereSettings::default(),
                AtmosphereEnvironmentMapLight::default(),
            ));
        }
        Sky::Gradient => {
            camera.insert((
                Skybox {
                    image: Some(gradient.0.clone()),
                    brightness: BRIGHTNESS,
                    ..default()
                },
                GeneratedEnvironmentMapLight {
                    environment_map: gradient.0.clone(),
                    intensity: BRIGHTNESS,
                    ..default()
                },
            ));
        }
        Sky::None => {}
    }
}

/// A cubemap that fades from blue overhead to pale at the horizon and below, with no ground.
fn gradient() -> Image {
    let zenith = Vec3::new(0.25, 0.45, 0.8);
    let horizon = Vec3::new(0.75, 0.85, 0.95);
    let nadir = Vec3::new(0.6, 0.65, 0.7);
    let mut data = Vec::with_capacity((SIZE * SIZE * 6 * 4) as usize);
    // Faces in order +X, -X, +Y, -Y, +Z, -Z; on the side faces, up is the top row.
    for face in 0..6 {
        for row in 0..SIZE {
            let v = (row as f32 + 0.5) / SIZE as f32 * 2.0 - 1.0;
            for column in 0..SIZE {
                let u = (column as f32 + 0.5) / SIZE as f32 * 2.0 - 1.0;
                let up = match face {
                    2 => Vec3::new(u, 1.0, v),
                    3 => Vec3::new(u, -1.0, -v),
                    _ => Vec3::new(1.0, -v, u),
                }
                .normalize()
                .y;
                let color = if up >= 0.0 {
                    horizon.lerp(zenith, up.sqrt())
                } else {
                    horizon.lerp(nadir, (-up).sqrt())
                };
                let color = Color::linear_rgb(color.x, color.y, color.z).to_srgba();
                data.extend(color.to_u8_array());
            }
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 6,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::Cube),
        ..default()
    });
    image
}
