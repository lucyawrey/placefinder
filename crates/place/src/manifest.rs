use std::collections::HashMap;
use std::fmt;

use check_core::modules::MemFiles;
use serde::{Deserialize, Serialize};

use crate::Text;

/// The manifest version this client reads.
pub const VERSION: i64 = 0;

const CLASS: &str = include_str!("place.check");

pub type Point = [f64; 3];

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Manifest {
    pub version: i64,
    pub name: String,
    /// A glTF path relative to the Place.
    pub scene: String,
    pub spawn: Point,
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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Portal {
    pub target: String,
    pub center: Point,
    pub size: Point,
    /// Replaces the target Place's name on the Portal's sign.
    pub label: Option<String>,
    pub show_label: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Label {
    pub text: String,
    pub center: Point,
    /// Degrees about the vertical axis; at 0 the text's front faces +Z.
    pub yaw: f64,
    /// The text's line height, in metres.
    pub height: f64,
}

#[derive(Debug)]
pub enum ManifestError {
    Invalid(String),
    Version(i64),
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ManifestError::Invalid(e) => write!(f, "{e}"),
            ManifestError::Version(v) => {
                let slots: [(&str, &dyn fmt::Display); 2] = [("version", v), ("reads", &VERSION)];
                f.write_str(&Text::unknown_manifest_version.fill(&slots))
            }
        }
    }
}

/// Also returns whether the file is marked as a Place by writing `Place(...)`.
pub fn read(path: &str, text: &str) -> Result<(Manifest, bool), ManifestError> {
    let files = MemFiles(HashMap::from([("place/place.check".into(), CLASS.into())]));
    let (manifest, class): (Manifest, _) =
        check_serde::from_str_with_class(&files, "place/", "place", path, text, "place.Place")
            .map_err(|e| ManifestError::Invalid(e.to_string()))?;
    if manifest.version != VERSION {
        return Err(ManifestError::Version(manifest.version));
    }
    Ok((manifest, class.as_deref() == Some("place.Place")))
}
