use std::collections::HashMap;
use std::fmt;

use check_core::modules::MemFiles;
use serde::{Deserialize, Serialize};

use crate::Text;

/// The manifest version this client reads: manifests with the same `x.y` and any `z`.
pub const VERSION: &str = "0.2.0";

const CLASS: &str = include_str!("place.check");

pub type Point = [f64; 3];

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Manifest {
    pub version: String,
    pub name: String,
    /// A glTF path relative to the Place. A Place has a scene or a `ui`, never both.
    pub scene: Option<String>,
    /// Set exactly when `scene` is.
    pub spawn: Option<Spawn>,
    /// A `.cml` path relative to the Place, which the Place shows instead of a scene.
    pub ui: Option<String>,
    pub portals: Vec<Portal>,
    pub labels: Vec<Label>,
    /// Whether clicking the scene captures the mouse, for looking around and walking.
    pub capture_mouse: bool,
    pub sky: Sky,
}

/// What shows behind a Place's scene and lights it.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum Sky {
    /// A planet's sky, with ground below the horizon.
    Atmosphere,
    /// A sky with no ground or horizon.
    Gradient,
    /// A plain background and no light from the sky.
    None,
}

/// Where arrivals that don't come through a linked Portal start, and where falling respawns.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Spawn {
    pub center: Point,
    /// Degrees about the vertical axis, counter-clockwise from above; at 0 the player faces -Z.
    pub yaw: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Portal {
    pub target: String,
    pub center: Point,
    pub size: Point,
    /// What other Portals' targets link to, as the fragment in `place#name`.
    pub name: Option<String>,
    /// Degrees about the vertical axis, counter-clockwise from above; at 0 the front, which
    /// players walk in through, faces +Z.
    pub yaw: f64,
    /// Replaces the target Place's name on the Portal's sign.
    pub label: Option<String>,
    pub show_label: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Label {
    pub text: String,
    pub center: Point,
    /// Degrees about the vertical axis, counter-clockwise from above; at 0 the text's front
    /// faces +Z.
    pub yaw: f64,
    /// The text's line height, in metres.
    pub height: f64,
}

#[derive(Debug)]
pub enum ManifestError {
    Invalid(String),
    Version(String),
    DuplicatePortal(String),
    SceneAndUi,
    NoSceneOrUi,
    NoSpawn,
    /// A field only a Place with a scene can have.
    NeedsScene(&'static str),
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ManifestError::Invalid(e) => write!(f, "{e}"),
            ManifestError::Version(v) => {
                let slots: [(&str, &dyn fmt::Display); 2] = [("version", v), ("reads", &VERSION)];
                f.write_str(&Text::unknown_manifest_version.fill(&slots))
            }
            ManifestError::DuplicatePortal(name) => {
                f.write_str(&Text::duplicate_portal_name.fill(&[("name", name)]))
            }
            ManifestError::SceneAndUi => f.write_str(Text::scene_and_ui.get()),
            ManifestError::NoSceneOrUi => f.write_str(Text::no_scene_or_ui.get()),
            ManifestError::NoSpawn => f.write_str(Text::no_spawn.get()),
            ManifestError::NeedsScene(field) => {
                f.write_str(&Text::field_needs_scene.fill(&[("field", field)]))
            }
        }
    }
}

/// Also returns whether the file is marked as a Place by writing `Place(...)`.
pub fn read(path: &str, text: &str) -> Result<(Manifest, bool), ManifestError> {
    // Before the checked read, so a newer manifest gets the version message, not a parse error.
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Version {
        Text(String),
        /// Manifests before 0.1.0 had an integer version.
        Number(i64),
    }
    #[derive(Deserialize)]
    struct Versioned {
        version: Version,
    }
    let versioned: Versioned = check_serde::from_str_unchecked(path, text)
        .map_err(|e| ManifestError::Invalid(e.to_string()))?;
    match versioned.version {
        Version::Text(v) if same_minor(&v, VERSION) => {}
        Version::Text(v) => return Err(ManifestError::Version(v)),
        Version::Number(n) => return Err(ManifestError::Version(n.to_string())),
    }
    let files = MemFiles(HashMap::from([("place/place.check".into(), CLASS.into())]));
    let (mut manifest, class): (Manifest, _) =
        check_serde::from_str_with_class(&files, "place/", "place", path, text, "place.Place")
            .map_err(|e| ManifestError::Invalid(e.to_string()))?;
    let names = manifest.portals.iter().filter_map(|p| p.name.as_ref());
    for (i, name) in names.clone().enumerate() {
        if names.clone().take(i).any(|n| n == name) {
            return Err(ManifestError::DuplicatePortal(name.clone()));
        }
    }
    match (&manifest.scene, &manifest.ui) {
        (Some(_), Some(_)) => return Err(ManifestError::SceneAndUi),
        (None, None) => return Err(ManifestError::NoSceneOrUi),
        (Some(_), None) if manifest.spawn.is_none() => return Err(ManifestError::NoSpawn),
        (Some(_), None) => {}
        (None, Some(_)) => {
            let needs_scene = [
                ("spawn", manifest.spawn.is_some()),
                ("portals", !manifest.portals.is_empty()),
                ("labels", !manifest.labels.is_empty()),
            ];
            if let Some((field, _)) = needs_scene.iter().find(|(_, set)| *set) {
                return Err(ManifestError::NeedsScene(field));
            }
            manifest.capture_mouse = false;
        }
    }
    Ok((manifest, class.as_deref() == Some("place.Place")))
}

/// Whether two `x.y.z` versions share `x.y`, the part that changes when manifests break.
fn same_minor(a: &str, b: &str) -> bool {
    let minor = |v: &str| v.rsplit_once('.').map(|(xy, _)| xy.to_string());
    minor(a).is_some() && minor(a) == minor(b)
}
