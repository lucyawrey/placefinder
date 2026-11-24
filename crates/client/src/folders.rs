use std::path::PathBuf;

pub fn logs() -> Option<PathBuf> {
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

pub fn config() -> Option<PathBuf> {
    if cfg!(windows) {
        Some(env_folder("APPDATA")?.join("Placefinder"))
    } else if cfg!(target_os = "macos") {
        Some(env_folder("HOME")?.join("Library/Application Support/Placefinder"))
    } else {
        let config =
            env_folder("XDG_CONFIG_HOME").or_else(|| Some(env_folder("HOME")?.join(".config")))?;
        Some(config.join("placefinder"))
    }
}

/// The folder an environment variable names, if it's an absolute path.
fn env_folder(name: &str) -> Option<PathBuf> {
    let path = PathBuf::from(std::env::var_os(name)?);
    path.is_absolute().then_some(path)
}
