use std::collections::HashMap;
use std::sync::OnceLock;

const FILE: &str = include_str!("../copy.con");

macro_rules! texts {
    ($($key:ident),* $(,)?) => {
        /// A line of `copy.con`, the interface text the maintainers write.
        #[allow(non_camel_case_types)]
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub enum Text {
            $($key),*
        }

        impl Text {
            #[cfg(test)]
            const ALL: &[Text] = &[$(Text::$key),*];

            fn key(self) -> &'static str {
                match self {
                    $(Text::$key => stringify!($key)),*
                }
            }
        }
    };
}

texts! {
    empty_address,
    address_without_host,
    bad_port,
    above_root,
    bad_path_part,
    file_outside_place,
    file_not_found,
    manifest_not_text,
    not_a_place,
    unknown_manifest_version,
    scene_without_scene,
    go_button,
    load_failed,
    port_too_large,
    mcp_listen_failed,
    mcp_replaced,
    mcp_timed_out,
    mcp_no_view,
    mcp_minimized,
    mcp_closed,
}

impl Text {
    pub fn get(self) -> &'static str {
        &copy()[self.key()]
    }

    /// The text with each `{name}` slot filled with its value.
    pub fn fill(self, slots: &[(&str, &dyn std::fmt::Display)]) -> String {
        slots
            .iter()
            .fold(self.get().to_string(), |text, (name, value)| {
                text.replace(&format!("{{{name}}}"), &value.to_string())
            })
    }
}

fn copy() -> &'static HashMap<String, String> {
    static COPY: OnceLock<HashMap<String, String>> = OnceLock::new();
    COPY.get_or_init(|| {
        check_serde::from_str_unchecked("copy.con", FILE)
            .unwrap_or_else(|e| panic!("copy.con doesn't read as text by key: {e}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_text_has_copy_and_no_copy_is_unused_or_repeated() {
        let keys: Vec<&str> = FILE
            .lines()
            .filter_map(|line| line.split_once(" = ").map(|(key, _)| key))
            .collect();
        assert_eq!(keys.len(), copy().len(), "a key is written twice");
        for text in Text::ALL {
            assert!(!text.get().is_empty(), "{text:?} is empty");
        }
        for key in keys {
            assert!(Text::ALL.iter().any(|t| t.key() == key), "{key} is unused");
        }
    }
}
