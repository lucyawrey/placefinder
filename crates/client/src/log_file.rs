use std::fs::{self, File};
use std::sync::Mutex;

use bevy::log::BoxedLayer;
use bevy::log::tracing_subscriber::{Layer, fmt};
use bevy::prelude::*;

use crate::{folders, mcp};

/// For `LogPlugin::custom_layer`: the MCP's log layer, and in release builds a log file.
pub fn layers(app: &mut App) -> Option<BoxedLayer> {
    let file = open().map(|file| fmt::layer().with_ansi(false).with_writer(Mutex::new(file)));
    Some(Box::new(Layer::and_then(mcp::log_layer(app), file)))
}

/// `placefinder.log` in the OS's log folder, emptied each run. None in debug builds, or when
/// the folder can't be found or made.
fn open() -> Option<File> {
    if cfg!(debug_assertions) {
        return None;
    }
    let folder = folders::logs()?;
    fs::create_dir_all(&folder).ok()?;
    File::create(folder.join("placefinder.log")).ok()
}
