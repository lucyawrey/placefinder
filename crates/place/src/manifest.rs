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
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Portal {
    pub target: String,
    pub center: Point,
    pub size: Point,
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
