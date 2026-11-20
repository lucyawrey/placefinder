use std::collections::VecDeque;
use std::fmt::Write as _;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use base64::Engine;
use bevy::camera::RenderTarget;
use bevy::ecs::system::SystemParam;
use bevy::log::BoxedLayer;
use bevy::log::tracing::field::{Field, Visit};
use bevy::log::tracing::{Event, Subscriber};
use bevy::log::tracing_subscriber::Layer;
use bevy::log::tracing_subscriber::layer::Context;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::window::PrimaryWindow;
use bevy::winit::WinitSettings;
use place::Address;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;
use rmcp::serde_json::{Value, json};
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{tool, tool_router};
use tokio::sync::oneshot;

use crate::places::{Status, Travel};
use crate::player::{HALF_SIZE, Player};

pub const PORT: u16 = 7414;
const KEPT_LINES: usize = 1000;
const TIMEOUT: Duration = Duration::from_secs(30);
const REPLACED: &str = "[a load that another travel replaced before it finished]";
const TIMED_OUT: &str = "[a request the client didn't answer within 30 seconds]";
const NO_VIEW: &str = "[a screenshot asked for before the client has a window and a camera]";
const CLOSED: &str = "[a request the client closed before answering]";

/// Serves the developer MCP on `127.0.0.1:<port>`; port 0 picks a free one.
pub struct McpPlugin {
    pub port: u16,
}

/// The port the MCP server listens on, for tests that ask for a free one.
#[cfg(test)]
#[derive(Resource)]
struct McpPort(u16);

impl Plugin for McpPlugin {
    fn build(&self, app: &mut App) {
        // Bound here rather than on the server thread, so a taken port stops the client at
        // startup instead of failing silently.
        let listener = std::net::TcpListener::bind(("127.0.0.1", self.port))
            .and_then(|l| l.set_nonblocking(true).map(|_| l))
            .unwrap_or_else(|e| {
                eprintln!(
                    "[the MCP server couldn't listen on port {}: {e}]",
                    self.port
                );
                std::process::exit(1);
            });
        let port = listener.local_addr().unwrap().port();
        let (requests, inbox) = mpsc::channel();
        let tools = Tools {
            requests,
            logs: app.world_mut().get_resource_or_init::<Logs>().clone(),
        };
        std::thread::spawn(move || serve_http(listener, port, tools));
        info!("serving the developer MCP at http://127.0.0.1:{port}/mcp");
        #[cfg(test)]
        app.insert_resource(McpPort(port));
        app.insert_resource(Inbox(Mutex::new(inbox)))
            // Agents drive the client from another window, so it keeps running unfocused.
            .insert_resource(WinitSettings::continuous())
            .add_systems(Update, answer);
    }
}

fn serve_http(listener: std::net::TcpListener, port: u16, tools: Tools) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let mut config = StreamableHttpServerConfig::default();
        config.legacy_session_mode = false;
        config.json_response = true;
        let config = config.with_allowed_origins([
            format!("http://127.0.0.1:{port}"),
            format!("http://localhost:{port}"),
        ]);
        let service = StreamableHttpService::<Tools, LocalSessionManager>::new(
            move || Ok(tools.clone()),
            Default::default(),
            config,
        );
        let listener = tokio::net::TcpListener::from_std(listener).unwrap();
        axum::serve(listener, axum::Router::new().nest_service("/mcp", service))
            .await
            .unwrap();
    });
}

enum Request {
    Load(String, oneshot::Sender<Result<String, String>>),
    Inspect(oneshot::Sender<Value>),
    Screenshot(oneshot::Sender<Result<Vec<u8>, String>>),
}

#[derive(Resource)]
struct Inbox(Mutex<mpsc::Receiver<Request>>);

#[derive(Clone)]
struct Tools {
    requests: mpsc::Sender<Request>,
    logs: Logs,
}

#[derive(Deserialize, JsonSchema)]
#[serde(crate = "rmcp::serde")]
#[schemars(crate = "rmcp::schemars")]
struct LoadPlace {
    /// A Place folder path, a `.con` manifest path, or a `place://` address.
    address: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(crate = "rmcp::serde")]
