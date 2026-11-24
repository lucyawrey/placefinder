use std::path::{Path, PathBuf};

use place::fetch::FetchError;
use place::manifest::ManifestError;
use place::{Address, LoadError, load};

fn example(name: &str) -> Address {
    Address::Folder(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../places")
            .join(name),
    )
}

/// A Place folder in a temp dir holding the given files.
fn temp_place(name: &str, files: &[(&str, &str)]) -> Address {
    let dir: PathBuf = std::env::temp_dir().join(format!("placefinder-test-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (file, text) in files {
        std::fs::write(dir.join(file), text).unwrap();
    }
    Address::Folder(dir)
}

#[test]
fn every_example_portal_leads_to_a_place_that_loads() {
    for name in ["", "meadow", "collision-test"] {
        let place = example(name);
        let (_, manifest) = load(&place).unwrap();
        assert!(!manifest.portals.is_empty(), "{name:?} has no portals");
        for portal in &manifest.portals {
            let target = place.resolve(&portal.target).unwrap();
            assert!(load(&target).is_ok(), "{name:?} → {}", portal.target);
        }
    }
}

#[test]
fn a_missing_manifest_is_not_found() {
    let address = temp_place("empty", &[]);
    assert!(matches!(
        load(&address),
        Err(LoadError::Fetch(FetchError::NotFound(_)))
    ));
}

#[test]
fn an_invalid_manifest_is_refused() {
    let address = temp_place("invalid", &[("place.con", "name = 3")]);
    assert!(matches!(
        load(&address),
        Err(LoadError::Manifest(ManifestError::Invalid(_)))
    ));
}

#[test]
fn an_unknown_manifest_version_is_refused() {
    let manifest = "version = 9\nname = \"A\"\nscene = \"s.gltf\"\nspawn = (0.0, 0.0, 0.0)";
    let address = temp_place("version", &[("place.con", manifest)]);
    assert!(matches!(
        load(&address),
        Err(LoadError::Manifest(ManifestError::Version(9)))
    ));
}

#[test]
fn refuses_files_outside_the_place_on_every_platform() {
    let base = example("").base();
    for path in ["../../x.gltf", "..\\..\\x.gltf", "C:/x.gltf", "a/C:x.gltf"] {
        assert!(
            matches!(base.fetch(path), Err(FetchError::OutsidePlace(_))),
            "{path}"
        );
    }
}

#[test]
fn loads_a_named_manifest_only_if_marked_as_a_place() {
    let fields = "version = 0, name = \"A\", scene = \"s.gltf\", spawn = (0.0, 0.0, 0.0)";
    let (written, plain) = (format!("Place({fields})"), format!("({fields})"));
    let folder = temp_place(
        "named",
        &[
            ("s.gltf", ""),
            ("written.con", &written),
            ("lobby.place.con", &plain),
            ("plain.con", &plain),
        ],
    );
    let named = |file: &str| folder.resolve(file).unwrap();
    assert_eq!(load(&named("written.con")).unwrap().1.name, "A");
    assert_eq!(load(&named("lobby.place.con")).unwrap().1.name, "A");
    assert!(matches!(
        load(&named("plain.con")),
        Err(LoadError::NotAPlace(_))
    ));
}
