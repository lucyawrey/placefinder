use std::fs::{self, File};
use std::path::PathBuf;
use std::sync::Mutex;

use bevy::log::BoxedLayer;
use bevy::log::tracing_subscriber::{Layer, fmt};
use bevy::prelude::*;

use crate::mcp;

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
    let folder = folder()?;
    fs::create_dir_all(&folder).ok()?;
    File::create(folder.join("placefinder.log")).ok()
}

fn folder() -> Option<PathBuf> {
    if cfg!(windows) {
        Some(env_folder("LOCALAPPDATA")?.join("Placefinder").join("logs"))
    } else if cfg!(target_os = "macos") {
        Some(env_folder("HOME")?.join("Library/Logs/Placefinder"))
    } else {
        let state = env_folder("XDG_STATE_HOME")
            .or_else(|| Some(env_folder("HOME")?.join(".local/state")))?;
        Some(state.join("placefinder"))
    }
}

/// The folder an environment variable names, if it's an absolute path.
fn env_folder(name: &str) -> Option<PathBuf> {
    let path = PathBuf::from(std::env::var_os(name)?);
    path.is_absolute().then_some(path)
}
