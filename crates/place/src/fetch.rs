use std::fmt;

use crate::address::{Base, normalize};

#[derive(Debug)]
pub enum FetchError {
    OutsidePlace(String),
    NotFound(String),
    Failed(String),
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            FetchError::OutsidePlace(p) => write!(f, "[a file path outside the Place: {p}]"),
            FetchError::NotFound(p) => write!(f, "[a file that wasn't found: {p}]"),
            FetchError::Failed(e) => write!(f, "{e}"),
        }
    }
}

impl Base {
    /// Reads a file of the Place. Every Place file goes through here, so caching and HTTPS
    /// belong here too.
    pub fn fetch(&self, path: &str) -> Result<Vec<u8>, FetchError> {
        let segments =
            normalize(path.split('/')).map_err(|_| FetchError::OutsidePlace(path.into()))?;
        let relative = segments.join("/");
        match self {
            Base::Folder(dir) => std::fs::read(dir.join(&relative)).map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => {
                    FetchError::NotFound(dir.join(&relative).display().to_string())
                }
                _ => FetchError::Failed(e.to_string()),
            }),
            Base::Http(url) => fetch_http(&format!("{url}{relative}")),
        }
    }
}

/// The largest file a Place can serve. ureq's own default is 10 MB, too small for a `.glb`.
#[cfg(not(target_family = "wasm"))]
const MAX_FILE: u64 = 1 << 30;

#[cfg(not(target_family = "wasm"))]
fn fetch_http(url: &str) -> Result<Vec<u8>, FetchError> {
    match ureq::get(url).call() {
        Ok(mut response) => response
            .body_mut()
            .with_config()
            .limit(MAX_FILE)
            .read_to_vec()
            .map_err(|e| FetchError::Failed(e.to_string())),
        Err(ureq::Error::StatusCode(404)) => Err(FetchError::NotFound(url.into())),
        Err(e) => Err(FetchError::Failed(format!("{url}: {e}"))),
    }
}

#[cfg(target_family = "wasm")]
fn fetch_http(url: &str) -> Result<Vec<u8>, FetchError> {
    Err(FetchError::Failed(format!(
        "{url}: HTTP isn't available on this target yet"
    )))
}
