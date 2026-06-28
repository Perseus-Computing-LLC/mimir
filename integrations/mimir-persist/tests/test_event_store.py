"""Tests for MimirEventStore — the persistent FastMCP EventStore over SQLite.

These exercise the real ``mcp.server.streamable_http.EventStore`` contract:
``store_event`` / ``replay_events_after`` plus the ``cleanup_before`` retention
helper. They use a temporary on-disk SQLite file so the restart-simulation test
can prove events survive a brand-new store instance on the same ``db_path``.
"""

from __future__ import annotations

import time

import pytest
from mcp.server.streamable_http import EventMessage
from mcp.types import JSONRPCMessage, JSONRPCRequest

from mimir_persist import MimirEventStore


@pytest.fixture
def db_path(tmp_path):
    return str(tmp_path / "mcp_events.db")


def _msg(request_id: int, method: str = "ping") -> JSONRPCMessage:
    """Build a valid JSON-RPC message to round-trip through the store."""
    return JSONRPCMessage(
        JSONRPCRequest(jsonrpc="2.0", id=request_id, method=method)
    )


async def _collect(store: MimirEventStore, last_event_id: str):
    """Replay after ``last_event_id`` and return (stream_id, [EventMessage...])."""
    captured: list[EventMessage] = []

    async def send(event_message: EventMessage) -> None:
        captured.append(event_message)

    stream_id = await store.replay_events_after(last_event_id, send)
    return stream_id, captured


@pytest.mark.asyncio
async def test_append_then_replay_returns_events_in_order(db_path):
    store = MimirEventStore(db_path=db_path)
    stream = "stream-A"

    first = await store.store_event(stream, _msg(1))
    second = await store.store_event(stream, _msg(2))
    third = await store.store_event(stream, _msg(3))

    # Replay after the first event -> returns the 2nd and 3rd, in order.
    stream_id, events = await _collect(store, first)

    assert stream_id == stream
    assert [e.event_id for e in events] == [second, third]
    ids = [e.message.root.id for e in events]
    assert ids == [2, 3]


@pytest.mark.asyncio
async def test_replay_isolated_per_stream(db_path):
    store = MimirEventStore(db_path=db_path)

    a1 = await store.store_event("stream-A", _msg(1))
    await store.store_event("stream-B", _msg(2))
    a3 = await store.store_event("stream-A", _msg(3))

    stream_id, events = await _collect(store, a1)

    # Only stream-A events after a1 replay; stream-B is not interleaved in.
    assert stream_id == "stream-A"
    assert [e.event_id for e in events] == [a3]


@pytest.mark.asyncio
async def test_replay_unknown_event_id_returns_none(db_path):
    store = MimirEventStore(db_path=db_path)
    await store.store_event("stream-A", _msg(1))

    stream_id, events = await _collect(store, "does-not-exist")

    assert stream_id is None
    assert events == []


@pytest.mark.asyncio
async def test_priming_event_none_payload_roundtrips(db_path):
    store = MimirEventStore(db_path=db_path)
    anchor = await store.store_event("stream-A", _msg(1))
    await store.store_event("stream-A", None)  # priming event, NULL payload

    _, events = await _collect(store, anchor)

    assert len(events) == 1
    assert events[0].message is None


@pytest.mark.asyncio
async def test_restart_simulation_new_store_replays_prior_events(db_path):
    """A fresh MimirEventStore on the same db_path replays earlier events."""
    writer = MimirEventStore(db_path=db_path)
    first = await writer.store_event("stream-A", _msg(1))
    await writer.store_event("stream-A", _msg(2))
    writer.close()  # simulate process shutdown

    # Brand-new instance, same file — must see the persisted history.
    reopened = MimirEventStore(db_path=db_path)
    stream_id, events = await _collect(reopened, first)

    assert stream_id == "stream-A"
    assert [e.message.root.id for e in events] == [2]


@pytest.mark.asyncio
async def test_cleanup_before_removes_old_events(db_path):
    store = MimirEventStore(db_path=db_path)

    old = await store.store_event("stream-A", _msg(1))
    # Backdate the first event well into the past.
    conn = store._connect()
    conn.execute(
        "UPDATE events SET created_at = ? WHERE event_id = ?",
        (time.time() - 10_000, old),
    )
    conn.commit()

    recent = await store.store_event("stream-A", _msg(2))

    removed = await store.cleanup_before(max_age_seconds=3600)
    assert removed == 1

    # The recent event survives and is still replayable from the start.
    rows = store._connect().execute("SELECT event_id FROM events").fetchall()
    assert [r["event_id"] for r in rows] == [recent]


@pytest.mark.asyncio
async def test_cleanup_before_absolute_cutoff(db_path):
    store = MimirEventStore(db_path=db_path)
    e1 = await store.store_event("s", _msg(1))
    conn = store._connect()
    conn.execute("UPDATE events SET created_at = 1000 WHERE event_id = ?", (e1,))
    conn.commit()
    await store.store_event("s", _msg(2))

    removed = await store.cleanup_before(cutoff=2000)
    assert removed == 1


@pytest.mark.asyncio
async def test_cleanup_before_noop_without_args(db_path):
    store = MimirEventStore(db_path=db_path)
    await store.store_event("s", _msg(1))
    assert await store.cleanup_before() == 0
