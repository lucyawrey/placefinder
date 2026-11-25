// Bevy systems take many parameters, often long query types.
#![allow(clippy::type_complexity, clippy::too_many_arguments)]
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod address_bar;
mod folders;
mod labels;
mod log_file;
mod mcp;
mod places;
mod player;
mod portals;
mod settings;
mod sky;
mod source;

use bevy::asset::AssetMetaCheck;
use bevy::log::{DEFAULT_FILTER, LogPlugin};
use bevy::prelude::*;

/// `client [--mcp [port]] [address]`: opens the Placefinder prototype at the address if one is
/// given, else at the home Place, serving the developer MCP if `--mcp` is.
fn main() {
    #[cfg(all(windows, not(debug_assertions)))]
    attach_console();
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
                eprintln!("{}", place::Text::port_too_large.fill(&[("port", &p)]));
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
            .set(WindowPlugin {
                // The AppImage's desktop file name, so Wayland desktops show its icon.
                primary_window: Some(Window {
                    name: Some("placefinder".into()),
                    ..default()
                }),
                ..default()
            })
            .set(LogPlugin {
                filter: format!("{DEFAULT_FILTER}rmcp=warn"),
                custom_layer: log_file::layers,
                ..default()
            }),
    )
    .add_plugins((
        places::PlacesPlugin,
        player::PlayerPlugin,
        portals::PortalsPlugin,
        labels::LabelsPlugin,
        sky::SkyPlugin,
        address_bar::AddressBarPlugin,
    ));
    if let Some(port) = mcp_port {
        app.add_plugins(mcp::McpPlugin { port });
    }
    // After `LogPlugin`, so a broken settings file is logged.
    let target = address.unwrap_or_else(settings::home);
    app.world_mut().write_message(places::Travel {
        target,
        from_portal: false,
    });
    app.run();
}

/// Without a console of its own, the client prints to the terminal it was started from, if any.
#[cfg(all(windows, not(debug_assertions)))]
fn attach_console() {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn AttachConsole(process_id: u32) -> i32;
    }
    const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
    // Fails when there's no parent terminal, which is fine.
    unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
}
