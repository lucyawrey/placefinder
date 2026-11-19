use std::fmt;
use std::path::{Component, Path, PathBuf};

use crate::MANIFEST;

/// Where a Place lives: a folder holding its `place.con`, or a `place://` address. Either may
/// instead name a `.con` manifest, whose folder is then the Place's base.
#[derive(Clone, Debug, PartialEq)]
pub enum Address {
    Folder(PathBuf),
    Served(Served),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Served {
    pub host: String,
    pub port: Option<u16>,
    pub path: Vec<String>,
    /// Kept for display; arrival ignores it for now.
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
            AddressError::Empty => write!(f, "[an empty address]"),
            AddressError::MissingHost => write!(f, "[an address with no host]"),
            AddressError::BadPort(p) => write!(f, "[a port that isn't a number: {p}]"),
            AddressError::AboveRoot => write!(f, "[a path that goes above the site's root]"),
            AddressError::BadSegment(s) => write!(
                f,
                "[a path part containing \\ or :, which aren't allowed: {s}]"
            ),
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
            return Ok(Address::Folder(PathBuf::from(input)));
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
            Address::Folder(_) => {
                let Base::Folder(dir) = self.base() else {
                    unreachable!()
                };
                Ok(Address::Folder(normalize_path(&dir.join(relative))))
            }
            Address::Served(served) => {
                let segments = served
                    .folder()
                    .iter()
                    .map(String::as_str)
                    .chain(relative.split('/'));
                Ok(Address::Served(Served {
                    path: normalize(segments)?,
                    fragment,
                    ..served.clone()
                }))
            }
        }
    }

    /// The manifest's file name: the one named, or `place.con`.
    pub fn manifest(&self) -> &str {
        self.named().unwrap_or(MANIFEST)
    }

    fn named(&self) -> Option<&str> {
        let last = match self {
            Address::Folder(path) => path.file_name().and_then(|n| n.to_str()),
            Address::Served(served) => served.path.last().map(String::as_str),
        };
        last.filter(|n| n.ends_with(".con"))
    }

    /// The folder holding the manifest.
    pub fn base(&self) -> Base {
        match self {
            Address::Folder(path) if self.named().is_some() => {
                Base::Folder(path.parent().unwrap_or(Path::new("")).to_path_buf())
            }
            Address::Folder(dir) => Base::Folder(dir.clone()),
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
            Address::Folder(dir) => write!(f, "{}", dir.display()),
            Address::Served(served) => {
                write!(f, "{SCHEME}{}", served.host)?;
                if let Some(port) = served.port {
                    write!(f, ":{port}")?;
                }
                for segment in &served.path {
                    write!(f, "/{segment}")?;
                }
                if let Some(fragment) = &served.fragment {
                    write!(f, "#{fragment}")?;
                }
                Ok(())
            }
        }
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
