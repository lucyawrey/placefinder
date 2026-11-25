pub mod address;
pub mod copy;
pub mod fetch;
pub mod manifest;

pub use address::{Address, Base};
pub use copy::Text;
pub use manifest::{Manifest, Sky};

use fetch::FetchError;

/// The manifest file inside every Place.
pub const MANIFEST: &str = "place.con";
/// Read when a Place's folder has no `MANIFEST`.
pub const MANIFEST_JSON: &str = "place.json";

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
            LoadError::NotText => f.write_str(Text::manifest_not_text.get()),
            LoadError::NotAPlace(name) => f.write_str(&Text::not_a_place.fill(&[("name", name)])),
        }
    }
}

/// Fetches and reads a Place's manifest: the one the address names, else `place.con`, else
/// `place.json`.
pub fn load(address: &Address) -> Result<(Base, Manifest), LoadError> {
    let base = address.base();
    let (name, fetched) = match address.manifest() {
        Some(name) => (name, base.fetch(name)),
        None => match base.fetch(MANIFEST) {
            Err(FetchError::NotFound(missing)) => {
                let json = base.fetch(MANIFEST_JSON).map_err(|e| match e {
                    FetchError::NotFound(_) => FetchError::NotFound(missing),
                    e => e,
                });
                (MANIFEST_JSON, json)
            }
            fetched => (MANIFEST, fetched),
        },
    };
    let bytes = fetched.map_err(LoadError::Fetch)?;
    let text = String::from_utf8(bytes).map_err(|_| LoadError::NotText)?;
    let (manifest, marked) = manifest::read(name, &text).map_err(LoadError::Manifest)?;
    let stem = address::manifest_stem(name);
    if !(marked || stem.is_some_and(|s| s == "place" || s.ends_with(".place"))) {
        return Err(LoadError::NotAPlace(name.into()));
    }
    Ok((base, manifest))
}
