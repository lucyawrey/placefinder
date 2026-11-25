use std::fmt;
use std::path::{Component, Path, PathBuf};

use crate::{MANIFEST, Text};

/// Where a Place lives: a folder holding its `place.con`, or a `place://` address. Either may
/// instead name a `.con` manifest, whose folder is then the Place's base.
#[derive(Clone, Debug, PartialEq)]
pub enum Address {
    Folder {
        path: PathBuf,
        fragment: Option<String>,
    },
    Served(Served),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Served {
    pub host: String,
    pub port: Option<u16>,
    pub path: Vec<String>,
    pub fragment: Option<String>,
}

/// The root a Place's files are fetched relative to.
#[derive(Clone, Debug, PartialEq)]
pub enum Base {
    Folder(PathBuf),
    /// An `http://` or `https://` URL ending in `/`.
    Http(String),
}

#[derive(Debug, PartialEq)]
pub enum AddressError {
    Empty,
    MissingHost,
    BadPort(String),
    AboveRoot,
    BadSegment(String),
}

impl fmt::Display for AddressError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            AddressError::Empty => f.write_str(Text::empty_address.get()),
            AddressError::MissingHost => f.write_str(Text::address_without_host.get()),
            AddressError::BadPort(p) => f.write_str(&Text::bad_port.fill(&[("port", p)])),
            AddressError::AboveRoot => f.write_str(Text::above_root.get()),
            AddressError::BadSegment(s) => f.write_str(&Text::bad_path_part.fill(&[("part", s)])),
        }
    }
}

const SCHEME: &str = "place://";

impl Address {
    pub fn parse(input: &str) -> Result<Address, AddressError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(AddressError::Empty);
        }
        let Some(rest) = input.strip_prefix(SCHEME) else {
            let (input, fragment) = split_fragment(input);
            let path = std::path::absolute(input).unwrap_or_else(|_| input.into());
            return Ok(Address::Folder {
                path: normalize_path(&path),
                fragment,
            });
        };
        let (rest, fragment) = split_fragment(rest);
        let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
        let port_at = authority
            .rfind(':')
            .filter(|&i| authority.rfind(']').is_none_or(|end| i > end));
        let (host, port) = match port_at {
            Some(i) => {
                let port = &authority[i + 1..];
                let port = port
                    .parse()
                    .map_err(|_| AddressError::BadPort(port.into()))?;
                (&authority[..i], Some(port))
            }
            None => (authority, None),
        };
        if host.is_empty() {
            return Err(AddressError::MissingHost);
        }
        Ok(Address::Served(Served {
            host: host.to_ascii_lowercase(),
            port,
            path: normalize(path.split('/'))?,
            fragment,
        }))
    }

    /// Resolves a Portal target: absolute targets stand alone, relative ones are relative to
    /// this Place, which counts as a folder (`../meadow` from `garden` is `meadow`).
    pub fn resolve(&self, target: &str) -> Result<Address, AddressError> {
        let target = target.trim();
        if target.starts_with(SCHEME) || Path::new(target).is_absolute() {
            return Address::parse(target);
        }
        let (relative, fragment) = split_fragment(target);
        match self {
            Address::Folder { .. } => {
                let Base::Folder(dir) = self.base() else {
                    unreachable!()
                };
                Ok(Address::Folder {
                    path: normalize_path(&dir.join(relative)),
                    fragment,
                })
            }
            Address::Served(served) => {
                let segments = served
                    .folder()
                    .iter()
                    .map(String::as_str)
                    .chain(relative.split('/'));
                Ok(Address::Served(Served {
                    host: served.host.clone(),
                    port: served.port,
                    path: normalize(segments)?,
                    fragment,
                }))
            }
        }
    }

    /// A folder address with no fragment.
    pub fn folder(path: impl Into<PathBuf>) -> Address {
        Address::Folder {
            path: path.into(),
            fragment: None,
        }
    }

    /// The part after `#`, which names a Portal in the Place.
    pub fn fragment(&self) -> Option<&str> {
        match self {
            Address::Folder { fragment, .. } | Address::Served(Served { fragment, .. }) => {
                fragment.as_deref()
            }
        }
    }

    pub fn without_fragment(&self) -> Address {
        let mut address = self.clone();
        match &mut address {
            Address::Folder { fragment, .. } | Address::Served(Served { fragment, .. }) => {
                *fragment = None
            }
        }
        address
    }

    /// The manifest's file name: the one named, or `place.con`.
    pub fn manifest(&self) -> &str {
        self.named().unwrap_or(MANIFEST)
    }

    fn named(&self) -> Option<&str> {
        let last = match self {
            Address::Folder { path, .. } => path.file_name().and_then(|n| n.to_str()),
            Address::Served(served) => served.path.last().map(String::as_str),
        };
        last.filter(|n| n.ends_with(".con"))
    }

    /// The folder holding the manifest.
    pub fn base(&self) -> Base {
        match self {
            Address::Folder { path, .. } if self.named().is_some() => {
                Base::Folder(path.parent().unwrap_or(Path::new("")).to_path_buf())
            }
            Address::Folder { path, .. } => Base::Folder(path.clone()),
            Address::Served(served) => {
                // Plain HTTP only for local development, as browsers allow for localhost.
                let local = matches!(served.host.as_str(), "localhost" | "127.0.0.1" | "[::1]");
                let (scheme, default_port) = if local { ("http", 80) } else { ("https", 443) };
                let port = served.port.unwrap_or(default_port);
                let path: String = served.folder().iter().map(|s| format!("{s}/")).collect();
                Base::Http(format!("{scheme}://{}:{port}/{path}", served.host))
            }
        }
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Address::Folder { path, .. } => write!(f, "{}", path.display())?,
            Address::Served(served) => {
                write!(f, "{SCHEME}{}", served.host)?;
                if let Some(port) = served.port {
                    write!(f, ":{port}")?;
                }
                for segment in &served.path {
                    write!(f, "/{segment}")?;
                }
            }
        }
        if let Some(fragment) = self.fragment() {
            write!(f, "#{fragment}")?;
        }
        Ok(())
    }
}

