# Placefinder

An experimental new type of web browser for 3D experiences.

## Requirements

- [Rust toolchain](https://rust-lang.org/tools/install)

## Build

```
cargo build
```

## Run

Open a Place from a folder directly:

```
cargo run -p client -- places/garden
```

Serve a folder of Places on a small HTTP server, then open one over HTTP:

```
cargo run -p server -- places 8080
cargo run -p client -- place://localhost:8080/garden
```

In the client, click the scene to look around, move with WASD, and press Esc to release the mouse. Type an address in the bar at the top to go there.

### Developer MCP

`--mcp` serves an MCP server on `127.0.0.1` (port 7414, or the one given after `--mcp`) with tools to load a Place, read the client's logs, inspect the current Place and take a screenshot:

```
cargo run -p client -- --mcp places/garden
claude mcp add --transport http placefinder http://127.0.0.1:7414/mcp
```

## Tests

```
cargo test --workspace
```

## License

All rights reserved. The source is public, but is not licensed for copying or distribution. See [LICENSE](LICENSE). The project is planned to be open-sourced at a later date.

Contributions are currently not accepted.
