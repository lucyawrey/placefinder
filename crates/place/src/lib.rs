pub mod address;
pub mod fetch;
pub mod manifest;

pub use address::{Address, Base};
pub use manifest::Manifest;

/// The manifest file inside every Place.
pub const MANIFEST: &str = "place.con";

#[derive(Debug)]
pub enum LoadError {
    Fetch(fetch::FetchError),
    Manifest(manifest::ManifestError),
    NotText,
    /// A named manifest that isn't marked as a Place.
    NotAPlace(String),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            LoadError::Fetch(e) => write!(f, "{e}"),
            LoadError::Manifest(e) => write!(f, "{e}"),
            LoadError::NotText => write!(f, "[a manifest that isn't UTF-8 text]"),
            LoadError::NotAPlace(name) => write!(
                f,
                "[a named manifest that isn't marked as a Place, and how to mark it: {name}]"
            ),
        }
    }
}

/// Fetches and reads a Place's manifest.
pub fn load(address: &Address) -> Result<(Base, Manifest), LoadError> {
    let base = address.base();
    let name = address.manifest();
    let bytes = base.fetch(name).map_err(LoadError::Fetch)?;
    let text = String::from_utf8(bytes).map_err(|_| LoadError::NotText)?;
    let (manifest, marked) = manifest::read(name, &text).map_err(LoadError::Manifest)?;
    if !(marked || name == MANIFEST || name.ends_with(".place.con")) {
        return Err(LoadError::NotAPlace(name.into()));
    }
    Ok((base, manifest))
}
