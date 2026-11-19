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
        _ => "application/octet-stream",
    }
}