#[schemars(crate = "rmcp::schemars")]
struct ReadLogs {
    /// Only return lines numbered after this one: the `last` of a previous call.
    since: Option<u64>,
}

#[tool_router(server_handler)]
impl Tools {
    #[tool(
        description = "Travel to a Place and wait until it has loaded. Returns the Place's address, or why it failed to load; on failure the current Place stays."
    )]
    async fn load_place(
        &self,
        Parameters(LoadPlace { address }): Parameters<LoadPlace>,
    ) -> CallToolResult {
        match self.ask(|reply| Request::Load(address, reply)).await {
            Ok(Ok(address)) => CallToolResult::success(vec![ContentBlock::text(address)]),
            Ok(Err(e)) | Err(e) => failure(e),
        }
    }

    #[tool(
        description = "Read the client's recent log lines (the last 1000 are kept), oldest first, each with a number. Pass `since` to get only newer lines."
    )]
    async fn read_logs(
        &self,
        Parameters(ReadLogs { since }): Parameters<ReadLogs>,
    ) -> CallToolResult {
        let lines = self.logs.0.lock().unwrap();
        let kept: Vec<Value> = lines
            .kept
            .iter()
            .filter(|(number, _)| since.is_none_or(|since| *number > since))
            .map(|(number, text)| json!({ "number": number, "text": text }))
            .collect();
        CallToolResult::structured(json!({ "lines": kept, "last": lines.next - 1 }))
    }

    #[tool(
        description = "Describe the current Place: its address and manifest, the named nodes of its scene with their world transforms (rotation as an x, y, z, w quaternion) and each one's parent (its index in the list, or null), and the player's position (at their feet) and facing direction. Address and manifest are null when no Place is loaded."
    )]
    async fn inspect_scene(&self) -> CallToolResult {
        match self.ask(Request::Inspect).await {
            Ok(scene) => CallToolResult::structured(scene),
            Err(e) => failure(e),
        }
    }

    #[tool(description = "Take a screenshot of the player's view, as a PNG image.")]
    async fn screenshot(&self) -> CallToolResult {
        match self.ask(Request::Screenshot).await {
            Ok(Ok(png)) => CallToolResult::success(vec![ContentBlock::image(
                base64::engine::general_purpose::STANDARD.encode(png),
                "image/png",
            )]),
            Ok(Err(e)) | Err(e) => failure(e),
        }
    }
}

impl Tools {
    /// Sends a request to the Bevy app and waits for its answer.
    async fn ask<T>(
        &self,
        request: impl FnOnce(oneshot::Sender<T>) -> Request,
    ) -> Result<T, String> {
        let (reply, answer) = oneshot::channel();
        self.requests
            .send(request(reply))
            .map_err(|_| CLOSED.to_string())?;
        match tokio::time::timeout(TIMEOUT, answer).await {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(_)) => Err(CLOSED.into()),
            Err(_) => Err(TIMED_OUT.into()),
        }
    }
}

fn failure(e: String) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(e)])
}

type LoadReply = oneshot::Sender<Result<String, String>>;

