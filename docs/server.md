# Contender Server

The Contender server exposes a JSON-RPC API for managing load-testing sessions and a web UI for interactive use. It runs two listeners:

- **JSON-RPC server** — accepts RPC calls and WebSocket subscriptions for session management and log streaming.
- **SSE / static server** — serves the web UI, API documentation, and SSE log streams.

## Quickstart

```sh
contender server
```

By default this starts:

| Service | Default Address |
|---|---|
| JSON-RPC | `127.0.0.1:3000` |
| Web UI / SSE | `127.0.0.1:3001` |

Once running, open [localhost:3001](http://127.0.0.1:3001/) in a browser to access the web UI.

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `RPC_HOST` | `127.0.0.1:3000` | Address and port for the JSON-RPC server. |
| `SSE_HOST` | `127.0.0.1:3001` | Address and port for the web UI / SSE server. |
| `RUST_LOG` | `info` | Log level filter (uses `tracing_subscriber` `EnvFilter` syntax, e.g. `debug`, `contender=trace`). |

Example — bind to all interfaces on custom ports:

```sh
RPC_HOST=0.0.0.0:9000 SSE_HOST=0.0.0.0:9001 contender server
```

## HTML Routes

The HTML/SSE server hosts the following endpoints:

| Route | Description |
|---|---|
| `/` , `/index.html` | Web UI for managing sessions and viewing logs. |
| `/docs` | Interactive API documentation (rendered from the OpenRPC spec). |
| [`/openrpc.json`](../crates/cli/src/server/static/openrpc.json) | Machine-readable [OpenRPC](https://open-rpc.org/) specification of the JSON-RPC API. |
| `/logs/{session_id}` | SSE stream of logs for a given session. |

## RPC Methods

The JSON-RPC API exposes the following methods (see `/docs` for full schemas):

| Method | Description |
|---|---|
| `status` | Get server status. |
| `addSession` | Create a new contender session. |
| `getSession` | Get info for a session by ID. |
| `getAllSessions` | List all sessions. |
| `removeSession` | Remove a session by ID. |
| `spam` | Start a spam run for a session. |
| `stop` | Stop a running session. |
| `fundAccounts` | Fund accounts for a session. |
| `subscribeLogs` | Subscribe to session logs (WebSocket). |
