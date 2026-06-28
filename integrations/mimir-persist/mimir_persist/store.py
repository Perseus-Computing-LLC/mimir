"""
MimirEventStore — a persistent FastMCP ``EventStore`` backed by SQLite.

This is **MCP infrastructure**, not a "Mimir Python library". It gives any
FastMCP / Streamable-HTTP server SSE *stream resumability* across restarts by
durably persisting the JSON-RPC events that flow over a session's SSE streams.
When a client reconnects with a ``Last-Event-ID``, the transport asks this store
to replay everything that came after it.

It implements the real ``mcp.server.streamable_http.EventStore`` ABC:

    * ``store_event(stream_id, message) -> event_id``
    * ``replay_events_after(last_event_id, send_callback) -> stream_id | None``

plus a ``cleanup_before(...)`` retention helper (not part of the ABC).

Usage::

    from mimir_persist import MimirEventStore
    from mcp.server.fastmcp import FastMCP

    mcp = FastMCP("my-server", event_store=MimirEventStore())

Storage model
-------------
Events live in a **dedicated** ``events`` table in their **own** SQLite file
(default ``~/.mimir/data/mcp_events.db``). This is deliberately separate from
Mimir's Rust ``entities`` table — the two never share a table or a file.

    CREATE TABLE events (
        event_id   TEXT PRIMARY KEY,   -- opaque UUID handed back to the client
        stream_id  TEXT NOT NULL,      -- the SSE stream the event belongs to
        seq        INTEGER NOT NULL,   -- global monotonic order (replay cursor)
        payload    TEXT,               -- JSON of the JSONRPCMessage, or NULL (priming)
        created_at REAL NOT NULL       -- unix seconds, used by retention cleanup
    );
    CREATE INDEX idx_events_stream_seq ON events(stream_id, seq);

.. warning::

   **Payloads are stored as PLAINTEXT.** This store does **NOT** share Mimir's
   AES-256-GCM encryption. That encryption is implemented in Mimir's Rust core
   and only ever applies at the *column* level to ``entities.body_json`` — it
   does not extend to this separate events database. If your JSON-RPC traffic
   carries sensitive data, encrypt the database file at the OS/volume level, or
   wrap this store (Python-side encryption of ``payload`` is an optional
   stretch, intentionally out of scope here).

Concurrency
-----------
The store opens one SQLite connection in WAL mode (one writer, many readers) and
serialises writes behind an ``asyncio.Lock`` so the monotonic ``seq`` counter is
assigned without races inside a single server process.
"""

from __future__ import annotations

import asyncio
import sqlite3
import time
import uuid
from pathlib import Path
from typing import Optional

from mcp.server.streamable_http import (
    EventCallback,
    EventId,
    EventMessage,
    EventStore,
    StreamId,
)
from mcp.types import JSONRPCMessage

__all__ = ["MimirEventStore"]


