use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use bevy::asset::io::{
    AssetReader, AssetReaderError, AssetSourceBuilder, PathStream, Reader, VecReader,
};
use bevy::prelude::*;
use place::Base;
use place::fetch::FetchError;

/// The internal asset source every Place file loads through; only Bevy sees it.
pub const SOURCE: &str = "placeload";

/// Each visit to a Place gets a number, and its asset paths start with it:
/// `placeload://3/scene.gltf` reads `scene.gltf` from Place 3's base.
#[derive(Resource, Clone, Default)]
pub struct Bases(Arc<RwLock<HashMap<u32, Base>>>);

impl Bases {
    pub fn insert(&self, number: u32, base: Base) {
        self.0.write().unwrap().insert(number, base);
    }

    pub fn remove(&self, number: u32) {
        self.0.write().unwrap().remove(&number);
    }
}

/// Registers the source. Must run before `AssetPlugin`, so before `DefaultPlugins`.
pub fn register(app: &mut App) {
    let bases = Bases::default();
    let reader_bases = bases.clone();
    app.insert_resource(bases).register_asset_source(
        SOURCE,
        AssetSourceBuilder::new(move || Box::new(PlaceReader(reader_bases.clone()))),
    );
}

struct PlaceReader(Bases);

impl PlaceReader {
    fn fetch(&self, path: &Path) -> Result<Vec<u8>, AssetReaderError> {
        let not_found = || AssetReaderError::NotFound(path.to_path_buf());
        let mut parts = path.components().map(|c| c.as_os_str().to_str());
        let number: u32 = parts
            .next()
            .flatten()
            .and_then(|n| n.parse().ok())
            .ok_or_else(not_found)?;
        let rest = parts
            .collect::<Option<Vec<_>>>()
            .ok_or_else(not_found)?
            .join("/");
        let base = self
            .0
            .0
            .read()
            .unwrap()
            .get(&number)
            .cloned()
            .ok_or_else(not_found)?;
        // Blocks one of Bevy's I/O threads while it reads; fine for the prototype.
        base.fetch(&rest).map_err(|e| match e {
            FetchError::NotFound(_) | FetchError::OutsidePlace(_) => not_found(),
            FetchError::Failed(e) => AssetReaderError::Io(Arc::new(std::io::Error::other(e))),
        })
    }
}

impl AssetReader for PlaceReader {
    async fn read<'a>(&'a self, path: &'a Path) -> Result<Box<dyn Reader>, AssetReaderError> {
        Ok(Box::new(VecReader::new(self.fetch(path)?)))
    }

    async fn read_meta<'a>(&'a self, path: &'a Path) -> Result<Box<dyn Reader>, AssetReaderError> {
        Err(AssetReaderError::NotFound(path.to_path_buf()))
    }

    async fn read_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Result<Box<PathStream>, AssetReaderError> {
        Err(AssetReaderError::NotFound(PathBuf::from(path)))
    }

    async fn is_directory<'a>(&'a self, _path: &'a Path) -> Result<bool, AssetReaderError> {
        Ok(false)
    }
}