impl Served {
    fn folder(&self) -> &[String] {
        match self.path.split_last() {
            Some((last, folder)) if last.ends_with(".con") => folder,
            _ => &self.path,
        }
    }
}

fn split_fragment(s: &str) -> (&str, Option<String>) {
    match s.split_once('#') {
        Some((rest, fragment)) => (rest, Some(fragment.into())),
        None => (s, None),
    }
}

/// Drops empty and `.` segments and applies `..`, refusing to go above the root. Segments with
/// `\` or `:` are refused on every platform, since Windows reads them as separators or drives.
pub(crate) fn normalize<'a>(
    segments: impl Iterator<Item = &'a str>,
) -> Result<Vec<String>, AddressError> {
    let mut out: Vec<String> = Vec::new();
    for segment in segments {
        match segment {
            "" | "." => {}
            ".." => {
                out.pop().ok_or(AddressError::AboveRoot)?;
            }
            s if s.contains(['\\', ':']) => return Err(AddressError::BadSegment(s.into())),
            s => out.push(s.into()),
        }
    }
    Ok(out)
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir if out.file_name().is_some() => {
                out.pop();
            }
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Address {
        Address::parse(s).unwrap()
    }

    #[test]
    fn maps_to_https_except_on_localhost() {
        assert_eq!(
            parse("place://localhost:8080/garden").base(),
            Base::Http("http://localhost:8080/garden/".into())
        );
        assert_eq!(
            parse("place://example.com").base(),
            Base::Http("https://example.com:443/".into())
        );
    }

    #[test]
    fn reads_ipv6_hosts_and_ignores_host_case() {
        assert_eq!(
            parse("place://[::1]/garden").base(),
            Base::Http("http://[::1]:80/garden/".into())
        );
        assert_eq!(
            parse("place://[::1]:8080/garden").base(),
            Base::Http("http://[::1]:8080/garden/".into())
        );
        assert_eq!(
            parse("place://LOCALHOST:8080/garden").base(),
            Base::Http("http://localhost:8080/garden/".into())
        );
    }

    #[test]
    fn ignores_a_trailing_slash() {
        assert_eq!(
            parse("place://a.com/garden/"),
            parse("place://a.com/garden")
        );
    }

    #[test]
    fn keeps_the_fragment_out_of_the_base() {
        for address in ["place://a.com/garden#lobby", "/x/garden#lobby"] {
            let address = parse(address);
            assert_eq!(address.fragment(), Some("lobby"));
            assert_eq!(address.base(), address.without_fragment().base());
        }
        assert_eq!(
            parse("place://a.com/garden#lobby").to_string(),
            "place://a.com/garden#lobby"
        );
    }

    #[test]
    fn refuses_bad_addresses() {
        assert_eq!(Address::parse("  "), Err(AddressError::Empty));
        assert_eq!(
            Address::parse("place:///garden"),
            Err(AddressError::MissingHost)
        );
        assert_eq!(
            Address::parse("place://a.com:x"),
            Err(AddressError::BadPort("x".into()))
        );
        assert_eq!(
            Address::parse("place://a.com/../x"),
            Err(AddressError::AboveRoot)
        );
    }

    #[test]
    fn resolves_relative_targets_from_served_places_and_folders() {
        let from = parse("place://a.com:8080/places/garden");
        assert_eq!(
            from.resolve("../meadow").unwrap(),
            parse("place://a.com:8080/places/meadow")
        );
        assert_eq!(
            from.resolve("pond").unwrap(),
            parse("place://a.com:8080/places/garden/pond")
        );
        let folder = Address::folder(PathBuf::from("/home/x/places/garden"));
        assert_eq!(
            folder.resolve("../meadow").unwrap(),
            Address::folder(PathBuf::from("/home/x/places/meadow"))
        );
    }

    #[test]
    fn a_named_manifest_loads_from_its_folder() {
        let lobby = parse("place://a.com/garden/lobby.con");
        assert_eq!(lobby.manifest(), "lobby.con");
        assert_eq!(lobby.base(), Base::Http("https://a.com:443/garden/".into()));
        assert_eq!(
            lobby.resolve("pond").unwrap(),
            parse("place://a.com/garden/pond")
        );
        let folder = Address::folder(PathBuf::from("/x/garden/lobby.con"));
        assert_eq!(folder.manifest(), "lobby.con");
        assert_eq!(folder.base(), Base::Folder(PathBuf::from("/x/garden")));
        assert_eq!(
            folder.resolve("pond").unwrap(),
            Address::folder(PathBuf::from("/x/garden/pond"))
        );
        assert_eq!(parse("place://a.com/garden").manifest(), "place.con");
    }

    #[test]
    fn absolute_targets_ignore_the_current_place() {
        let from = parse("place://a.com/garden");
        assert_eq!(
            from.resolve("place://b.com/x").unwrap(),
            parse("place://b.com/x")
        );
        let absolute = std::env::temp_dir().join("x");
        assert_eq!(
            from.resolve(absolute.to_str().unwrap()).unwrap(),
            Address::folder(absolute)
        );
    }

    #[test]
    fn relative_folders_become_absolute() {
        let cwd = std::env::current_dir().unwrap();
        assert_eq!(
            parse("places/./x/../garden"),
            Address::folder(cwd.join("places").join("garden"))
        );
    }
}