class MimirEventStore(EventStore):
    """Durable SQLite-backed :class:`EventStore` for FastMCP SSE resumability.

    Args:
        db_path: Path to the dedicated events SQLite file. Created (with parent
            dirs) on first use. Defaults to ``~/.mimir/data/mcp_events.db``.
            This file is **separate** from Mimir's entity database.
    """

    def __init__(self, db_path: str = "~/.mimir/data/mcp_events.db") -> None:
        self.db_path = str(Path(db_path).expanduser())
        self._conn: Optional[sqlite3.Connection] = None
        self._lock = asyncio.Lock()

    # ------------------------------------------------------------------ #
    # Lazy connection / schema
    # ------------------------------------------------------------------ #
    def _connect(self) -> sqlite3.Connection:
        """Open (once) the SQLite connection, enabling WAL and the schema.

        Initialisation is lazy so constructing the store never touches disk —
        the file and schema appear on the first ``store_event`` / replay call.
        """
        if self._conn is not None:
            return self._conn

        if self.db_path != ":memory:":
            Path(self.db_path).parent.mkdir(parents=True, exist_ok=True)

        conn = sqlite3.connect(self.db_path, check_same_thread=False)
        conn.row_factory = sqlite3.Row
        # WAL: durable across restarts, one writer + concurrent readers.
        # A :memory: db rejects WAL, so only set it for real files.
        if self.db_path != ":memory:":
            conn.execute("PRAGMA journal_mode=WAL")
        conn.execute("PRAGMA synchronous=NORMAL")
        conn.execute(
            """
            CREATE TABLE IF NOT EXISTS events (
                event_id   TEXT PRIMARY KEY,
                stream_id  TEXT NOT NULL,
                seq        INTEGER NOT NULL,
                payload    TEXT,
                created_at REAL NOT NULL
            )
            """
        )
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_events_stream_seq "
            "ON events(stream_id, seq)"
        )
        conn.commit()
        self._conn = conn
        return conn

    def _next_seq(self, conn: sqlite3.Connection) -> int:
        """Return the next global monotonic sequence value.

        Called under ``self._lock`` so concurrent ``store_event`` coroutines in
        the same process can't hand out a duplicate ``seq``.
        """
        row = conn.execute("SELECT COALESCE(MAX(seq), 0) AS m FROM events").fetchone()
        return int(row["m"]) + 1

    # ------------------------------------------------------------------ #
    # EventStore ABC
    # ------------------------------------------------------------------ #
    async def store_event(
        self, stream_id: StreamId, message: JSONRPCMessage | None
    ) -> EventId:
        """Persist one event for ``stream_id`` and return its new event id.

        ``message`` may be ``None`` for transport "priming" events; that is
        stored as a NULL payload and replayed back as ``None``.
        """
        event_id = uuid.uuid4().hex
        payload = message.model_dump_json() if message is not None else None
        created_at = time.time()

        async with self._lock:
            conn = self._connect()
            seq = self._next_seq(conn)
            conn.execute(
                "INSERT INTO events (event_id, stream_id, seq, payload, created_at) "
                "VALUES (?, ?, ?, ?, ?)",
                (event_id, stream_id, seq, payload, created_at),
            )
            conn.commit()
        return event_id

    async def replay_events_after(
        self,
        last_event_id: EventId,
        send_callback: EventCallback,
    ) -> StreamId | None:
        """Replay every event that followed ``last_event_id`` on its stream.

        Resolves ``last_event_id`` to its ``(stream_id, seq)``, then streams all
        later events of that same stream — in ``seq`` order — through
        ``send_callback``. Returns the resolved stream id, or ``None`` when the
        event id is unknown (e.g. already cleaned up, or never existed).
        """
        async with self._lock:
            conn = self._connect()
            anchor = conn.execute(
                "SELECT stream_id, seq FROM events WHERE event_id = ?",
                (last_event_id,),
            ).fetchone()
            if anchor is None:
                return None

            stream_id = anchor["stream_id"]
            rows = conn.execute(
                "SELECT event_id, payload FROM events "
                "WHERE stream_id = ? AND seq > ? ORDER BY seq ASC",
                (stream_id, anchor["seq"]),
            ).fetchall()

        # Fire callbacks outside the lock — send_callback is user/transport code.
        for row in rows:
            payload = row["payload"]
            message = (
                JSONRPCMessage.model_validate_json(payload)
                if payload is not None
                else None
            )
            await send_callback(EventMessage(message=message, event_id=row["event_id"]))

        return stream_id

    # ------------------------------------------------------------------ #
    # Retention (not part of the ABC)
    # ------------------------------------------------------------------ #
    async def cleanup_before(
        self,
        cutoff: float | None = None,
        *,
        max_age_seconds: float | None = None,
    ) -> int:
        """Delete events older than a retention boundary; return rows removed.

        Provide either an absolute ``cutoff`` (unix seconds — delete events with
        ``created_at < cutoff``) or ``max_age_seconds`` (delete events older than
        that, relative to now). ``max_age_seconds`` wins if both are given.
        Calling with neither is a no-op that returns 0.
        """
        if max_age_seconds is not None:
            cutoff = time.time() - max_age_seconds
        if cutoff is None:
            return 0

        async with self._lock:
            conn = self._connect()
            cur = conn.execute("DELETE FROM events WHERE created_at < ?", (cutoff,))
            conn.commit()
            return cur.rowcount

    def close(self) -> None:
        """Close the underlying SQLite connection if open."""
        if self._conn is not None:
            self._conn.close()
            self._conn = None