fn answer(
    mut commands: Commands,
    inbox: Res<Inbox>,
    mut loads: Local<Vec<(String, LoadReply)>>,
    status: Res<Status>,
    mut travel: MessageWriter<Travel>,
    scene: Scene,
    window: Query<&Window, With<PrimaryWindow>>,
    mut images: ResMut<Assets<Image>>,
) {
    for (address, reply) in std::mem::take(&mut *loads) {
        match outcome(&status, &address) {
            Some(result) => _ = reply.send(result),
            None => loads.push((address, reply)),
        }
    }
    for request in inbox.0.lock().unwrap().try_iter() {
        match request {
            Request::Load(target, reply) => match Address::parse(&target) {
                Ok(address) => {
                    travel.write(Travel {
                        target,
                        from_portal: false,
                    });
                    loads.push((address.to_string(), reply));
                }
                Err(e) => _ = reply.send(Err(e.to_string())),
            },
            Request::Inspect(reply) => _ = reply.send(scene.inspect(&status)),
            // macOS doesn't draw a covered window, so a window screenshot comes back empty.
            // A camera following the player's renders into an image instead.
            Request::Screenshot(reply) => {
                let (Ok(window), Ok(eye)) = (window.single(), scene.camera.single()) else {
                    _ = reply.send(Err(NO_VIEW.into()));
                    continue;
                };
                let image = images.add(Image::new_target_texture(
                    window.physical_width(),
                    window.physical_height(),
                    TextureFormat::Rgba8UnormSrgb,
                    None,
                ));
                let camera = commands
                    .spawn((
                        Camera3d::default(),
                        RenderTarget::from(image.clone()),
                        eye.compute_transform(),
                        Offscreen,
                    ))
                    .id();
                let mut reply = Some(reply);
                commands.spawn(Screenshot::image(image)).observe(
                    move |captured: On<ScreenshotCaptured>, mut commands: Commands| {
                        if let Some(reply) = reply.take() {
                            _ = reply.send(png(captured.image.clone()));
                        }
                        commands.entity(camera).despawn();
                    },
                );
            }
        }
    }
}

/// The camera a screenshot renders with.
#[derive(Component)]
struct Offscreen;

#[derive(SystemParam)]
struct Scene<'w, 's> {
    children: Query<'w, 's, &'static Children>,
    parents: Query<'w, 's, &'static ChildOf>,
    nodes: Query<'w, 's, (&'static Name, &'static GlobalTransform)>,
    player: Query<'w, 's, &'static GlobalTransform, With<Player>>,
    camera: Query<'w, 's, &'static GlobalTransform, (With<Camera3d>, Without<Offscreen>)>,
}

impl Scene<'_, '_> {
    fn inspect(&self, status: &Status) -> Value {
        let current = status.current.as_ref();
        let named: Vec<Entity> = current
            .into_iter()
            .flat_map(|c| self.children.iter_descendants(c.root))
            .filter(|&entity| self.nodes.contains(entity))
            .collect();
        let nodes: Vec<Value> = named
            .iter()
            .map(|&entity| {
                let (name, transform) = self.nodes.get(entity).unwrap();
                let parent = self
                    .parents
                    .iter_ancestors(entity)
                    .find_map(|a| named.iter().position(|&e| e == a));
                let (scale, rotation, translation) = transform.to_scale_rotation_translation();
                json!({
                    "name": name.as_str(),
                    "parent": parent,
                    "translation": translation.to_array(),
                    "rotation": rotation.to_array(),
                    "scale": scale.to_array(),
                })
            })
            .collect();
        let player = self.player.single().ok().map(|p| {
            json!({
                "position": (p.translation() - Vec3::Y * HALF_SIZE.y).to_array(),
                "facing": self.camera.single().ok().map(|c| c.forward().to_array()),
            })
        });
        json!({
            "address": current.map(|c| c.address.to_string()),
            "manifest": current.map(|c| &c.manifest),
            "nodes": nodes,
            "player": player,
        })
    }
}

/// How a load the MCP asked for ended, checked after the travel systems have seen its
/// `Travel`. `None` while it's still loading.
fn outcome(status: &Status, address: &str) -> Option<Result<String, String>> {
    match (&status.loading, &status.error, &status.current) {
        (Some(loading), _, _) if loading == address => None,
        (None, Some(error), _) => Some(Err(error.clone())),
        (None, None, Some(current)) if current.address.to_string() == address => {
            Some(Ok(address.into()))
        }
        _ => Some(Err(REPLACED.into())),
    }
}

fn png(image: Image) -> Result<Vec<u8>, String> {
    let rgba = image
        .try_into_dynamic()
        .map_err(|e| e.to_string())?
        .to_rgba8();
    let mut bytes = Vec::new();
    rgba.write_to(
        &mut std::io::Cursor::new(&mut bytes),
        image::ImageFormat::Png,
    )
    .map_err(|e| e.to_string())?;
    Ok(bytes)
}

/// The client's recent log lines, kept for `read_logs`.
#[derive(Resource, Clone, Default)]
pub struct Logs(Arc<Mutex<Lines>>);

