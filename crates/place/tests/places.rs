use std::path::{Path, PathBuf};

use place::fetch::FetchError;
use place::manifest::ManifestError;
use place::{Address, LoadError, load};

fn example(name: &str) -> Address {
    Address::folder(
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
    Address::folder(dir)
}

#[test]
fn every_example_portal_leads_to_a_place_that_loads() {
    let materials = [
        "anisotropy",
        "clearcoat",
        "emissive-strength",
        "ior",
        "lights-punctual",
        "specular",
        "texture-transform",
        "transmission",
        "unlit",
        "volume",
    ]
    .map(|m| format!("materials/{m}"));
    for name in ["", "meadow", "collision-test"]
        .into_iter()
        .chain(materials.iter().map(String::as_str))
    {
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

/// A manifest with the given version and otherwise the least it needs.
fn versioned(version: &str) -> String {
    format!("version = {version}\nname = \"A\"\nscene = \"s.gltf\"\nspawn = (center = (0.0, 0.0, 0.0))")
}

#[test]
fn refuses_other_manifest_versions_but_reads_any_patch() {
    for version in ["\"0.2.0\"", "\"1.1.0\"", "0"] {
        let address = temp_place("version", &[("place.con", &versioned(version))]);
        assert!(
            matches!(load(&address), Err(LoadError::Manifest(ManifestError::Version(_)))),
            "{version}"
        );
    }
    let address = temp_place("patch", &[("place.con", &versioned("\"0.1.7\""))]);
    assert!(load(&address).is_ok());
}

#[test]
fn refuses_two_portals_with_one_name() {
    let portal = "(target = \"b\", center = (0.0, 1.0, 0.0), size = (1.0, 2.0, 0.3), name = \"gate\")";
    let manifest = format!("{}\nportals = [{portal}, {portal}]", versioned("\"0.1.0\""));
    let address = temp_place("duplicate", &[("place.con", &manifest)]);
    assert!(matches!(
        load(&address),
        Err(LoadError::Manifest(ManifestError::DuplicatePortal(name))) if name == "gate"
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
    let fields = "version = \"0.1.0\", name = \"A\", scene = \"s.gltf\", spawn = (center = (0.0, 0.0, 0.0))";
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

#[test]
fn reads_labels_and_portal_signs() {
    let manifest = r#"version = "0.1.0"
name = "A"
scene = "s.gltf"
spawn = (center = (0.0, 0.0, 0.0))
portals = [
    (target = "b", center = (0.0, 1.0, 0.0), size = (1.0, 2.0, 0.3), label = "To B")
    (target = "c", center = (2.0, 1.0, 0.0), size = (1.0, 2.0, 0.3), show_label = false)
]
labels = [(text = "Hello", center = (0.0, 2.0, 0.0), yaw = 90.0)]"#;
    let (_, place) = load(&temp_place("labels", &[("place.con", manifest)])).unwrap();
    assert_eq!(place.portals[0].label.as_deref(), Some("To B"));
    assert!(place.portals[0].show_label);
    assert_eq!(place.portals[1].label, None);
    assert!(!place.portals[1].show_label);
    assert_eq!(place.labels[0].text, "Hello");
    assert_eq!((place.labels[0].yaw, place.labels[0].height), (90.0, 0.25));
}
