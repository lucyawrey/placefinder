// Bevy systems take many parameters, often long query types.
#![allow(clippy::type_complexity, clippy::too_many_arguments)]

mod address_bar;
mod mcp;
mod places;
mod player;
mod portals;
mod source;

use bevy::asset::AssetMetaCheck;
use bevy::log::{DEFAULT_FILTER, LogPlugin};
use bevy::prelude::*;

/// `client [--mcp [port]] [address]`: opens the Placefinder prototype, at the address if one is
/// given, serving the developer MCP if `--mcp` is.
fn main() {
    let mut args = std::env::args().skip(1).peekable();
    let (mut mcp_port, mut address) = (None, None);
    while let Some(arg) = args.next() {
        if arg != "--mcp" {
            address = Some(arg);
            continue;
        }
        let port = args.next_if(|a| !a.is_empty() && a.bytes().all(|b| b.is_ascii_digit()));
        mcp_port = Some(port.map_or(mcp::PORT, |p| {
            p.parse().unwrap_or_else(|_| {
                eprintln!("[a port number that's too large: {p}]");
                std::process::exit(1);
            })
        }));
    }

    let mut app = App::new();
    source::register(&mut app);
    if mcp_port.is_some() {
        // Before `LogPlugin`, whose `custom_layer` reads it.
        app.init_resource::<mcp::Logs>();
    }
    app.add_plugins(
        DefaultPlugins
            .set(AssetPlugin {
                meta_check: AssetMetaCheck::Never,
                ..default()
            })
            .set(LogPlugin {
                filter: format!("{DEFAULT_FILTER}rmcp=warn"),
                custom_layer: mcp::log_layer,
                ..default()
            }),
    )
    .add_plugins((
        places::PlacesPlugin,
        player::PlayerPlugin,
        portals::PortalsPlugin,
        address_bar::AddressBarPlugin,
    ));
    if let Some(port) = mcp_port {
        app.add_plugins(mcp::McpPlugin { port });
    }
    if let Some(address) = address {
        app.world_mut().write_message(places::Travel {
            target: address,
            from_portal: false,
        });
    }
    app.run();
}
