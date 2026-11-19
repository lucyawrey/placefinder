// Bevy system parameters are long query types by nature.
#![allow(clippy::type_complexity)]

mod address_bar;
mod places;
mod player;
mod portals;
mod source;

use bevy::asset::AssetMetaCheck;
use bevy::prelude::*;

/// `client [address]`: opens the Placefinder prototype, at the address if one is given.
fn main() {
    let mut app = App::new();
    source::register(&mut app);
    app.add_plugins(DefaultPlugins.set(AssetPlugin {
        meta_check: AssetMetaCheck::Never,
        ..default()
    }))
    .add_plugins((
        places::PlacesPlugin,
        player::PlayerPlugin,
        portals::PortalsPlugin,
        address_bar::AddressBarPlugin,
    ));
    if let Some(address) = std::env::args().nth(1) {
        app.world_mut().write_message(places::Travel {
            target: address,
            from_portal: false,
        });
    }
    app.run();
}
