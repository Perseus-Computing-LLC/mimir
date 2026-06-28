# mimir-persist

**Persistent FastMCP `EventStore` over SQLite.** Drop-in SSE stream
resumability for any [FastMCP](https://github.com/modelcontextprotocol/python-sdk)
/ Streamable-HTTP server, so client connections survive a server restart and
replay the events they missed.

This is **MCP infrastructure** — a persistent `EventStore` backend that lives
*below* the tool layer. It is **not** a "Mimir Python library" and does not
expose Mimir's memory tools. It implements the MCP Python SDK's real
`EventStore` ABC and stores events in its own dedicated SQLite file.

## Install

```bash
pip install -e integrations/mimir-persist/
```

This pulls in the pinned `mcp==1.28.1` SDK (the `EventStore` ABC signature is
SDK-version specific).

## Quick Start

```python
from mimir_persist import MimirEventStore
from mcp.server.fastmcp import FastMCP

# Persist SSE events to a dedicated SQLite file.
store = MimirEventStore(db_path="~/.mimir/data/mcp_events.db")

mcp = FastMCP("my-server", event_store=store)
```

Now when a client reconnects with a `Last-Event-ID`, the transport replays the
events that occurred after it — even if the server process was restarted in
between.

## Interface

`MimirEventStore` implements the real
`mcp.server.streamable_http.EventStore` ABC:

| Method | Purpose |
|---|---|
| `store_event(stream_id, message) -> event_id` | Persist one JSON-RPC event (or a `None` priming event), return its id |
| `replay_events_after(last_event_id, send_callback) -> stream_id \| None` | Replay everything after `last_event_id` on its stream, in order |

Plus a retention helper that is **not** part of the ABC:

| Method | Purpose |
|---|---|
| `cleanup_before(cutoff=None, *, max_age_seconds=None) -> int` | Delete events past a retention boundary, return rows removed |

> The MCP SDK's method names evolve across versions. The names above
> (`store_event` / `replay_events_after`) are what `mcp==1.28.1` actually
> exposes — older proposals used `append_event` / `get_events_after`. This
> package is implemented against the installed ABC, not a doc.

## Storage model

Events are stored in a **dedicated `events` table** in their **own** SQLite
file — never in Mimir's Rust `entities` table.

```sql
CREATE TABLE events (
    event_id   TEXT PRIMARY KEY,   -- opaque UUID returned to the client
    stream_id  TEXT NOT NULL,      -- the SSE stream the event belongs to
    seq        INTEGER NOT NULL,   -- global monotonic replay cursor
    payload    TEXT,               -- JSON of the JSONRPCMessage, NULL = priming
    created_at REAL NOT NULL       -- unix seconds, used by cleanup_before
);
CREATE INDEX idx_events_stream_seq ON events(stream_id, seq);
```

The connection is opened lazily in **WAL mode** (one writer, concurrent
readers) and writes are serialised behind an `asyncio.Lock` so the monotonic
`seq` counter is race-free within a process.

## ⚠️ Encryption: plaintext payloads

**Event payloads in this database are PLAINTEXT.** This store does **NOT**
share Mimir's AES-256-GCM encryption.

Mimir's AES-256-GCM is implemented in the **Rust core** and applies only at the
**column level** to `entities.body_json`. It does **not** extend to this
separate, Python-side events database. If your JSON-RPC traffic is sensitive,
encrypt the DB file at the OS/volume level. Optional Python-side payload
encryption is a possible future enhancement, intentionally out of scope here.

## Requirements

- Python 3.10+
- `mcp==1.28.1`
