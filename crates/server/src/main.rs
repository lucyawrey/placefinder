use std::path::PathBuf;

use axum::Router;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use place::Base;
use place::fetch::FetchError;

/// Serves a folder of Places over HTTP: `server <folder> [port]`.
#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let root = PathBuf::from(args.next().expect("usage: server <folder> [port]"));
    let port: u16 = args
        .next()
        .map_or(8080, |p| p.parse().expect("port must be a number"));
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .unwrap();
    println!("serving on http://127.0.0.1:{port}");
    axum::serve(listener, app(root)).await.unwrap();
}

fn app(root: PathBuf) -> Router {
    Router::new()
        .route("/{*path}", axum::routing::get(serve))
        .with_state(Base::Folder(root))
}

async fn serve(State(root): State<Base>, Path(path): Path<String>) -> Response {
    match root.fetch(&path) {
        Ok(bytes) => ([(header::CONTENT_TYPE, content_type(&path))], bytes).into_response(),
        Err(FetchError::OutsidePlace(_)) => StatusCode::FORBIDDEN.into_response(),
        Err(FetchError::NotFound(_)) => StatusCode::NOT_FOUND.into_response(),
        Err(FetchError::Failed(_)) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

fn content_type(path: &str) -> &'static str {
    match path.rsplit_once('.').map(|(_, ext)| ext) {
        Some("gltf") => "model/gltf+json",
        Some("glb") => "model/gltf-binary",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("con") => "text/plain; charset=utf-8",
        Some("json") => "application/json",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::path::PathBuf;

    use place::fetch::FetchError;
    use place::{Address, Base, LoadError, load};

    fn examples() -> PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../places")
    }

    /// A folder of Places in a temp dir holding the given files.
    fn temp_root(name: &str, files: &[(&str, &[u8])]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("placefinder-server-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        for (file, bytes) in files {
            let path = dir.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
        dir
    }

    /// Serves `root` on a free port and returns the port.
    async fn serve(root: PathBuf) -> u16 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(axum::serve(listener, super::app(root)).into_future());
        port
    }

    async fn load_served(root: PathBuf, path: &str) -> Result<(Base, place::Manifest), LoadError> {
        let port = serve(root).await;
        let address = Address::parse(&format!("place://127.0.0.1:{port}/{path}")).unwrap();
        tokio::task::spawn_blocking(move || load(&address))
            .await
            .unwrap()
    }

    /// Sends a GET with `path` exactly as written, so encoded separators reach the server.
    async fn status(root: PathBuf, path: &str) -> String {
        let port = serve(root).await;
        let request =
            format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
        tokio::task::spawn_blocking(move || {
            let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
            stream.write_all(request.as_bytes()).unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            response.split(' ').nth(1).unwrap().to_string()
        })
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn loads_a_place_over_http() {
        let (_, manifest) = load_served(examples(), "").await.unwrap();
        assert_eq!(manifest.name, "Home");
    }

    #[tokio::test]
    async fn a_missing_place_over_http_is_not_found() {
        let result = load_served(examples(), "nowhere").await;
        assert!(matches!(
            result,
            Err(LoadError::Fetch(FetchError::NotFound(_)))
        ));
    }

    #[tokio::test]
    async fn an_invalid_manifest_over_http_is_refused() {
        let root = temp_root("invalid", &[("a/place.con", b"name = 3")]);
        assert!(matches!(
            load_served(root, "a").await,
            Err(LoadError::Manifest(_))
        ));
    }

    #[tokio::test]
    async fn loads_a_json_place_over_http_and_reports_a_missing_place_con() {
        let manifest = br#"{"version": "0.1.0", "name": "A", "scene": "s.gltf", "spawn": {"center": [0, 0, 0]}}"#;
        let root = temp_root("json", &[("a/place.json", manifest), ("b/x.txt", b"")]);
        let (_, manifest) = load_served(root.clone(), "a").await.unwrap();
        assert_eq!(manifest.name, "A");
        match load_served(root, "b").await {
            Err(LoadError::Fetch(FetchError::NotFound(url))) => assert!(url.ends_with("place.con")),
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn a_missing_scene_over_http_is_not_found() {
        let manifest = b"version = \"0.1.0\"\nname = \"A\"\nscene = \"s.gltf\"\nspawn = (center = (0.0, 0.0, 0.0))";
        let root = temp_root("no-scene", &[("a/place.con", manifest)]);
        let (base, manifest) = load_served(root, "a").await.unwrap();
        let result = tokio::task::spawn_blocking(move || base.fetch(&manifest.scene))
            .await
            .unwrap();
        assert!(matches!(result, Err(FetchError::NotFound(_))));
    }

    #[tokio::test]
    async fn refuses_paths_that_leave_the_folder() {
        for path in ["/..%2F..%2Fsecret", "/..%5C..%5Csecret", "/C%3A%5Csecret"] {
            assert_eq!(status(examples(), path).await, "403", "{path}");
        }
    }

    #[tokio::test]
    async fn serves_files_over_10_mb() {
        let big = vec![7u8; 11 << 20];
        let root = temp_root("big", &[("a/scene.glb", &big)]);
        let port = serve(root).await;
        let base = Base::Http(format!("http://127.0.0.1:{port}/a/"));
        let bytes = tokio::task::spawn_blocking(move || base.fetch("scene.glb"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(bytes.len(), big.len());
    }
}