struct Lines {
    /// The number the next line gets; lines are numbered from 1.
    next: u64,
    kept: VecDeque<(u64, String)>,
}

impl Default for Lines {
    fn default() -> Self {
        Lines {
            next: 1,
            kept: VecDeque::new(),
        }
    }
}

/// For `LogPlugin::custom_layer`: keeps log lines when the app has `Logs`.
pub fn log_layer(app: &mut App) -> Option<BoxedLayer> {
    let logs = app.world().get_resource::<Logs>()?.clone();
    Some(Box::new(logs))
}

impl<S: Subscriber> Layer<S> for Logs {
    fn on_event(&self, event: &Event, _: Context<S>) {
        let meta = event.metadata();
        let mut text = format!("{} {}:", meta.level(), meta.target());
        event.record(&mut Fields(&mut text));
        let mut lines = self.0.lock().unwrap();
        let number = lines.next;
        lines.next += 1;
        if lines.kept.len() == KEPT_LINES {
            lines.kept.pop_front();
        }
        lines.kept.push_back((number, text));
    }
}

struct Fields<'a>(&'a mut String);

impl Visit for Fields<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        match field.name() {
            "message" => _ = write!(self.0, " {value:?}"),
            name => _ = write!(self.0, " {name}={value:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::sync::OnceLock;
    use std::time::Instant;

    use bevy::log::tracing;
    use bevy::log::tracing_subscriber::Registry;
    use bevy::log::tracing_subscriber::layer::SubscriberExt;

    use super::*;
    use crate::places::tests::{app_with, current, example};

    /// The test app with the MCP on a free port, keeping logs in the one global subscriber a
    /// test binary can have.
    fn mcp_app() -> (App, u16) {
        static LOGS: OnceLock<Logs> = OnceLock::new();
        let logs = LOGS.get_or_init(|| {
            let logs = Logs::default();
            tracing::subscriber::set_global_default(Registry::default().with(logs.clone()))
                .unwrap();
            logs
        });
        let app = app_with(|app| {
            app.insert_resource(logs.clone())
                .add_plugins(McpPlugin { port: 0 });
        });
        let port = app.world().resource::<McpPort>().0;
        (app, port)
    }

    /// Posts a JSON-RPC message to the MCP endpoint and runs the app until the response comes,
    /// calling `each_frame` before every update.
    fn post(
        app: &mut App,
        host: &str,
        origin: Option<&str>,
        body: Value,
        mut each_frame: impl FnMut(&mut App),
    ) -> String {
        let port = app.world().resource::<McpPort>().0;
        let body = body.to_string();
        let origin = origin.map_or(String::new(), |o| format!("Origin: {o}\r\n"));
        let request = format!(
            "POST /mcp HTTP/1.1\r\nHost: {host}\r\n{origin}Content-Type: application/json\r\n\
             Accept: application/json, text/event-stream\r\nContent-Length: {}\r\n\
             Connection: close\r\n\r\n{body}",
            body.len()
        );
        let response = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            stream.write_all(request.as_bytes()).unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            response
        });
        let start = Instant::now();
        while !response.is_finished() {
            assert!(start.elapsed() < Duration::from_secs(40), "no response");
            each_frame(app);
            app.update();
            std::thread::sleep(Duration::from_millis(5));
        }
        response.join().unwrap()
    }

    /// Calls a tool and returns its result.
    fn call(app: &mut App, port: u16, tool: &str, arguments: Value) -> Value {
        call_each_frame(app, port, tool, arguments, |_| {})
    }

    fn call_each_frame(
        app: &mut App,
        port: u16,
        tool: &str,
        arguments: Value,
        each_frame: impl FnMut(&mut App),
    ) -> Value {
        let message = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": { "name": tool, "arguments": arguments },
        });
        let response = post(app, &format!("127.0.0.1:{port}"), None, message, each_frame);
        let body = &response[response.find("\r\n\r\n").unwrap() + 4..];
        let message: Value = rmcp::serde_json::from_str(body)
            .unwrap_or_else(|e| panic!("{e} in the response: {response}"));
        message["result"].clone()
    }

    fn text(result: &Value) -> &str {
        result["content"][0]["text"].as_str().unwrap()
    }

    #[test]
    fn load_place_answers_with_the_place_or_why_it_failed() {
        let (mut app, port) = mcp_app();
        let loaded = call(
            &mut app,
            port,
            "load_place",
            json!({ "address": example("meadow") }),
        );
        assert_eq!(loaded["isError"], false, "{loaded}");
        let meadow = current(&app);
        assert_eq!(text(&loaded), meadow);

        let missing = json!({ "address": example("missing") });
        assert_eq!(call(&mut app, port, "load_place", missing)["isError"], true);
        assert_eq!(current(&app), meadow);

        let unparsable = call(
            &mut app,
            port,
            "load_place",
            json!({ "address": "place:///x" }),
        );
        assert_eq!(
            text(&unparsable),
            Address::parse("place:///x").unwrap_err().to_string()
        );
    }

    #[test]
    fn a_load_replaced_by_another_travel_says_so() {
        let (mut app, port) = mcp_app();
        let garden = example("garden");
        let replaced = call_each_frame(
            &mut app,
            port,
            "load_place",
            json!({ "address": example("meadow") }),
            |app| {
                let status = app.world().resource::<Status>();
                if status
                    .loading
                    .as_ref()
                    .is_some_and(|l| l.contains("meadow"))
                {
                    app.world_mut().write_message(Travel {
                        target: garden.clone(),
                        from_portal: false,
                    });
                }
            },
        );
        assert_eq!(text(&replaced), REPLACED);
    }

    #[test]
    fn inspect_scene_lists_the_manifest_and_named_nodes() {
        let (mut app, port) = mcp_app();
        let empty = call(&mut app, port, "inspect_scene", json!({}));
        assert_eq!(empty["structuredContent"]["manifest"], Value::Null);

        call(
            &mut app,
            port,
            "load_place",
            json!({ "address": example("garden") }),
        );
        let scene = call(&mut app, port, "inspect_scene", json!({}));
        let scene = &scene["structuredContent"];
        assert_eq!(scene["address"], current(&app));
        assert_eq!(scene["manifest"]["name"], "Garden");
        assert_eq!(scene["manifest"]["portals"][0]["target"], "../meadow");
        let names: Vec<&str> = scene["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"crate"), "{scene}");
        let meshes = scene["nodes"].as_array().unwrap().iter();
        let mut meshes = meshes.filter(|n| n["name"] == "Mesh").peekable();
        assert!(meshes.peek().is_some(), "{scene}");
        assert!(meshes.all(|n| n["parent"].is_u64()), "{scene}");
    }

    #[test]
    fn read_logs_returns_the_lines_after_since() {
        let (mut app, port) = mcp_app();
        let before = call(&mut app, port, "read_logs", json!({}));
        let before = before["structuredContent"]["last"].clone();
        call(
            &mut app,
            port,
            "load_place",
            json!({ "address": example("meadow") }),
        );
        let logs = call(&mut app, port, "read_logs", json!({ "since": before }));
        let logs = &logs["structuredContent"];
        let lines = logs["lines"].as_array().unwrap();
        let arrived = format!("arrived at {}", current(&app));
        assert!(
            lines
                .iter()
                .any(|l| l["text"].as_str().unwrap().contains(&arrived)),
            "{logs}"
        );
        assert!(lines.iter().all(|l| l["number"].as_u64() > before.as_u64()));
    }

    #[test]
    fn refuses_requests_from_other_origins_and_hosts() {
        let (mut app, port) = mcp_app();
        let host = format!("127.0.0.1:{port}");
        let local = format!("http://localhost:{port}");
        let list = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" });
        let mut status = |host: &str, origin: Option<&str>| {
            let response = post(&mut app, host, origin, list.clone(), |_| {});
            response.split(' ').nth(1).unwrap().to_string()
        };
        assert_eq!(status(&host, None), "200");
        assert_eq!(status(&host, Some(&local)), "200");
        assert_ne!(status(&host, Some("http://example.com")), "200");
        assert_ne!(status("example.com", None), "200");
    }
}
