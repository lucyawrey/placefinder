# Placefinder

An experimental new type of web browser for 3D experiences.

## Download

Builds for Linux (x86_64), Windows (x86_64) and macOS (Apple Silicon) are on the [Releases](https://github.com/lucyawrey/placefinder/releases) page. The client comes as an AppImage on Linux, a `.dmg` on macOS and an installer on Windows. The server (`placefinder-server`) is a zip per platform. Both take the same arguments as the `cargo run` commands below.

- **Linux:** the AppImage needs ALSA, xkbcommon and libwayland-client, which most desktops already have. Make it executable (`chmod +x`) and run it.
- **macOS:** the builds aren't signed, so macOS blocks them at first. After dragging Placefinder to Applications, allow it with `xattr -dr com.apple.quarantine /Applications/Placefinder.app` (for the server, `xattr -d com.apple.quarantine placefinder-server` in the unzipped folder).
- **Windows:** the installer isn't signed, so SmartScreen warns about it: choose "More info", then "Run anyway".
- **Logs:** the client writes its log to `~/.local/state/placefinder/placefinder.log` on Linux (under `$XDG_STATE_HOME` if set), `~/Library/Logs/Placefinder/placefinder.log` on macOS and `%LOCALAPPDATA%\Placefinder\logs\placefinder.log` on Windows, replacing it each run.

## Requirements

- [Rust toolchain](https://rust-lang.org/tools/install)

## Build

```
cargo build
```

## Run

Started without an address, the client opens the home Place, by default the example Places hosted at `place://lucyawrey.github.io/placefinder`. To use another home, set `home = "<address>"` in `settings.con` in the config folder: `~/.config/placefinder/` on Linux, `~/Library/Application Support/Placefinder/` on macOS, `%APPDATA%\Placefinder\` on Windows.

```
cargo run -p client
```

Open a Place from a folder directly:

```
cargo run -p client -- places
```

A Place's folder holds its manifest as `place.con`, or as `place.json` instead.

Play in a VR headset with `--vr`, which needs an OpenXR runtime (such as SteamVR, Monado or WiVRn). Without one, the client runs on the desktop.

```
cargo run -p client -- --vr
```

Serve a folder of Places on a small HTTP server, then open one over HTTP:

```
cargo run -p server -- places 8080
cargo run -p client -- place://localhost:8080
```

In the client, click the scene to look around, move with WASD, and press Esc to release the mouse. Type an address in the bar at the top to go there.

### Developer MCP

`--mcp` serves an MCP server on `127.0.0.1` (port 7414, or the one given after `--mcp`) with tools to load a Place, read the client's logs, inspect the current Place and take a screenshot:

```
cargo run -p client -- --mcp places
claude mcp add --transport http placefinder http://127.0.0.1:7414/mcp
```

## Tests

```
cargo test --workspace
```

## License

All rights reserved. The source is public, but is not licensed for copying or distribution. See [LICENSE](LICENSE). The project is planned to be open-sourced at a later date.

Contributions are currently not accepted.
