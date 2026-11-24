use std::collections::HashMap;
use std::io::ErrorKind;
use std::path::Path;

use bevy::log::error;
use check_core::modules::MemFiles;
use place::Address;
use serde::Deserialize;

pub const OFFICIAL_HOME: &str = "place://lucyawrey.github.io/placefinder";

const CLASS: &str = include_str!("settings.check");

#[derive(Deserialize)]
struct Settings {
    home: Option<String>,
}

/// The address of the home Place set in `settings.con` in the OS's config folder, or the
/// official home if none is set or the file is broken.
pub fn home() -> String {
    match crate::folders::config() {
        Some(folder) => home_in(&folder.join("settings.con")),
        None => OFFICIAL_HOME.into(),
    }
}

fn home_in(path: &Path) -> String {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == ErrorKind::NotFound => return OFFICIAL_HOME.into(),
        Err(e) => {
            error!("couldn't read {}: {e}", path.display());
            return OFFICIAL_HOME.into();
        }
    };
    let files = MemFiles(HashMap::from([(
        "settings/settings.check".into(),
        CLASS.into(),
    )]));
    let settings = check_serde::from_str_with_class::<Settings>(
        &files,
        "settings/",
        "settings",
        &path.display().to_string(),
        &text,
        "settings.Settings",
    );
    let home = match settings {
        Ok((settings, _)) => settings.home,
        Err(e) => {
            error!("invalid {}: {e}", path.display());
            None
        }
    };
    let Some(home) = home else {
        return OFFICIAL_HOME.into();
    };
    match Address::parse(&home) {
        Ok(address) => address.to_string(),
        Err(e) => {
            error!("invalid home address in {}: {e}", path.display());
            OFFICIAL_HOME.into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home_from(name: &str, text: Option<&str>) -> String {
        let dir = std::env::temp_dir().join(format!("placefinder-settings-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.con");
        if let Some(text) = text {
            std::fs::write(&path, text).unwrap();
        }
        home_in(&path)
    }

    #[test]
    fn without_a_file_home_is_the_official_one() {
        assert_eq!(home_from("missing", None), OFFICIAL_HOME);
        assert_eq!(home_from("empty", Some("")), OFFICIAL_HOME);
    }

    #[test]
    fn a_set_home_is_used() {
        let home = home_from("set", Some("home = \"place://a.com/x\""));
        assert_eq!(home, "place://a.com/x");
    }

    #[test]
    fn a_broken_file_or_address_falls_back_to_the_official_home() {
        assert_eq!(home_from("invalid", Some("home = 3")), OFFICIAL_HOME);
        assert_eq!(
            home_from("address", Some("home = \"place://\"")),
            OFFICIAL_HOME
        );
    }
}
